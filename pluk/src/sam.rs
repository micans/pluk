// SAM records: the input rewrites and (from step17 item 3) the per-row SAM view.
// Design: plan/SAMVIEW.design.txt.

use crate::dict::{self, Dicts};
use crate::num::Int;
use crate::row::Row;

// SAM columns, 0-based.
pub const FLAG: usize = 1;
pub const RNAME: usize = 2;
pub const POS: usize = 3;
pub const CIGAR: usize = 5;
pub const SEQ: usize = 9;
pub const QUAL: usize = 10;
pub const OPTIONAL: usize = 11;     // the optional fields start here (with -O12: all in this one)

const FLAG_UNMAPPED: u64 = 0x4;
const FLAG_REVERSE: u64 = 0x10;

fn number(field: &[u8]) -> Option<u64> {
    if field.is_empty() || !field.iter().all(u8::is_ascii_digit) {
        return None;
    }
    std::str::from_utf8(field).ok()?.parse().ok()
}

// ---- CIGAR (SAMVIEW.design.txt section 4) ----

// One CIGAR operation: its length, its letter, and where it is in the CIGAR string.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Op {
    pub len:   u64,
    pub op:    u8,
    pub start: usize,
    pub end:   usize,
}

impl Op {
    pub fn consumes_reference(&self) -> bool {
        matches!(self.op, b'M' | b'D' | b'N' | b'=' | b'X')
    }
    pub fn consumes_query(&self) -> bool {
        matches!(self.op, b'M' | b'I' | b'S' | b'=' | b'X')
    }
    fn is_aligned(&self) -> bool {
        matches!(self.op, b'M' | b'=' | b'X')
    }
    // Operations that make up the trails (section 5).
    fn is_trail(&self) -> bool {
        matches!(self.op, b'S' | b'H' | b'D' | b'N' | b'I')
    }
}

// The operations of a CIGAR; None if it does not parse ('*' included: no alignment).
pub fn parse_cigar(cigar: &[u8]) -> Option<Vec<Op>> {
    let mut ops = Vec::new();
    let mut i = 0;
    while i < cigar.len() {
        let start = i;
        let mut len: u64 = 0;
        while i < cigar.len() && cigar[i].is_ascii_digit() {
            len = len.checked_mul(10)?.checked_add((cigar[i] - b'0') as u64)?;
            i += 1;
        }
        if i == start || i == cigar.len() || len == 0 || !b"MIDNSHP=X".contains(&cigar[i]) {
            return None;
        }
        ops.push(Op { len, op: cigar[i], start, end: i + 1 });
        i += 1;
    }
    (!ops.is_empty()).then_some(ops)
}

fn ref_len(ops: &[Op]) -> u64 {
    ops.iter().filter(|o| o.consumes_reference()).map(|o| o.len).sum()
}

fn qry_len(ops: &[Op]) -> u64 {
    ops.iter().filter(|o| o.consumes_query()).map(|o| o.len).sum()
}

fn sum_of(ops: &[Op], letters: &[u8]) -> u64 {
    ops.iter().filter(|o| letters.contains(&o.op)).map(|o| o.len).sum()
}

// The 5' trail, matched part and 3' trail as index ranges into ops (section 5): the
// leading run of trail operations, the trailing run of what is left, the middle.
pub fn trails(ops: &[Op]) -> (std::ops::Range<usize>, std::ops::Range<usize>, std::ops::Range<usize>) {
    let t5 = ops.iter().take_while(|o| o.is_trail()).count();
    let t3 = ops[t5..].iter().rev().take_while(|o| o.is_trail()).count();
    let m = ops.len() - t3;
    (0..t5, t5..m, m..ops.len())
}

// ---- The view (SAMVIEW.design.txt sections 4-6) ----

// Why a value is not available (section 6); each is counted once per row.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Na {
    Unmapped,
    Cigar,
    RefLength,
    SeqLength,
    RefSequence,
    RefEnd,
}

const N_REASONS: usize = 6;
const REASONS: [Na; N_REASONS] = [Na::Unmapped, Na::Cigar, Na::RefLength, Na::SeqLength, Na::RefSequence, Na::RefEnd];

impl Na {
    fn index(self) -> usize {
        self as usize
    }
    fn describe(self) -> &'static str {
        match self {
            Na::Unmapped => "unmapped (FLAG 0x4, RNAME or CIGAR '*')",
            Na::Cigar => "CIGAR or POS not valid",
            Na::RefLength => "reference length not known (no @SQ line or fasta for RNAME)",
            Na::SeqLength => "SEQ or QUAL length differs from the CIGAR",
            Na::RefSequence => "RNAME not in the reference sequences (SAMFA)",
            Na::RefEnd => "CIGAR past the reference end",
        }
    }
}

// The count stage: the trails and the counts of section 5.
#[derive(Clone, Debug, Default)]
pub struct Counts {
    pub cgr_trail5p: Vec<u8>,
    pub cgr_matched: Vec<u8>,
    pub cgr_trail3p: Vec<u8>,
    pub ref_trail5p: Int,       // ref_trail5p_N
    pub ref_matched: Int,
    pub ref_trail3p: Option<Int>,   // needs the reference length
    pub ref_trail5p_v: Int,
    pub ref_trail3p_v: Int,
    pub ref_len: Option<Int>,
    pub qry_trail5p: Int,
    pub qry_matched: Int,
    pub qry_trail3p: Int,
    pub aln_nmatch: Int,
    pub matched_indel: Int,     // D and I of the matched part, for aln_nmatchx
}

impl Counts {
    pub fn ref_posx(&self) -> Int { self.ref_trail5p + 1 }
    pub fn ref_posy(&self) -> Int { self.ref_trail5p + self.ref_matched }
    pub fn qry_len(&self) -> Int { self.qry_trail5p + self.qry_matched + self.qry_trail3p }
    pub fn qry_posx(&self) -> Int { self.qry_trail5p + 1 }
    pub fn qry_posy(&self) -> Int { self.qry_trail5p + self.qry_matched }
}

// The query sequence of a row, set by the fill (main.rs) before the computes:
// SEQ and QUAL of the row, the filled ones, or a SEQ that does not fit the CIGAR.
#[derive(Clone, Debug, Default)]
pub enum Query {
    #[default]
    Row,
    Filled { seq: Vec<u8>, qual: Vec<u8> },
    Mismatch,
}

// The three parts of a sequence: before, in and after the matched part.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Part {
    Trail5p,
    Matched,
    Trail3p,
}

// Part of seq, cut at a and b (a <= b <= seq.len()).
fn cut(seq: &[u8], a: usize, b: usize, part: Part) -> &[u8] {
    match part {
        Part::Trail5p => &seq[..a],
        Part::Matched => &seq[a..b],
        Part::Trail3p => &seq[b..],
    }
}

// Operators that need the reference sequence (SAMFA, from --sam/FILE; a compile-time
// error without it) and those that need the query sequence (the fill is active).
const NEEDS_REFERENCE: &[&str] = &["ref_trail5p", "ref_matched", "ref_trail3p"];
const NEEDS_QUERY: &[&str] = &["qry_seq", "qry_qual", "qry_trail5p", "qry_matched", "qry_trail3p"];

pub fn needs_reference(op: &str) -> bool {
    NEEDS_REFERENCE.contains(&op)
}

pub fn needs_query(op: &str) -> bool {
    NEEDS_QUERY.contains(&op)
}

// The per-row SAM view, held by the machine; reset for every row. A stage is
// computed when an operator first asks for it (section 1). 'd: the dictionaries.
pub struct View<'d> {
    seqlen:  Option<&'d dict::Dict>,        // the seqlen dictionary, if any
    samfa:   Option<&'d dict::Dict>,        // the reference sequences (--sam/FILE), if any
    counts:  Option<Result<Counts, Na>>,
    reference: Option<Result<&'d [u8], Na>>,
    pub query: Query,                       // set for every row while the fill is active
    noted:   [bool; N_REASONS],             // this row has been counted for the reason
    pub n_na: [u64; N_REASONS],             // rows per reason, for the report at the end
}

impl<'d> View<'d> {
    pub fn new(dicts: &'d Dicts) -> View<'d> {
        View { seqlen: dicts.get(dict::SEQ_LENGTHS), samfa: dicts.get(dict::SAM_REFERENCE), counts: None,
               reference: None, query: Query::Row, noted: [false; N_REASONS], n_na: [0; N_REASONS] }
    }

    pub fn reset(&mut self) {
        self.counts = None;
        self.reference = None;
        self.noted = [false; N_REASONS];
    }

    // Count reason for this row (once).
    pub fn note(&mut self, reason: Na) {
        if !self.noted[reason.index()] {
            self.noted[reason.index()] = true;
            self.n_na[reason.index()] += 1;
        }
    }

    // Count the reason of an Err, and pass the result on.
    fn noted<T>(&mut self, result: Result<T, Na>) -> Result<T, Na> {
        if let Err(reason) = result {
            self.note(reason);
        }
        result
    }

    // The count stage, computed once per row.
    fn count_stage(&mut self, row: &Row) -> Result<&Counts, Na> {
        if self.counts.is_none() {
            self.counts = Some(compute_counts(row, self.seqlen));
        }
        match &self.counts {
            Some(Ok(c)) => Ok(c),
            Some(Err(reason)) => Err(*reason),
            None => unreachable!("counts computed above"),
        }
    }

    // The count stage; an Err is counted.
    pub fn counts(&mut self, row: &Row) -> Result<&Counts, Na> {
        if let Err(reason) = self.count_stage(row) {
            self.note(reason);
            return Err(reason);
        }
        self.count_stage(row)
    }

    // The reference stage: the value of RNAME in SAMFA, looked up once per row.
    // Unmapped records are not looked up (the count stage fails first).
    fn reference_stage(&mut self, row: &Row) -> Result<&'d [u8], Na> {
        self.count_stage(row)?;
        if self.reference.is_none() {
            let found = self.samfa.and_then(|d| d.map.get(row.field(RNAME)));
            self.reference = Some(found.map(|v| v.as_slice()).ok_or(Na::RefSequence));
        }
        self.reference.expect("reference looked up above")
    }

    // Whether an RNAME that is not in SAMFA ends the run (its not-found value is
    // __EXIT__, the default of --sam/FILE) rather than giving NA.
    pub fn reference_exits(&self) -> bool {
        self.samfa.is_some_and(|d| d.not_found == dict::NotFound::Exit)
    }

    // The query sequence: SEQ, or the filled one.
    fn query_seq<'a>(&'a self, row: &'a Row) -> Result<&'a [u8], Na> {
        match &self.query {
            Query::Row => Ok(row.field(SEQ)),
            Query::Filled { seq, .. } => Ok(seq),
            Query::Mismatch => Err(Na::SeqLength),
        }
    }

    // A part of the reference (decision 2 of plan/step18.txt): before position
    // ref_trail5p_N + 1, the matched part, the rest. An Err is counted.
    pub fn ref_part(&mut self, row: &Row, part: Part) -> Result<Vec<u8>, Na> {
        let result = self.ref_cut(row, part);
        self.noted(result)
    }

    fn ref_cut(&mut self, row: &Row, part: Part) -> Result<Vec<u8>, Na> {
        let c = self.count_stage(row)?;
        let (a, m) = (c.ref_trail5p, c.ref_matched);
        let reference = self.reference_stage(row)?;
        let a = usize::try_from(a).map_err(|_| Na::RefEnd)?;
        let b = usize::try_from(m).ok().and_then(|m| a.checked_add(m)).ok_or(Na::RefEnd)?;
        if b > reference.len() {
            return Err(Na::RefEnd);
        }
        Ok(cut(reference, a, b, part).to_vec())
    }

    // A part of the query sequence (SEQ orientation), cut at qry_trail5p_N and
    // qry_trail5p_N + qry_matched_N. An Err is counted.
    pub fn qry_part(&mut self, row: &Row, part: Part) -> Result<Vec<u8>, Na> {
        let result = self.qry_cut(row, part);
        self.noted(result)
    }

    fn qry_cut(&mut self, row: &Row, part: Part) -> Result<Vec<u8>, Na> {
        let c = self.count_stage(row)?;
        let (a, m, total) = (c.qry_trail5p as usize, c.qry_matched as usize, c.qry_len() as usize);
        let seq = self.query_seq(row)?;
        if seq.len() != total {
            return Err(Na::SeqLength);
        }
        Ok(cut(seq, a, a + m, part).to_vec())
    }

    // The end-of-run report (silenced by -q).
    pub fn report(&self) {
        for reason in REASONS {
            let n = self.n_na[reason.index()];
            if n > 0 {
                crate::note::note!("pluk: SAM values not available (NA) for {} row{}: {}", n,
                                   if n == 1 { "" } else { "s" }, reason.describe());
            }
        }
    }
}

pub fn flag(row: &Row) -> Option<u64> {
    number(row.field(FLAG))
}

pub fn is_unmapped(row: &Row) -> bool {
    flag(row).is_some_and(|f| f & FLAG_UNMAPPED != 0) || row.field(CIGAR) == b"*" || row.field(RNAME) == b"*"
}

pub fn is_forward(row: &Row) -> Option<bool> {
    flag(row).map(|f| f & FLAG_REVERSE == 0)
}

// The value of the first optional field TAG:TYPE:VALUE whose tag and type are tt
// (given as "XX:T:"), in the optional fields (column 12 on; with -O12 they are all
// in column 12, separated by TABs).
pub fn optional_field<'r>(row: &'r Row, tt: &[u8]) -> Option<&'r [u8]> {
    (OPTIONAL..row.n_fields())
        .flat_map(|i| row.field(i).split(|&b| b == b'\t'))
        .find(|f| f.starts_with(tt))
        .map(|f| &f[tt.len()..])
}

// NM:i, 0 without one (pick); None if present but not a non-negative integer.
pub fn nm(row: &Row) -> Option<u64> {
    match optional_field(row, b"NM:i:") {
        None => Some(0),
        Some(v) => number(v),
    }
}

fn compute_counts(row: &Row, seqlen: Option<&dict::Dict>) -> Result<Counts, Na> {
    if is_unmapped(row) {
        return Err(Na::Unmapped);
    }
    let ops = parse_cigar(row.field(CIGAR)).ok_or(Na::Cigar)?;
    let pos = number(row.field(POS)).filter(|&p| p > 0).ok_or(Na::Cigar)?;
    let (t5, m, t3) = trails(&ops);
    let cigar = row.field(CIGAR);
    let part = |r: std::ops::Range<usize>| -> Vec<u8> {
        if r.is_empty() { Vec::new() } else { cigar[ops[r.start].start..ops[r.end - 1].end].to_vec() }
    };
    let i = |x: u64| x as Int;
    let reflen = seqlen.and_then(|d| d.map.get(row.field(RNAME))).and_then(|v| number(v)).map(i);
    let ref_trail5p = i(pos - 1 + ref_len(&ops[t5.clone()]));
    let ref_matched = i(ref_len(&ops[m.clone()]));
    Ok(Counts {
        cgr_trail5p: part(t5.clone()),
        cgr_matched: part(m.clone()),
        cgr_trail3p: part(t3.clone()),
        ref_trail5p,
        ref_matched,
        ref_trail3p: reflen.map(|l| l - ref_trail5p - ref_matched),
        ref_trail5p_v: i(ref_len(&ops[t5.clone()])),
        ref_trail3p_v: i(ref_len(&ops[t3.clone()])),
        ref_len: reflen,
        qry_trail5p: i(qry_len(&ops[t5])),
        qry_matched: i(qry_len(&ops[m.clone()])),
        qry_trail3p: i(qry_len(&ops[t3])),
        aln_nmatch: i(sum_of(&ops[m.clone()], b"M=X")),
        matched_indel: i(sum_of(&ops[m], b"DI")),
    })
}

// The length of the first cigar operation if it is op (digits, then op), and the
// number of bytes it takes.
fn first_op(cigar: &[u8], op: u8) -> Option<(u64, usize)> {
    let digits = cigar.iter().take_while(|b| b.is_ascii_digit()).count();
    if digits == 0 || cigar.get(digits) != Some(&op) {
        return None;
    }
    let n = std::str::from_utf8(&cigar[..digits]).ok()?.parse().ok()?;
    Some((n, digits + 1))
}

// The number of bytes the last cigar operation takes if it is op.
fn last_op(cigar: &[u8], op: u8) -> Option<usize> {
    let (&last, rest) = cigar.split_last()?;
    let digits = rest.iter().rev().take_while(|b| b.is_ascii_digit()).count();
    (last == op && digits > 0).then_some(digits + 1)
}

// Normal form (SAMVIEW.design.txt section 3), as pick's sam_normalform: a leading
// nD is removed and POS increased by n, a trailing nD removed, a leading and a
// trailing nI become nS; each rule once, in that order, on the first or last
// operation only. A rule that would leave an empty CIGAR is not applied (pick
// writes an empty CIGAR for e.g. 5D). Returns the new POS and CIGAR, or None if
// nothing changes (or POS is not a number).
pub fn normal_form(pos: &[u8], cigar: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    let mut pos: u64 = std::str::from_utf8(pos).ok()?.parse().ok()?;
    let mut c = cigar.to_vec();
    let mut changed = false;
    if let Some((n, len)) = first_op(&c, b'D') {
        if len < c.len() {
            c.drain(..len);
            pos = pos.checked_add(n)?;
            changed = true;
        }
    }
    if let Some(len) = last_op(&c, b'D') {
        if len < c.len() {
            c.truncate(c.len() - len);
            changed = true;
        }
    }
    if let Some((_, len)) = first_op(&c, b'I') {
        c[len - 1] = b'S';
        changed = true;
    }
    if last_op(&c, b'I').is_some() {
        let last = c.len() - 1;
        c[last] = b'S';
        changed = true;
    }
    changed.then(|| (pos.to_string().into_bytes(), c))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nf(pos: &str, cigar: &str) -> Option<(String, String)> {
        normal_form(pos.as_bytes(), cigar.as_bytes())
            .map(|(p, c)| (String::from_utf8(p).unwrap(), String::from_utf8(c).unwrap()))
    }

    #[test]
    fn normal_form_rules() {
        let some = |p: &str, c: &str| Some((p.to_string(), c.to_string()));
        assert_eq!(nf("1", "2D3I10M2I1D"), some("3", "3S10M2S"));
        assert_eq!(nf("7", "3I2D10M"), some("7", "3S2D10M"));
        assert_eq!(nf("7", "12D10M"), some("19", "10M"));
        assert_eq!(nf("7", "10M3D"), some("7", "10M"));
        assert_eq!(nf("7", "10M3I"), some("7", "10M3S"));
        assert_eq!(nf("7", "2D2D10M"), some("9", "2D10M"));     // once only
        assert_eq!(nf("7", "5S3I10M"), None);                   // not behind S
        assert_eq!(nf("7", "5H3I10M"), None);
        assert_eq!(nf("7", "10M"), None);
        assert_eq!(nf("7", "*"), None);
        assert_eq!(nf("7", "5D"), None);                        // would leave nothing
        assert_eq!(nf("7", "4I"), some("7", "4S"));
        assert_eq!(nf("x", "2D10M"), None);
    }
}
