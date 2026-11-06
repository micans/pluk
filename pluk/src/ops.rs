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
    Precision,
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

// Most operators produce a number; the precision operators text.
const NUM: Shape = Shape::Result(Kind::Num);
const STR: Shape = Shape::Result(Kind::Str);

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
    OpDef { name: "rand",   pops: Pops::N(1), pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Arithmetic,
            operands: "x", result: "rand(x)", doc: "Random real in range [0, x) (x 0: [0, 1); see --seed)", fun: op_rand },
    OpDef { name: "addall", pops: Pops::All,  pushes: 1, shape: Shape::Result(Kind::Num), prepare: Prepare::Nothing, category: Category::Arithmetic,
            operands: "*", result: "sum(Stack)", doc: "Sum of all entries in stack", fun: op_addall },
    OpDef { name: "mulall", pops: Pops::All,  pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Arithmetic,
            operands: "*", result: "product(Stack)", doc: "Product of all entries in stack", fun: op_mulall },
    OpDef { name: "maxall", pops: Pops::All,  pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Arithmetic,
            operands: "*", result: "max(Stack)", doc: "Max over all entries in stack", fun: op_maxall },
    OpDef { name: "minall", pops: Pops::All,  pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Arithmetic,
            operands: "*", result: "min(Stack)", doc: "Min over all entries in stack", fun: op_minall },
    OpDef { name: "meanall", pops: Pops::All, pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Arithmetic,
            operands: "*", result: "mean(Stack)", doc: "Mean of all entries in stack", fun: op_meanall },
    OpDef { name: "hmeanall", pops: Pops::All, pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Arithmetic,
            operands: "*", result: "hmean(Stack)", doc: "Harmonic mean of the absolute values in stack", fun: op_hmeanall },
    OpDef { name: "gmeanall", pops: Pops::All, pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Arithmetic,
            operands: "*", result: "gmean(Stack)", doc: "Geometric mean of the absolute values in stack", fun: op_gmeanall },
    OpDef { name: "dd",     pops: Pops::N(2), pushes: 1, shape: STR, prepare: Prepare::Nothing, category: Category::Precision,
            operands: "x N", result: "x'", doc: "x written with N decimal digits", fun: op_dd },
    OpDef { name: "sn",     pops: Pops::N(2), pushes: 1, shape: STR, prepare: Prepare::Nothing, category: Category::Precision,
            operands: "x N", result: "x'", doc: "x in scientific notation with N decimal digits", fun: op_sn },
    OpDef { name: "frac",   pops: Pops::N(3), pushes: 1, shape: STR, prepare: Prepare::Nothing, category: Category::Precision,
            operands: "x y N", result: "x/y", doc: "Fraction x/y with N decimal digits", fun: op_frac },
    OpDef { name: "pct",    pops: Pops::N(3), pushes: 1, shape: STR, prepare: Prepare::Nothing, category: Category::Precision,
            operands: "x y N", result: "pct(x/y)", doc: "Percentage of x relative to y with N decimal digits", fun: op_pct },
    OpDef { name: "pml",    pops: Pops::N(3), pushes: 1, shape: STR, prepare: Prepare::Nothing, category: Category::Precision,
            operands: "x y N", result: "pml(x/y)", doc: "Per mille of x relative to y with N decimal digits", fun: op_pml },
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

// A random float in [0, x), as Perl's rand: x = 0 counts as 1, a negative x gives
// (x, 0]. Seeded from the clock, or with --seed=N.
fn op_rand(m: &mut Machine, row: &Row, _: Option<&Regex>) {
    let x = m.pop();
    let result = match m.num(&x, row) {
        Some(a) => {
            let limit = match a.to_f64() { 0.0 => 1.0, v => v };
            Value::Num(Num::F(limit * m.random()))
        }
        None => Value::Na,
    };
    m.push(result);
}

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

// Whole-stack operators (plan/step12.txt decision 9). whole() takes the stack,
// gets the numbers, empties the stack (keeping its capacity) and pushes f's
// result: NA if a value is not a number or if f has no result (an empty stack for
// the means, maxall and minall).
fn whole(m: &mut Machine, row: &Row, f: impl Fn(&[Num]) -> Option<Num>) {
    let mut values = std::mem::take(&mut m.stack);
    let mut nums = Vec::with_capacity(values.len());
    let mut na = false;
    for v in &values {
        match m.num(v, row) {
            Some(n) => nums.push(n),
            None => na = true,
        }
    }
    values.clear();
    m.stack = values;
    m.push(if na { Value::Na } else { f(&nums).map_or(Value::Na, Value::Num) });
}

// The sum and product of the whole stack: 0 and 1 for an empty stack (pick).
fn op_addall(m: &mut Machine, row: &Row, _: Option<&Regex>) {
    whole(m, row, |xs| Some(xs.iter().fold(Num::I(0), |a, &b| num::add(a, b))))
}

fn op_mulall(m: &mut Machine, row: &Row, _: Option<&Regex>) {
    whole(m, row, |xs| Some(xs.iter().fold(Num::I(1), |a, &b| num::mul(a, b))))
}

fn op_meanall(m: &mut Machine, row: &Row, _: Option<&Regex>) {
    whole(m, row, |xs| {
        let sum = xs.iter().fold(Num::I(0), |a, &b| num::add(a, b));
        (!xs.is_empty()).then(|| num::div(sum, Num::I(xs.len() as num::Int)))
    })
}

// As pick: n / sum(1/|x|), and (product |x|)^(1/n). A zero makes the harmonic
// mean 0 (IEEE: n / inf; pick dies).
fn op_hmeanall(m: &mut Machine, row: &Row, _: Option<&Regex>) {
    whole(m, row, |xs| {
        let inv: f64 = xs.iter().map(|x| 1.0 / x.to_f64().abs()).sum();
        (!xs.is_empty()).then(|| Num::F(xs.len() as f64 / inv))
    })
}

// The geometric mean as the exponent of the mean logarithm, which cannot overflow
// on long stacks (pick multiplies first; the last digits can differ). A zero gives
// ln 0 = -inf, so the mean is 0.
fn op_gmeanall(m: &mut Machine, row: &Row, _: Option<&Regex>) {
    whole(m, row, |xs| {
        let logs: f64 = xs.iter().map(|x| x.to_f64().abs().ln()).sum();
        (!xs.is_empty()).then(|| Num::F((logs / xs.len() as f64).exp()))
    })
}

// Precision operators (pick's sprintf %.*f and %.*e): the value of 'operands'
// numbers, written with N digits; the result is text. N is the top operand: a
// non-negative integer up to MAX_DIGITS, else NA (as a non-number operand).
const MAX_DIGITS: num::Int = 1000;

fn precision(m: &mut Machine, row: &Row, operands: usize, sci: bool, value: impl Fn(&[f64]) -> f64) {
    let n = m.pop();
    let mut xs: Vec<Value> = (0..operands).map(|_| m.pop()).collect();
    xs.reverse();
    let digits = match m.num(&n, row) {
        Some(Num::I(d)) if (0..=MAX_DIGITS).contains(&d) => Some(d as usize),
        _ => None,
    };
    let nums: Option<Vec<f64>> = xs.iter().map(|x| m.num(x, row).map(Num::to_f64)).collect();
    let result = match (digits, nums) {
        (Some(d), Some(v)) => {
            let mut text = Vec::new();
            num::write_fixed(&mut text, value(&v), d, sci, m.perl());
            Value::Bytes(text)
        }
        _ => Value::Na,
    };
    m.push(result);
}

fn op_dd(m: &mut Machine, row: &Row, _: Option<&Regex>) { precision(m, row, 1, false, |v| v[0]) }
fn op_sn(m: &mut Machine, row: &Row, _: Option<&Regex>) { precision(m, row, 1, true, |v| v[0]) }
// Computed as pick: x*100/y and x*1000/y (division by zero follows IEEE).
fn op_frac(m: &mut Machine, row: &Row, _: Option<&Regex>) { precision(m, row, 2, false, |v| v[0] / v[1]) }
fn op_pct(m: &mut Machine, row: &Row, _: Option<&Regex>) { precision(m, row, 2, false, |v| v[0] * 100.0 / v[1]) }
fn op_pml(m: &mut Machine, row: &Row, _: Option<&Regex>) { precision(m, row, 2, false, |v| v[0] * 1000.0 / v[1]) }

// maxall and minall push the winning operand itself, as max and min: the first
// value, replaced by a later one that is greater (smaller); NaN never wins (as
// pick, which starts from -DBL_MAX); all NaN gives NaN. NA if a value is not a
// number; an empty stack is NA.
fn op_maxall(m: &mut Machine, row: &Row, _: Option<&Regex>) { choose_all(m, row, true) }
fn op_minall(m: &mut Machine, row: &Row, _: Option<&Regex>) { choose_all(m, row, false) }

fn choose_all(m: &mut Machine, row: &Row, max: bool) {
    let mut values = std::mem::take(&mut m.stack);
    let mut best: Option<(usize, Num)> = None;
    let mut na = values.is_empty();
    for (k, v) in values.iter().enumerate() {
        match m.num(v, row) {
            None => na = true,
            Some(n) => match best {
                None => best = Some((k, n)),
                Some((_, b)) if b.is_nan() && !n.is_nan() => best = Some((k, n)),
                Some((_, b)) if num::replaces(b, n, max) => best = Some((k, n)),
                _ => (),
            },
        }
    }
    let result = match (na, best) {
        (false, Some((k, _))) => values.swap_remove(k),
        _ => Value::Na,
    };
    values.clear();
    m.stack = values;
    m.push(result);
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
