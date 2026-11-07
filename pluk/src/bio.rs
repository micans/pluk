// Sequence operations (plan/step21.txt): the complement shared by the bio operators
// and the SAM sequence fill (samseq.rs), and the conversions of the bio operators.

// Complement of a base, IUPAC codes included, case kept; U becomes A. Other bytes
// (N, S, W, gaps, anything else) are returned unchanged.
pub fn complement(b: u8) -> u8 {
    match b {
        b'A' => b'T', b'T' => b'A', b'C' => b'G', b'G' => b'C', b'U' => b'A',
        b'R' => b'Y', b'Y' => b'R', b'K' => b'M', b'M' => b'K',
        b'B' => b'V', b'V' => b'B', b'D' => b'H', b'H' => b'D',
        b'a' => b't', b't' => b'a', b'c' => b'g', b'g' => b'c', b'u' => b'a',
        b'r' => b'y', b'y' => b'r', b'k' => b'm', b'm' => b'k',
        b'b' => b'v', b'v' => b'b', b'd' => b'h', b'h' => b'd',
        _ => b,
    }
}

pub fn reverse_complement(seq: &[u8]) -> Vec<u8> {
    seq.iter().rev().map(|&b| complement(b)).collect()
}

// todna (pick): u U become t T, other letters n or N (case kept), and every byte
// that is not one of acgntACGNT becomes N.
pub fn todna(seq: &[u8]) -> Vec<u8> {
    seq.iter()
        .map(|&b| match b {
            b'u' => b't',
            b'U' => b'T',
            b'a' | b'c' | b'g' | b'n' | b't' | b'A' | b'C' | b'G' | b'N' | b'T' => b,
            b'a'..=b'z' => b'n',
            _ => b'N',
        })
        .collect()
}

// The standard genetic code (NCBI table 1, pick's table), codons in the order TTT TTC
// TTA TTG TCT .. GGG (bases T C A G); '*' for the stop codons TAA TAG TGA.
const CODE: &[u8; 64] = b"FFLLSSSSYY**CC*WLLLLPPPPHHQQRRRRIIIMTTTTNNKKSSRRVVVVAAAADDEEGGGG";

fn base_index(b: u8) -> Option<usize> {
    match b {
        b'T' => Some(0),
        b'C' => Some(1),
        b'A' => Some(2),
        b'G' => Some(3),
        _ => None,
    }
}

// dnatl (pick's dna_tl, U read as T): the translation of seq from offset frame; a
// codon not in the table gives '?', the 1 or 2 bases left at the end are added as
// {..}; a seq no longer than frame gives the empty string.
pub fn translate(seq: &[u8], frame: usize) -> Vec<u8> {
    if seq.len() <= frame {
        return Vec::new();
    }
    let bases: Vec<u8> = seq[frame..].iter().map(|b| match b.to_ascii_uppercase() { b'U' => b'T', c => c }).collect();
    let mut out: Vec<u8> = bases.chunks_exact(3)
        .map(|c| match (base_index(c[0]), base_index(c[1]), base_index(c[2])) {
            (Some(i), Some(j), Some(k)) => CODE[16 * i + 4 * j + k],
            _ => b'?',
        })
        .collect();
    let rest = &bases[bases.len() / 3 * 3..];
    if !rest.is_empty() {
        out.push(b'{');
        out.extend_from_slice(rest);
        out.push(b'}');
    }
    out
}

// dnagc: the number of G C and of A C G T U bases (either case) in seq.
pub fn gc_counts(seq: &[u8]) -> (usize, usize) {
    let gc = seq.iter().filter(|b| matches!(b.to_ascii_uppercase(), b'G' | b'C')).count();
    let all = seq.iter().filter(|b| matches!(b.to_ascii_uppercase(), b'A' | b'C' | b'G' | b'T' | b'U')).count();
    (gc, all)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revcomp() {
        assert_eq!(reverse_complement(b"AACGTN"), b"NACGTT");
        assert_eq!(reverse_complement(b"acgRYkm"), b"kmRYcgt");
        assert_eq!(reverse_complement(b"UuSWswBDHVbdhv-.*"), b"*.-bdhvBDHVwsWSaA");
        assert_eq!(reverse_complement(b""), b"");
    }

    #[test]
    fn translation() {
        assert_eq!(translate(b"ATGGCCTAA", 0), b"MA*");
        assert_eq!(translate(b"atggcctaa", 0), b"MA*");
        assert_eq!(translate(b"AUGUUU", 0), b"MF");
        assert_eq!(translate(b"XATGGC", 1), b"M{GC}");
        assert_eq!(translate(b"ATGNNNTGA", 0), b"M?*");
        assert_eq!(translate(b"ATG", 3), b"");
        assert_eq!(translate(b"AT", 0), b"{AT}");
        // Every codon of the table.
        let all: Vec<u8> = b"TCAG".iter().flat_map(|&a| b"TCAG".iter().flat_map(move |&b| b"TCAG".iter().map(move |&c| [a, b, c])))
            .flatten().collect();
        assert_eq!(translate(&all, 0), CODE.to_vec());
    }

    #[test]
    fn gc() {
        assert_eq!(gc_counts(b"ACGTacgt"), (4, 8));
        assert_eq!(gc_counts(b"GGNN--u"), (2, 3));
        assert_eq!(gc_counts(b"NNN"), (0, 0));
    }

    #[test]
    fn dna() {
        assert_eq!(todna(b"ACGTUacgtuNn"), b"ACGTTacgttNn");
        assert_eq!(todna(b"RYkmXz"), b"NNnnNn");
        assert_eq!(todna(b"A-C.G*T 1\xff"), b"ANCNGNTNNN");
    }
}
