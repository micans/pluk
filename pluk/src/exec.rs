// Running computes: the stack machine (plan/COMPUTE.design.txt sections 3 and 7).
//
// For each data row that passes the @@ stage, every compute runs its instructions
// on one reused stack; what is left on the stack is written as text, concatenated,
// and appended to the row as a computed field.

use std::borrow::Cow;

use regex::bytes::Regex;

use crate::compute::{Compute, Instr};
use crate::num::{self, Num};
use crate::row::Row;

// A value on the stack.
#[derive(Clone, Debug)]
pub enum Value {
    Field(usize),       // a column of the row; its bytes stay in the row
    Bytes(Vec<u8>),     // a string, owned (a constant, or produced by an operator)
    Num(Num),
    Na,
}

// The implementation of an operator: it pops its operands from the machine's
// stack and pushes its results. The regex is the compiled constant pattern of a
// regex operator (Instr::OpRegex), None otherwise.
pub type OpFn = fn(&mut Machine, &Row, Option<&Regex>);

pub struct Machine {
    pub stack:  Vec<Value>,
    cache:      Vec<(u64, Option<Num>)>,    // per column: (row generation, parse result)
    generation: u64,                        // increases per row; older cache entries are stale
    perl:       bool,                       // --num-perl
    text:       Vec<u8>,                    // the result of a compute, as text
}

impl Machine {
    pub fn new(perl: bool) -> Machine {
        Machine { stack: Vec::with_capacity(16), cache: Vec::new(), generation: 0, perl, text: Vec::new() }
    }

    // Run all computes on a row; each appends its result to the row.
    pub fn run(&mut self, computes: &[Compute], row: &mut Row) {
        self.generation += 1;
        row.clear_computed();
        for compute in computes {
            self.stack.clear();
            for instr in &compute.instrs {
                match instr {
                    Instr::Field(i) => self.stack.push(Value::Field(*i)),
                    Instr::Const(b) => self.stack.push(Value::Bytes(b.clone())),
                    Instr::Op(op) => (op.fun)(self, row, None),
                    Instr::OpRegex(op, re) => (op.fun)(self, row, Some(re)),
                }
            }
            // The stack, left to right, as text; an empty stack gives an empty field.
            self.text.clear();
            for value in &self.stack {
                write_value(&mut self.text, value, row, self.perl);
            }
            row.push_computed(&self.text);
        }
    }

    pub fn pop(&mut self) -> Value {
        // The compiler has checked the stack depth (no underflow at run time).
        self.stack.pop().expect("stack checked at compile time")
    }

    // --num-perl: for operators that write numbers as text (dd, sn, ...).
    pub fn perl(&self) -> bool {
        self.perl
    }

    pub fn push(&mut self, value: Value) {
        self.stack.push(value);
    }

    // A value as a number, None for NA. A field is parsed once per row and column.
    pub fn num(&mut self, value: &Value, row: &Row) -> Option<Num> {
        match value {
            Value::Num(n) => Some(*n),
            Value::Na => None,
            Value::Bytes(b) => num::parse_num(b),
            Value::Field(i) => {
                if *i >= self.cache.len() {
                    self.cache.resize(*i + 1, (0, None));
                }
                let (generation, parsed) = self.cache[*i];
                if generation == self.generation {
                    return parsed;
                }
                let parsed = num::parse_num(row.field(*i));
                self.cache[*i] = (self.generation, parsed);
                parsed
            }
        }
    }

    // A value as bytes, borrowed where possible (fields, owned strings).
    pub fn bytes<'a>(&self, value: &'a Value, row: &'a Row) -> Cow<'a, [u8]> {
        match value {
            Value::Field(i) => Cow::Borrowed(row.field(*i)),
            Value::Bytes(b) => Cow::Borrowed(b),
            _ => {
                let mut out = Vec::new();
                write_value(&mut out, value, row, self.perl);
                Cow::Owned(out)
            }
        }
    }

    // A value as owned bytes, for an operator that changes them: an owned string
    // is taken over without copying, a field is copied.
    pub fn into_bytes(&self, value: Value, row: &Row) -> Vec<u8> {
        match value {
            Value::Bytes(b) => b,
            other => self.bytes(&other, row).into_owned(),
        }
    }
}

// A value as text: fields and strings as they are, numbers per write_num, NA as NA.
fn write_value(out: &mut Vec<u8>, value: &Value, row: &Row, perl: bool) {
    match value {
        Value::Field(i) => out.extend_from_slice(row.field(*i)),
        Value::Bytes(b) => out.extend_from_slice(b),
        Value::Num(n) => num::write_num(out, *n, perl),
        Value::Na => out.extend_from_slice(b"NA"),
    }
}
