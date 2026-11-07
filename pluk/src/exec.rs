// Running computes: the stack machine (plan/COMPUTE.design.txt sections 3 and 7).
//
// For each data row that passes the @@ stage, every compute runs its instructions
// on one reused stack; what is left on the stack is written as text, concatenated,
// and appended to the row as a computed field.

use std::borrow::Cow;

use regex::bytes::Regex;

use crate::compute::{Attached, Compute, Instr};
use std::collections::HashMap;

use crate::dict::Dicts;
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
// stack and pushes its results. Attached is what the prepare step compiled for it
// (a regex, a dictionary position), Attached::None otherwise.
pub type OpFn = fn(&mut Machine<'_>, &Row, &Attached);

// 'd: the machine refers to the dictionaries, which outlive it (main.rs).
pub struct Machine<'d> {
    pub dicts:  &'d Dicts,
    fatal:      Option<String>,             // an error that ends the run (map with __EXIT__)
    pub stack:  Vec<Value>,
    cache:      Vec<(u64, Option<Num>)>,    // per column: (row generation, parse result)
    generation: u64,                        // increases per row; older cache entries are stale
    perl:       bool,                       // --num-perl
    text:       Vec<u8>,                    // the result of a compute, as text
    rng:        SplitMix64,                 // for rand
    // Per row (plan/step15.txt decisions 3, 4): the registers of save and fetch, by
    // slot (constant names) and by name (names from columns without a slot), and
    // the row and line number.
    pub registers: Vec<Option<Value>>,
    register_slots: HashMap<Vec<u8>, usize>,
    other_registers: HashMap<Vec<u8>, Value>,
    pub rowno:  u64,
    pub lineno: u64,
    time_zone:  Option<jiff::tz::TimeZone>,     // for todate, looked up on first use
    pub sam:    crate::sam::View,               // the per-row SAM view (plan/SAMVIEW.design.txt)
}

// A small random number generator, splitmix64 (Steele, Lea and Flood, 2014): a
// 64-bit state advanced by a constant, then mixed. Good enough for rand; it avoids
// a dependency (Rust's standard library has no generator).
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub fn new(seed: u64) -> SplitMix64 {
        SplitMix64 { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }

    // A float in [0, 1): the top 53 bits, scaled.
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }
}

// The seed when --seed is not given: the clock and the process id.
pub fn clock_seed() -> u64 {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    nanos ^ ((std::process::id() as u64) << 32)
}

impl<'d> Machine<'d> {
    pub fn new(perl: bool, seed: u64, dicts: &'d Dicts) -> Machine<'d> {
        Machine { dicts, fatal: None, stack: Vec::with_capacity(16), cache: Vec::new(), generation: 0, perl,
                  text: Vec::new(), rng: SplitMix64::new(seed), registers: Vec::new(),
                  register_slots: HashMap::new(), other_registers: HashMap::new(), rowno: 0, lineno: 0,
                  time_zone: None, sam: crate::sam::View::new(dicts) }
    }

    // The register slots of the compiled computes (constant names of save and fetch).
    pub fn set_registers(&mut self, computes: &[Compute]) {
        for compute in computes {
            for instr in &compute.instrs {
                if let Instr::OpWith(_, Attached::Register(slot, name)) = instr {
                    self.register_slots.insert(name.clone(), *slot);
                }
            }
        }
        self.registers = vec![None; self.register_slots.len()];
    }

    // save and fetch with a name from a column: the slot if the name has one.
    pub fn store_register(&mut self, name: Vec<u8>, value: Value) {
        match self.register_slots.get(&name) {
            Some(&slot) => self.registers[slot] = Some(value),
            None => {
                self.other_registers.insert(name, value);
            }
        }
    }

    pub fn find_register(&self, name: &[u8]) -> Option<Value> {
        match self.register_slots.get(name) {
            Some(&slot) => self.registers[slot].clone(),
            None => self.other_registers.get(name).cloned(),
        }
    }

    // An operator that cannot continue (map with __EXIT__) records an error; the
    // run stops after the current compute.
    pub fn fail(&mut self, msg: String) {
        self.fatal.get_or_insert(msg);
    }

    // The system time zone (TZ honoured), looked up once.
    pub fn time_zone(&mut self) -> &jiff::tz::TimeZone {
        self.time_zone.get_or_insert_with(jiff::tz::TimeZone::system)
    }

    // A random float in [0, 1).
    pub fn random(&mut self) -> f64 {
        self.rng.next_f64()
    }

    // Run all computes on a row; each appends its result to the row. Err: an
    // operator failed (Machine::fail).
    // rowno: the data row number; lineno: the physical line number.
    pub fn run(&mut self, computes: &[Compute], row: &mut Row, rowno: u64, lineno: u64) -> Result<(), String> {
        self.generation += 1;
        row.clear_computed();
        (self.rowno, self.lineno) = (rowno, lineno);
        self.registers.fill(None);
        self.other_registers.clear();
        self.sam.reset();
        for compute in computes {
            self.stack.clear();
            for instr in &compute.instrs {
                match instr {
                    Instr::Field(i) => self.stack.push(Value::Field(*i)),
                    Instr::Const(b) => self.stack.push(Value::Bytes(b.clone())),
                    Instr::Op(op) => (op.fun)(self, row, &Attached::None),
                    Instr::OpWith(op, attached) => (op.fun)(self, row, attached),
                }
            }
            // The stack, left to right, as text; an empty stack gives an empty field.
            self.text.clear();
            for value in &self.stack {
                write_value(&mut self.text, value, row, self.perl);
            }
            row.push_computed(&self.text);
            if let Some(msg) = self.fatal.take() {
                return Err(msg);
            }
        }
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitmix() {
        // Reference values of splitmix64 for seed 0.
        let mut r = SplitMix64::new(0);
        assert_eq!(r.next_u64(), 0xe220a8397b1dcdaf);
        assert_eq!(r.next_u64(), 0x6e789e6aa1b965f4);
        // Floats in [0, 1) with a plausible mean.
        let mut r = SplitMix64::new(42);
        let xs: Vec<f64> = (0..100000).map(|_| r.next_f64()).collect();
        assert!(xs.iter().all(|&x| (0.0..1.0).contains(&x)));
        let mean = xs.iter().sum::<f64>() / xs.len() as f64;
        assert!((mean - 0.5).abs() < 0.01, "{}", mean);
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
