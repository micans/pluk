// Row filters: @NAME<op>VALUE (applied after computes) and @@NAME<op>VALUE
// (applied before computes); plan/step7.txt. Assertions @assert@... and
// @@assert@... have the same form; a row that fails one ends the run
// (plan/step8.txt item 5).
//
// A filter argument is parsed during option parsing (parse_filter: syntax and
// constants), and compiled once the column names are known (Filters::compile:
// column indexes). Each data row is then tested on its field bytes (keep).
// The handle NAME is a column specification (plan/COLSPEC.design.txt): with a
// list, regex or range the filter holds if the test holds for every column.
// Numeric operators follow plan/NUMERIC.design.txt: a value that is not a number
// makes the comparison fail and is counted. Regex operators (~ /~) use the
// dialect of -/PAT; with :NAME they match the other column as a fixed string.
// Dictionary operators (~isin~ ~hasval~ ...) look up the field in a dictionary.

use std::cmp::Ordering;
use std::collections::HashMap;

use memchr::memmem;
use regex::bytes::Regex;

use crate::colspec::{self, ColSpec, Element};
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
    Ep(bool),       // /ep/, /epx/ (negated): within an additive band
    Om(bool),       // /om/, /omx/ (negated): within a multiplicative band
    Bit(BitTest, bool), // /all/ /any/, /notall/ /none/ (negated)
    Pred(Pred, bool),   // /ISINT /ISNUM, /NOTINT /NOTNUM (negated): no VALUE
}

// Unary predicates on a value (step8 item 4).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Pred {
    Int,            // parse_num gives an integer (I)
    Num,            // parse_num gives a number (I or F, including inf and nan)
}

const PREDICATES: [(&[u8], Op); 4] = [
    (b"ISINT", Op::Pred(Pred::Int, false)), (b"NOTINT", Op::Pred(Pred::Int, true)),
    (b"ISNUM", Op::Pred(Pred::Num, false)), (b"NOTNUM", Op::Pred(Pred::Num, true)),
];

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum BitTest {
    All,            // every bit of the mask is set
    Any,            // at least one bit of the mask is set
}

impl Op {
    // Whether VALUE is a dictionary name.
    fn is_dict(self) -> bool {
        matches!(self, Op::IsIn(_) | Op::HasVal(_))
    }

    // Whether the operands are numbers.
    fn is_num(self) -> bool {
        matches!(self, Op::Num(_) | Op::Ep(_) | Op::Om(_) | Op::Bit(..))
    }
}

// The bands of /ep/ and /om/ when none is given, as pick ($::EPSILON, $::OOM).
const DEFAULT_EPSILON: Num = Num::F(0.0001);
const DEFAULT_OOM: Num = Num::I(2);

// Whether an operator takes an extra argument after VALUE, and its separator.
enum Extra {
    No,
    Required(u8),
    Optional(u8),
}

fn extra_rule(op: Op) -> Extra {
    match op {
        Op::HasVal(_) => Extra::Required(b'~'),
        Op::Ep(_) | Op::Om(_) => Extra::Optional(b'/'),
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
    (b"/ep/", Some(Op::Ep(false))), (b"/epx/", Some(Op::Ep(true))),
    (b"/om/", Some(Op::Om(false))), (b"/omx/", Some(Op::Om(true))),
    (b"~isin~", Some(Op::IsIn(false))), (b"~isnotin~", Some(Op::IsIn(true))),
    (b"~hasval~", Some(Op::HasVal(false))), (b"~hasnotval~", Some(Op::HasVal(true))),
    (b"/all/", Some(Op::Bit(BitTest::All, false))), (b"/any/", Some(Op::Bit(BitTest::Any, false))),
    (b"/none/", Some(Op::Bit(BitTest::Any, true))), (b"/notall/", Some(Op::Bit(BitTest::All, true))),
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
    pub assert: bool,               // @assert@ / @@assert@
    pub handle: ColSpec,            // the column(s) tested
    pub op:     Op,
    pub value:  Value,              // %-decoded; a dictionary name for ~isin~ etc.
    pub extra:  Option<Vec<u8>>,    // the extra argument (~hasval~D~v: v; /ep/y/eps: eps), %-decoded
    const_num:  Option<Num>,        // a numeric operator's constant
    band:       Option<Num>,        // /ep/ /om/: the band (eps, k), default if not given
    regex:      Option<Regex>,      // a regex operator's constant, compiled
    literal:    bool,               // VALUE was given as ^VALUE
}

// Parse a filter argument (starting with @ or @@). Syntax as pick:
//   @NAME/op/VALUE, @NAME~op~VALUE   op: 2 to 19 lowercase letters; the first
//                                    such token ends NAME
//   @NAME=VALUE, /=, ~, /~           the first '=' or '~' ends NAME
//   @NAME~hasval~DICT~v              an extra argument after VALUE (required)
//   @NAME/ep/VALUE/eps, /om/VALUE/k   an extra argument after VALUE (optional)
//   @NAME/ISINT                      a predicate: uppercase, last, no VALUE
// VALUE ':NAME' refers to a column, '^...' is a literal; both tested before
// %-decoding, so %3A is a literal ':' (pick decodes first).
// headless (-k): ranges are allowed in the handle.
pub fn parse_filter(arg: &[u8], headless: bool) -> Result<FilterSpec, String> {
    let shown = String::from_utf8_lossy(arg).into_owned();
    let (stage, assert, spec) = if let Some(s) = arg.strip_prefix(b"@@assert@") {
        (Stage::Pre, true, s)
    } else if let Some(s) = arg.strip_prefix(b"@assert@") {
        (Stage::Post, true, s)
    } else if let Some(s) = arg.strip_prefix(b"@@") {
        (Stage::Pre, false, s)
    } else if let Some(s) = arg.strip_prefix(b"@") {
        (Stage::Post, false, s)
    } else {
        return Err(format!("[{}]: not a filter", shown));
    };

    let (name, op_token, rest) = match find_operator(spec) {
        Some(found) => found,
        None => match find_predicate(spec) {
            Some((name, token)) => return parse_predicate(arg, stage, assert, name, token, headless),
            None => return Err(format!("Filter invalid [{}]: expected @NAME<op>VALUE, @NAME<op>:NAME or @NAME/PREDICATE",
                                       shown)),
        },
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
        (Extra::Optional(_), None) => None,
        (Extra::Required(sep) | Extra::Optional(sep), Some(e)) => {
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
    let handle = colspec::parse(name, headless).map_err(|e| format!("[{}]: {}", shown, e))?;
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
        // A single plain column: lists, regexes and ranges are refused here.
        match colspec::parse(column, headless).map_err(|e| format!("[{}]: {}", shown, e))?.elements.as_slice() {
            [Element::Name(n)] => Value::Column(n.clone()),
            _ => return Err(format!("[{}]: a single column name is required after ':' (no list, regex or range)", shown)),
        }
    } else if let Some(literal) = rest.strip_prefix(b"^") {
        Value::Const(pct_decode(literal))
    } else {
        Value::Const(pct_decode(rest))
    };
    let const_num = match (&op, &value) {
        (op, Value::Const(c)) if op.is_num() => match num::parse_num(c) {
            Some(n) => Some(n),
            None => return Err(format!("[{}]: [{}] is not a number", shown, String::from_utf8_lossy(c))),
        },
        _ => None,
    };
    // A constant mask of a bit filter is a non-negative integer.
    if let (Op::Bit(..), Some(n)) = (op, const_num) {
        match n {
            Num::I(m) if m >= 0 => (),
            Num::I(_) => return Err(format!("[{}]: the mask of {} cannot be negative", shown, op_shown)),
            Num::F(_) => return Err(format!("[{}]: the mask of {} must be an integer", shown, op_shown)),
        }
    }
    // The band of /ep/ (eps >= 0) and /om/ (k >= 1); nan is refused.
    let band = match op {
        Op::Ep(_) | Op::Om(_) => {
            let (default, min, what) = match op {
                Op::Ep(_) => (DEFAULT_EPSILON, 0, "at least 0"),
                _ => (DEFAULT_OOM, 1, "at least 1"),
            };
            match &extra {
                None => Some(default),
                Some(e) => match num::parse_num(e) {
                    Some(b) if num::compare(b, Num::I(min)).is_some_and(|o| o != Ordering::Less) => Some(b),
                    Some(_) => return Err(format!("[{}]: the band of {} must be {}", shown, op_shown, what)),
                    None => return Err(format!("[{}]: band [{}] is not a number", shown, String::from_utf8_lossy(e))),
                },
            }
        }
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
    Ok(FilterSpec { text: arg.to_vec(), stage, assert, handle, op, value, extra, const_num, band, regex, literal })
}

// A predicate: the argument ends in /UPPERCASE, with no other operator (only
// tried when find_operator() finds none, so NAME contains no '=' or '~').
// Returns NAME and the predicate name.
fn find_predicate(spec: &[u8]) -> Option<(&[u8], &[u8])> {
    let k = spec.iter().rposition(|&b| b == b'/')?;
    let token = &spec[k + 1..];
    if !token.is_empty() && token.iter().all(u8::is_ascii_uppercase) {
        Some((&spec[..k], token))
    } else {
        None
    }
}

fn parse_predicate(arg: &[u8], stage: Stage, assert: bool, name: &[u8], token: &[u8], headless: bool)
    -> Result<FilterSpec, String>
{
    let shown = String::from_utf8_lossy(arg);
    let op = match PREDICATES.iter().find(|(t, _)| *t == token) {
        Some((_, op)) => *op,
        None => return Err(format!("[{}]: unrecognised predicate /{} (known: /ISINT /NOTINT /ISNUM /NOTNUM)",
                                   shown, String::from_utf8_lossy(token))),
    };
    if name.is_empty() {
        return Err(format!("[{}]: column name missing", shown));
    }
    let handle = colspec::parse(name, headless).map_err(|e| format!("[{}]: {}", shown, e))?;
    Ok(FilterSpec { text: arg.to_vec(), stage, assert, handle, op, value: Value::Const(Vec::new()), extra: None,
                    const_num: None, band: None, regex: None, literal: false })
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

// The outcome of a test on one column.
#[derive(Clone, Copy, PartialEq)]
enum Outcome {
    Pass,
    Fail,
    NotNumber,      // a numeric test met a value that is not a number
    NotInteger,     // a bit test met a number that is not an integer
}

struct Filter<'d> {
    text:    Vec<u8>,
    columns: Vec<usize>,    // the columns tested, without duplicates
    names:   Vec<Vec<u8>>,  // their names, for the message of a failed assertion
    op:      Op,
    operand: Operand<'d>,
    extra:   Vec<u8>,   // ~hasval~: the value to compare with
    band:    Num,       // /ep/ /om/: eps or k
    n_na:    u64,       // rows not kept because a value was not a number
    n_nonint: u64,      // rows not kept because a value was not an integer (bit tests)
}

impl Filter<'_> {
    // The test must hold for every column; the first column that fails decides,
    // and its reason is counted (so counts are per row, not per value).
    fn accept(&mut self, row: &Row) -> bool {
        match self.first_failure(row) {
            None => true,
            Some((_, Outcome::NotNumber)) => {
                self.n_na += 1;
                false
            }
            Some((_, Outcome::NotInteger)) => {
                self.n_nonint += 1;
                false
            }
            Some(_) => false,
        }
    }

    // The position (in self.columns) of the first column that does not pass, and
    // why; None if the test holds for every column.
    fn first_failure(&self, row: &Row) -> Option<(usize, Outcome)> {
        for (k, &c) in self.columns.iter().enumerate() {
            match self.test(row.field(c), row) {
                Outcome::Pass => (),
                other => return Some((k, other)),
            }
        }
        None
    }

    // An assertion: Err with a description of the first failing column.
    fn check(&self, row: &Row) -> Result<(), String> {
        match self.first_failure(row) {
            None => Ok(()),
            Some((k, outcome)) => {
                let why = match outcome {
                    Outcome::NotNumber => " (not a number)",
                    Outcome::NotInteger => " (not an integer)",
                    _ => "",
                };
                Err(format!("assertion {} failed: column {} value [{}]{}", String::from_utf8_lossy(&self.text),
                            String::from_utf8_lossy(&self.names[k]),
                            String::from_utf8_lossy(row.field(self.columns[k])), why))
            }
        }
    }

    fn test(&self, field: &[u8], row: &Row) -> Outcome {
        let pass = match self.op {
            Op::Num(_) | Op::Ep(_) | Op::Om(_) | Op::Bit(..) => {
                let lhs = num::parse_num(field);
                let rhs = match &self.operand {
                    Operand::Num(n) => Some(*n),
                    Operand::Column(j) => num::parse_num(row.field(*j)),
                    _ => None,                      // not produced for numeric operators
                };
                match (lhs, rhs) {
                    (Some(a), Some(b)) => match self.op {
                        Op::Num(cmp) => cmp.holds(num::compare(a, b)),
                        Op::Ep(negated) => num::within_eps(a, b, self.band) != negated,
                        Op::Om(negated) => num::within_oom(a, b, self.band) != negated,
                        Op::Bit(test, negated) => match (a, b) {
                            (Num::I(x), Num::I(m)) => match test {
                                BitTest::All => num::bits_all(x, m) != negated,
                                BitTest::Any => num::bits_any(x, m) != negated,
                            },
                            _ => return Outcome::NotInteger,
                        },
                        _ => false,                 // not a numeric operator
                    },
                    _ => return Outcome::NotNumber,
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
            Op::Pred(pred, negated) => Filter::predicate(field, pred, negated),
            Op::HasVal(negated) => {
                let found = match &self.operand {
                    Operand::Dict(d) => d.get(field).is_some_and(|v| *v == self.extra),
                    _ => false,                     // not produced for dictionary operators
                };
                found != negated
            }
        };
        if pass { Outcome::Pass } else { Outcome::Fail }
    }

    // Predicates never count: a value that is not a number is what they test.
    fn predicate(field: &[u8], pred: Pred, negated: bool) -> bool {
        let holds = match (pred, num::parse_num(field)) {
            (Pred::Int, Some(Num::I(_))) => true,
            (Pred::Int, _) => false,
            (Pred::Num, n) => n.is_some(),
        };
        holds != negated
    }
}

// What the filters decided for a row. --demux needs to know the stage: a row
// rejected by an @@ filter never reaches the output, one rejected by an @ filter
// still opens its demux file (as pick).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Verdict {
    Keep,
    RejectPre,      // by an @@ filter
    RejectPost,     // by an @ filter
}

// All filters and assertions of a run, by stage. Within a stage filters are
// ANDed, or ORed with -o (@) and -s (@@); evaluation stops at the first filter
// that decides. Assertions are checked before the filters of their stage, and
// are not affected by -o and -s.
pub struct Filters<'d> {
    pre:         Vec<Filter<'d>>,
    post:        Vec<Filter<'d>>,
    pre_assert:  Vec<Filter<'d>>,
    post_assert: Vec<Filter<'d>>,
    pre_or:      bool,
    post_or:     bool,
}

impl<'d> Filters<'d> {
    pub fn none() -> Filters<'d> {
        Filters { pre: Vec::new(), post: Vec::new(), pre_assert: Vec::new(), post_assert: Vec::new(),
                  pre_or: false, post_or: false }
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
        let mut filters = Filters { pre_or, post_or, ..Filters::none() };
        for spec in specs {
            let (what, kind) = match (spec.stage, spec.assert) {
                (Stage::Pre, false)  => ("Preselect", "preselect"),
                (Stage::Post, false) => ("Select", "select"),
                (Stage::Pre, true)   => ("Preselect", "preassert"),
                (Stage::Post, true)  => ("Select", "assert"),       // as pick
            };
            let lookup = |name: &[u8], label: &str| names.index_of(name).ok_or_else(|| {
                format!("({}) handle [{}] in {} does not reference a known entity among {}",
                        label, String::from_utf8_lossy(name), kind, names.joined())
            });
            let mut columns: Vec<usize> = Vec::new();
            for c in spec.handle.expand(names, what, kind)? {
                if !columns.contains(&c) {
                    columns.push(c);
                }
            }
            let operand = match &spec.value {
                Value::Const(d) if spec.op.is_dict() => match dicts.get(d) {
                    Some(dict) => Operand::Dict(&dict.map),
                    None => return Err(format!("Dictionary [{}] not found (in {})", String::from_utf8_lossy(d),
                                               String::from_utf8_lossy(&spec.text))),
                },
                Value::Column(name) => Operand::Column(lookup(name, &format!("Indirection {}", kind))?),
                Value::Const(c) => {
                    if !c.is_empty() && names.index_of(c).is_some() && spec.const_num.is_none() && !spec.literal {
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
            let band = spec.band.unwrap_or(Num::I(0));
            let column_names = columns.iter().map(|&c| names.name(c).to_vec()).collect();
            let filter = Filter { text: spec.text.clone(), columns, names: column_names, op: spec.op, operand, extra, band,
                                  n_na: 0, n_nonint: 0 };
            match (spec.stage, spec.assert) {
                (Stage::Pre, false)  => filters.pre.push(filter),
                (Stage::Post, false) => filters.post.push(filter),
                (Stage::Pre, true)   => filters.pre_assert.push(filter),
                (Stage::Post, true)  => filters.post_assert.push(filter),
            }
        }
        Ok(filters)
    }

    // Whether a data row passes the @@ filters, then (after the place where
    // computes will go) the @ filters. Each stage first checks its assertions:
    // Err describes the first assertion that fails, which ends the run. So @@assert@
    // sees every row, and @assert@ every row that passes the @@ filters (as pick).
    pub fn keep(&mut self, row: &Row) -> Result<Verdict, String> {
        check_all(&self.pre_assert, row)?;
        if !stage_accepts(&mut self.pre, self.pre_or, row) {
            return Ok(Verdict::RejectPre);
        }
        check_all(&self.post_assert, row)?;
        Ok(if stage_accepts(&mut self.post, self.post_or, row) { Verdict::Keep } else { Verdict::RejectPost })
    }

    // Per filter, the number of rows not kept because a value was not a number,
    // or (bit filters) not an integer, for the end-of-run report. With one column
    // a row has one value, and the message says so.
    pub fn report_na(&self) {
        for f in self.pre.iter().chain(&self.post) {
            for (n, what) in [(f.n_na, "a number"), (f.n_nonint, "an integer")] {
                if n > 0 {
                    let s = if n == 1 { "" } else { "s" };
                    let text = String::from_utf8_lossy(&f.text);
                    if f.columns.len() == 1 {
                        eprintln!("pluk: {}: {} value{} not {} (row{} not kept)", text, n, s, what, s);
                    } else {
                        eprintln!("pluk: {}: {} row{} with a value not {} (not kept)", text, n, s, what);
                    }
                }
            }
        }
    }
}

fn check_all(assertions: &[Filter<'_>], row: &Row) -> Result<(), String> {
    for a in assertions {
        a.check(row)?;
    }
    Ok(())
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
        parse_filter(arg.as_bytes(), false)
    }

    // The single plain name of a handle, for the tests below.
    fn name(f: &FilterSpec) -> Vec<u8> {
        match f.handle.elements.as_slice() {
            [Element::Name(n)] => n.clone(),
            _ => panic!("not a single name"),
        }
    }

    #[test]
    fn parsing() {
        let f = spec("@x/gt/3").unwrap();
        assert_eq!((f.stage, name(&f).as_slice(), f.op), (Stage::Post, &b"x"[..], Op::Num(Cmp::Gt)));
        assert_eq!(f.const_num, Some(Num::I(3)));
        let f = spec("@@a%2Fb~le~:c%7Ed").unwrap();
        assert_eq!((f.stage, name(&f).as_slice(), f.op), (Stage::Pre, &b"a/b"[..], Op::Str(Cmp::Le)));
        assert_eq!(f.value, Value::Column(b"c~d".to_vec()));
        assert_eq!(spec("@x~eq~^:y").unwrap().value, Value::Const(b":y".to_vec()));
        assert_eq!(spec("@x~eq~%3Ay").unwrap().value, Value::Const(b":y".to_vec()));
        assert_eq!(spec("@x~eq~").unwrap().value, Value::Const(Vec::new()));
        assert_eq!(spec("@x/ne/-1.5e3").unwrap().const_num, Some(Num::F(-1500.0)));
        // The first operator token ends the name.
        assert_eq!(name(&spec("@x/lt/5").unwrap()), b"x");
        for bad in ["@x/gt/abc", "@x/gt/", "@/gt/3", "@x/foo/3", "@x/gt/3/4", "@x~eq~a/b",
                    "@x", "@+/gt/3", "@+a(/gt/3", "@x/gt/:", "@assert@x", "@@assert@/gt/3", "@x/gt/:a:b", "@x/gt/:+a", "@x::y/gt/3", "@x~", "@x/~",
                    "@x~^", "@x~a(", "@x~a/b", "@x=a~b", "@x~isin~", "@x~isin~:y", "@x~isin~^D",
                    "@x~isin~D~v", "@x~hasval~D", "@x~hasval~D/v", "@x~hasval~D~:y", "@x~hasval~D~a~b",
                    "@x/ep/1/", "@x/ep/1/-1", "@x/ep/1/nan", "@x/ep/1/abc", "@x/ep/1~2", "@x/ep/1/:y",
                    "@x/om/1/0.5", "@x/om/1/0", "@x/omx/1/2/3", "@x/ep/abc",
                    "@x/all/5.0", "@x/any/-1", "@x/none/abc", "@x/notall/1e3", "@x/all/", "@x/all/5/1"] {
            assert!(spec(bad).is_err(), "{}", bad);
        }
    }

    #[test]
    fn parsing_eq_match() {
        assert_eq!(spec("@x=3").unwrap().op, Op::Str(Cmp::Eq));
        assert_eq!(spec("@x/=3").unwrap().op, Op::Str(Cmp::Ne));
        assert_eq!(spec("@x=").unwrap().value, Value::Const(Vec::new()));
        let f = spec("@x~a.b").unwrap();
        assert_eq!((name(&f).as_slice(), f.op), (&b"x"[..], Op::Match(false)));
        assert!(f.regex.unwrap().is_match(b"xaxbx"));
        let f = spec("@x/~^a%2Fb%7Ec").unwrap();
        assert_eq!(f.op, Op::Match(true));
        assert!(f.regex.unwrap().is_match(b"a/b~c"));
        // ':' makes a column reference: no regex.
        let f = spec("@x~:y").unwrap();
        assert_eq!(f.value, Value::Column(b"y".to_vec()));
        assert!(f.regex.is_none());
        // A /xx/ token takes precedence over = and ~.
        assert_eq!(name(&spec("@a=b/gt/3").unwrap()), b"a=b");
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
    fn parsing_band() {
        let f = spec("@x/ep/1").unwrap();
        assert_eq!((f.op, f.const_num, f.band), (Op::Ep(false), Some(Num::I(1)), Some(DEFAULT_EPSILON)));
        let f = spec("@x/omx/:y").unwrap();
        assert_eq!((f.op, f.band), (Op::Om(true), Some(DEFAULT_OOM)));
        assert_eq!(spec("@x/ep/:y/0").unwrap().band, Some(Num::I(0)));
        assert_eq!(spec("@x/om/1/1.5").unwrap().band, Some(Num::F(1.5)));
        assert_eq!(spec("@x/om/1/inf").unwrap().band, Some(Num::F(f64::INFINITY)));
    }

    #[test]
    fn parsing_bits() {
        let f = spec("@flag/notall/12").unwrap();
        assert_eq!((f.op, f.const_num), (Op::Bit(BitTest::All, true), Some(Num::I(12))));
        assert_eq!(spec("@flag/none/0").unwrap().op, Op::Bit(BitTest::Any, true));
        assert_eq!(spec("@flag/any/:m").unwrap().value, Value::Column(b"m".to_vec()));
    }

    #[test]
    fn parsing_handles() {
        assert_eq!(spec("@a:+b\\d/gt/0").unwrap().handle.elements.len(), 2);
        assert!(matches!(spec("@2-4/gt/0").unwrap().handle.elements[0], Element::Name(_)));
        let f = parse_filter(b"@2-4/gt/0", true).unwrap();
        assert!(matches!(f.handle.elements[0], Element::Range(_)));
        assert!(parse_filter(b"@2/gt/:3-4", true).is_err());
        assert!(parse_filter(b"@2/gt/:3", true).is_ok());
    }

    #[test]
    fn parsing_predicates() {
        let f = spec("@x/ISINT").unwrap();
        assert_eq!((name(&f), f.op), (b"x".to_vec(), Op::Pred(Pred::Int, false)));
        assert_eq!(spec("@@+x.*/NOTNUM").unwrap().op, Op::Pred(Pred::Num, true));
        assert_eq!(name(&spec("@a%2Fb/ISNUM").unwrap()), b"a/b");
        // A lowercase operator comes first: /gt/ with the (non-numeric) constant ISINT.
        assert!(spec("@x/gt/ISINT").unwrap_err().contains("is not a number"));
        for bad in ["@x/ISFOO", "@x/isint", "@/ISINT", "@x/ISINT/3", "@x/", "@+/ISINT"] {
            assert!(spec(bad).is_err(), "{}", bad);
        }
    }

    #[test]
    fn predicates() {
        for (v, int, numeric) in [("5", true, true), ("-3", true, true), ("+7", true, true), ("007", true, true),
                                  ("5.0", false, true), ("1e3", false, true), ("inf", false, true),
                                  ("nan", false, true), ("", false, false), ("fox", false, false),
                                  (" 4", false, false), ("0x10", false, false)] {
            assert_eq!(Filter::predicate(v.as_bytes(), Pred::Int, false), int, "{}", v);
            assert_eq!(Filter::predicate(v.as_bytes(), Pred::Int, true), !int, "{}", v);
            assert_eq!(Filter::predicate(v.as_bytes(), Pred::Num, false), numeric, "{}", v);
            assert_eq!(Filter::predicate(v.as_bytes(), Pred::Num, true), !numeric, "{}", v);
        }
    }

    #[test]
    fn parsing_assertions() {
        let f = spec("@assert@x/gt/3").unwrap();
        assert_eq!((f.stage, f.assert, name(&f)), (Stage::Post, true, b"x".to_vec()));
        let f = spec("@@assert@+y.*/ISNUM").unwrap();
        assert_eq!((f.stage, f.assert, f.op), (Stage::Pre, true, Op::Pred(Pred::Num, false)));
        assert!(!spec("@@x/gt/3").unwrap().assert);
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
