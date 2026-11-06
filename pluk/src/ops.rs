// The compute operators (plan/COMPUTE.design.txt section 5).
//
// One static table entry per operator. Operator families are added step by step;
// step10 has a seed set that exercises every path of the machinery. An entry has
// the stack effect and description (parsing), the shape and prepare step
// (compilation) and the implementation (execution, exec.rs).

use regex::bytes::Regex;

use crate::exec::{Machine, OpFn, Value};
use crate::num::{self, Num};
use crate::row::Row;

// How many values an operator takes from the stack.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Pops {
    N(usize),       // exactly N; fewer is a stack underflow (a compile-time error)
    All,            // the whole stack, possibly empty (addall, joinall, ...)
}

// How an operator changes the static stack (compute.rs keeps track of what is on
// the stack at compile time): most push one result of a known kind; the stack
// operators rearrange.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Shape {
    Result(Kind),
    Dup,
    Xch,
    Pop,
}

// What an operator result is known to be.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Str,
    Num,
}

// Work done at compile time on a constant operand (COMPUTE.design.txt section 5).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Prepare {
    Nothing,
    ConstRegex,     // the top operand must be a constant; it is compiled as a regex
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Category {
    Stack,
    String,
    Arithmetic,
    Math,
    Regex,
}

#[derive(Debug)]
pub struct OpDef {
    pub name:     &'static str,
    pub pops:     Pops,
    pub pushes:   usize,
    pub shape:    Shape,
    pub prepare:  Prepare,
    pub category: Category,
    pub operands: &'static str,     // as pick's help: the operands, top of stack last
    pub result:   &'static str,     // and what is pushed
    pub doc:      &'static str,
    pub fun:      OpFn,
}

// Most operators produce a number.
const NUM: Shape = Shape::Result(Kind::Num);

// The table; descriptions follow pick's.
pub static OPERATORS: &[OpDef] = &[
    OpDef { name: "dup",    pops: Pops::N(1), pushes: 2, shape: Shape::Dup, prepare: Prepare::Nothing, category: Category::Stack,
            operands: "x", result: "x x", doc: "Duplicate top entry x", fun: op_dup },
    OpDef { name: "pop",    pops: Pops::N(1), pushes: 0, shape: Shape::Pop, prepare: Prepare::Nothing, category: Category::Stack,
            operands: "x", result: "-", doc: "Remove top entry x from stack", fun: op_pop },
    OpDef { name: "xch",    pops: Pops::N(2), pushes: 2, shape: Shape::Xch, prepare: Prepare::Nothing, category: Category::Stack,
            operands: "x y", result: "y x", doc: "Exchange x and y", fun: op_xch },
    OpDef { name: "cat",    pops: Pops::N(2), pushes: 1, shape: Shape::Result(Kind::Str), prepare: Prepare::Nothing, category: Category::String,
            operands: "x y", result: "xy", doc: "Concatenation of x and y", fun: op_cat },
    OpDef { name: "add",    pops: Pops::N(2), pushes: 1, shape: Shape::Result(Kind::Num), prepare: Prepare::Nothing, category: Category::Arithmetic,
            operands: "x y", result: "x+y", doc: "Add x and y", fun: op_add },
    OpDef { name: "sub",    pops: Pops::N(2), pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Arithmetic,
            operands: "x y", result: "x-y", doc: "Subtract y from x", fun: op_sub },
    OpDef { name: "mul",    pops: Pops::N(2), pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Arithmetic,
            operands: "x y", result: "x*y", doc: "Multiply x and y", fun: op_mul },
    OpDef { name: "div",    pops: Pops::N(2), pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Arithmetic,
            operands: "x y", result: "x/y", doc: "Division (a float; by zero: inf or nan)", fun: op_div },
    OpDef { name: "idiv",   pops: Pops::N(2), pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Arithmetic,
            operands: "x y", result: "x // y", doc: "Integer division, floored (by zero: NA)", fun: op_idiv },
    OpDef { name: "mod",    pops: Pops::N(2), pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Arithmetic,
            operands: "x y", result: "x mod y", doc: "x modulo y, sign of y (by zero: NA)", fun: op_mod },
    OpDef { name: "pow",    pops: Pops::N(2), pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Arithmetic,
            operands: "x y", result: "x**y", doc: "x raised to power y", fun: op_pow },
    OpDef { name: "ipow",   pops: Pops::N(2), pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Arithmetic,
            operands: "x y", result: "x**y", doc: "x raised to power y in integers (negative y as pow; overflow: NA)", fun: op_ipow },
    OpDef { name: "max",    pops: Pops::N(2), pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Arithmetic,
            operands: "x y", result: "max(x,y)", doc: "Maximum of x and y", fun: op_max },
    OpDef { name: "min",    pops: Pops::N(2), pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Arithmetic,
            operands: "x y", result: "min(x,y)", doc: "Minimum of x and y", fun: op_min },
    OpDef { name: "neg",    pops: Pops::N(1), pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Arithmetic,
            operands: "x", result: "-x", doc: "The sign-reversed value of x", fun: op_neg },
    OpDef { name: "abs",    pops: Pops::N(1), pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Arithmetic,
            operands: "x", result: "abs(x)", doc: "Absolute value of x", fun: op_abs },
    OpDef { name: "sign",   pops: Pops::N(1), pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Arithmetic,
            operands: "x", result: "sign(x)", doc: "The sign of x (-1, 0 or 1)", fun: op_sign },
    OpDef { name: "sq",     pops: Pops::N(1), pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Arithmetic,
            operands: "x", result: "x^2", doc: "Square of x", fun: op_sq },
    OpDef { name: "incr",   pops: Pops::N(1), pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Arithmetic,
            operands: "x", result: "x++", doc: "x incremented by one", fun: op_incr },
    OpDef { name: "decr",   pops: Pops::N(1), pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Arithmetic,
            operands: "x", result: "x--", doc: "x decremented by one", fun: op_decr },
    OpDef { name: "int",    pops: Pops::N(1), pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Math,
            operands: "x", result: "int(x)", doc: "x truncated towards zero (do not use for rounding)", fun: op_int },
    OpDef { name: "floor",  pops: Pops::N(1), pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Math,
            operands: "x", result: "floor(x)", doc: "The floor of x", fun: op_floor },
    OpDef { name: "ceil",   pops: Pops::N(1), pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Math,
            operands: "x", result: "ceil(x)", doc: "The ceil of x", fun: op_ceil },
    OpDef { name: "round",  pops: Pops::N(1), pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Math,
            operands: "x", result: "round(x)", doc: "The integer nearest to x, halves to even (2.5 -> 2, 3.5 -> 4)", fun: op_round },
    OpDef { name: "sqrt",   pops: Pops::N(1), pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Math,
            operands: "x", result: "sqrt(x)", doc: "Square root of x", fun: op_sqrt },
    OpDef { name: "exp",    pops: Pops::N(1), pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Math,
            operands: "x", result: "e**x", doc: "Exponential function applied to x", fun: op_exp },
    OpDef { name: "exp2",   pops: Pops::N(1), pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Math,
            operands: "x", result: "2^x", doc: "2 to the power of x", fun: op_exp2 },
    OpDef { name: "exp10",  pops: Pops::N(1), pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Math,
            operands: "x", result: "10^x", doc: "10 to the power of x", fun: op_exp10 },
    OpDef { name: "log",    pops: Pops::N(1), pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Math,
            operands: "x", result: "log(x)", doc: "Natural logarithm of x", fun: op_log },
    OpDef { name: "log2",   pops: Pops::N(1), pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Math,
            operands: "x", result: "log2(x)", doc: "Logarithm of x in base 2", fun: op_log2 },
    OpDef { name: "log10",  pops: Pops::N(1), pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Math,
            operands: "x", result: "log10(x)", doc: "Logarithm of x in base 10", fun: op_log10 },
    OpDef { name: "sin",    pops: Pops::N(1), pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Math,
            operands: "x", result: "sin(x)", doc: "Sine of x", fun: op_sin },
    OpDef { name: "cos",    pops: Pops::N(1), pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Math,
            operands: "x", result: "cos(x)", doc: "Cosine of x", fun: op_cos },
    OpDef { name: "tan",    pops: Pops::N(1), pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Math,
            operands: "x", result: "tan(x)", doc: "Tangent of x", fun: op_tan },
    OpDef { name: "addall", pops: Pops::All,  pushes: 1, shape: Shape::Result(Kind::Num), prepare: Prepare::Nothing, category: Category::Arithmetic,
            operands: "*", result: "sum(Stack)", doc: "Sum of all entries in stack", fun: op_addall },
    OpDef { name: "del",    pops: Pops::N(2), pushes: 1, shape: Shape::Result(Kind::Str), prepare: Prepare::ConstRegex, category: Category::Regex,
            operands: "x p", result: "x =~ s/p//", doc: "Delete pattern p in x", fun: op_del },
];

pub fn lookup(name: &[u8]) -> Option<&'static OpDef> {
    OPERATORS.iter().find(|op| op.name.as_bytes() == name)
}


// Implementations. Operands are popped top first: for 'x y op', y comes first.
// A value that is not a number makes a numeric result NA (NUMERIC.design.txt).

fn op_dup(m: &mut Machine, _: &Row, _: Option<&Regex>) {
    let top = m.stack.last().expect("stack checked at compile time").clone();
    m.push(top);
}

fn op_pop(m: &mut Machine, _: &Row, _: Option<&Regex>) {
    m.pop();
}

fn op_xch(m: &mut Machine, _: &Row, _: Option<&Regex>) {
    let n = m.stack.len();
    m.stack.swap(n - 1, n - 2);
}

fn op_cat(m: &mut Machine, row: &Row, _: Option<&Regex>) {
    let y = m.pop();
    let x = m.pop();
    let mut s = m.into_bytes(x, row);
    s.extend_from_slice(&m.bytes(&y, row));
    m.push(Value::Bytes(s));
}

// Numeric operators: pop the operands (y first), push f's result; NA if an
// operand is not a number or f has no result (NUMERIC.design.txt section 9).
fn numeric2(m: &mut Machine, row: &Row, f: impl Fn(Num, Num) -> Option<Num>) {
    let y = m.pop();
    let x = m.pop();
    let result = match (m.num(&x, row), m.num(&y, row)) {
        (Some(a), Some(b)) => f(a, b).map_or(Value::Na, Value::Num),
        _ => Value::Na,
    };
    m.push(result);
}

fn numeric1(m: &mut Machine, row: &Row, f: impl Fn(Num) -> Num) {
    let x = m.pop();
    let result = m.num(&x, row).map_or(Value::Na, |a| Value::Num(f(a)));
    m.push(result);
}

fn op_add(m: &mut Machine, row: &Row, _: Option<&Regex>) { numeric2(m, row, |a, b| Some(num::add(a, b))) }
fn op_sub(m: &mut Machine, row: &Row, _: Option<&Regex>) { numeric2(m, row, |a, b| Some(num::sub(a, b))) }
fn op_mul(m: &mut Machine, row: &Row, _: Option<&Regex>) { numeric2(m, row, |a, b| Some(num::mul(a, b))) }
fn op_div(m: &mut Machine, row: &Row, _: Option<&Regex>) { numeric2(m, row, |a, b| Some(num::div(a, b))) }
fn op_idiv(m: &mut Machine, row: &Row, _: Option<&Regex>) { numeric2(m, row, num::idiv) }
fn op_mod(m: &mut Machine, row: &Row, _: Option<&Regex>) { numeric2(m, row, num::imod) }
fn op_pow(m: &mut Machine, row: &Row, _: Option<&Regex>) { numeric2(m, row, |a, b| Some(num::pow(a, b))) }
fn op_ipow(m: &mut Machine, row: &Row, _: Option<&Regex>) { numeric2(m, row, num::ipow) }
fn op_max(m: &mut Machine, row: &Row, _: Option<&Regex>) { choose(m, row, true) }
fn op_min(m: &mut Machine, row: &Row, _: Option<&Regex>) { choose(m, row, false) }

// max and min push the winning operand itself, not its numeric value (as pick:
// 1.000000 stays 1.000000); NA if an operand is not a number.
fn choose(m: &mut Machine, row: &Row, max: bool) {
    let y = m.pop();
    let x = m.pop();
    let result = match (m.num(&x, row), m.num(&y, row)) {
        (Some(a), Some(b)) => if num::replaces(a, b, max) { y } else { x },
        _ => Value::Na,
    };
    m.push(result);
}
fn op_neg(m: &mut Machine, row: &Row, _: Option<&Regex>) { numeric1(m, row, num::neg) }
fn op_abs(m: &mut Machine, row: &Row, _: Option<&Regex>) { numeric1(m, row, num::abs) }
fn op_sign(m: &mut Machine, row: &Row, _: Option<&Regex>) { numeric1(m, row, num::sign) }
fn op_sq(m: &mut Machine, row: &Row, _: Option<&Regex>) { numeric1(m, row, num::sq) }
fn op_incr(m: &mut Machine, row: &Row, _: Option<&Regex>) { numeric1(m, row, num::incr) }
fn op_decr(m: &mut Machine, row: &Row, _: Option<&Regex>) { numeric1(m, row, num::decr) }

fn op_int(m: &mut Machine, row: &Row, _: Option<&Regex>) { numeric1(m, row, |a| num::round_with(a, f64::trunc)) }
fn op_floor(m: &mut Machine, row: &Row, _: Option<&Regex>) { numeric1(m, row, |a| num::round_with(a, f64::floor)) }
fn op_ceil(m: &mut Machine, row: &Row, _: Option<&Regex>) { numeric1(m, row, |a| num::round_with(a, f64::ceil)) }
fn op_round(m: &mut Machine, row: &Row, _: Option<&Regex>) { numeric1(m, row, |a| num::round_with(a, f64::round_ties_even)) }

// Functions on floats: the operand as f64 (an integer is promoted), the result a
// float; domain errors follow IEEE (log(0) is -inf, sqrt(-1) nan; step12.txt
// decision 6).
fn float1(m: &mut Machine, row: &Row, f: fn(f64) -> f64) {
    numeric1(m, row, |a| Num::F(f(a.to_f64())))
}

fn op_sqrt(m: &mut Machine, row: &Row, _: Option<&Regex>) { float1(m, row, f64::sqrt) }
fn op_exp(m: &mut Machine, row: &Row, _: Option<&Regex>) { float1(m, row, f64::exp) }
fn op_exp2(m: &mut Machine, row: &Row, _: Option<&Regex>) { float1(m, row, f64::exp2) }
fn op_exp10(m: &mut Machine, row: &Row, _: Option<&Regex>) { float1(m, row, |x| 10f64.powf(x)) }
fn op_log(m: &mut Machine, row: &Row, _: Option<&Regex>) { float1(m, row, f64::ln) }
fn op_log2(m: &mut Machine, row: &Row, _: Option<&Regex>) { float1(m, row, f64::log2) }
fn op_log10(m: &mut Machine, row: &Row, _: Option<&Regex>) { float1(m, row, f64::log10) }
fn op_sin(m: &mut Machine, row: &Row, _: Option<&Regex>) { float1(m, row, f64::sin) }
fn op_cos(m: &mut Machine, row: &Row, _: Option<&Regex>) { float1(m, row, f64::cos) }
fn op_tan(m: &mut Machine, row: &Row, _: Option<&Regex>) { float1(m, row, f64::tan) }

// The sum of the whole stack; an empty stack sums to 0, as pick.
fn op_addall(m: &mut Machine, row: &Row, _: Option<&Regex>) {
    let mut values = std::mem::take(&mut m.stack);
    let mut sum = Some(Num::I(0));
    for v in values.drain(..) {
        sum = match (sum, m.num(&v, row)) {
            (Some(a), Some(b)) => Some(num::add(a, b)),
            _ => None,
        };
    }
    m.stack = values;           // empty again, with its capacity
    m.push(sum.map_or(Value::Na, Value::Num));
}

// Delete the first match of the pattern (compiled at compile time; the pattern
// operand on the stack is the constant it came from).
fn op_del(m: &mut Machine, row: &Row, re: Option<&Regex>) {
    let re = re.expect("del has a compiled pattern");
    m.pop();
    let x = m.pop();
    let mut s = m.into_bytes(x, row);
    if let Some(found) = re.find(&s) {
        s.drain(found.range());
    }
    m.push(Value::Bytes(s));
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table() {
        assert_eq!(lookup(b"add").map(|op| op.pops), Some(Pops::N(2)));
        assert_eq!(lookup(b"addall").map(|op| op.pops), Some(Pops::All));
        assert!(lookup(b"nosuch").is_none());
        // Names are unique.
        for (i, a) in OPERATORS.iter().enumerate() {
            assert!(OPERATORS[i + 1..].iter().all(|b| b.name != a.name), "{}", a.name);
        }
        // The shape agrees with the stack effect.
        for op in OPERATORS {
            let ok = match op.shape {
                Shape::Result(_) => op.pushes == 1,
                Shape::Dup => op.pops == Pops::N(1) && op.pushes == 2,
                Shape::Xch => op.pops == Pops::N(2) && op.pushes == 2,
                Shape::Pop => op.pops == Pops::N(1) && op.pushes == 0,
            };
            assert!(ok, "{}", op.name);
        }
    }
}
