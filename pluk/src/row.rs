// Row: one input line and the positions of its fields.
//
// The fields are not copied. Field i is stored as the pair (start, end) of
// byte offsets into the line, and retrieved as the slice line[start..end].
// See plan/DATASTRUCTURE-cell.design.txt, point 2.

use std::cmp::Ordering;

pub struct Row {
    pub line:   Vec<u8>,                // the line, newline removed
    pub fields: Vec<(usize, usize)>,    // byte offsets of each field in line
}

impl Row {

    pub fn new() -> Row {
        Row { line: Vec::with_capacity(1024), fields: Vec::with_capacity(64) }
    }

    // Find the fields in self.line, which are separated by TAB characters.
    // A line without TAB is a single field; an empty line is a single empty
    // field (as in pick). Adjacent TABs delimit an empty field.
    pub fn split_fields(&mut self) {
        self.fields.clear();                // keeps the allocated capacity
        let mut start = 0;
        for (i, &byte) in self.line.iter().enumerate() {
            if byte == b'\t' {
                self.fields.push((start, i));
                start = i + 1;
            }
        }
        self.fields.push((start, self.line.len()));
    }

    // As split_fields(), for ragged input (-O <n>, n >= 1): the result always has n fields.
    // Splitting stops after n-1 TABs, so field n runs to the end of the line and may
    // contain TABs (as Perl's split with a limit). Missing fields are added as empty
    // fields at the end of the line; the line itself is not changed.
    // Returns Less, Equal or Greater: the line had fewer, exactly, or more than n fields.
    pub fn split_fields_limit(&mut self, n: usize) -> Ordering {
        self.fields.clear();
        let len = self.line.len();
        let mut start = 0;
        for (i, &byte) in self.line.iter().enumerate() {
            if self.fields.len() == n - 1 {
                break;
            }
            if byte == b'\t' {
                self.fields.push((start, i));
                start = i + 1;
            }
        }
        self.fields.push((start, len));

        let order = if self.fields.len() < n {
            Ordering::Less
        } else if self.line[start..].contains(&b'\t') {
            Ordering::Greater
        } else {
            Ordering::Equal
        };
        while self.fields.len() < n {
            self.fields.push((len, len));
        }
        order
    }

    pub fn n_fields(&self) -> usize {
        self.fields.len()
    }

    // Field i (0-based) as a byte slice: a pointer into self.line plus a length.
    // Panics if i is out of range; callers check against n_fields().
    pub fn field(&self, i: usize) -> &[u8] {
        let (start, end) = self.fields[i];
        &self.line[start..end]
    }
}


// Unit tests, run with 'cargo test'. Compiled only for testing.
#[cfg(test)]
mod tests {
    use super::*;

    fn fields_of(line: &[u8]) -> Vec<Vec<u8>> {
        let mut row = Row::new();
        row.line.extend_from_slice(line);
        row.split_fields();
        (0..row.n_fields()).map(|i| row.field(i).to_vec()).collect()
    }

    fn fields_limit(line: &[u8], n: usize) -> (Vec<Vec<u8>>, Ordering) {
        let mut row = Row::new();
        row.line.extend_from_slice(line);
        let order = row.split_fields_limit(n);
        ((0..row.n_fields()).map(|i| row.field(i).to_vec()).collect(), order)
    }

    #[test]
    fn split_limit() {
        let v = |xs: &[&[u8]]| xs.iter().map(|x| x.to_vec()).collect::<Vec<_>>();
        assert_eq!(fields_limit(b"a\tb\tc",       3), (v(&[b"a", b"b", b"c"]),     Ordering::Equal));
        assert_eq!(fields_limit(b"a\tb\tc\td\te", 3), (v(&[b"a", b"b", b"c\td\te"]), Ordering::Greater));
        assert_eq!(fields_limit(b"a",             3), (v(&[b"a", b"", b""]),       Ordering::Less));
        assert_eq!(fields_limit(b"",              2), (v(&[b"", b""]),            Ordering::Less));
        assert_eq!(fields_limit(b"a\tb\t",        3), (v(&[b"a", b"b", b""]),      Ordering::Equal));
        assert_eq!(fields_limit(b"a\tb\t\t",      3), (v(&[b"a", b"b", b"\t"]),    Ordering::Greater));
        assert_eq!(fields_limit(b"a\tb",          1), (v(&[b"a\tb"]),             Ordering::Greater));
        assert_eq!(fields_limit(b"ab",            1), (v(&[b"ab"]),               Ordering::Equal));
    }

    #[test]
    fn split() {
        assert_eq!(fields_of(b"a\tbc\td"),  vec![b"a".to_vec(), b"bc".to_vec(), b"d".to_vec()]);
        assert_eq!(fields_of(b"abc"),       vec![b"abc".to_vec()]);
        assert_eq!(fields_of(b""),          vec![b"".to_vec()]);
        assert_eq!(fields_of(b"\t"),        vec![b"".to_vec(), b"".to_vec()]);
        assert_eq!(fields_of(b"a\t\tb\t"),  vec![b"a".to_vec(), b"".to_vec(), b"b".to_vec(), b"".to_vec()]);
    }
}
