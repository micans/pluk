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

pub fn number(field: &[u8]) -> Option<u64> {
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
// context: the number of operations at each end added as context (plan/step19.txt
// decision 1); they belong to the trails whatever their letter (X).
pub fn trails(ops: &[Op], context: (usize, usize))
    -> (std::ops::Range<usize>, std::ops::Range<usize>, std::ops::Range<usize>)
{
    let n = ops.len();
    let mut t5 = context.0.min(n);
    t5 += ops[t5..].iter().take_while(|o| o.is_trail()).count();
    let mut m = n - context.1.min(n - t5);
    m -= ops[t5..m].iter().rev().take_while(|o| o.is_trail()).count();
    (0..t5, t5..m, m..n)
}

// Whether operation i of n is context (added at either end).
fn is_context(i: usize, n: usize, context: (usize, usize)) -> bool {
    i < context.0 || i + context.1 >= n
}

// ---- Context extension (plan/step19.txt decisions 2, 6) ----

// --sam-aln-context=N and --sam-aln-xy=X,Y.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ContextOptions {
    pub n:  u64,
    pub xy: Option<(u64, u64)>,
}

impl ContextOptions {
    // The budgets at the left and the right: per side the larger of N and what is needed
    // to include X..Y (never a reduction). ref_posy: the last reference position.
    fn budgets(&self, pos: u64, ref_posy: u64) -> (u64, u64) {
        let (mut left, mut right) = (self.n, self.n);
        if let Some((x, y)) = self.xy {
            if x < pos {
                left = left.max(pos - x);
            }
            if y > ref_posy {
                right = right.max(y - ref_posy);
            }
        }
        (left, right)
    }
}

// A record extended with context: the new POS and CIGAR, and the number of operations
// at each end that are context (with the hard and soft clips there).
#[derive(Clone, Debug, PartialEq)]
pub struct Extension {
    pub pos:     u64,
    pub cigar:   Vec<u8>,
    pub context: (usize, usize),
}

// The extension of a record, as pick's sam_adjust_view_extend: per side, first as many
// bases as the budget, the reference clip and the soft clip allow become shared context
// (the S becomes nX, POS moves left), then the rest of the budget goes to the side that
// still has bases: the reference (nD, POS moves left) or the query (nI, the rest stays
// nS). A soft clip behind a hard clip is used; H stays outermost. reflen: the reference
// length; without it the right side is not extended. Returns the extension (None if
// nothing changes) and whether the right side needed the unknown reference length.
pub fn extend(pos: u64, cigar: &[u8], reflen: Option<u64>, options: &ContextOptions) -> (Option<Extension>, bool) {
    let Some(ops) = parse_cigar(cigar) else { return (None, false) };
    if pos == 0 {
        return (None, false);
    }
    let n = ops.len();
    let refsum = ref_len(&ops);
    let (left, right) = options.budgets(pos, pos + refsum - 1);

    // The clips at each end: hard clips, then a soft clip; the middle is kept as written.
    let h5 = ops.iter().take_while(|o| o.op == b'H').count();
    let s5 = ops.get(h5).filter(|o| o.op == b'S').map_or(0, |o| o.len);
    let a = h5 + usize::from(s5 > 0);
    let h3 = ops[a..].iter().rev().take_while(|o| o.op == b'H').count();
    let s3 = (n - h3 > a).then(|| ops[n - h3 - 1]).filter(|o| o.op == b'S').map_or(0, |o| o.len);
    let b = n - h3 - usize::from(s3 > 0);

    let mut prefix: Vec<(u64, u8)> = ops[..h5].iter().map(|o| (o.len, o.op)).collect();
    let mut suffix: Vec<(u64, u8)> = Vec::new();
    let mut new_pos = pos;
    let mut changed = false;

    // Left: shared (X), then reference (D) or query (I, the rest S).
    let (x, d, i, rest) = side(pos - 1, s5, left);
    new_pos -= x + d;
    changed |= x + d + i > 0;
    for (len, op) in [(rest, b'S'), (i, b'I'), (d, b'D'), (x, b'X')] {
        if len > 0 {
            prefix.push((len, op));
        }
    }

    // Right: the reference after the alignment needs the reference length.
    let mut no_reflen = false;
    let (x, d, i, rest) = match reflen {
        Some(l) => side(l.saturating_sub(pos - 1 + refsum), s3, right),
        None => {
            no_reflen = right > 0;
            (0, 0, 0, s3)
        }
    };
    changed |= x + d + i > 0;
    for (len, op) in [(x, b'X'), (d, b'D'), (i, b'I'), (rest, b'S')] {
        if len > 0 {
            suffix.push((len, op));
        }
    }
    suffix.extend(ops[n - h3..].iter().map(|o| (o.len, o.op)));

    if !changed {
        return (None, no_reflen);
    }
    let mut new = Vec::new();
    for (len, op) in &prefix {
        new.extend_from_slice(format!("{}{}", len, *op as char).as_bytes());
    }
    if a < b {
        new.extend_from_slice(&cigar[ops[a].start..ops[b - 1].end]);
    }
    for (len, op) in &suffix {
        new.extend_from_slice(format!("{}{}", len, *op as char).as_bytes());
    }
    let context = (prefix.len(), suffix.len());
    (Some(Extension { pos: new_pos, cigar: new, context }), no_reflen)
}

// One side of the extension: the reference clip, the soft clip and the budget give the
// shared context x, then d (reference) or i (query), and the soft clip that is left.
fn side(ref_clip: u64, soft_clip: u64, budget: u64) -> (u64, u64, u64, u64) {
    let x = ref_clip.min(soft_clip).min(budget);
    let (ref_clip, soft_clip, budget) = (ref_clip - x, soft_clip - x, budget - x);
    let d = ref_clip.min(budget);
    let i = if d == 0 { soft_clip.min(budget) } else { 0 };
    (x, d, i, soft_clip - i)
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
const ALIGNMENT: [&str; 11] = ["aln_ref", "aln_aln", "aln_qry", "aln_rlr", "aln_qlt", "aln_all", "aln_allq", "aln_all_pack",
                               "aln_nedit_pia", "aln_nedit_piu", "aln_posinfo"];
const NEEDS_REFERENCE: &[&str] = &["ref_trail5p", "ref_matched", "ref_trail3p"];
const NEEDS_QUERY: &[&str] = &["qry_seq", "qry_qual", "qry_trail5p", "qry_matched", "qry_trail3p"];

pub fn needs_reference(op: &str) -> bool {
    NEEDS_REFERENCE.contains(&op) || ALIGNMENT.contains(&op)
}

pub fn needs_query(op: &str) -> bool {
    NEEDS_QUERY.contains(&op) || ALIGNMENT.contains(&op)
}

// ---- The alignment display (plan/step18.txt decisions 3-6) ----

// The options of the alignment display.
#[derive(Clone, Debug, Default)]
pub struct AlnOptions {
    pub prefix:  Vec<u8>,   // --sam-aln-prefix=STR, before every line of aln_all aln_allq aln_all_pack
    pub markers: bool,      // --sam-aln-mrk
    pub qlt_sim: bool,      // --sam-qlt-sim
}

// --sam-qlt-sim: the quality symbols ! .. ~ in steps of five quality values (as pick).
const QLT_SIM: &[u8; 94] =
    b".....,,,,,aaaaaAAAAAbbbbbBBBBBcccccCCCCCdddddDDDDDeeeeeEEEEEfffffFFFFFgggggGGGGGhhhhhHHHHHiiii";

pub fn qlt_sim(qual: &[u8]) -> Vec<u8> {
    qual.iter().map(|&b| if (b'!'..=b'~').contains(&b) { QLT_SIM[(b - b'!') as usize] } else { b }).collect()
}

// The five alignment strings.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Aln {
    pub reference: Vec<u8>,     // aln_ref
    pub aln:       Vec<u8>,     // aln_aln
    pub query:     Vec<u8>,     // aln_qry
    pub ruler:     Vec<u8>,     // aln_rlr
    pub quality:   Vec<u8>,     // aln_qlt
}

// One CIGAR operation placed on the reference and the query: the ranges it
// covers (empty for what it does not consume).
struct Span {
    op: u8,
    n:  usize,
    r:  std::ops::Range<usize>,
    q:  std::ops::Range<usize>,
}

// The operations of a CIGAR placed from POS on, as pick walks them in
// sam_compute_aln and getalnposx. Err: the CIGAR runs past the end of the reference
// (RefEnd) or the query (SeqLength).
fn spans(ops: &[Op], pos: u64, ref_len: usize, qry_len: usize) -> Result<Vec<Span>, Na> {
    let mut pr = usize::try_from(pos - 1).map_err(|_| Na::RefEnd)?;
    let mut pq = 0usize;
    let mut spans = Vec::with_capacity(ops.len());
    for op in ops {
        let n = usize::try_from(op.len).map_err(|_| Na::RefEnd)?;
        let r_end = if op.consumes_reference() { pr.checked_add(n).filter(|&e| e <= ref_len).ok_or(Na::RefEnd)? } else { pr };
        let q_end = if op.consumes_query() { pq.checked_add(n).filter(|&e| e <= qry_len).ok_or(Na::SeqLength)? } else { pq };
        spans.push(Span { op: op.op, n, r: pr..r_end, q: pq..q_end });
        (pr, pq) = (r_end, q_end);
    }
    Ok(spans)
}

fn t_for_u(b: u8) -> u8 {
    match b { b'u' => b't', b'U' => b'T', _ => b }
}

// The alignment strings from the reference, the query sequence and quality (SEQ
// orientation; QUAL '*' already replaced), the CIGAR and POS, as pick's
// sam_compute_aln. Err: the CIGAR runs past the end of the reference or the query.
// A context X (plan/step19.txt decision 3) shows reference and query with '.' between.
pub fn draw(reference: &[u8], seq: &[u8], qual: &[u8], ops: &[Op], pos: u64, context: (usize, usize))
    -> Result<Aln, Na>
{
    let reference: Vec<u8> = reference.iter().map(|&b| t_for_u(b)).collect();
    let seq: Vec<u8> = seq.iter().map(|&b| t_for_u(b)).collect();
    let mut a = Aln::default();
    for (k, Span { op, n, r, q }) in spans(ops, pos, reference.len(), seq.len())?.into_iter().enumerate() {
        let (pr, r_end, pq, q_end) = (r.start, r.end, q.start, q.end);
        match op {
            b'X' if is_context(k, ops.len(), context) => {
                a.aln.resize(a.aln.len() + n, b'.');
                a.reference.extend_from_slice(&reference[pr..r_end]);
                a.query.extend_from_slice(&seq[pq..q_end]);
                a.quality.extend_from_slice(&qual[pq..q_end]);
            }
            b'M' | b'=' | b'X' => {
                let (r, q) = (&reference[pr..r_end], &seq[pq..q_end]);
                a.aln.extend(r.iter().zip(q).map(|(x, y)| if x.eq_ignore_ascii_case(y) { b'|' } else { b'x' }));
                a.reference.extend_from_slice(r);
                a.query.extend_from_slice(q);
                a.quality.extend_from_slice(&qual[pq..q_end]);
            }
            b'I' => {
                a.reference.resize(a.reference.len() + n, b'-');
                a.aln.resize(a.aln.len() + n, b'~');
                a.query.extend_from_slice(&seq[pq..q_end]);
                a.quality.extend_from_slice(&qual[pq..q_end]);
            }
            b'D' | b'N' => {
                a.reference.extend_from_slice(&reference[pr..r_end]);
                a.aln.resize(a.aln.len() + n, if op == b'D' { b'-' } else { b'=' });
                a.query.resize(a.query.len() + n, b'-');
                a.quality.resize(a.quality.len() + n, b' ');
            }
            _ => {}     // S: the query advances, nothing drawn; H P: nothing
        }
    }
    mark_ends(&mut a.aln);
    a.ruler = ruler(&a.reference, pos);
    Ok(a)
}

// aln_posinfo (plan/step18.txt decision 8), as pick's getalnposx: the differences
// along the alignment in CIGAR order, joined by ':'. x=P,c=RQ for a mismatch at
// reference position P; i=P,n=L for an insertion before position P; d=P,n=L and
// e=P,n=L for a deletion and an N starting at P; the inserted or deleted sequence is
// added as ,s=SEQ when limit is 0 or L <= limit. Both sequences are upper-cased (U
// becomes T) first; M = X are compared alike. Context operations give no items.
pub fn posinfo(reference: &[u8], seq: &[u8], ops: &[Op], pos: u64, limit: u64, context: (usize, usize))
    -> Result<Vec<u8>, Na>
{
    let upper = |s: &[u8]| -> Vec<u8> { s.iter().map(|b| t_for_u(b.to_ascii_uppercase())).collect() };
    let (reference, seq) = (upper(reference), upper(seq));
    let mut items: Vec<String> = Vec::new();
    for (k, Span { op, n, r, q }) in spans(ops, pos, reference.len(), seq.len())?.into_iter().enumerate() {
        if is_context(k, ops.len(), context) {
            continue;
        }
        let p = r.start + 1;
        let shown = |s: &[u8]| -> String {
            if limit == 0 || n as u64 <= limit { format!(",s={}", String::from_utf8_lossy(s)) } else { String::new() }
        };
        match op {
            b'M' | b'=' | b'X' => {
                for (k, (x, y)) in reference[r.clone()].iter().zip(&seq[q]).enumerate() {
                    if x != y {
                        items.push(format!("x={},c={}{}", p + k, *x as char, *y as char));
                    }
                }
            }
            b'I' => items.push(format!("i={},n={}{}", p, n, shown(&seq[q]))),
            b'D' | b'N' => {
                let kind = if op == b'D' { 'd' } else { 'e' };
                items.push(format!("{}={},n={}{}", kind, p, n, shown(&reference[r])));
            }
            _ => {}     // S H P
        }
    }
    Ok(items.join(":").into_bytes())
}

// patchiness and patchiness2 (plan/step18.txt decision 9), pick's sliding-window
// scores over an alignment string s. s is read as runs of one character; each run
// has a penalty (below) and a position, and the result is the largest sum of the
// penalties of runs whose positions lie within window of the latest run's.
// patchiness: a run's position is its first column. patchiness2: a counter that
// advances by the length of every run except _ . ` ~ - (columns without a
// reference-and-query pair), taken after the run. ( and ) count as |, a space as x.
// None: a character without a penalty.
pub fn patchiness(s: &[u8], window: u64, columns_counted: bool) -> Option<u64> {
    fn penalty(b: u8) -> Option<u64> {
        match b {
            b'.' | b'|' | b'`' | b'_' | b'=' | b'(' | b')' => Some(0),
            b'+' | b'-' | b'~' => Some(1),
            b'x' | b' ' => Some(2),
            _ => None,
        }
    }
    let class = |b: u8| match b { b'(' | b')' => b'|', b' ' => b'x', _ => b };
    let mut runs: std::collections::VecDeque<(u64, u64)> = std::collections::VecDeque::new();
    let (mut score, mut best, mut counter) = (0u64, 0u64, 0u64);
    let mut i = 0;
    while i < s.len() {
        let c = class(s[i]);
        let len = s[i..].iter().take_while(|&&b| class(b) == c).count();
        let fine = penalty(c)?;
        let p = if columns_counted {
            if !b"_.`~-".contains(&c) {
                counter += len as u64;
            }
            counter
        } else {
            i as u64
        };
        runs.push_back((p, fine));
        score += fine;
        while let Some(&(first, out)) = runs.front() {
            if p - first <= window {
                break;
            }
            runs.pop_front();
            score -= out;
        }
        best = best.max(score);
        i += len;
    }
    Some(best)
}

// A leading run of ~ (insertion) becomes _, else a leading run of - (deletion)
// becomes `; the same at the end (pick).
fn mark_ends(aln: &mut [u8]) {
    for (from, to) in [(b'~', b'_'), (b'-', b'`')] {
        let n = aln.iter().take_while(|&&b| b == from).count();
        aln[..n].fill(to);
        if n > 0 {
            break;
        }
    }
    for (from, to) in [(b'~', b'_'), (b'-', b'`')] {
        let n = aln.iter().rev().take_while(|&&b| b == from).count();
        let len = aln.len();
        aln[len - n..].fill(to);
        if n > 0 {
            break;
        }
    }
}

// The ruler: column i is reference position POS + i minus the number of '-' in
// aln_ref before it; where that position is a multiple of 10 its number is written
// from that column. As long as aln_ref.
fn ruler(aln_ref: &[u8], pos: u64) -> Vec<u8> {
    let mut rlr = vec![b' '; aln_ref.len()];
    let mut skip = 0u64;
    for (i, &b) in aln_ref.iter().enumerate() {
        let n = pos + i as u64 - skip;
        if b == b'-' {
            skip += 1;
        } else if n % 10 == 0 {
            for (k, d) in n.to_string().bytes().enumerate() {
                if let Some(c) = rlr.get_mut(i + k) {
                    *c = d;
                }
            }
        }
    }
    rlr
}

// The per-row SAM view, held by the machine; reset for every row. A stage is
// computed when an operator first asks for it (section 1). 'd: the dictionaries.
pub struct View<'d> {
    seqlen:  Option<&'d dict::Dict>,        // the seqlen dictionary, if any
    samfa:   Option<&'d dict::Dict>,        // the reference sequences (--sam/FILE), if any
    counts:  Option<Result<Counts, Na>>,
    reference: Option<Result<&'d [u8], Na>>,
    aln:     Option<Result<Aln, Na>>,
    pub options: AlnOptions,                // set by main.rs
    pub context: (usize, usize),            // context operations at each end, set per row by main.rs
    pub query: Query,                       // set for every row while the fill is active
    noted:   [bool; N_REASONS],             // this row has been counted for the reason
    pub n_na: [u64; N_REASONS],             // rows per reason, for the report at the end
}

impl<'d> View<'d> {
    pub fn new(dicts: &'d Dicts) -> View<'d> {
        View { seqlen: dicts.get(dict::SEQ_LENGTHS), samfa: dicts.get(dict::SAM_REFERENCE), counts: None,
               reference: None, aln: None, options: AlnOptions::default(), context: (0, 0), query: Query::Row, noted: [false; N_REASONS], n_na: [0; N_REASONS] }
    }

    pub fn reset(&mut self) {
        self.counts = None;
        self.reference = None;
        self.aln = None;
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
            self.counts = Some(compute_counts(row, self.seqlen, self.context));
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

    // The query quality: QUAL, or the filled one.
    fn query_qual<'a>(&'a self, row: &'a Row) -> Result<&'a [u8], Na> {
        match &self.query {
            Query::Row => Ok(row.field(QUAL)),
            Query::Filled { qual, .. } => Ok(qual),
            Query::Mismatch => Err(Na::SeqLength),
        }
    }

    // qry_qual: QUAL or the filled one, translated with --sam-qlt-sim ('*' stays).
    pub fn qry_qual(&mut self, row: &Row) -> Result<Vec<u8>, Na> {
        let result = match self.query_qual(row) {
            Ok(q) if self.options.qlt_sim && q != b"*" => Ok(qlt_sim(q)),
            Ok(q) => Ok(q.to_vec()),
            Err(reason) => Err(reason),
        };
        self.noted(result)
    }

    // The alignment stage, computed once per row; an Err is counted.
    pub fn aln(&mut self, row: &Row) -> Result<&Aln, Na> {
        if self.aln.is_none() {
            let aln = self.compute_aln(row);
            self.aln = Some(aln);
        }
        if let Some(Err(reason)) = self.aln {
            self.note(reason);
            return Err(reason);
        }
        match &self.aln {
            Some(Ok(a)) => Ok(a),
            _ => unreachable!("alignment computed above"),
        }
    }

    fn compute_aln(&mut self, row: &Row) -> Result<Aln, Na> {
        let c = self.count_stage(row)?;
        let marks = (c.ref_trail5p, c.ref_matched, c.ref_trail3p, c.qry_trail5p, c.qry_matched, c.qry_trail3p);
        let reference = self.reference_stage(row)?;
        let ops = parse_cigar(row.field(CIGAR)).ok_or(Na::Cigar)?;
        let pos = number(row.field(POS)).ok_or(Na::Cigar)?;
        let seq = self.query_seq(row)?;
        let qual = self.query_qual(row)?;
        // QUAL '*' (decision 3): '!' for every base, '.' with --sam-qlt-sim.
        let qual = if qual == b"*" {
            vec![if self.options.qlt_sim { b'.' } else { b'!' }; seq.len()]
        } else if qual.len() != seq.len() {
            return Err(Na::SeqLength);
        } else if self.options.qlt_sim {
            qlt_sim(qual)
        } else {
            qual.to_vec()
        };
        let mut aln = draw(reference, seq, &qual, &ops, pos, self.context)?;
        // --sam-aln-mrk (decision 5): after the ruler, which keeps the unmarked length.
        if self.options.markers {
            let (r5, rm, r3, q5, qm, q3) = marks;
            let r3 = r3.ok_or(Na::RefLength)?;
            let mark = |t5: Int, m: Int, t3: Int| {
                format!("  {}{}  {}, {}, {}", if t5 > 0 { '<' } else { '[' }, if t3 > 0 { '>' } else { ']' }, t5, m, t3)
            };
            aln.reference.extend_from_slice(mark(r5, rm, r3).as_bytes());
            aln.query.extend_from_slice(mark(q5, qm, q3).as_bytes());
        }
        Ok(aln)
    }

    // aln_posinfo with the given limit (0: always show the sequence); an Err is counted.
    pub fn posinfo(&mut self, row: &Row, limit: u64) -> Result<Vec<u8>, Na> {
        let result = self.compute_posinfo(row, limit);
        self.noted(result)
    }

    fn compute_posinfo(&mut self, row: &Row, limit: u64) -> Result<Vec<u8>, Na> {
        self.count_stage(row)?;
        let reference = self.reference_stage(row)?;
        let ops = parse_cigar(row.field(CIGAR)).ok_or(Na::Cigar)?;
        let pos = number(row.field(POS)).ok_or(Na::Cigar)?;
        let seq = self.query_seq(row)?;
        posinfo(reference, seq, &ops, pos, limit, self.context)
    }

    // The reference length of RNAME (seqlen), for the extension.
    pub fn reference_length(&self, row: &Row) -> Option<u64> {
        self.seqlen.and_then(|d| d.map.get(row.field(RNAME))).and_then(|v| number(v))
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

fn compute_counts(row: &Row, seqlen: Option<&dict::Dict>, context: (usize, usize)) -> Result<Counts, Na> {
    if is_unmapped(row) {
        return Err(Na::Unmapped);
    }
    let ops = parse_cigar(row.field(CIGAR)).ok_or(Na::Cigar)?;
    let pos = number(row.field(POS)).filter(|&p| p > 0).ok_or(Na::Cigar)?;
    let (t5, m, t3) = trails(&ops, context);
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

    fn drawn(reference: &str, seq: &str, cigar: &str, pos: u64) -> Result<[String; 5], Na> {
        let ops = parse_cigar(cigar.as_bytes()).unwrap();
        let qual = vec![b'!'; seq.len()];
        let a = draw(reference.as_bytes(), seq.as_bytes(), &qual, &ops, pos, (0, 0))?;
        let s = |v: Vec<u8>| String::from_utf8(v).unwrap();
        Ok([s(a.ruler), s(a.reference), s(a.aln), s(a.query), s(a.quality)])
    }

    #[test]
    fn alignment_strings() {
        // Matches ignore case, X is compared, S is skipped, N drawn as =.
        let a = drawn("ACGTACGTACGT", "TTaCGtCX", "2S3M2N2X1S", 1).unwrap();
        assert_eq!(a[1], "ACGTACG");
        assert_eq!(a[2], "|||==xx");
        assert_eq!(a[3], "aCG--tC");
        assert_eq!(a[4], "!!!  !!");
        // U becomes T in both; leading and trailing runs of ~ and -.
        assert_eq!(drawn("ACGU", "AGGCGTTT", "2I4M2I", 1).unwrap()[2], "__x|||__");
        assert_eq!(drawn("ACGU", "ACGT", "4M", 1).unwrap()[2], "||||");
        assert_eq!(drawn("ACGTACGT", "CGTA", "1D4M1D", 1).unwrap()[2], "`||||`");
        // One class per end: a leading run of ~ hides a following run of -.
        assert_eq!(drawn("ACGTACGT", "GGCGT", "2I1D3M", 1).unwrap()[2], "__-|||");
        // Past the reference end, at a match and at a deletion.
        assert_eq!(drawn("ACGT", "CGTA", "4M", 2), Err(Na::RefEnd));
        assert_eq!(drawn("ACGT", "CG", "2M3D", 2), Err(Na::RefEnd));
        assert_eq!(drawn("ACGT", "CG", "3M", 2), Err(Na::SeqLength));
        // H and P draw nothing.
        assert_eq!(drawn("ACGTAC", "ACGTAC", "2H3M1P3M", 1).unwrap()[2], "||||||");
    }

    fn info(reference: &str, seq: &str, cigar: &str, pos: u64, limit: u64) -> Result<String, Na> {
        let ops = parse_cigar(cigar.as_bytes()).unwrap();
        posinfo(reference.as_bytes(), seq.as_bytes(), &ops, pos, limit, (0, 0)).map(|v| String::from_utf8(v).unwrap())
    }

    #[test]
    fn alignment_posinfo() {
        assert_eq!(info("ACGTACGTAC", "ACGTACGTAC", "10M", 1, 0).unwrap(), "");
        // Mismatches at reference positions, upper case; U is T; = and X compared as M.
        assert_eq!(info("ACGTACGU", "TCgTAAGT", "2=1X5M", 1, 0).unwrap(), "x=1,c=AT:x=6,c=CA");
        // Insertion before position P, deletion and N from P; s= up to the limit; S skipped.
        assert_eq!(info("ACGTACGTAC", "TTACGGGTAC", "2S3M2I3M", 1, 2).unwrap(), "i=4,n=2,s=GG");
        assert_eq!(info("ACGTACGTAC", "TTACGGGTAC", "2S3M2I3M", 1, 1).unwrap(), "i=4,n=2");
        assert_eq!(info("ACGTACGTAC", "CGGTA", "2M3D3M", 2, 0).unwrap(), "d=4,n=3,s=TAC");
        assert_eq!(info("ACGTACGTAC", "CGGTA", "2M3N3M", 2, 3).unwrap(), "e=4,n=3,s=TAC");
        assert_eq!(info("ACGTACGTAC", "CGGTA", "2M3N3M", 2, 2).unwrap(), "e=4,n=3");
        // H and P skipped; past the reference end.
        assert_eq!(info("ACGTAC", "ACGTAC", "2H3M1P3M", 1, 0).unwrap(), "");
        assert_eq!(info("ACGT", "CGTA", "4M", 2, 0), Err(Na::RefEnd));
    }

    #[test]
    fn patchiness_scores() {
        assert_eq!(patchiness(b"", 10, false), Some(0));
        assert_eq!(patchiness(b"||||||||", 10, false), Some(0));
        // One penalty per run: xx is 2, x|x is 4.
        assert_eq!(patchiness(b"||xx||", 10, false), Some(2));
        assert_eq!(patchiness(b"||x|x||", 10, false), Some(4));
        // Runs further apart than the window do not add up.
        assert_eq!(patchiness(b"x|||||x", 5, false), Some(2));
        assert_eq!(patchiness(b"x|||||x", 6, false), Some(4));
        // patchiness2 does not count - ~ _ ` . columns.
        assert_eq!(patchiness(b"x-----|x", 3, false), Some(3));
        assert_eq!(patchiness(b"x-----|x", 3, true), Some(5));
        // ( ) are |, a space is x; other characters have no penalty.
        assert_eq!(patchiness(b"(x) x", 10, false), Some(4));     // " x" is one run, as pick
        assert_eq!(patchiness(b"||a||", 10, false), None);
    }

    fn ext(pos: u64, cigar: &str, reflen: Option<u64>, n: u64, xy: Option<(u64, u64)>)
        -> (Option<(u64, String, (usize, usize))>, bool)
    {
        let (e, no_reflen) = extend(pos, cigar.as_bytes(), reflen, &ContextOptions { n, xy });
        (e.map(|e| (e.pos, String::from_utf8(e.cigar).unwrap(), e.context)), no_reflen)
    }

    #[test]
    fn context_extension() {
        let some = |p: u64, c: &str, k: (usize, usize)| (Some((p, c.to_string(), k)), false);
        // Shared context first, then reference (D) or query (I, the rest S); as pick.
        assert_eq!(ext(11, "3S10M4S", Some(40), 2, None), some(9, "1S2X10M2X2S", (2, 2)));
        assert_eq!(ext(11, "3S10M4S", Some(40), 10, None), some(1, "7D3X10M4X6D", (2, 2)));
        assert_eq!(ext(3, "5S10M", Some(40), 10, None), some(1, "3I2X10M10D", (2, 1)));
        // A soft clip behind a hard clip is used; H stays outermost.
        assert_eq!(ext(5, "2H3S10M1H", Some(40), 10, None), some(1, "2H1D3X10M10D1H", (3, 2)));
        // xy: only what is needed to include X..Y; the larger budget per side with N.
        assert_eq!(ext(11, "3S10M4S", Some(40), 0, Some((12, 18))), (None, false));
        assert_eq!(ext(3, "5S10M", Some(40), 0, Some((8, 30))), some(3, "5S10M18D", (1, 1)));
        assert_eq!(ext(3, "5S10M", Some(40), 1, Some((8, 30))), some(2, "4S1X10M18D", (2, 1)));
        // Unknown reference length: the right side is not extended, and that is reported.
        assert_eq!(ext(11, "3S10M4S", None, 2, None), (Some((9, "1S2X10M4S".to_string(), (2, 1))), true));
        assert_eq!(ext(1, "10M", None, 2, None), (None, true));
        // At the reference ends nothing can be added; the middle is kept as written.
        assert_eq!(ext(1, "010M", Some(10), 5, None), (None, false));
        assert_eq!(ext(2, "3S2I08M", Some(9), 5, None), some(1, "2I1X2I08M", (2, 0)));
    }

    #[test]
    fn trails_with_context() {
        let ops = parse_cigar(b"1S2X10M2X2S").unwrap();
        assert_eq!(trails(&ops, (0, 0)), (0..1, 1..4, 4..5));
        assert_eq!(trails(&ops, (2, 2)), (0..2, 2..3, 3..5));
        let ops = parse_cigar(b"2H1D3X3I10M").unwrap();
        assert_eq!(trails(&ops, (3, 0)), (0..4, 4..5, 5..5));
        // Context never makes the trails overlap.
        let ops = parse_cigar(b"2X3S").unwrap();
        assert_eq!(trails(&ops, (1, 2)), (0..2, 2..2, 2..2));
    }

    #[test]
    fn alignment_ruler() {
        assert_eq!(String::from_utf8(ruler(b"ACGTACGTACGTAC", 7)).unwrap(), "   10        2");
        // Columns of '-' in aln_ref do not advance the position; a number is cut at the end.
        assert_eq!(String::from_utf8(ruler(b"AC--GTACGTAC", 18)).unwrap(), "    20      ");
        assert_eq!(String::from_utf8(ruler(b"ACG", 98)).unwrap(), "  1");
    }

    #[test]
    fn quality_simplified() {
        assert_eq!(qlt_sim(b"!&+05?I~ "), b".,aAbcdi ".to_vec());
    }
}
