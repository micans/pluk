// The compute operators (plan/COMPUTE.design.txt section 5).
//
// One static table entry per operator. Operator families are added step by step;
// step10 has a seed set that exercises every path of the machinery. An entry has
// the stack effect and description (parsing), the shape and prepare step
// (compilation) and the implementation (execution, exec.rs).

use md5::{Digest, Md5};
use memchr::memmem;
use regex::bytes::{Captures, NoExpand, Regex};

use std::cmp::Ordering;

use crate::compute::{Attached, TestRhs};
use crate::filter::{self, Outcome, Rhs};
use crate::num::compare;
use crate::dict::{Dict, NotFound};
use crate::exec::{Machine, OpFn, Value};
use crate::num::{self, Num};
use crate::opt::pct_decode;
use crate::row::Row;

// How many values an operator takes from the stack.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Pops {
    N(usize),       // exactly N; fewer is a stack underflow (a compile-time error)
    All(usize),     // the whole stack, which must hold at least this many values
                    // (addall: 0; joinall: 1, the separator)
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
    ConstRegex(usize), // the operand this many places below the top must be a constant;
                       // it is compiled as a regex (del: 0, ed: 1)
    ConstDict(usize),  // the operand this many places below the top must be a constant
                       // naming a dictionary (map, dord: 0)
    Test,              // test: the filter operator and its right-hand side (compute.rs)
    FormatCount,    // a constant bottom operand is a fmtall format: its # count must
                    // match the number of values above it
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Category {
    Stack,
    String,
    Arithmetic,
    Math,
    Precision,
    Dictionary,
    Branch,
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
    OpDef { name: "len",    pops: Pops::N(1), pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::String,
            operands: "x", result: "len(x)", doc: "Length of string x (bytes)", fun: op_len },
    OpDef { name: "lc",     pops: Pops::N(1), pushes: 1, shape: STR, prepare: Prepare::Nothing, category: Category::String,
            operands: "x", result: "lc(x)", doc: "Lower case of x (ASCII)", fun: op_lc },
    OpDef { name: "uc",     pops: Pops::N(1), pushes: 1, shape: STR, prepare: Prepare::Nothing, category: Category::String,
            operands: "x", result: "uc(x)", doc: "Upper case of x (ASCII)", fun: op_uc },
    OpDef { name: "rev",    pops: Pops::N(1), pushes: 1, shape: STR, prepare: Prepare::Nothing, category: Category::String,
            operands: "x", result: "rev(x)", doc: "String reverse of x (bytes)", fun: op_rev },
    OpDef { name: "rot13",  pops: Pops::N(1), pushes: 1, shape: STR, prepare: Prepare::Nothing, category: Category::String,
            operands: "x", result: "rot13(x)", doc: "Rot13 encoding of x", fun: op_rot13 },
    OpDef { name: "substr", pops: Pops::N(3), pushes: 1, shape: STR, prepare: Prepare::Nothing, category: Category::String,
            operands: "x i k", result: "x[i:i+k-1]", doc: "Substring of x starting at i (zero-based) of length k (as Perl)", fun: op_substr },
    OpDef { name: "head",   pops: Pops::N(2), pushes: 1, shape: STR, prepare: Prepare::Nothing, category: Category::String,
            operands: "x k", result: "x[0:k-1]", doc: "Initial part of string x, of length k [head,tail,skip,shed]", fun: op_head },
    OpDef { name: "tail",   pops: Pops::N(2), pushes: 1, shape: STR, prepare: Prepare::Nothing, category: Category::String,
            operands: "x k", result: "x[#x-k:#x-1]", doc: "Trailing part of string x, of length k [head,tail,skip,shed]", fun: op_tail },
    OpDef { name: "skip",   pops: Pops::N(2), pushes: 1, shape: STR, prepare: Prepare::Nothing, category: Category::String,
            operands: "x k", result: "x[k:#x-1]", doc: "Trailing part of string x, skipping first k [head,tail,skip,shed]", fun: op_skip },
    OpDef { name: "shed",   pops: Pops::N(2), pushes: 1, shape: STR, prepare: Prepare::Nothing, category: Category::String,
            operands: "x k", result: "x[0:#x-k-1]", doc: "Leading part of string x, shedding last k [head,tail,skip,shed]", fun: op_shed },
    OpDef { name: "strim",  pops: Pops::N(3), pushes: 1, shape: STR, prepare: Prepare::Nothing, category: Category::String,
            operands: "s nx ny", result: "s'", doc: "Trim nx bytes from the start and ny from the end of s", fun: op_strim },
    OpDef { name: "ppl",    pops: Pops::N(2), pushes: 1, shape: STR, prepare: Prepare::Nothing, category: Category::String,
            operands: "x N", result: "x'", doc: "x right-padded with blanks to width N (left-aligned)", fun: op_ppl },
    OpDef { name: "ppr",    pops: Pops::N(2), pushes: 1, shape: STR, prepare: Prepare::Nothing, category: Category::String,
            operands: "x N", result: "x'", doc: "x left-padded with blanks to width N (right-aligned)", fun: op_ppr },
    OpDef { name: "zp",     pops: Pops::N(2), pushes: 1, shape: STR, prepare: Prepare::Nothing, category: Category::String,
            operands: "x N", result: "x'", doc: "x left zero-padded to width N", fun: op_zp },
    OpDef { name: "nchar",  pops: Pops::N(2), pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::String,
            operands: "x c", result: "nc", doc: "Number of occurrences of c in x (non-overlapping)", fun: op_nchar },
    OpDef { name: "catall", pops: Pops::All(0), pushes: 1, shape: STR, prepare: Prepare::Nothing, category: Category::String,
            operands: "*", result: "Stack-joined", doc: "Stringified stack", fun: op_catall },
    OpDef { name: "joinall", pops: Pops::All(1), pushes: 1, shape: STR, prepare: Prepare::Nothing, category: Category::String,
            operands: "* s", result: "Stack-joined-by-s", doc: "Stringified stack with s as separator", fun: op_joinall },
    OpDef { name: "join",   pops: Pops::N(3), pushes: 1, shape: STR, prepare: Prepare::Nothing, category: Category::String,
            operands: "x y j", result: "xjy", doc: "Join two elements with string separator", fun: op_join },
    OpDef { name: "uie",    pops: Pops::N(2), pushes: 1, shape: STR, prepare: Prepare::Nothing, category: Category::String,
            operands: "x y", result: "x-or-y", doc: "Use x if not empty (and not NA), otherwise use y", fun: op_uie },
    OpDef { name: "urlec",  pops: Pops::N(1), pushes: 1, shape: STR, prepare: Prepare::Nothing, category: Category::String,
            operands: "x", result: "urlec(x)", doc: "URL encoding of x (all but A-Z a-z 0-9 _ as %XX)", fun: op_urlec },
    OpDef { name: "urldc",  pops: Pops::N(1), pushes: 1, shape: STR, prepare: Prepare::Nothing, category: Category::String,
            operands: "x", result: "urldc(x)", doc: "URL decoding of x (%XX)", fun: op_urldc },
    OpDef { name: "md5",    pops: Pops::N(1), pushes: 1, shape: STR, prepare: Prepare::Nothing, category: Category::String,
            operands: "x", result: "md5(x)", doc: "MD5 sum of x (hexadecimal)", fun: op_md5 },
    OpDef { name: "fmtall", pops: Pops::All(1), pushes: 1, shape: STR, prepare: Prepare::FormatCount, category: Category::String,
            operands: "f *", result: "Map-*-into-f", doc: "In f replace # instances by stack (\\# is a literal #)", fun: op_fmtall },
    OpDef { name: "ifelse", pops: Pops::N(3), pushes: 1, shape: STR, prepare: Prepare::Nothing, category: Category::Branch,
            operands: "B x y", result: "x or y", doc: "If B put x else put y (B: a non-zero number, or a non-empty string)", fun: op_ifelse },
    OpDef { name: "test",   pops: Pops::N(3), pushes: 1, shape: NUM, prepare: Prepare::Test, category: Category::Branch,
            operands: "x y T", result: "B", doc: "Test T applied to x y: 1 or 0 (T a filter operator: /gt/ ~eq~ ~ ~isin~ ...)", fun: op_test },
    OpDef { name: "map",    pops: Pops::N(2), pushes: 1, shape: STR, prepare: Prepare::ConstDict(0), category: Category::Dictionary,
            operands: "x D", result: "map-of-x", doc: "Value of x in dictionary D (not found: as D's default)", fun: op_map },
    OpDef { name: "dord",   pops: Pops::N(2), pushes: 1, shape: NUM, prepare: Prepare::ConstDict(0), category: Category::Dictionary,
            operands: "x D", result: "ord-of-x", doc: "Order of x in dictionary D (first appearance, 1-based; NA if absent)", fun: op_dord },
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
    OpDef { name: "addall", pops: Pops::All(0),pushes: 1, shape: Shape::Result(Kind::Num), prepare: Prepare::Nothing, category: Category::Arithmetic,
            operands: "*", result: "sum(Stack)", doc: "Sum of all entries in stack", fun: op_addall },
    OpDef { name: "mulall", pops: Pops::All(0),pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Arithmetic,
            operands: "*", result: "product(Stack)", doc: "Product of all entries in stack", fun: op_mulall },
    OpDef { name: "maxall", pops: Pops::All(0),pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Arithmetic,
            operands: "*", result: "max(Stack)", doc: "Max over all entries in stack", fun: op_maxall },
    OpDef { name: "minall", pops: Pops::All(0),pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Arithmetic,
            operands: "*", result: "min(Stack)", doc: "Min over all entries in stack", fun: op_minall },
    OpDef { name: "meanall", pops: Pops::All(0),pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Arithmetic,
            operands: "*", result: "mean(Stack)", doc: "Mean of all entries in stack", fun: op_meanall },
    OpDef { name: "hmeanall", pops: Pops::All(0),pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Arithmetic,
            operands: "*", result: "hmean(Stack)", doc: "Harmonic mean of the absolute values in stack", fun: op_hmeanall },
    OpDef { name: "gmeanall", pops: Pops::All(0),pushes: 1, shape: NUM, prepare: Prepare::Nothing, category: Category::Arithmetic,
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
    OpDef { name: "del",    pops: Pops::N(2), pushes: 1, shape: Shape::Result(Kind::Str), prepare: Prepare::ConstRegex(0), category: Category::Regex,
            operands: "x p", result: "x =~ s/p//", doc: "Delete pattern p in x", fun: op_del },
    OpDef { name: "delg",   pops: Pops::N(2), pushes: 1, shape: STR, prepare: Prepare::ConstRegex(0), category: Category::Regex,
            operands: "x p", result: "x =~ s/p//g", doc: "Globally delete pattern p in x", fun: op_delg },
    OpDef { name: "ed",     pops: Pops::N(3), pushes: 1, shape: STR, prepare: Prepare::ConstRegex(1), category: Category::Regex,
            operands: "x p s", result: "x =~ s/p/s/", doc: "Substitute pattern p by s in x (s literal)", fun: op_ed },
    OpDef { name: "edg",    pops: Pops::N(3), pushes: 1, shape: STR, prepare: Prepare::ConstRegex(1), category: Category::Regex,
            operands: "x p s", result: "x =~ s/p/s/g", doc: "Globally substitute pattern p by s in x (s literal)", fun: op_edg },
    OpDef { name: "get",    pops: Pops::N(2), pushes: 1, shape: STR, prepare: Prepare::ConstRegex(0), category: Category::Regex,
            operands: "x p", result: "p-match-of-x", doc: "Group 1 of the match of p in x, or the entire match; empty if no match", fun: op_get },
    OpDef { name: "npat",   pops: Pops::N(2), pushes: 1, shape: NUM, prepare: Prepare::ConstRegex(0), category: Category::Regex,
            operands: "x p", result: "np", doc: "Number of (non-overlapping) matches of p in x", fun: op_npat },
    OpDef { name: "edbr",   pops: Pops::N(3), pushes: 1, shape: STR, prepare: Prepare::ConstRegex(1), category: Category::Regex,
            operands: "x p s", result: "x =~ s/p/s/", doc: "As ed, $1 $2 .. (groups) and $0 (match) in s", fun: op_edbr },
    OpDef { name: "edgbr",  pops: Pops::N(3), pushes: 1, shape: STR, prepare: Prepare::ConstRegex(1), category: Category::Regex,
            operands: "x p s", result: "x =~ s/p/s/g", doc: "As edg, $1 $2 .. (groups) and $0 (match) in s", fun: op_edgbr },
];

pub fn lookup(name: &[u8]) -> Option<&'static OpDef> {
    OPERATORS.iter().find(|op| op.name.as_bytes() == name)
}


// Implementations. Operands are popped top first: for 'x y op', y comes first.
// A value that is not a number makes a numeric result NA (NUMERIC.design.txt).

fn op_dup(m: &mut Machine, _: &Row, _: &Attached) {
    let top = m.stack.last().expect("stack checked at compile time").clone();
    m.push(top);
}

fn op_pop(m: &mut Machine, _: &Row, _: &Attached) {
    m.pop();
}

fn op_xch(m: &mut Machine, _: &Row, _: &Attached) {
    let n = m.stack.len();
    m.stack.swap(n - 1, n - 2);
}

fn op_cat(m: &mut Machine, row: &Row, _: &Attached) {
    let y = m.pop();
    let x = m.pop();
    let mut s = m.into_bytes(x, row);
    s.extend_from_slice(&m.bytes(&y, row));
    m.push(Value::Bytes(s));
}

// String operators (plan/step13.txt). Strings are bytes (decision 1); a number is
// written as text first (Machine::into_bytes). An operand that is NA gives NA, as
// does a count that is not an integer.

// The integer value of a count operand, None if it is not an integer.
fn int_operand(m: &mut Machine, v: &Value, row: &Row) -> Option<num::Int> {
    match m.num(v, row) {
        Some(Num::I(i)) => Some(i),
        _ => None,
    }
}

// Pop x; push f(x) as text, or NA.
fn string1(m: &mut Machine, row: &Row, f: impl Fn(&mut Vec<u8>)) {
    let x = m.pop();
    if matches!(x, Value::Na) {
        return m.push(Value::Na);
    }
    let mut s = m.into_bytes(x, row);
    f(&mut s);
    m.push(Value::Bytes(s));
}

// Pop k and x; push x[range] where range is computed from the length and k, or NA.
fn slice2(m: &mut Machine, row: &Row, range: impl Fn(usize, num::Int) -> (usize, usize)) {
    let k = m.pop();
    let x = m.pop();
    let k = int_operand(m, &k, row);
    let result = match (x, k) {
        (Value::Na, _) | (_, None) => Value::Na,
        (x, Some(k)) => {
            let mut s = m.into_bytes(x, row);
            let (start, end) = range(s.len(), k);
            s.truncate(end);
            s.drain(..start);
            Value::Bytes(s)
        }
    };
    m.push(result);
}

// Padding (decision 4): pop the width N and x; pad x with fill bytes to N bytes,
// at the front or the back; a longer x is unchanged. A negative or non-integer N is
// NA (Perl's sprintf would left-align with a negative width).
fn pad(m: &mut Machine, row: &Row, fill: u8, front: bool) {
    let n = m.pop();
    let x = m.pop();
    let n = int_operand(m, &n, row);
    let result = match (x, n) {
        (Value::Na, _) | (_, None) => Value::Na,
        (_, Some(n)) if n < 0 => Value::Na,
        (x, Some(n)) => {
            let mut s = m.into_bytes(x, row);
            let missing = (n as u128).saturating_sub(s.len() as u128) as usize;
            if front {
                s.splice(0..0, std::iter::repeat_n(fill, missing));
            } else {
                s.resize(s.len() + missing, fill);
            }
            Value::Bytes(s)
        }
    };
    m.push(result);
}

fn op_ppl(m: &mut Machine, row: &Row, _: &Attached) { pad(m, row, b' ', false) }
fn op_ppr(m: &mut Machine, row: &Row, _: &Attached) { pad(m, row, b' ', true) }
fn op_zp(m: &mut Machine, row: &Row, _: &Attached) { pad(m, row, b'0', true) }

// nchar x c: non-overlapping occurrences of c in x (decision 5); an empty c
// counts 0.
fn op_nchar(m: &mut Machine, row: &Row, _: &Attached) {
    let c = m.pop();
    let x = m.pop();
    let result = match (&x, &c) {
        (Value::Na, _) | (_, Value::Na) => Value::Na,
        _ => {
            let (x, c) = (m.bytes(&x, row), m.bytes(&c, row));
            let n = if c.is_empty() { 0 } else { memmem::find_iter(&x, &c).count() };
            Value::Num(Num::I(n as num::Int))
        }
    };
    m.push(result);
}

// Joining (decision 7): the values as text, joined by sep; NA if a value (or the
// separator) is NA; an empty stack gives an empty string.
fn join_values(m: &mut Machine, row: &Row, values: &[Value], sep: &[u8]) -> Value {
    if values.iter().any(|v| matches!(v, Value::Na)) {
        return Value::Na;
    }
    let mut out = Vec::new();
    for (k, v) in values.iter().enumerate() {
        if k > 0 {
            out.extend_from_slice(sep);
        }
        out.extend_from_slice(&m.bytes(v, row));
    }
    Value::Bytes(out)
}

fn op_catall(m: &mut Machine, row: &Row, _: &Attached) {
    let mut values = std::mem::take(&mut m.stack);
    let result = join_values(m, row, &values, b"");
    values.clear();
    m.stack = values;
    m.push(result);
}

// The separator is the top of the stack (Pops::All(1) guarantees it is there).
fn op_joinall(m: &mut Machine, row: &Row, _: &Attached) {
    let sep = m.pop();
    let mut values = std::mem::take(&mut m.stack);
    let result = match sep {
        Value::Na => Value::Na,
        sep => {
            let sep = m.bytes(&sep, row).into_owned();
            join_values(m, row, &values, &sep)
        }
    };
    values.clear();
    m.stack = values;
    m.push(result);
}

// join x y j gives x j y (pick).
fn op_join(m: &mut Machine, row: &Row, _: &Attached) {
    let j = m.pop();
    let y = m.pop();
    let x = m.pop();
    let result = match j {
        Value::Na => Value::Na,
        j => {
            let sep = m.bytes(&j, row).into_owned();
            join_values(m, row, &[x, y], &sep)
        }
    };
    m.push(result);
}

// uie x y: x if it is not empty and not NA, otherwise y (decision 6; pick: empty
// only). The chosen operand is pushed as it is.
fn op_uie(m: &mut Machine, row: &Row, _: &Attached) {
    let y = m.pop();
    let x = m.pop();
    let use_x = !matches!(x, Value::Na) && !m.bytes(&x, row).is_empty();
    m.push(if use_x { x } else { y });
}

// Encoding (decisions 8, 9), as pick: urlec writes every byte except A-Z a-z 0-9
// _ as %XX (uppercase hexadecimal); urldc decodes %XX (either case), the same
// decoding pluk applies to its arguments (opt::pct_decode); md5 gives the
// lowercase hexadecimal digest (md-5 crate).
fn op_urlec(m: &mut Machine, row: &Row, _: &Attached) {
    string1(m, row, |s| {
        let mut out = Vec::with_capacity(s.len());
        for &b in s.iter() {
            if b.is_ascii_alphanumeric() || b == b'_' {
                out.push(b);
            } else {
                out.extend_from_slice(format!("%{:02X}", b).as_bytes());
            }
        }
        *s = out;
    })
}

fn op_urldc(m: &mut Machine, row: &Row, _: &Attached) {
    string1(m, row, |s| *s = pct_decode(s))
}

fn op_md5(m: &mut Machine, row: &Row, _: &Attached) {
    string1(m, row, |s| {
        let digest = Md5::digest(&s[..]);
        *s = digest.iter().map(|b| format!("{:02x}", b)).collect::<String>().into_bytes();
    })
}

// fmtall f *: f is the bottom of the stack; each # in f is replaced by the next
// value, \# is a literal # (decision 10). The number of # instances in a format,
// for the compile-time check of a constant format and at run time.
pub fn format_slots(f: &[u8]) -> usize {
    let mut n = 0;
    let mut i = 0;
    while i < f.len() {
        if f[i] == b'\\' && f.get(i + 1) == Some(&b'#') {
            i += 2;
        } else {
            n += (f[i] == b'#') as usize;
            i += 1;
        }
    }
    n
}

// A format whose # count differs from the number of values gives NA for the row
// (pick reports an error per row and continues); so does an NA value or format.
fn op_fmtall(m: &mut Machine, row: &Row, _: &Attached) {
    let mut values = std::mem::take(&mut m.stack);
    let result = if values.iter().any(|v| matches!(v, Value::Na)) {
        Value::Na
    } else {
        let f = m.bytes(&values[0], row).into_owned();
        if format_slots(&f) != values.len() - 1 {
            Value::Na
        } else {
            let mut out = Vec::with_capacity(f.len());
            let mut next = values[1..].iter();
            let mut i = 0;
            while i < f.len() {
                if f[i] == b'\\' && f.get(i + 1) == Some(&b'#') {
                    out.push(b'#');
                    i += 2;
                    continue;
                }
                match f[i] {
                    b'#' => out.extend_from_slice(&m.bytes(next.next().unwrap(), row)),
                    b => out.push(b),
                }
                i += 1;
            }
            Value::Bytes(out)
        }
    };
    values.clear();
    m.stack = values;
    m.push(result);
}

// pick's boxit: k clamped to 0 .. len.
fn clamp(k: num::Int, len: usize) -> usize {
    k.clamp(0, len as num::Int) as usize
}

fn op_len(m: &mut Machine, row: &Row, _: &Attached) {
    let x = m.pop();
    let result = match x {
        Value::Na => Value::Na,
        x => Value::Num(Num::I(m.bytes(&x, row).len() as num::Int)),
    };
    m.push(result);
}

fn op_lc(m: &mut Machine, row: &Row, _: &Attached) { string1(m, row, |s| s.make_ascii_lowercase()) }
fn op_uc(m: &mut Machine, row: &Row, _: &Attached) { string1(m, row, |s| s.make_ascii_uppercase()) }
fn op_rev(m: &mut Machine, row: &Row, _: &Attached) { string1(m, row, |s| s.reverse()) }

fn op_rot13(m: &mut Machine, row: &Row, _: &Attached) {
    string1(m, row, |s| for b in s.iter_mut() {
        *b = match *b {
            b'a'..=b'z' => (*b - b'a' + 13) % 26 + b'a',
            b'A'..=b'Z' => (*b - b'A' + 13) % 26 + b'A',
            other => other,
        };
    })
}

fn op_head(m: &mut Machine, row: &Row, _: &Attached) { slice2(m, row, |len, k| (0, clamp(k, len))) }
fn op_tail(m: &mut Machine, row: &Row, _: &Attached) { slice2(m, row, |len, k| (len - clamp(k, len), len)) }
fn op_skip(m: &mut Machine, row: &Row, _: &Attached) { slice2(m, row, |len, k| (clamp(k, len), len)) }
fn op_shed(m: &mut Machine, row: &Row, _: &Attached) { slice2(m, row, |len, k| (0, len - clamp(k, len))) }

// The byte range of Perl's substr($x, $i, $k) in a string of length len, None
// where Perl returns undef (the substring lies beyond either end): a negative i
// counts from the end, a negative k leaves -k bytes off the end; a substring
// partly outside the string is cut to the part inside.
pub fn substr_range(len: usize, i: num::Int, k: num::Int) -> Option<(usize, usize)> {
    let len = len as num::Int;
    let start = if i < 0 { len + i } else { i };
    let end = if k < 0 { len + k } else { start.saturating_add(k) };
    if start > len || end < 0 {
        return None;
    }
    let (start, end) = (start.max(0), end.min(len));
    Some((start as usize, end.max(start) as usize))
}

fn op_substr(m: &mut Machine, row: &Row, _: &Attached) {
    let k = m.pop();
    let i = m.pop();
    let x = m.pop();
    let (i, k) = (int_operand(m, &i, row), int_operand(m, &k, row));
    let result = match (x, i, k) {
        (Value::Na, _, _) | (_, None, _) | (_, _, None) => Value::Na,
        (x, Some(i), Some(k)) => {
            let s = m.into_bytes(x, row);
            match substr_range(s.len(), i, k) {
                Some((a, b)) => Value::Bytes(s[a..b].to_vec()),
                None => Value::Bytes(Vec::new()),        // Perl: undef (and a warning)
            }
        }
    };
    m.push(result);
}

// strim s nx ny: nx bytes off the start and ny off the end (negative counts are 0);
// empty if nothing remains (pick).
fn op_strim(m: &mut Machine, row: &Row, _: &Attached) {
    let ny = m.pop();
    let nx = m.pop();
    let s = m.pop();
    let (nx, ny) = (int_operand(m, &nx, row), int_operand(m, &ny, row));
    let result = match (s, nx, ny) {
        (Value::Na, _, _) | (_, None, _) | (_, _, None) => Value::Na,
        (s, Some(nx), Some(ny)) => {
            let s = m.into_bytes(s, row);
            let (nx, ny) = (nx.max(0) as u128, ny.max(0) as u128);
            if nx + ny < s.len() as u128 {
                Value::Bytes(s[nx as usize..s.len() - ny as usize].to_vec())
            } else {
                Value::Bytes(Vec::new())
            }
        }
    };
    m.push(result);
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

fn op_add(m: &mut Machine, row: &Row, _: &Attached) { numeric2(m, row, |a, b| Some(num::add(a, b))) }
fn op_sub(m: &mut Machine, row: &Row, _: &Attached) { numeric2(m, row, |a, b| Some(num::sub(a, b))) }
fn op_mul(m: &mut Machine, row: &Row, _: &Attached) { numeric2(m, row, |a, b| Some(num::mul(a, b))) }
fn op_div(m: &mut Machine, row: &Row, _: &Attached) { numeric2(m, row, |a, b| Some(num::div(a, b))) }
fn op_idiv(m: &mut Machine, row: &Row, _: &Attached) { numeric2(m, row, num::idiv) }
fn op_mod(m: &mut Machine, row: &Row, _: &Attached) { numeric2(m, row, num::imod) }
fn op_pow(m: &mut Machine, row: &Row, _: &Attached) { numeric2(m, row, |a, b| Some(num::pow(a, b))) }
fn op_ipow(m: &mut Machine, row: &Row, _: &Attached) { numeric2(m, row, num::ipow) }
fn op_max(m: &mut Machine, row: &Row, _: &Attached) { choose(m, row, true) }
fn op_min(m: &mut Machine, row: &Row, _: &Attached) { choose(m, row, false) }

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
fn op_neg(m: &mut Machine, row: &Row, _: &Attached) { numeric1(m, row, num::neg) }
fn op_abs(m: &mut Machine, row: &Row, _: &Attached) { numeric1(m, row, num::abs) }
fn op_sign(m: &mut Machine, row: &Row, _: &Attached) { numeric1(m, row, num::sign) }
fn op_sq(m: &mut Machine, row: &Row, _: &Attached) { numeric1(m, row, num::sq) }
fn op_incr(m: &mut Machine, row: &Row, _: &Attached) { numeric1(m, row, num::incr) }
fn op_decr(m: &mut Machine, row: &Row, _: &Attached) { numeric1(m, row, num::decr) }

// A random float in [0, x), as Perl's rand: x = 0 counts as 1, a negative x gives
// (x, 0]. Seeded from the clock, or with --seed=N.
fn op_rand(m: &mut Machine, row: &Row, _: &Attached) {
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

fn op_int(m: &mut Machine, row: &Row, _: &Attached) { numeric1(m, row, |a| num::round_with(a, f64::trunc)) }
fn op_floor(m: &mut Machine, row: &Row, _: &Attached) { numeric1(m, row, |a| num::round_with(a, f64::floor)) }
fn op_ceil(m: &mut Machine, row: &Row, _: &Attached) { numeric1(m, row, |a| num::round_with(a, f64::ceil)) }
fn op_round(m: &mut Machine, row: &Row, _: &Attached) { numeric1(m, row, |a| num::round_with(a, f64::round_ties_even)) }

// Functions on floats: the operand as f64 (an integer is promoted), the result a
// float; domain errors follow IEEE (log(0) is -inf, sqrt(-1) nan; step12.txt
// decision 6).
fn float1(m: &mut Machine, row: &Row, f: fn(f64) -> f64) {
    numeric1(m, row, |a| Num::F(f(a.to_f64())))
}

fn op_sqrt(m: &mut Machine, row: &Row, _: &Attached) { float1(m, row, f64::sqrt) }
fn op_exp(m: &mut Machine, row: &Row, _: &Attached) { float1(m, row, f64::exp) }
fn op_exp2(m: &mut Machine, row: &Row, _: &Attached) { float1(m, row, f64::exp2) }
fn op_exp10(m: &mut Machine, row: &Row, _: &Attached) { float1(m, row, |x| 10f64.powf(x)) }
fn op_log(m: &mut Machine, row: &Row, _: &Attached) { float1(m, row, f64::ln) }
fn op_log2(m: &mut Machine, row: &Row, _: &Attached) { float1(m, row, f64::log2) }
fn op_log10(m: &mut Machine, row: &Row, _: &Attached) { float1(m, row, f64::log10) }
fn op_sin(m: &mut Machine, row: &Row, _: &Attached) { float1(m, row, f64::sin) }
fn op_cos(m: &mut Machine, row: &Row, _: &Attached) { float1(m, row, f64::cos) }
fn op_tan(m: &mut Machine, row: &Row, _: &Attached) { float1(m, row, f64::tan) }

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
fn op_addall(m: &mut Machine, row: &Row, _: &Attached) {
    whole(m, row, |xs| Some(xs.iter().fold(Num::I(0), |a, &b| num::add(a, b))))
}

fn op_mulall(m: &mut Machine, row: &Row, _: &Attached) {
    whole(m, row, |xs| Some(xs.iter().fold(Num::I(1), |a, &b| num::mul(a, b))))
}

fn op_meanall(m: &mut Machine, row: &Row, _: &Attached) {
    whole(m, row, |xs| {
        let sum = xs.iter().fold(Num::I(0), |a, &b| num::add(a, b));
        (!xs.is_empty()).then(|| num::div(sum, Num::I(xs.len() as num::Int)))
    })
}

// As pick: n / sum(1/|x|), and (product |x|)^(1/n). A zero makes the harmonic
// mean 0 (IEEE: n / inf; pick dies).
fn op_hmeanall(m: &mut Machine, row: &Row, _: &Attached) {
    whole(m, row, |xs| {
        let inv: f64 = xs.iter().map(|x| 1.0 / x.to_f64().abs()).sum();
        (!xs.is_empty()).then(|| Num::F(xs.len() as f64 / inv))
    })
}

// The geometric mean as the exponent of the mean logarithm, which cannot overflow
// on long stacks (pick multiplies first; the last digits can differ). A zero gives
// ln 0 = -inf, so the mean is 0.
fn op_gmeanall(m: &mut Machine, row: &Row, _: &Attached) {
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

fn op_dd(m: &mut Machine, row: &Row, _: &Attached) { precision(m, row, 1, false, |v| v[0]) }
fn op_sn(m: &mut Machine, row: &Row, _: &Attached) { precision(m, row, 1, true, |v| v[0]) }
// Computed as pick: x*100/y and x*1000/y (division by zero follows IEEE).
fn op_frac(m: &mut Machine, row: &Row, _: &Attached) { precision(m, row, 2, false, |v| v[0] / v[1]) }
fn op_pct(m: &mut Machine, row: &Row, _: &Attached) { precision(m, row, 2, false, |v| v[0] * 100.0 / v[1]) }
fn op_pml(m: &mut Machine, row: &Row, _: &Attached) { precision(m, row, 2, false, |v| v[0] * 1000.0 / v[1]) }

// maxall and minall push the winning operand itself, as max and min: the first
// value, replaced by a later one that is greater (smaller); NaN never wins (as
// pick, which starts from -DBL_MAX); all NaN gives NaN. NA if a value is not a
// number; an empty stack is NA.
fn op_maxall(m: &mut Machine, row: &Row, _: &Attached) { choose_all(m, row, true) }
fn op_minall(m: &mut Machine, row: &Row, _: &Attached) { choose_all(m, row, false) }

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

// ifelse B x y (plan/step15.txt decision 2): x if B is true, otherwise y, passed on
// as they are. B is true if it is a number other than 0, or else if it is not
// empty (pick's do_ifelse); an NA condition gives NA.
fn op_ifelse(m: &mut Machine, row: &Row, _: &Attached) {
    let y = m.pop();
    let x = m.pop();
    let b = m.pop();
    let result = match b {
        Value::Na => Value::Na,
        b => {
            let truth = match m.num(&b, row) {
                Some(n) => compare(n, Num::I(0)) != Some(Ordering::Equal),
                None => !m.bytes(&b, row).is_empty(),
            };
            if truth { x } else { y }
        }
    };
    m.push(result);
}

// test x y T (decision 1): 1 if filter operator T holds for x and y, 0 if not, NA
// where a filter would reject the row for a value that is not a number (or not an
// integer), or if an operand is NA. Numeric operators use the numbers (cached per
// field), not their text; /ep/ and /om/ use their default bands.
fn op_test(m: &mut Machine, row: &Row, a: &Attached) {
    let Attached::Test(op, rhs) = a else { unreachable!("test has its operator attached") };
    m.pop();
    let y = m.pop();
    let x = m.pop();
    let outcome = if matches!(x, Value::Na) || (matches!(rhs, TestRhs::Value) && matches!(y, Value::Na)) {
        Outcome::NotNumber
    } else if op.is_num() {
        filter::numeric(*op, m.num(&x, row), m.num(&y, row), filter::default_band(*op))
    } else {
        let field = m.bytes(&x, row).into_owned();
        let band = filter::default_band(*op);
        match rhs {
            TestRhs::Regex(re) => filter::evaluate(*op, &field, Rhs::Regex(re), b"", band),
            TestRhs::Dict(d) => filter::evaluate(*op, &field, Rhs::Dict(&m.dicts.at(*d).map), b"", band),
            TestRhs::Value => filter::evaluate(*op, &field, Rhs::Bytes(&m.bytes(&y, row)), b"", band),
        }
    };
    m.push(match outcome {
        Outcome::Pass => Value::Num(Num::I(1)),
        Outcome::Fail => Value::Num(Num::I(0)),
        _ => Value::Na,
    });
}

// Dictionary operators (plan/step15.txt decision 0). The dictionary was resolved
// at compile time (Prepare::ConstDict); its name is still on the stack and popped
// unused. An NA key gives NA.
fn dict_key<'a>(m: &mut Machine<'a>, a: &Attached) -> (Value, &'a Dict) {
    let Attached::Dict(d) = a else { unreachable!("a dictionary operator has a dictionary") };
    m.pop();
    (m.pop(), m.dicts.at(*d))
}

// map: the value of the key, or the dictionary's not-found rule (step4): the key
// itself (passed on as it is), the default, or __EXIT__, which ends the run.
fn op_map(m: &mut Machine, row: &Row, a: &Attached) {
    let (x, dict) = dict_key(m, a);
    if matches!(x, Value::Na) {
        return m.push(Value::Na);
    }
    let found = dict.map.get(&*m.bytes(&x, row)).cloned();
    let result = match (found, &dict.not_found) {
        (Some(v), _) => Value::Bytes(v),
        (None, NotFound::Key) => x,
        (None, NotFound::Value(d)) => Value::Bytes(d.clone()),
        (None, NotFound::Exit) => {
            let msg = format!("map: key [{}] not found in dictionary [{}] (__EXIT__)",
                              String::from_utf8_lossy(&m.bytes(&x, row)), String::from_utf8_lossy(&dict.name));
            m.fail(msg);
            Value::Na
        }
    };
    m.push(result);
}

// dord: the 1-based position of the key's first appearance; NA if absent (or if the
// dictionary has no order, as seqlen).
fn op_dord(m: &mut Machine, row: &Row, a: &Attached) {
    let (x, dict) = dict_key(m, a);
    let result = match x {
        Value::Na => Value::Na,
        x => dict.order.as_ref().and_then(|o| o.get(&*m.bytes(&x, row)))
            .map_or(Value::Na, |&n| Value::Num(Num::I(n as num::Int))),
    };
    m.push(result);
}

// The compiled regex of a regex operator (Prepare::ConstRegex).
fn regex_of(a: &Attached) -> &Regex {
    match a {
        Attached::Regex(re) => re,
        _ => unreachable!("a regex operator has a compiled pattern"),
    }
}

// Regex operators (plan/step14.txt). The pattern was compiled at compile time
// (Prepare::ConstRegex); its constant is still on the stack, so that the run-time
// stack matches the static one, and is popped unused. An NA x or replacement gives
// NA; numbers are used as their text.
fn substitute(m: &mut Machine, row: &Row, re: &Attached, with_replacement: bool,
              edit: impl Fn(&Regex, &[u8], &[u8]) -> Vec<u8>) {
    let re = regex_of(re);
    let s = if with_replacement { m.pop() } else { Value::Bytes(Vec::new()) };
    m.pop();
    let x = m.pop();
    let result = match (&x, &s) {
        (Value::Na, _) | (_, Value::Na) => Value::Na,
        _ => {
            let (x, s) = (m.bytes(&x, row), m.bytes(&s, row));
            Value::Bytes(edit(re, &x, &s))
        }
    };
    m.push(result);
}

// The matches of re in h as Perl's //g (and s///g) finds them (decision 5). The
// regex crate's own iteration does not allow an empty match directly after a
// non-empty one; Perl does, and refuses an empty match only where the previous
// match was empty, then moving on by one byte (s/a*/-/g on aaa: Perl and pluk --,
// the regex crate -). Python follows Perl since 3.7. Not reproduced: after an empty
// match Perl backtracks into a longer match at the same position, which matters
// for lazy patterns that can match empty (a*?).
pub fn perl_matches(re: &Regex, h: &[u8]) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    let mut pos = 0;
    let mut last_empty: Option<usize> = None;
    while pos <= h.len() {
        let Some(m) = re.find_at(h, pos) else { break };
        if m.is_empty() && last_empty == Some(m.start()) {
            pos = m.start() + 1;
            continue;
        }
        found.push((m.start(), m.end()));
        last_empty = if m.is_empty() { Some(m.end()) } else { None };
        pos = m.end();
    }
    found
}

// x with each of the matches replaced by what f returns for it.
fn replace_matches(x: &[u8], matches: &[(usize, usize)], mut f: impl FnMut(usize, usize) -> Vec<u8>) -> Vec<u8> {
    let mut out = Vec::with_capacity(x.len());
    let mut done = 0;
    for &(start, end) in matches {
        out.extend_from_slice(&x[done..start]);
        out.extend_from_slice(&f(start, end));
        done = end;
    }
    out.extend_from_slice(&x[done..]);
    out
}

// The replacement is literal (NoExpand): in Perl's s/$p/$s/ the replacement is a
// variable, so a $1 in it stays as it is (decision 3).
fn op_del(m: &mut Machine, row: &Row, re: &Attached) {
    substitute(m, row, re, false, |re, x, _| re.replacen(x, 1, NoExpand(b"")).into_owned())
}

fn op_delg(m: &mut Machine, row: &Row, re: &Attached) {
    substitute(m, row, re, false, |re, x, _| replace_matches(x, &perl_matches(re, x), |_, _| Vec::new()))
}

fn op_ed(m: &mut Machine, row: &Row, re: &Attached) {
    substitute(m, row, re, true, |re, x, s| re.replacen(x, 1, NoExpand(s)).into_owned())
}

fn op_edg(m: &mut Machine, row: &Row, re: &Attached) {
    substitute(m, row, re, true, |re, x, s| replace_matches(x, &perl_matches(re, x), |_, _| s.to_vec()))
}

// The replacement of edbr and edgbr for one match, as pick's do_ed_br_helper
// (decision 4): $ followed by digits (all of them: $12 is group 12) is that group,
// empty if it did not take part or does not exist; $0 is the whole match; all else
// is literal. Written here rather than with the regex crate's expansion, which
// reads $1a as a group named '1a'.
pub fn expand_groups(template: &[u8], caps: &Captures) -> Vec<u8> {
    let mut out = Vec::with_capacity(template.len());
    let mut i = 0;
    while i < template.len() {
        let digits = template[i + 1..].iter().take_while(|b| b.is_ascii_digit()).count();
        if template[i] == b'$' && digits > 0 {
            let group = std::str::from_utf8(&template[i + 1..i + 1 + digits]).unwrap().parse::<usize>();
            if let Some(g) = group.ok().and_then(|n| caps.get(n)) {
                out.extend_from_slice(g.as_bytes());
            }
            i += 1 + digits;
        } else {
            out.push(template[i]);
            i += 1;
        }
    }
    out
}

fn op_edbr(m: &mut Machine, row: &Row, re: &Attached) {
    substitute(m, row, re, true, |re, x, s| match re.captures(x) {
        Some(caps) => {
            let whole = caps.get(0).unwrap();
            replace_matches(x, &[(whole.start(), whole.end())], |_, _| expand_groups(s, &caps))
        }
        None => x.to_vec(),
    })
}

// get and npat: pop the pattern constant and x; NA for an NA x.
fn match_op(m: &mut Machine, row: &Row, re: &Attached, f: impl Fn(&Regex, &[u8]) -> Value) {
    let re = regex_of(re);
    m.pop();
    let x = m.pop();
    let result = match x {
        Value::Na => Value::Na,
        x => f(re, &m.bytes(&x, row)),
    };
    m.push(result);
}

// As pick: $1 if it is defined (group 1 took part), else $&; empty if no match.
fn op_get(m: &mut Machine, row: &Row, re: &Attached) {
    match_op(m, row, re, |re, x| match re.captures(x) {
        Some(caps) => Value::Bytes(caps.get(1).or(caps.get(0)).unwrap().as_bytes().to_vec()),
        None => Value::Bytes(Vec::new()),
    })
}

// As pick's '$n++ while /p/g': the matches as Perl finds them (perl_matches).
fn op_npat(m: &mut Machine, row: &Row, re: &Attached) {
    match_op(m, row, re, |re, x| Value::Num(Num::I(perl_matches(re, x).len() as num::Int)))
}

// The groups of each match: the match positions as Perl finds them (perl_matches),
// the groups from a search at the match's start, which finds that same match.
fn op_edgbr(m: &mut Machine, row: &Row, re: &Attached) {
    substitute(m, row, re, true, |re, x, s| {
        replace_matches(x, &perl_matches(re, x), |start, _| match re.captures_at(x, start) {
            Some(caps) => expand_groups(s, &caps),
            None => Vec::new(),
        })
    })
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table() {
        assert_eq!(lookup(b"add").map(|op| op.pops), Some(Pops::N(2)));
        assert_eq!(lookup(b"addall").map(|op| op.pops), Some(Pops::All(0)));
        assert!(lookup(b"nosuch").is_none());
        // md5 of known test vectors (RFC 1321).
        let md5 = |x: &str| Md5::digest(x.as_bytes()).iter().map(|b| format!("{:02x}", b)).collect::<String>();
        assert_eq!(md5(""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(md5("abc"), "900150983cd24fb0d6963f7d28e17f72");
        let re = |p: &str| crate::io::compile_pattern(p.as_bytes(), "test").unwrap();
        assert_eq!(perl_matches(&re("a*"), b"baab"), vec![(0, 0), (1, 3), (3, 3), (4, 4)]);
        assert_eq!(perl_matches(&re("a*"), b"aaa"), vec![(0, 3), (3, 3)]);
        assert_eq!(perl_matches(&re("x"), b"axbxc"), vec![(1, 2), (3, 4)]);
        assert_eq!(perl_matches(&re("x*"), b""), vec![(0, 0)]);
        let caps = re("(a)(b)?(c)").captures(b"xacy").unwrap();
        assert_eq!(expand_groups(b"[$1|$2|$3|$0|$12|$4|$|$x]", &caps), b"[a||c|ac|||$|$x]".to_vec());
        assert_eq!((format_slots(b"a#b#"), format_slots(b"\\#x#"), format_slots(b"\\x#"), format_slots(b"#\\")), (2, 1, 1, 1));
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
