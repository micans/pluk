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
    pub name:   Vec<u8>,        // %-decoded; PLUKAAAAA, ... for an unnamed compute
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

    let name = if raw_name.is_empty() {
        unnamed.next()
    } else {
        check_name(raw_name, headless).map_err(|e| format!("[{}]: {}", shown, e))?;
        pct_decode(raw_name)
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
    Ok(ComputeSpec { text: arg.to_vec(), name, output, tokens, position: 0 })
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

// NAME is a single plain name. A list, '+' regex or range (map) and '/' (pick's
// prefix/suffix names) are deferred (COMPUTE.design.txt section 8).
fn check_name(raw: &[u8], headless: bool) -> Result<(), String> {
    if raw.contains(&b'/') {
        return Err("prefix/suffix names ('/' in NAME) not yet supported".to_string());
    }
    match colspec::parse(raw, headless)?.elements.as_slice() {
        [Element::Name(_)] => Ok(()),
        _ => Err("a list, regex or range as compute NAME (map) not yet supported".to_string()),
    }
}

// A compiled instruction.
#[derive(Debug)]
pub enum Instr {
    Field(usize),                   // push a column of the row (input or computed)
    Const(Vec<u8>),                 // push a constant
    Op(&'static OpDef),             // apply an operator
    OpRegex(&'static OpDef, Regex), // an operator whose pattern operand (a constant) is compiled
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
// computes; its name is added at the next index. Returns the computes and the
// extended names (input names followed by the compute names).
pub fn compile(specs: &[ComputeSpec], input: &ColumnNames) -> Result<(Vec<Compute>, ColumnNames), String> {
    let mut names = input.clone();
    let mut computes = Vec::with_capacity(specs.len());
    for spec in specs {
        let shown = String::from_utf8_lossy(&spec.text);
        let name = String::from_utf8_lossy(&spec.name);
        if names.index_of(&spec.name).is_some() {
            return Err(format!("[{}]: name [{}] already exists (in-place computes are not yet supported)",
                               shown, name));
        }
        let mut instrs: Vec<Instr> = Vec::new();
        let mut stack: Vec<Slot> = Vec::new();
        for token in &spec.tokens {
            match token {
                Token::Const(c) => {
                    instrs.push(Instr::Const(c.clone()));
                    stack.push(Slot::Const(instrs.len() - 1));
                }
                Token::Column(cs) => {
                    // A multi-column specification pushes every selected column (splat).
                    for i in cs.expand(&names, "Compute", &format!("compute {}", name))? {
                        instrs.push(Instr::Field(i));
                        stack.push(Slot::Column);
                    }
                }
                Token::Op(op) => {
                    let needed = match op.pops { Pops::N(n) => n, Pops::All => 0 };
                    if stack.len() < needed {
                        return Err(format!("[{}]: stack underflow at operator {} (needs {}, has {})",
                                           shown, op.name, needed, stack.len()));
                    }
                    let instr = match op.prepare {
                        Prepare::Nothing => Instr::Op(op),
                        Prepare::ConstRegex => Instr::OpRegex(op, const_regex(&instrs, &stack, op, &shown)?),
                    };
                    instrs.push(instr);
                    match op.shape {
                        Shape::Result(kind) => {
                            let keep = match op.pops { Pops::N(n) => stack.len() - n, Pops::All => 0 };
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
        let index = names.push(&spec.name);
        computes.push(Compute { name: spec.name.clone(), index, output: spec.output, position: spec.position,
                                instrs, stack });
    }
    Ok((computes, names))
}

// The pattern operand (top of stack) of a regex operator must be a constant: it is
// compiled now, so that errors come before input is read. An empty pattern is
// refused, as for filters (in Perl it silently reuses the last pattern).
fn const_regex(instrs: &[Instr], stack: &[Slot], op: &OpDef, shown: &str) -> Result<Regex, String> {
    let pattern = match stack.last() {
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

// The compiled form, for --test-compute.
pub fn describe_compiled(c: &Compute, names: &ColumnNames) -> String {
    let mut out = format!("compiled {} -> column {} ({})
", String::from_utf8_lossy(&c.name), c.index + 1,
                          if c.output { "output" } else { "no output" });
    for instr in &c.instrs {
        let line = match instr {
            Instr::Field(i) => format!("  field {} ({})
", i + 1, String::from_utf8_lossy(names.name(*i))),
            Instr::Const(b) => format!("  const [{}]
", String::from_utf8_lossy(b)),
            Instr::Op(op) => format!("  op {}
", op.name),
            Instr::OpRegex(op, re) => format!("  op {} (regex {})
", op.name, re.as_str()),
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
    let mut out = format!("compute {} ({})\n", String::from_utf8_lossy(&spec.name),
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
        compile(&specs, &ColumnNames::from_header(&row, false, false).unwrap())
    }

    #[test]
    fn compiling() {
        let (c, names) = compile_args(&["s::+n\\d,addall", "t::s^x,cat"]).unwrap();
        assert_eq!((c[0].index, c[1].index, names.len()), (4, 5, 6));
        assert!(matches!(c[0].instrs[..], [Instr::Field(1), Instr::Field(2), Instr::Op(_)]));
        assert!(matches!(c[1].instrs[0], Instr::Field(4)));      // the earlier compute
        assert_eq!(c[1].stack, vec![Slot::Result(Kind::Str)]);
        let (c, _) = compile_args(&["d::a^x,del"]).unwrap();
        assert!(matches!(c[0].instrs[2], Instr::OpRegex(..)));
        let (c, _) = compile_args(&["e::^1^2,xch,pop"]).unwrap();
        assert_eq!(c[0].stack, vec![Slot::Const(1)]);
        for bad in [&["a::^1"][..], &["x::^1", "x::^2"], &["x::^1,add"], &["x::a:b,del"], &["x::zz"], &["x::y", "y::^1"]] {
            assert!(compile_args(bad).is_err(), "{:?}", bad);
        }
    }

    #[test]
    fn errors() {
        for bad in ["x::", "x::,", "x::^a:", "x::^a,nosuch", "+x.*::^1", "a:b::^1", "p/x/s::^1", "x::+(a"] {
            assert!(parse1(bad).is_err(), "{}", bad);
        }
        assert!(parse(b"2-3::^1", true, &mut UnnamedNames::new()).is_err());
        assert!(parse(b"2-3::^1", false, &mut UnnamedNames::new()).is_ok());
    }
}
