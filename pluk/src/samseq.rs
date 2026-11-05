// SAM: filling in a missing read sequence (SEQ '*') from an earlier row of the
// same read (plan/SAMSEQ.design.txt).
//
// Consecutive rows with the same QNAME form a group. GroupCache keeps the rows of
// the current group that store a sequence (the sources). A row with SEQ '*' (the
// target) is filled from the most recent source of the same segment and the same
// full read length whose piece of the read contains the piece the target needs.
//
// This module only computes; the caller decides what to do with each outcome
// (the missing-sequence mode, diagnostics, counters).

// FLAG bits used here.
const FLAG_PAIRED:  u16 = 0x1;
const FLAG_REVERSE: u16 = 0x10;
const FLAG_FIRST:   u16 = 0x40;
const FLAG_LAST:    u16 = 0x80;

// The segment of a row within its template (design section 2).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Segment {
    Single,         // 0x1 unset
    First,          // 0x1 set, 0x40 set, 0x80 unset
    Last,           // 0x1 set, 0x80 set, 0x40 unset
    Ambiguous,      // 0x1 set, 0x40 and 0x80 both set or both unset
}

pub fn segment(flag: u16) -> Segment {
    if flag & FLAG_PAIRED == 0 {
        return Segment::Single;
    }
    match (flag & FLAG_FIRST != 0, flag & FLAG_LAST != 0) {
        (true, false) => Segment::First,
        (false, true) => Segment::Last,
        _             => Segment::Ambiguous,
    }
}

// The lengths a CIGAR string gives (design section 3).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Clips {
    pub left_h:  usize,     // hard clip at the start of the CIGAR
    pub right_h: usize,     // hard clip at the end of the CIGAR
    pub q:       usize,     // sum of M, I, S, =, X: the length of SEQ when stored
}

impl Clips {
    // The full read length L.
    pub fn read_length(&self) -> usize {
        self.left_h + self.q + self.right_h
    }

    // The part of the original read that SEQ holds, as [start, end) in original
    // read coordinates. On the reverse strand the CIGAR start is the read end.
    pub fn piece(&self, reverse: bool) -> (usize, usize) {
        let l = self.read_length();
        if reverse {
            (self.right_h, l - self.left_h)
        } else {
            (self.left_h, l - self.right_h)
        }
    }
}

// Parse a CIGAR string. None for '*', an empty string, an operation without a
// length, an unknown operation, a length that does not fit, or H anywhere but
// at either end.
pub fn parse_cigar(cigar: &[u8]) -> Option<Clips> {
    let mut clips = Clips { left_h: 0, right_h: 0, q: 0 };
    let mut n: usize = 0;
    let mut have_digits = false;
    let mut n_ops = 0;           // operations read so far
    let mut seen_body = false;   // an operation other than H has been read
    let mut h_after_body = false;

    for &b in cigar {
        if b.is_ascii_digit() {
            n = n.checked_mul(10)?.checked_add((b - b'0') as usize)?;
            have_digits = true;
            continue;
        }
        if !have_digits {
            return None;
        }
        if h_after_body {
            return None;            // an operation after the trailing H
        }
        match b {
            b'H' => {
                if seen_body {
                    clips.right_h = n;
                    h_after_body = true;
                } else if n_ops == 0 {
                    clips.left_h = n;
                } else {
                    return None;    // two H operations at the start
                }
            }
            b'M' | b'I' | b'S' | b'=' | b'X' => {
                clips.q = clips.q.checked_add(n)?;
                seen_body = true;
            }
            b'D' | b'N' | b'P' => seen_body = true,
            _ => return None,
        }
        n_ops += 1;
        n = 0;
        have_digits = false;
    }
    if have_digits || n_ops == 0 {
        return None;                // trailing digits, or no operation ('*', empty)
    }
    Some(clips)
}

// Complement of a base, IUPAC codes included, case kept. Other bytes are
// returned unchanged.
fn complement(b: u8) -> u8 {
    match b {
        b'A' => b'T', b'T' => b'A', b'C' => b'G', b'G' => b'C', b'U' => b'A',
        b'R' => b'Y', b'Y' => b'R', b'K' => b'M', b'M' => b'K',
        b'B' => b'V', b'V' => b'B', b'D' => b'H', b'H' => b'D',
        b'a' => b't', b't' => b'a', b'c' => b'g', b'g' => b'c', b'u' => b'a',
        b'r' => b'y', b'y' => b'r', b'k' => b'm', b'm' => b'k',
        b'b' => b'v', b'v' => b'b', b'd' => b'h', b'h' => b'd',
        _ => b,                     // N, S, W and anything else
    }
}

pub fn reverse_complement(seq: &[u8]) -> Vec<u8> {
    seq.iter().rev().map(|&b| complement(b)).collect()
}

// SEQ and QUAL for a target that cannot be filled, in mode N: Q times 'N' and
// Q times '!' (Phred 0), design section 5.
pub fn unknown_sequence(q: usize) -> (Vec<u8>, Vec<u8>) {
    (vec![b'N'; q], vec![b'!'; q])
}

// A cached row that stores a sequence. SEQ and QUAL are kept as in the row
// (reference orientation); they are turned around only when a target is filled.
struct Source {
    segment: Segment,
    reverse: bool,
    l:       usize,             // full read length
    start:   usize,             // piece of the original read, [start, end)
    end:     usize,
    seq:     Vec<u8>,
    qual:    Option<Vec<u8>>,   // None for QUAL '*'
}

// The outcome for one row.
#[derive(PartialEq, Debug)]
pub enum Fill {
    Present,                // SEQ stored; cached as a source
    NotSource,              // SEQ stored; not a source (ambiguous segment, no usable CIGAR)
    LengthMismatch,         // SEQ stored, but its length differs from Q, or QUAL's from SEQ's
    Filled { seq: Vec<u8>, qual: Vec<u8> },  // QUAL is '*' if the source has QUAL '*'
    Ambiguous,              // SEQ '*', ambiguous segment: skip
    NoCigar,                // SEQ '*', CIGAR '*' or malformed: Q unknown, cannot be filled
    LengthConflict { q: usize },  // SEQ '*', a source of the segment has another L
    NotFound { q: usize },  // SEQ '*', no source contains the piece
}

pub struct GroupCache {
    qname:   Vec<u8>,
    sources: Vec<Source>,
}

impl GroupCache {
    pub fn new() -> GroupCache {
        GroupCache { qname: Vec::new(), sources: Vec::new() }
    }

    // Process one SAM data row: start a new group if QNAME differs from the
    // previous row, cache the row if it is a source, fill it if SEQ is '*'.
    // Filled rows are not cached: their piece is part of an existing source.
    pub fn process(&mut self, qname: &[u8], flag: u16, cigar: &[u8], seq: &[u8], qual: &[u8]) -> Fill {
        if qname != self.qname.as_slice() {
            self.qname.clear();
            self.qname.extend_from_slice(qname);
            self.sources.clear();
        }
        let segment = segment(flag);
        let reverse = flag & FLAG_REVERSE != 0;
        let clips = parse_cigar(cigar);

        if seq != b"*" {
            return self.add_source(segment, reverse, clips, seq, qual);
        }
        if segment == Segment::Ambiguous {
            return Fill::Ambiguous;
        }
        let Some(clips) = clips else {
            return Fill::NoCigar;
        };
        let l = clips.read_length();
        let (start, end) = clips.piece(reverse);
        let mut other_length = false;

        for s in self.sources.iter().rev() {
            if s.segment != segment {
                continue;
            }
            if s.l != l {
                other_length = true;
                continue;
            }
            if s.start <= start && end <= s.end {
                return fill_from(s, start, end, reverse);
            }
        }
        if other_length {
            Fill::LengthConflict { q: clips.q }
        } else {
            Fill::NotFound { q: clips.q }
        }
    }

    fn add_source(&mut self, segment: Segment, reverse: bool, clips: Option<Clips>,
                  seq: &[u8], qual: &[u8]) -> Fill {
        let Some(clips) = clips else {
            return Fill::NotSource;
        };
        if seq.len() != clips.q || (qual != b"*" && qual.len() != seq.len()) {
            return Fill::LengthMismatch;
        }
        if segment == Segment::Ambiguous {
            return Fill::NotSource;
        }
        let (start, end) = clips.piece(reverse);
        self.sources.push(Source {
            segment,
            reverse,
            l: clips.read_length(),
            start,
            end,
            seq: seq.to_vec(),
            qual: if qual == b"*" { None } else { Some(qual.to_vec()) },
        });
        Fill::Present
    }
}

// Cut the piece [start, end) from source s and put it in the target's orientation:
// SEQ to the original orientation (reverse complement if the source is reverse),
// slice, then reverse complement if the target is reverse. QUAL likewise, reversed.
fn fill_from(s: &Source, start: usize, end: usize, reverse: bool) -> Fill {
    let from = start - s.start;
    let to = end - s.start;

    let original = if s.reverse { reverse_complement(&s.seq) } else { s.seq.clone() };
    let piece = &original[from..to];
    let seq = if reverse { reverse_complement(piece) } else { piece.to_vec() };

    let qual = match &s.qual {
        None => b"*".to_vec(),
        Some(q) => {
            let mut original = q.clone();
            if s.reverse {
                original.reverse();
            }
            let mut piece = original[from..to].to_vec();
            if reverse {
                piece.reverse();
            }
            piece
        }
    };
    Fill::Filled { seq, qual }
}


#[cfg(test)]
mod tests {
    use super::*;

    const REV: u16 = 0x10;
    const SEC: u16 = 0x100;
    const SUP: u16 = 0x800;
    const R1:  u16 = 0x1 | 0x40;
    const R2:  u16 = 0x1 | 0x80;

    fn filled(seq: &str, qual: &str) -> Fill {
        Fill::Filled { seq: seq.as_bytes().to_vec(), qual: qual.as_bytes().to_vec() }
    }

    #[test]
    fn segments() {
        assert_eq!(segment(0), Segment::Single);
        assert_eq!(segment(0x40), Segment::Single);
        assert_eq!(segment(R1 | REV), Segment::First);
        assert_eq!(segment(R2), Segment::Last);
        assert_eq!(segment(0x1), Segment::Ambiguous);
        assert_eq!(segment(0x1 | 0x40 | 0x80), Segment::Ambiguous);
    }

    #[test]
    fn cigars() {
        let c = |l, q, r| Some(Clips { left_h: l, right_h: r, q });
        assert_eq!(parse_cigar(b"8M"), c(0, 8, 0));
        assert_eq!(parse_cigar(b"3S6M"), c(0, 9, 0));
        assert_eq!(parse_cigar(b"5H3M2I4D1M10N2=1X3S2H"), c(5, 12, 2));
        assert_eq!(parse_cigar(b"100H50M"), c(100, 50, 0));
        assert_eq!(parse_cigar(b"*"), None);
        assert_eq!(parse_cigar(b""), None);
        assert_eq!(parse_cigar(b"M"), None);
        assert_eq!(parse_cigar(b"5M3"), None);
        assert_eq!(parse_cigar(b"5Q"), None);
        assert_eq!(parse_cigar(b"5M2H3M"), None);
        assert_eq!(parse_cigar(b"2H2H5M"), None);
        assert_eq!(parse_cigar(b"5M2H2H"), None);
        assert_eq!(parse_cigar(b"99999999999999999999M"), None);
    }

    #[test]
    fn pieces() {
        let c = parse_cigar(b"3H5M2H").unwrap();
        assert_eq!(c.read_length(), 10);
        assert_eq!(c.piece(false), (3, 8));
        assert_eq!(c.piece(true), (2, 7));
    }

    #[test]
    fn revcomp() {
        assert_eq!(reverse_complement(b"AACGTN"), b"NACGTT");
        assert_eq!(reverse_complement(b"acgRYkm"), b"kmRYcgt");
        assert_eq!(unknown_sequence(3), (b"NNN".to_vec(), b"!!!".to_vec()));
    }

    #[test]
    fn same_piece() {
        let mut g = GroupCache::new();
        assert_eq!(g.process(b"r", 0, b"4M", b"ACGT", b"ABCD"), Fill::Present);
        assert_eq!(g.process(b"r", SEC, b"4M", b"*", b"*"), filled("ACGT", "ABCD"));
        assert_eq!(g.process(b"r", SEC | REV, b"4M", b"*", b"*"), filled("ACGT", "DCBA"));
    }

    #[test]
    fn strand() {
        // Read AACCGGTTTA stored reverse: SEQ = TAAACCGGTT, QUAL reversed.
        let mut g = GroupCache::new();
        assert_eq!(g.process(b"r", REV, b"10M", b"TAAACCGGTT", b"9876543210"), Fill::Present);
        assert_eq!(g.process(b"r", SEC, b"10M", b"*", b"*"), filled("AACCGGTTTA", "0123456789"));
        // Forward target, 3 bases hard-clipped at the start: piece [3,10) = CGGTTTA.
        assert_eq!(g.process(b"r", SUP, b"3H7M", b"*", b"*"), filled("CGGTTTA", "3456789"));
        // Reverse target 3H7M: piece [0,7) = AACCGGT, reverse complemented.
        assert_eq!(g.process(b"r", SUP | REV, b"3H7M", b"*", b"*"), filled("ACCGGTT", "6543210"));
    }

    #[test]
    fn slice_from_unclipped_primary() {
        // L = 150 as in the design file: primary 100M50S, B 100H50M takes [100,150).
        let read: String = "ACGTTGCA".repeat(19)[..150].to_string();
        let mut g = GroupCache::new();
        assert_eq!(g.process(b"r", 0, b"100M50S", read.as_bytes(), b"*"), Fill::Present);
        assert_eq!(g.process(b"r", SUP, b"100H50M", b"*", b"*"), filled(&read[100..], "*"));
    }

    #[test]
    fn source_is_supplementary() {
        // Primary 6M4H holds [0,6) of AACCGGTTTA; supplementary A 6H4M holds [6,10);
        // a later supplementary with SEQ '*' and the same clips takes it from A.
        let mut g = GroupCache::new();
        assert_eq!(g.process(b"r", 0, b"6M4H", b"AACCGG", b"IIIIII"), Fill::Present);
        assert_eq!(g.process(b"r", SUP, b"6H4M", b"TTTA", b"abcd"), Fill::Present);
        assert_eq!(g.process(b"r", SUP | SEC, b"6H4M", b"*", b"*"), filled("TTTA", "abcd"));
        // A soft-clipped target needs the whole read: no source contains it.
        assert_eq!(g.process(b"r", SEC, b"6S4M", b"*", b"*"), Fill::NotFound { q: 10 });
        // A target within the primary's piece takes it from the primary.
        assert_eq!(g.process(b"r", SEC, b"2H4M4H", b"*", b"*"), filled("CCGG", "IIII"));
    }

    #[test]
    fn most_recent_source() {
        let mut g = GroupCache::new();
        assert_eq!(g.process(b"r", 0, b"4M", b"ACGT", b"*"), Fill::Present);
        assert_eq!(g.process(b"r", SUP, b"4M", b"TTTT", b"*"), Fill::Present);
        assert_eq!(g.process(b"r", SEC, b"4M", b"*", b"*"), filled("TTTT", "*"));
    }

    #[test]
    fn new_group() {
        let mut g = GroupCache::new();
        assert_eq!(g.process(b"r1", 0, b"4M", b"ACGT", b"*"), Fill::Present);
        assert_eq!(g.process(b"r2", SEC, b"4M", b"*", b"*"), Fill::NotFound { q: 4 });
        assert_eq!(g.process(b"r1", SEC, b"4M", b"*", b"*"), Fill::NotFound { q: 4 });
    }

    #[test]
    fn length_conflict() {
        let mut g = GroupCache::new();
        assert_eq!(g.process(b"r", 0, b"4M", b"ACGT", b"*"), Fill::Present);
        assert_eq!(g.process(b"r", SEC, b"5M", b"*", b"*"), Fill::LengthConflict { q: 5 });
        assert_eq!(g.process(b"r", SEC, b"1H4M", b"*", b"*"), Fill::LengthConflict { q: 4 });
    }

    #[test]
    fn invalid_sources() {
        let mut g = GroupCache::new();
        assert_eq!(g.process(b"r", 0, b"5M", b"ACGT", b"*"), Fill::LengthMismatch);
        assert_eq!(g.process(b"r", 0, b"4M", b"ACGT", b"II"), Fill::LengthMismatch);
        assert_eq!(g.process(b"r", 4, b"*", b"ACGT", b"*"), Fill::NotSource);
        assert_eq!(g.process(b"r", SEC, b"4M", b"*", b"*"), Fill::NotFound { q: 4 });
        assert_eq!(g.process(b"r", SEC, b"*", b"*", b"*"), Fill::NoCigar);
    }

    #[test]
    fn paired_segments() {
        // Interleaved rows of read1 and read2; each is filled from its own segment.
        let mut g = GroupCache::new();
        assert_eq!(g.process(b"t", R1, b"4M", b"AAAC", b"1111"), Fill::Present);
        assert_eq!(g.process(b"t", R2 | REV, b"4M", b"GGGT", b"2222"), Fill::Present);
        assert_eq!(g.process(b"t", R1 | SEC, b"4M", b"*", b"*"), filled("AAAC", "1111"));
        assert_eq!(g.process(b"t", R2 | SEC, b"4M", b"*", b"*"), filled("ACCC", "2222"));
        assert_eq!(g.process(b"t", R1 | SUP, b"2H2M", b"*", b"*"), filled("AC", "11"));
        // An unpaired row with the same QNAME has no source.
        assert_eq!(g.process(b"t", SEC, b"4M", b"*", b"*"), Fill::NotFound { q: 4 });
    }

    #[test]
    fn ambiguous_segments() {
        let mut g = GroupCache::new();
        let both = 0x1 | 0x40 | 0x80;
        assert_eq!(g.process(b"t", both, b"4M", b"ACGT", b"*"), Fill::NotSource);
        assert_eq!(g.process(b"t", 0x1, b"4M", b"*", b"*"), Fill::Ambiguous);
        assert_eq!(g.process(b"t", both | SEC, b"4M", b"*", b"*"), Fill::Ambiguous);
        assert_eq!(g.process(b"t", R1 | SEC, b"4M", b"*", b"*"), Fill::NotFound { q: 4 });
    }
}
