// Row filters: @NAME<op>VALUE (applied after computes) and @@NAME<op>VALUE
// (applied before computes); plan/step7.txt.
//
// A filter argument is parsed during option parsing (parse_filter: syntax and
// constants), and compiled once the column names are known (Filters::compile:
// column indexes). Each data row is then tested on its field bytes (keep).
// Numeric operators follow plan/NUMERIC.design.txt: a value that is not a number
// makes the comparison fail and is counted. Regex operators (~ /~) use the
// dialect of -/PAT; with :NAME they match the other column as a fixed string.
// Dictionary operators (~isin~ ~hasval~ ...) look up the field in a dictionary.

use std::cmp::Ordering;
use std::collections::HashMap;

use memchr::memmem;
use regex::bytes::Regex;

use crate::dict::Dicts;
use crate::io::compile_pattern;
use crate::names::ColumnNames;
use crate::num::{self, Num};
use crate::opt::pct_decode;
use crate::row::Row;

// @@ filters are applied before computes, @ filters after.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Stage {
    Pre,
    Post,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Cmp {
    Lt,
    Le,
    Eq,
    Ge,
    Gt,
    Ne,
}

impl Cmp {
    // Whether an ordering satisfies the comparison. None (unordered: a NaN) only
    // satisfies Ne, as in IEEE arithmetic.
    fn holds(self, ord: Option<Ordering>) -> bool {
        match ord {
            None => self == Cmp::Ne,
            Some(o) => match self {
                Cmp::Lt => o == Ordering::Less,
                Cmp::Le => o != Ordering::Greater,
                Cmp::Eq => o == Ordering::Equal,
                Cmp::Ge => o != Ordering::Less,
                Cmp::Gt => o == Ordering::Greater,
                Cmp::Ne => o != Ordering::Equal,
            },
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Op {
    Num(Cmp),       // /lt/ /le/ /eq/ /ge/ /gt/ /ne/
    Str(Cmp),       // ~lt~ ~le~ ~eq~ ~ge~ ~gt~ ~ne~ (byte comparison), = /=
    Match(bool),    // ~ (false) and /~ (true: negated)
    IsIn(bool),     // ~isin~, ~isnotin~ (negated)
    HasVal(bool),   // ~hasval~, ~hasnotval~ (negated)
}

impl Op {
    // Whether VALUE is a dictionary name.
    fn is_dict(self) -> bool {
        matches!(self, Op::IsIn(_) | Op::HasVal(_))
    }
}

// Whether an operator takes an extra argument after VALUE, and its separator.
enum Extra {
    No,
    Required(u8),
}

fn extra_rule(op: Op) -> Extra {
    match op {
        Op::HasVal(_) => Extra::Required(b'~'),
        _ => Extra::No,
    }
}

// The operators of the form /xx/ and ~xx~, with the ones not yet implemented.
const OPERATORS: [(&[u8], Option<Op>); 28] = [
    (b"/lt/", Some(Op::Num(Cmp::Lt))), (b"/le/", Some(Op::Num(Cmp::Le))),
    (b"/eq/", Some(Op::Num(Cmp::Eq))), (b"/ge/", Some(Op::Num(Cmp::Ge))),
    (b"/gt/", Some(Op::Num(Cmp::Gt))), (b"/ne/", Some(Op::Num(Cmp::Ne))),
    (b"~lt~", Some(Op::Str(Cmp::Lt))), (b"~le~", Some(Op::Str(Cmp::Le))),
    (b"~eq~", Some(Op::Str(Cmp::Eq))), (b"~ge~", Some(Op::Str(Cmp::Ge))),
    (b"~gt~", Some(Op::Str(Cmp::Gt))), (b"~ne~", Some(Op::Str(Cmp::Ne))),
    (b"/ep/", None), (b"/epx/", None), (b"/om/", None), (b"/omx/", None),
    (b"~isin~", Some(Op::IsIn(false))), (b"~isnotin~", Some(Op::IsIn(true))),
    (b"~hasval~", Some(Op::HasVal(false))), (b"~hasnotval~", Some(Op::HasVal(true))),
    (b"/all/", None), (b"/any/", None), (b"/none/", None), (b"/notall/", None),
    (b"=", Some(Op::Str(Cmp::Eq))), (b"/=", Some(Op::Str(Cmp::Ne))),
    (b"~", Some(Op::Match(false))), (b"/~", Some(Op::Match(true))),
];

// The value to compare with: a constant, or another column (:NAME).
#[derive(Clone, PartialEq, Debug)]
pub enum Value {
    Const(Vec<u8>),
    Column(Vec<u8>),
}

// A filter as given on the command line, before column names are known.
// (No PartialEq: a Regex cannot be compared.)
#[derive(Clone, Debug)]
pub struct FilterSpec {
    pub text:   Vec<u8>,            // the argument, for messages
    pub stage:  Stage,
    pub column: Vec<u8>,            // %-decoded
    pub op:     Op,
    pub value:  Value,              // %-decoded; a dictionary name for ~isin~ etc.
    pub extra:  Option<Vec<u8>>,    // the extra argument (~hasval~D~v: v), %-decoded
    const_num:  Option<Num>,        // a numeric operator's constant
    regex:      Option<Regex>,      // a regex operator's constant, compiled
    literal:    bool,               // VALUE was given as ^VALUE
}

// Parse a filter argument (starting with @ or @@). Syntax as pick:
//   @NAME/op/VALUE, @NAME~op~VALUE   op: 2 to 19 lowercase letters; the first
//                                    such token ends NAME
//   @NAME=VALUE, /=, ~, /~           the first '=' or '~' ends NAME
//   @NAME~hasval~DICT~v              an extra argument after VALUE
// VALUE ':NAME' refers to a column, '^...' is a literal; both tested before
// %-decoding, so %3A is a literal ':' (pick decodes first).
pub fn parse_filter(arg: &[u8]) -> Result<FilterSpec, String> {
    let shown = String::from_utf8_lossy(arg).into_owned();
    let (stage, spec) = if arg.starts_with(b"@assert@") {
        return Err(format!("[{}]: assertions (@assert@) not yet supported", shown));
    } else if let Some(s) = arg.strip_prefix(b"@@") {
        (Stage::Pre, s)
    } else if let Some(s) = arg.strip_prefix(b"@") {
        (Stage::Post, s)
    } else {
        return Err(format!("[{}]: not a filter", shown));
    };

    let (name, op_token, rest) = match find_operator(spec) {
        Some(found) => found,
        None => return Err(format!("Filter invalid [{}]: expected @NAME<op>VALUE or @NAME<op>:NAME", shown)),
    };
    let op = match OPERATORS.iter().find(|(token, _)| *token == op_token) {
        Some((_, Some(op))) => *op,
        Some((_, None)) => return Err(format!("[{}]: operator {} not yet supported", shown,
                                              String::from_utf8_lossy(op_token))),
        None => return Err(format!("[{}]: unrecognised operator {}", shown, String::from_utf8_lossy(op_token))),
    };

    // VALUE extends to the first '/' or '~'; what follows is an extra argument,
    // which only ~hasval~ ~hasnotval~ (and /ep/ /om/, item 4) take. The extra
    // argument itself cannot contain a further '/' or '~'.
    let op_shown = String::from_utf8_lossy(op_token);
    let is_delim = |b: &u8| *b == b'/' || *b == b'~';
    let (rest, extra) = match rest.iter().position(is_delim) {
        Some(k) => (&rest[..k], Some(&rest[k..])),
        None => (rest, None),
    };
    let extra = match (extra_rule(op), extra) {
        (Extra::No, None) => None,
        (Extra::No, Some(e)) => {
            return Err(format!("[{}]: extra argument [{}] not allowed with {} (write '/' or '~' in VALUE as %2F or %7E)",
                               shown, String::from_utf8_lossy(e), op_shown));
        }
        (Extra::Required(sep), None) => {
            return Err(format!("[{}]: {} requires an extra argument: @NAME{}VALUE{}ARG",
                               shown, op_shown, op_shown, sep as char));
        }
        (Extra::Required(sep), Some(e)) => {
            let arg = &e[1..];
            if e[0] != sep {
                return Err(format!("[{}]: the extra argument of {} is separated by '{}', not '{}'",
                                   shown, op_shown, sep as char, e[0] as char));
            }
            if arg.iter().any(is_delim) {
                return Err(format!("[{}]: extra argument [{}] contains '/' or '~' (write them as %2F or %7E)",
                                   shown, String::from_utf8_lossy(arg)));
            }
            if arg.starts_with(b":") {
                return Err(format!("[{}]: a column (':') as extra argument not yet supported", shown));
            }
            Some(pct_decode(arg))
        }
    };
    if name.is_empty() {
        return Err(format!("[{}]: column name missing", shown));
    }
    if name.starts_with(b"+") {
        return Err(format!("[{}]: regex column names (leading '+') not yet supported", shown));
    }
    if op.is_dict() {
        if rest.is_empty() {
            return Err(format!("[{}]: dictionary name missing", shown));
        }
        if rest.starts_with(b":") || rest.starts_with(b"^") {
            return Err(format!("[{}]: {} takes a dictionary name, not a column or literal (':' or '^')",
                               shown, op_shown));
        }
    }
    let literal = rest.starts_with(b"^");
    let value = if let Some(column) = rest.strip_prefix(b":") {
        if column.is_empty() {
            return Err(format!("[{}]: column name missing after ':'", shown));
        }
        Value::Column(pct_decode(column))
    } else if let Some(literal) = rest.strip_prefix(b"^") {
        Value::Const(pct_decode(literal))
    } else {
        Value::Const(pct_decode(rest))
    };
    let const_num = match (&op, &value) {
        (Op::Num(_), Value::Const(c)) => match num::parse_num(c) {
            Some(n) => Some(n),
            None => return Err(format!("[{}]: [{}] is not a number", shown, String::from_utf8_lossy(c))),
        },
        _ => None,
    };
    // A constant pattern is compiled now, so that a syntax error is an option error.
    // An empty pattern is refused, as for -/PAT.
    let regex = match (&op, &value) {
        (Op::Match(_), Value::Const(c)) => {
            if c.is_empty() {
                return Err(format!("[{}]: empty pattern", shown));
            }
            Some(compile_pattern(c, &format!("[{}]", shown))?)
        }
        _ => None,
    };
    Ok(FilterSpec { text: arg.to_vec(), stage, column: pct_decode(name), op, value, extra, const_num, regex, literal })
}

// Split at the first operator: a token [/~][a-z]{2,19}[/~] (as pick), else the
// first '=' or '~', with a preceding '/' included (/= /~). Returns NAME, the
// operator token and the rest.
fn find_operator(spec: &[u8]) -> Option<(&[u8], &[u8], &[u8])> {
    let is_delim = |b: u8| b == b'/' || b == b'~';
    for i in 0..spec.len() {
        if is_delim(spec[i]) {
            let letters = spec[i + 1..].iter().take_while(|b| b.is_ascii_lowercase()).count();
            let end = i + 1 + letters;
            if (2..=19).contains(&letters) && end < spec.len() && is_delim(spec[end]) {
                return Some((&spec[..i], &spec[i..=end], &spec[end + 1..]));
            }
        }
    }
    let k = spec.iter().position(|&b| b == b'=' || b == b'~')?;
    let start = if k > 0 && spec[k - 1] == b'/' { k - 1 } else { k };
    Some((&spec[..start], &spec[start..=k], &spec[k + 1..]))
}


// The operand a column is compared with, after compilation. A dictionary is
// borrowed from Dicts, which outlives the filters: the lifetime 'd says so.
enum Operand<'d> {
    Num(Num),                                   // numeric constant
    Bytes(Vec<u8>),                             // string constant
    Regex(Regex),                               // pattern constant
    Column(usize),                              // another column
    Dict(&'d HashMap<Vec<u8>, Vec<u8>>),        // dictionary (~isin~ etc.)
}

struct Filter<'d> {
    text:    Vec<u8>,
    column:  usize,
    op:      Op,
    operand: Operand<'d>,
    extra:   Vec<u8>,   // ~hasval~: the value to compare with
    n_na:    u64,       // numeric comparisons that failed on a value that is not a number
}

impl Filter<'_> {
    fn accept(&mut self, row: &Row) -> bool {
        let field = row.field(self.column);
        match self.op {
            Op::Num(cmp) => {
                let lhs = num::parse_num(field);
                let rhs = match &self.operand {
                    Operand::Num(n) => Some(*n),
                    Operand::Column(j) => num::parse_num(row.field(*j)),
                    _ => None,                      // not produced for numeric operators
                };
                match (lhs, rhs) {
                    (Some(a), Some(b)) => cmp.holds(num::compare(a, b)),
                    _ => {
                        self.n_na += 1;
                        false
                    }
                }
            }
            Op::Str(cmp) => {
                let rhs: &[u8] = match &self.operand {
                    Operand::Bytes(b) => b,
                    Operand::Column(j) => row.field(*j),
                    _ => &[],                       // not produced for string operators
                };
                cmp.holds(Some(field.cmp(rhs)))
            }
            Op::Match(negated) => {
                let found = match &self.operand {
                    Operand::Regex(re) => re.is_match(field),
                    Operand::Column(j) => memmem::find(field, row.field(*j)).is_some(),
                    _ => false,                     // not produced for regex operators
                };
                found != negated
            }
            Op::IsIn(negated) => {
                let found = match &self.operand {
                    Operand::Dict(d) => d.contains_key(field),
                    _ => false,                     // not produced for dictionary operators
                };
                found != negated
            }
            Op::HasVal(negated) => {
                let found = match &self.operand {
                    Operand::Dict(d) => d.get(field).is_some_and(|v| *v == self.extra),
                    _ => false,                     // not produced for dictionary operators
                };
                found != negated
            }
        }
    }
}

// All filters of a run, by stage. Within a stage filters are ANDed, or ORed with
// -o (@) and -s (@@); evaluation stops at the first filter that decides.
pub struct Filters<'d> {
    pre:     Vec<Filter<'d>>,
    post:    Vec<Filter<'d>>,
    pre_or:  bool,
    post_or: bool,
}

impl<'d> Filters<'d> {
    pub fn none() -> Filters<'d> {
        Filters { pre: Vec::new(), post: Vec::new(), pre_or: false, post_or: false }
    }

    pub fn is_empty(&self) -> bool {
        self.pre.is_empty() && self.post.is_empty()
    }

    // Resolve column names. As pick, a constant equal to a column name gives a hint;
    // unlike pick, not for a constant given as ^VALUE (explicitly literal), nor for a
    // number with a numeric operator (with -k every column is named by a number, and
    // the hint would be given for every numeric constant).
    // Dictionary names are resolved here too: seqlen and the --td dictionaries
    // exist only once the header has been read.
    pub fn compile(specs: &[FilterSpec], names: &ColumnNames, dicts: &'d Dicts, pre_or: bool, post_or: bool)
        -> Result<Filters<'d>, String>
    {
        let mut filters = Filters { pre: Vec::new(), post: Vec::new(), pre_or, post_or };
        for spec in specs {
            let (what, kind) = match spec.stage {
                Stage::Pre  => ("Preselect", "preselect"),
                Stage::Post => ("Select", "select"),
            };
            let lookup = |name: &[u8], label: &str| names.index_of(name).ok_or_else(|| {
                format!("({}) handle [{}] in {} does not reference a known entity among {}",
                        label, String::from_utf8_lossy(name), kind, names.joined())
            });
            let column = lookup(&spec.column, what)?;
            let operand = match &spec.value {
                Value::Const(d) if spec.op.is_dict() => match dicts.get(d) {
                    Some(dict) => Operand::Dict(&dict.map),
                    None => return Err(format!("Dictionary [{}] not found (in {})", String::from_utf8_lossy(d),
                                               String::from_utf8_lossy(&spec.text))),
                },
                Value::Column(name) => Operand::Column(lookup(name, &format!("Indirection {}", kind))?),
                Value::Const(c) => {
                    if names.index_of(c).is_some() && spec.const_num.is_none() && !spec.literal {
                        eprintln!("-- You used {} (a column handle) as a constant in {} test, did you want :{}? --",
                                  String::from_utf8_lossy(c), String::from_utf8_lossy(&spec.text),
                                  String::from_utf8_lossy(c));
                    }
                    match (spec.const_num, &spec.regex) {
                        (Some(n), _) => Operand::Num(n),
                        (None, Some(re)) => Operand::Regex(re.clone()),
                        (None, None) => Operand::Bytes(c.clone()),
                    }
                }
            };
            let extra = spec.extra.clone().unwrap_or_default();
            let filter = Filter { text: spec.text.clone(), column, op: spec.op, operand, extra, n_na: 0 };
            match spec.stage {
                Stage::Pre  => filters.pre.push(filter),
                Stage::Post => filters.post.push(filter),
            }
        }
        Ok(filters)
    }

    // Whether a data row passes the @@ filters, then (after the place where
    // computes will go) the @ filters.
    pub fn keep(&mut self, row: &Row) -> bool {
        stage_accepts(&mut self.pre, self.pre_or, row) && stage_accepts(&mut self.post, self.post_or, row)
    }

    // Per filter, the number of values that were not numbers, for the end-of-run report.
    pub fn report_na(&self) {
        for f in self.pre.iter().chain(&self.post) {
            if f.n_na > 0 {
                eprintln!("pluk: {}: {} value{} not a number (row{} not kept)",
                          String::from_utf8_lossy(&f.text), f.n_na,
                          if f.n_na == 1 { "" } else { "s" }, if f.n_na == 1 { "" } else { "s" });
            }
        }
    }
}

fn stage_accepts(filters: &mut [Filter<'_>], or: bool, row: &Row) -> bool {
    if filters.is_empty() {
        return true;
    }
    if or {
        filters.iter_mut().any(|f| f.accept(row))
    } else {
        filters.iter_mut().all(|f| f.accept(row))
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn spec(arg: &str) -> Result<FilterSpec, String> {
        parse_filter(arg.as_bytes())
    }

    #[test]
    fn parsing() {
        let f = spec("@x/gt/3").unwrap();
        assert_eq!((f.stage, f.column.as_slice(), f.op), (Stage::Post, &b"x"[..], Op::Num(Cmp::Gt)));
        assert_eq!(f.const_num, Some(Num::I(3)));
        let f = spec("@@a%2Fb~le~:c%7Ed").unwrap();
        assert_eq!((f.stage, f.column.as_slice(), f.op), (Stage::Pre, &b"a/b"[..], Op::Str(Cmp::Le)));
        assert_eq!(f.value, Value::Column(b"c~d".to_vec()));
        assert_eq!(spec("@x~eq~^:y").unwrap().value, Value::Const(b":y".to_vec()));
        assert_eq!(spec("@x~eq~%3Ay").unwrap().value, Value::Const(b":y".to_vec()));
        assert_eq!(spec("@x~eq~").unwrap().value, Value::Const(Vec::new()));
        assert_eq!(spec("@x/ne/-1.5e3").unwrap().const_num, Some(Num::F(-1500.0)));
        // The first operator token ends the name.
        assert_eq!(spec("@x/lt/5").unwrap().column, b"x");
        for bad in ["@x/gt/abc", "@x/gt/", "@/gt/3", "@x/foo/3", "@x/gt/3/4", "@x~eq~a/b",
                    "@x", "@+x/gt/3", "@x/gt/:", "@assert@x/gt/3", "@x/ep/3", "@x~", "@x/~",
                    "@x~^", "@x~a(", "@x~a/b", "@x=a~b", "@x~isin~", "@x~isin~:y", "@x~isin~^D",
                    "@x~isin~D~v", "@x~hasval~D", "@x~hasval~D/v", "@x~hasval~D~:y", "@x~hasval~D~a~b"] {
            assert!(spec(bad).is_err(), "{}", bad);
        }
    }

    #[test]
    fn parsing_eq_match() {
        assert_eq!(spec("@x=3").unwrap().op, Op::Str(Cmp::Eq));
        assert_eq!(spec("@x/=3").unwrap().op, Op::Str(Cmp::Ne));
        assert_eq!(spec("@x=").unwrap().value, Value::Const(Vec::new()));
        let f = spec("@x~a.b").unwrap();
        assert_eq!((f.column.as_slice(), f.op), (&b"x"[..], Op::Match(false)));
        assert!(f.regex.unwrap().is_match(b"xaxbx"));
        let f = spec("@x/~^a%2Fb%7Ec").unwrap();
        assert_eq!(f.op, Op::Match(true));
        assert!(f.regex.unwrap().is_match(b"a/b~c"));
        // ':' makes a column reference: no regex.
        let f = spec("@x~:y").unwrap();
        assert_eq!(f.value, Value::Column(b"y".to_vec()));
        assert!(f.regex.is_none());
        // A /xx/ token takes precedence over = and ~.
        assert_eq!(spec("@a=b/gt/3").unwrap().column, b"a=b");
    }

    #[test]
    fn parsing_dict() {
        let f = spec("@x~isin~D").unwrap();
        assert_eq!((f.op, f.value, f.extra), (Op::IsIn(false), Value::Const(b"D".to_vec()), None));
        let f = spec("@x~hasnotval~D~a%7Eb").unwrap();
        assert_eq!((f.op, f.extra), (Op::HasVal(true), Some(b"a~b".to_vec())));
        assert_eq!(spec("@x~hasval~D~").unwrap().extra, Some(Vec::new()));
        assert_eq!(spec("@x~hasval~D~%3Ay").unwrap().extra, Some(b":y".to_vec()));
    }

    #[test]
    fn comparisons() {
        assert!(Cmp::Lt.holds(Some(Ordering::Less)));
        assert!(Cmp::Le.holds(Some(Ordering::Equal)));
        assert!(!Cmp::Ge.holds(Some(Ordering::Less)));
        assert!(Cmp::Ne.holds(None));
        assert!(!Cmp::Eq.holds(None));
        assert!(!Cmp::Lt.holds(None));
    }
}
