// SAM records: the input rewrites and (from step17 item 3) the per-row SAM view.
// Design: plan/SAMVIEW.design.txt.

// SAM columns, 0-based.
pub const POS: usize = 3;
pub const CIGAR: usize = 5;

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
