// Row: one input line and the positions of its fields.
//
// The fields are not copied. Field i is stored as the pair (start, end) of
// byte offsets into the line, and retrieved as the slice line[start..end].
// See plan/DATASTRUCTURE-cell.design.txt, point 2.
//
// Computed fields (plan/COMPUTE.design.txt section 7) are kept in a second
// buffer with their own offsets; field(i) serves the input fields first and the
// computed fields after them, so that output and filters treat both alike.

use std::cmp::Ordering;

pub struct Row {
    pub line:   Vec<u8>,                // the line, newline removed
    pub fields: Vec<(usize, usize)>,    // byte offsets of each field in line
    computed:   Vec<u8>,                // computed fields, one after the other
    computed_fields: Vec<(usize, usize)>,
}

impl Row {

    pub fn new() -> Row {
        Row { line: Vec::with_capacity(1024), fields: Vec::with_capacity(64),
              computed: Vec::with_capacity(256), computed_fields: Vec::with_capacity(16) }
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
    // Returns the number of fields found before padding (at most n), and Less, Equal
    // or Greater: the line had fewer, exactly, or more than n fields.
    pub fn split_fields_limit(&mut self, n: usize) -> (usize, Ordering) {
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
        let n_found = self.fields.len();

        let order = if n_found < n {
            Ordering::Less
        } else if self.line[start..].contains(&b'\t') {
            Ordering::Greater
        } else {
            Ordering::Equal
        };
        while self.fields.len() < n {
            self.fields.push((len, len));
        }
        (n_found, order)
    }

    pub fn n_fields(&self) -> usize {
        self.fields.len()
    }

    // Field i (0-based) as a byte slice: a pointer into self.line plus a length.
    // Indexes from n_fields() on are the computed fields; a computed field that was
    // not computed for this row (a row rejected by an @@ filter) is empty.
    pub fn field(&self, i: usize) -> &[u8] {
        match self.fields.get(i) {
            Some(&(start, end)) => &self.line[start..end],
            None => match self.computed_fields.get(i - self.fields.len()) {
                Some(&(start, end)) => &self.computed[start..end],
                None => &[],
            },
        }
    }

    // Replace input field i by value: the line is changed and the offsets of the
    // later fields (including padding fields at the end) are shifted, so that whole-
    // line output and every field see the new record (SAMVIEW.design.txt section 3).
    pub fn replace_field(&mut self, i: usize, value: &[u8]) {
        let (start, end) = self.fields[i];
        self.line.splice(start..end, value.iter().copied());
        let new_end = start + value.len();
        self.fields[i] = (start, new_end);
        for f in &mut self.fields[i + 1..] {
            *f = (f.0 + new_end - end, f.1 + new_end - end);
        }
    }

    // Remove the computed fields of the previous row (capacity is kept).
    pub fn clear_computed(&mut self) {
        self.computed.clear();
        self.computed_fields.clear();
    }

    // Append a computed field; it gets the next index after the input fields
    // and the fields computed before it.
    pub fn push_computed(&mut self, value: &[u8]) {
        let start = self.computed.len();
        self.computed.extend_from_slice(value);
        self.computed_fields.push((start, self.computed.len()));
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
        let (_, order) = row.split_fields_limit(n);
        ((0..row.n_fields()).map(|i| row.field(i).to_vec()).collect(), order)
    }

    #[test]
    fn replace_field() {
        let mut row = Row::new();
        row.line.extend_from_slice(b"a\tbb\tccc");
        row.split_fields_limit(5);
        row.replace_field(1, b"XXXX");
        assert_eq!(row.line, b"a\tXXXX\tccc");
        row.replace_field(0, b"");
        assert_eq!(row.line, b"\tXXXX\tccc");
        row.replace_field(2, b"z");
        let fields: Vec<&[u8]> = (0..5).map(|i| row.field(i)).collect();
        assert_eq!(fields, [&b""[..], b"XXXX", b"z", b"", b""]);
        assert_eq!(row.line, b"\tXXXX\tz");
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
    fn split_limit_found() {
        let mut row = Row::new();
        row.line.extend_from_slice(b"a\tb");
        assert_eq!(row.split_fields_limit(4), (2, Ordering::Less));
        row.line.extend_from_slice(b"\tc\td\te");
        assert_eq!(row.split_fields_limit(4), (4, Ordering::Greater));
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
