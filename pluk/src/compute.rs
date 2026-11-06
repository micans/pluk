// Computes: NAME::SPEC, NAME:=SPEC and ::SPEC (plan/COMPUTE.design.txt).
//
// Parsing (option time): an argument is split into NAME and SPEC, and
// SPEC into tokens, each introduced by a character:
//    ^VALUE    a constant, %-decoded
//    :COLSPEC  a column specification (a name, a '+' regex, a -k range)
//    ,OP       an operator, looked up in the table of ops.rs
// A SPEC not starting with one of ^ : , starts with a column (as pick).
//
// Compilation (once the header is known): column tokens are resolved to indexes,
// the stack is tracked statically, and each compute gets the next column index.

use regex::bytes::Regex;

use crate::colspec::{self, ColSpec, Element};
use crate::dict::Dicts;
use crate::filter;
use crate::io::compile_pattern;
use crate::names::ColumnNames;
use crate::ops::{self, Kind, OpDef, Pops, Prepare, Shape};
use crate::opt::pct_decode;

#[derive(Debug)]
pub enum Token {
    Const(Vec<u8>),
    Column(ColSpec),
    Op(&'static OpDef),
}

#[derive(Debug)]
pub struct ComputeSpec {
    pub text:   Vec<u8>,        // the argument, for messages
    pub name:   Vec<u8>,        // %-decoded; PLUKAAAAA, ... for an unnamed compute; for a map
                                // the NAME as given
    pub map:    Option<ColSpec>,// a list, '+' regex or -k range as NAME (INPLACE.design.txt 3)
    pub output: bool,           // '::' (true) or ':=' (false)
    pub tokens: Vec<Token>,
    pub position: usize,        // the number of column arguments before it (output order)
}

// Whether an argument is a compute.
pub fn is_compute(arg: &[u8]) -> bool {
    arg.windows(2).any(|w| w == b"::" || w == b":=")
}

// Names for unnamed computes: PLUKAAAAA, PLUKAAAAB, ..., PLUKAAAAZ, PLUKAAABA, ...
// (pick counts PICKAAAAA, ... with Perl's string increment).
pub struct UnnamedNames {
    letters: [u8; 5],
}

impl UnnamedNames {
    pub fn new() -> UnnamedNames {
        UnnamedNames { letters: *b"AAAAA" }
    }

    fn next(&mut self) -> Vec<u8> {
        let mut name = b"PLUK".to_vec();
        name.extend_from_slice(&self.letters);
        // Increment from the right, Z wrapping to A with a carry.
        for b in self.letters.iter_mut().rev() {
            if *b == b'Z' {
                *b = b'A';
            } else {
                *b += 1;
                break;
            }
        }
        name
    }
}

// headless (-k): ranges are allowed in column tokens.
pub fn parse(arg: &[u8], headless: bool, unnamed: &mut UnnamedNames) -> Result<ComputeSpec, String> {
    let shown = String::from_utf8_lossy(arg).into_owned();
    let k = arg.windows(2).position(|w| w == b"::" || w == b":=")
        .ok_or_else(|| format!("[{}]: not a compute", shown))?;
    let (raw_name, output, spec) = (&arg[..k], arg[k + 1] == b':', &arg[k + 2..]);

    let (name, map) = if raw_name.is_empty() {
        (unnamed.next(), None)
    } else {
        match parse_name(raw_name, headless).map_err(|e| format!("[{}]: {}", shown, e))? {
            Some(cs) => (raw_name.to_vec(), Some(cs)),
            None => (pct_decode(raw_name), None),
        }
    };
    if spec.is_empty() {
        return Err(format!("[{}]: compute is empty", shown));
    }

    let mut tokens = Vec::new();
    let starts_with_token = matches!(spec[0], b'^' | b':' | b',');
    let mut rest = spec;
    let mut kind = if starts_with_token { None } else { Some(b':') };
    loop {
        // The current token runs to the next ^ : , (or the end).
        let (kind_char, body_start) = match kind {
            Some(c) => (c, 0),
            None => (rest[0], 1),
        };
        let body_len = rest[body_start..].iter().position(|b| matches!(b, b'^' | b':' | b','))
            .unwrap_or(rest.len() - body_start);
        let body = &rest[body_start..body_start + body_len];
        tokens.push(token(kind_char, body, headless).map_err(|e| format!("[{}]: {}", shown, e))?);
        rest = &rest[body_start + body_len..];
        if rest.is_empty() {
            break;
        }
        kind = None;
    }
    Ok(ComputeSpec { text: arg.to_vec(), name, map, output, tokens, position: 0 })
}

fn token(kind: u8, body: &[u8], headless: bool) -> Result<Token, String> {
    match kind {
        b'^' => Ok(Token::Const(pct_decode(body))),
        b':' => {
            if body.is_empty() {
                return Err("column name missing after ':'".to_string());
            }
            Ok(Token::Column(colspec::parse(body, headless)?))
        }
        _ => {
            if body.is_empty() {
                return Err("operator missing after ','".to_string());
            }
            match ops::lookup(body) {
                Some(op) => Ok(Token::Op(op)),
                None => Err(format!("unknown operator [{}]", String::from_utf8_lossy(body))),
            }
        }
    }
}

// NAME: a single plain name (None), or a list, '+' regex or -k range (a map,
// INPLACE.design.txt section 3). '/' (pick's prefix/suffix names) is deferred.
fn parse_name(raw: &[u8], headless: bool) -> Result<Option<ColSpec>, String> {
    if raw.contains(&b'/') {
        return Err("prefix/suffix names ('/' in NAME) not yet supported".to_string());
    }
    let cs = colspec::parse(raw, headless)?;
    Ok(match cs.elements.as_slice() {
        [Element::Name(_)] => None,
        _ => Some(cs),
    })
}

// A compiled instruction.
#[derive(Debug)]
pub enum Instr {
    Field(usize),                   // push a column of the row (input or computed)
    Const(Vec<u8>),                 // push a constant
    Op(&'static OpDef),             // apply an operator
    OpWith(&'static OpDef, Attached), // an operator with what was prepared at compile time
}

// What the prepare step attaches to an operator instruction: a compiled constant
// regex (del, ed, ...), or the position of a dictionary in Dicts (map, dord).
#[derive(Debug)]
pub enum Attached {
    None,
    Regex(Regex),
    Dict(usize),
    Test(filter::Op, TestRhs),      // test x y T: the filter operator T and how y is used
}

// The right-hand side of test: the value y per row, or y prepared at compile time
// (a constant regex for ~ and /~, a constant dictionary for ~isin~ and ~isnotin~).
#[derive(Debug)]
pub enum TestRhs {
    Value,
    Regex(Regex),
    Dict(usize),
}

// What is known at compile time about a value on the stack.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Slot {
    Const(usize),       // a constant: the position of its instruction
    Column,
    Result(Kind),
}

#[derive(Debug)]
pub struct Compute {
    pub name:     Vec<u8>,
    pub index:    usize,        // its column index (N + i)
    pub output:   bool,
    pub position: usize,        // as ComputeSpec
    pub instrs:   Vec<Instr>,
    pub stack:    Vec<Slot>,    // the static stack at the end, for --test-compute
}

// Compile the computes in order. Each sees the input columns and the earlier
// computes; its name is added at the next index, or, for an existing name (-i,
// plan/INPLACE.design.txt), bound to that new index. Returns the computes and the
// extended names (input names followed by the compute names).
//
// inplace (-i): a compute may redefine a name; an input column at most once, a
// computed name any number of times.
pub fn compile(specs: &[ComputeSpec], input: &ColumnNames, inplace: bool, dicts: &Dicts)
    -> Result<(Vec<Compute>, ColumnNames), String>
{
    let mut names = input.clone();
    let mut computes = Vec::with_capacity(specs.len());
    let mut redefined = vec![false; input.len()];  // input columns redefined so far
    for spec in specs {
        let shown = String::from_utf8_lossy(&spec.text);
        // A map is one compute per selected column (current bindings), in COLSPEC
        // order, duplicates merged; each is compiled as a single-name compute named
        // after its column, so it sees the applications before it.
        let targets: Vec<Vec<u8>> = match &spec.map {
            None => vec![spec.name.clone()],
            Some(cs) => {
                let mut columns: Vec<usize> = Vec::new();
                for i in cs.expand(&names, "Compute", &format!("compute {}", String::from_utf8_lossy(&spec.name)))? {
                    if !columns.contains(&i) {
                        columns.push(i);
                    }
                }
                columns.iter().map(|&i| names.name(i).to_vec()).collect()
            }
        };
        for target in targets {
            let name = String::from_utf8_lossy(&target);
            if names.index_of(&target).is_some() {
                if !inplace {
                    return Err(format!("[{}]: name [{}] already exists (use -i for in-place change)", shown, name));
                }
                if let Some(i) = input.index_of(&target) {
                    if redefined[i] {
                        return Err(format!("[{}]: input column [{}] is redefined a second time \
                                            (with -i an input column can be redefined once)", shown, name));
                    }
                    redefined[i] = true;
                }
            }
            let mut instrs: Vec<Instr> = Vec::new();
            let mut stack: Vec<Slot> = Vec::new();
            for token in &spec.tokens {
                match token {
                    Token::Const(c) => {
                        instrs.push(Instr::Const(c.clone()));
                        stack.push(Slot::Const(instrs.len() - 1));
                    }
                    // __ is the compute's own name, before this compute binds it: the
                    // value being replaced (INPLACE.design.txt section 4). Tested on the
                    // raw text, so %5F%5F is a column called __.
                    Token::Column(cs) if cs.text == b"__" => match names.index_of(&target) {
                        Some(i) => {
                            instrs.push(Instr::Field(i));
                            stack.push(Slot::Column);
                        }
                        None => return Err(format!("(Compute) handle [{}] in compute {} does not reference a known \
                                                    entity among {}", name, name, names.joined())),
                    },
                    Token::Column(cs) => {
                        // A multi-column specification pushes every selected column (splat).
                        for i in cs.expand(&names, "Compute", &format!("compute {}", name))? {
                            instrs.push(Instr::Field(i));
                            stack.push(Slot::Column);
                        }
                    }
                    Token::Op(op) => {
                        let needed = match op.pops { Pops::N(n) | Pops::All(n) => n };
                        if stack.len() < needed {
                            return Err(format!("[{}]: stack underflow at operator {} (needs {}, has {})",
                                               shown, op.name, needed, stack.len()));
                        }
                        let instr = match op.prepare {
                            Prepare::Nothing => Instr::Op(op),
                            Prepare::ConstRegex(pos) =>
                                Instr::OpWith(op, Attached::Regex(const_regex(&instrs, &stack, pos, op, &shown)?)),
                            Prepare::ConstDict(pos) =>
                                Instr::OpWith(op, Attached::Dict(const_dict(&instrs, &stack, pos, op, dicts, &shown)?)),
                            Prepare::Test => Instr::OpWith(op, prepare_test(&instrs, &stack, dicts, &shown)?),
                            Prepare::FormatCount => {
                                check_format(&instrs, &stack, &shown)?;
                                Instr::Op(op)
                            }
                        };
                        instrs.push(instr);
                        match op.shape {
                            Shape::Result(kind) => {
                                let keep = match op.pops { Pops::N(n) => stack.len() - n, Pops::All(_) => 0 };
                                stack.truncate(keep);
                                stack.push(Slot::Result(kind));
                            }
                            Shape::Dup => stack.push(*stack.last().unwrap()),
                            Shape::Xch => {
                                let n = stack.len();
                                stack.swap(n - 1, n - 2);
                            }
                            Shape::Pop => {
                                stack.pop();
                            }
                        }
                    }
                }
            }
            let index = names.push(&target);
            computes.push(Compute { name: target.clone(), index, output: spec.output, position: spec.position,
                                    instrs, stack });
        }
    }
    Ok((computes, names))
}

// The pattern operand of a regex operator (pos places below the top of the stack:
// 0 for del, 1 for ed) must be a constant: it is compiled now, so that errors come
// before input is read. An empty pattern is refused, as for filters (in Perl it
// silently reuses the last pattern).
fn const_regex(instrs: &[Instr], stack: &[Slot], pos: usize, op: &OpDef, shown: &str) -> Result<Regex, String> {
    let pattern = match stack.len().checked_sub(pos + 1).map(|k| &stack[k]) {
        Some(Slot::Const(k)) => match &instrs[*k] {
            Instr::Const(c) => c,
            _ => unreachable!("Slot::Const refers to a constant"),
        },
        _ => return Err(format!("[{}]: the pattern operand of {} must be a constant (^PATTERN)", shown, op.name)),
    };
    if pattern.is_empty() {
        return Err(format!("[{}]: empty pattern for {}", shown, op.name));
    }
    compile_pattern(pattern, &format!("[{}]", shown))
}

// The dictionary operand of map and dord (pos places below the top) must be a
// constant naming a dictionary; it is resolved now to its position in Dicts.
fn const_dict(instrs: &[Instr], stack: &[Slot], pos: usize, op: &OpDef, dicts: &Dicts, shown: &str)
    -> Result<usize, String>
{
    let name = match stack.len().checked_sub(pos + 1).map(|k| &stack[k]) {
        Some(Slot::Const(k)) => match &instrs[*k] {
            Instr::Const(c) => c,
            _ => unreachable!("Slot::Const refers to a constant"),
        },
        _ => return Err(format!("[{}]: the dictionary operand of {} must be a constant (^NAME)", shown, op.name)),
    };
    dicts.index_of(name).ok_or_else(|| format!("[{}]: Dictionary [{}] not found", shown, String::from_utf8_lossy(name)))
}

// test x y T (plan/step15.txt decision 1): T, the top operand, must be a constant
// naming a filter operator with two operands (not ~hasval~ ~hasnotval~, which need
// a third value pick's test does not pass, nor the predicates); for ~ and /~ the
// pattern y must be a constant (compiled now), for ~isin~ and ~isnotin~ the
// dictionary name y.
fn prepare_test(instrs: &[Instr], stack: &[Slot], dicts: &Dicts, shown: &str) -> Result<Attached, String> {
    let token = match stack.last() {
        Some(Slot::Const(k)) => match &instrs[*k] {
            Instr::Const(c) => c,
            _ => unreachable!("Slot::Const refers to a constant"),
        },
        _ => return Err(format!("[{}]: the operator operand of test must be a constant (^/gt/, ^~eq~, ...)", shown)),
    };
    let t = String::from_utf8_lossy(token);
    let op = filter::operator(token)
        .ok_or_else(|| format!("[{}]: test: [{}] is not a filter operator", shown, t))?;
    let test_op = ops::lookup(b"test").expect("test is an operator");
    let rhs = match op {
        filter::Op::HasVal(_) | filter::Op::Pred(..) =>
            return Err(format!("[{}]: test: [{}] is not supported in test", shown, t)),
        filter::Op::Match(_) => TestRhs::Regex(const_regex(instrs, stack, 1, test_op, shown)?),
        filter::Op::IsIn(_) => TestRhs::Dict(const_dict(instrs, stack, 1, test_op, dicts, shown)?),
        _ => TestRhs::Value,
    };
    Ok(Attached::Test(op, rhs))
}

// The dictionaries that a dord names with a constant (a constant token directly
// before the operator, as pick's check for ^D,dord): they record the order of
// their keys while they are read, which happens before computes are compiled.
pub fn dord_dictionaries(specs: &[ComputeSpec]) -> Vec<Vec<u8>> {
    let mut names = Vec::new();
    for spec in specs {
        for pair in spec.tokens.windows(2) {
            if let [Token::Const(name), Token::Op(op)] = pair {
                if op.name == "dord" && !names.contains(name) {
                    names.push(name.clone());
                }
            }
        }
    }
    names
}

// fmtall with a constant format (the bottom of the stack): the stack depth is
// known, so a # count that does not match the values would fail on every row; it
// is an error before input is read. A format from a column is checked per row.
fn check_format(instrs: &[Instr], stack: &[Slot], shown: &str) -> Result<(), String> {
    if let Some(Slot::Const(k)) = stack.first() {
        if let Instr::Const(f) = &instrs[*k] {
            let (slots, values) = (ops::format_slots(f), stack.len() - 1);
            if slots != values {
                return Err(format!("[{}]: fmtall format [{}] has {} # for {} value{}", shown,
                                   String::from_utf8_lossy(f), slots, values, if values == 1 { "" } else { "s" }));
            }
        }
    }
    Ok(())
}

// The compiled form, for --test-compute.
pub fn describe_compiled(c: &Compute, names: &ColumnNames) -> String {
    let mut out = format!("compiled {} -> column {} ({})\n", String::from_utf8_lossy(&c.name), c.index + 1,
                          if c.output { "output" } else { "no output" });
    for instr in &c.instrs {
        let line = match instr {
            Instr::Field(i) => format!("  field {} ({})\n", i + 1, String::from_utf8_lossy(names.name(*i))),
            Instr::Const(b) => format!("  const [{}]\n", String::from_utf8_lossy(b)),
            Instr::Op(op) => format!("  op {}\n", op.name),
            Instr::OpWith(op, Attached::Regex(re)) => format!("  op {} (regex {})\n", op.name, re.as_str()),
            Instr::OpWith(op, Attached::Dict(d)) => format!("  op {} (dictionary {})\n", op.name, d),
            Instr::OpWith(op, Attached::None) => format!("  op {}\n", op.name),
            Instr::OpWith(op, Attached::Test(t, _)) => format!("  op {} ({:?})\n", op.name, t),
        };
        out.push_str(&line);
    }
    let slots: Vec<&str> = c.stack.iter().map(|s| match s {
        Slot::Const(_) => "const",
        Slot::Column => "column",
        Slot::Result(Kind::Str) => "str",
        Slot::Result(Kind::Num) => "num",
    }).collect();
    out.push_str(&format!("  stack: [{}]\n", slots.join(" ")));
    out
}

// The parsed form, for --test-compute.
pub fn describe(spec: &ComputeSpec) -> String {
    let what = if spec.map.is_some() { "map " } else { "" };
    let mut out = format!("compute {}{} ({})\n", what, String::from_utf8_lossy(&spec.name),
                          if spec.output { "output" } else { "no output" });
    for token in &spec.tokens {
        let line = match token {
            Token::Const(c) => format!("  const [{}]\n", String::from_utf8_lossy(c)),
            Token::Column(c) => format!("  column [{}]\n", String::from_utf8_lossy(&c.text)),
            Token::Op(op) => format!("  op {}\n", op.name),
        };
        out.push_str(&line);
    }
    out
}


#[cfg(test)]
mod tests {
    use super::*;

    fn parse1(arg: &str) -> Result<ComputeSpec, String> {
        parse(arg.as_bytes(), false, &mut UnnamedNames::new())
    }

    fn kinds(spec: &ComputeSpec) -> String {
        spec.tokens.iter().map(|t| match t {
            Token::Const(c) => format!("^{}", String::from_utf8_lossy(c)),
            Token::Column(c) => format!(":{}", String::from_utf8_lossy(&c.text)),
            Token::Op(op) => format!(",{}", op.name),
        }).collect()
    }

    #[test]
    fn syntax() {
        let s = parse1("x::fib^1,add").unwrap();
        assert_eq!((s.name.as_slice(), s.output, kinds(&s).as_str()), (&b"x"[..], true, ":fib^1,add"));
        let s = parse1("y:=^a:fib,cat").unwrap();
        assert_eq!((s.output, kinds(&s).as_str()), (false, "^a:fib,cat"));
        assert_eq!(kinds(&parse1("z::,dup").unwrap()), ",dup");
        assert_eq!(kinds(&parse1("z::^").unwrap()), "^");
        let s = parse1("a%2Fb::^x%2Cy%3Az%5E").unwrap();
        assert_eq!((s.name.as_slice(), kinds(&s).as_str()), (&b"a/b"[..], "^x,y:z^"));
        assert_eq!(kinds(&parse1("s::+num\\d,addall").unwrap()), ":+num\\d,addall");
        // The first '::' or ':=' ends NAME; later ones are ordinary ':' tokens.
        assert_eq!(kinds(&parse1("x::a:=b").unwrap()), ":a:=b");
    }

    #[test]
    fn unnamed() {
        let mut names = UnnamedNames::new();
        let a = parse(b"::^1", false, &mut names).unwrap();
        let b = parse(b"::^2", false, &mut names).unwrap();
        assert_eq!((a.name.as_slice(), b.name.as_slice()), (&b"PLUKAAAAA"[..], &b"PLUKAAAAB"[..]));
        let mut names = UnnamedNames { letters: *b"AAAAZ" };
        names.next();
        assert_eq!(names.next(), b"PLUKAAABA");
    }

    fn compile_args(args: &[&str]) -> Result<(Vec<Compute>, ColumnNames), String> {
        let mut unnamed = UnnamedNames::new();
        let specs: Vec<ComputeSpec> = args.iter().map(|a| parse(a.as_bytes(), false, &mut unnamed).unwrap()).collect();
        let mut row = crate::row::Row::new();
        row.line.extend_from_slice(b"a\tn1\tn2\tb");
        row.split_fields();
        compile(&specs, &ColumnNames::from_header(&row, false, false).unwrap(), false, &Dicts::new())
    }

    #[test]
    fn compiling() {
        let (c, names) = compile_args(&["s::+n\\d,addall", "t::s^x,cat"]).unwrap();
        assert_eq!((c[0].index, c[1].index, names.len()), (4, 5, 6));
        assert!(matches!(c[0].instrs[..], [Instr::Field(1), Instr::Field(2), Instr::Op(_)]));
        assert!(matches!(c[1].instrs[0], Instr::Field(4)));      // the earlier compute
        assert_eq!(c[1].stack, vec![Slot::Result(Kind::Str)]);
        let (c, _) = compile_args(&["d::a^x,del"]).unwrap();
        assert!(matches!(c[0].instrs[2], Instr::OpWith(_, Attached::Regex(_))));
        let (c, _) = compile_args(&["e::^1^2,xch,pop"]).unwrap();
        assert_eq!(c[0].stack, vec![Slot::Const(1)]);
        for bad in [&["a::^1"][..], &["x::^1", "x::^2"], &["x::^1,add"], &["x::a:b,del"], &["x::zz"], &["x::y", "y::^1"]] {
            assert!(compile_args(bad).is_err(), "{:?}", bad);
        }
    }

    fn compile_inplace(args: &[&str]) -> Result<(Vec<Compute>, ColumnNames), String> {
        let mut unnamed = UnnamedNames::new();
        let specs: Vec<ComputeSpec> = args.iter().map(|a| parse(a.as_bytes(), false, &mut unnamed).unwrap()).collect();
        let mut row = crate::row::Row::new();
        row.line.extend_from_slice(b"a\tb");
        row.split_fields();
        compile(&specs, &ColumnNames::from_header(&row, false, false).unwrap(), true, &Dicts::new())
    }

    #[test]
    fn inplace() {
        // a is rebound to the compute (index 2); the compute reads the old a (index 0).
        let (c, names) = compile_inplace(&["a::__^1,add"]).unwrap();
        assert!(matches!(c[0].instrs[0], Instr::Field(0)));
        assert_eq!((names.index_of(b"a"), names.current(0), names.is_current(0)), (Some(2), 2, false));
        assert_eq!(names.joined(), "b,a");
        // Computed names may be redefined repeatedly; each sees the previous binding.
        let (c, _) = compile_inplace(&["x:=^1", "x:=x^1,add", "x::x^1,add"]).unwrap();
        assert!(matches!(c[2].instrs[0], Instr::Field(3)));
        // An input column at most once.
        assert!(compile_inplace(&["a::^1", "a::^2"]).is_err());
        assert!(compile_inplace(&["x::__"]).is_err());
    }

    #[test]
    fn map() {
        // One compute per column, duplicates merged; the second sees the first.
        let (c, names) = compile_inplace(&["b:a:b::__:a,cat"]).unwrap();
        assert_eq!(c.len(), 2);
        assert_eq!((c[0].name.as_slice(), c[1].name.as_slice()), (&b"b"[..], &b"a"[..]));
        assert!(matches!(c[0].instrs[..], [Instr::Field(1), Instr::Field(0), Instr::Op(_)]));
        assert!(matches!(c[1].instrs[..], [Instr::Field(0), Instr::Field(0), Instr::Op(_)]));
        assert_eq!((names.index_of(b"b"), names.index_of(b"a")), (Some(2), Some(3)));
        // A list element must exist; the input-once rule holds per column.
        assert!(compile_inplace(&["a:zz::^1"]).is_err());
        assert!(compile_inplace(&["+.::^1", "a::^2"]).is_err());
    }

    #[test]
    fn errors() {
        for bad in ["x::", "x::,", "x::^a:", "x::^a,nosuch", "p/x/s::^1", "x::+(a", "+(::^1"] {
            assert!(parse1(bad).is_err(), "{}", bad);
        }
        // Map NAMEs; a range only with -k.
        assert!(parse1("+x.*::^1").unwrap().map.is_some());
        assert!(parse1("a:b::^1").unwrap().map.is_some());
        assert!(parse1("a::^1").unwrap().map.is_none());
        assert!(parse(b"2-3::^1", true, &mut UnnamedNames::new()).unwrap().map.is_some());
        assert!(parse(b"2-3::^1", false, &mut UnnamedNames::new()).unwrap().map.is_none());
    }
}
