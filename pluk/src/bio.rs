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
    fn dna() {
        assert_eq!(todna(b"ACGTUacgtuNn"), b"ACGTTacgttNn");
        assert_eq!(todna(b"RYkmXz"), b"NNnnNn");
        assert_eq!(todna(b"A-C.G*T 1\xff"), b"ANCNGNTNNN");
    }
}
