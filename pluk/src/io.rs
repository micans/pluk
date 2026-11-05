// Input: reading lines from STDIN.
//
// Input may be plain text or gzip-compressed; this is detected from the first
// two bytes. Lines are read into a single reusable byte buffer. See
// plan/step1-notes.txt for the rationale.

use std::cmp::Ordering;
use std::io::{self, BufRead, BufReader, Read};

use flate2::bufread::MultiGzDecoder;
use regex::bytes::{Regex, RegexBuilder};

use crate::names::ColumnNames;
use crate::row::Row;

// Size of the read buffers. The standard library default is 8 KiB.
const BUFSIZE: usize = 128 * 1024;

// The first two bytes of every gzip (and bgzip) stream.
const GZIP_MAGIC: [u8; 2] = [0x1f, 0x8b];

// SAM input (--sam, --sam-h): what to do with header lines, i.e. lines starting with '@'.
#[derive(Clone, Copy, PartialEq)]
pub enum SamMode {
    Off,            // not SAM input
    Skip,           // --sam: skip '@' lines
    PassThrough,    // --sam-h: output '@' lines unchanged
}

// The SO: (sort order) and GO: (grouping) tags of a SAM @HD line. Recorded for the
// fill of a missing SEQ, which assumes the rows of a read to be adjacent
// (plan/SAMSEQ.design.txt section 6). Without an @HD line, or without the tag, the
// spec defaults apply: SO:unknown, GO:none.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SamSort {
    Unknown,
    Unsorted,
    QueryName,
    Coordinate,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SamGroup {
    None,
    Query,
    Reference,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SamOrder {
    pub sort:  SamSort,
    pub group: SamGroup,
}

impl SamOrder {
    pub const DEFAULT: SamOrder = SamOrder { sort: SamSort::Unknown, group: SamGroup::None };

    // Whether the rows of a read can be assumed adjacent. Only coordinate order
    // and grouping by reference say otherwise. A sort order implies a grouping, so
    // SO decides when SO and GO disagree.
    pub fn grouped_by_read(&self) -> bool {
        match self.sort {
            SamSort::Coordinate => false,
            SamSort::QueryName  => true,
            SamSort::Unknown | SamSort::Unsorted => self.group != SamGroup::Reference,
        }
    }

    pub fn sort_name(&self) -> &'static str {
        match self.sort {
            SamSort::Unknown    => "unknown",
            SamSort::Unsorted   => "unsorted",
            SamSort::QueryName  => "queryname",
            SamSort::Coordinate => "coordinate",
        }
    }

    pub fn group_name(&self) -> &'static str {
        match self.group {
            SamGroup::None      => "none",
            SamGroup::Query     => "query",
            SamGroup::Reference => "reference",
        }
    }
}

// The kind of line made available by next_line().
#[derive(Clone, Copy, PartialEq)]
pub enum LineKind {
    Data,           // a data line, split into fields
    PassThrough,    // a line to be output unchanged (-//PAT, --sam-h); not split
}

// Settings that affect input processing, filled in from the command line by main.
// B_header keeps the name used in the plan; Rust convention would be b_header.
#[allow(non_snake_case)]
pub struct InputSettings {
    pub pat_purge: Option<Regex>,       // -/PAT: skip matching lines
    pub pat_pass: Option<Regex>,        // -//PAT: output matching lines unchanged
    pub B_header: bool,                 // first accepted line holds column names (no -k)
    pub omega: Option<usize>,           // -O <N>: ragged input, N columns
    pub add_row_column: bool,           // -R: header lacks the name of the first (row name) column
    pub make_unique: bool,              // -u: make duplicate column names unique
    pub sam: SamMode,                   // --sam, --sam-h
}

pub struct LineReader {
    reader:              Box<dyn BufRead>,
    pub settings:        InputSettings,
    pending:             bool,      // self.row holds a data line not yet returned by next_line()
    split_order:         Ordering,  // with -O N: current line had fewer/exactly/more than N fields
    pub row:             Row,       // current line and its fields
    pub header:          Row,       // header line and column names, if B_header
    pub header_lineno:   u64,       // physical line number of the header; 0 if none
    pub is_empty:        bool,      // input has no accepted line at all
    pub n_columns:       usize,     // number of columns in the data; 0 for empty input
    pub row_column_added: bool,     // -R took effect: n_columns is one more than the header fields
    pub names:           ColumnNames, // column names and name -> index dictionary
    pub passthrough_preamble: Vec<Vec<u8>>, // pass-through lines read before the first data line
    pub sam_seqlen:      Vec<(Vec<u8>, u64)>, // SAM @SQ lines before the first data line: name, length
    pub sam_order:       SamOrder,  // SAM @HD sort order and grouping (first line only)
    in_preamble:         bool,      // no data line read yet (read_first() has not finished)
    late_sq_warned:      bool,      // an @SQ line after the first data line has been reported
    pub n_lines:         u64,       // number of lines read so far (physical line number)
    pub n_skipped:       u64,       // lines skipped (including pass-through lines, as in pick)
    pub n_passthrough:   u64,       // lines passed through unchanged
    pub n_accepted:      u64,       // data lines returned by next_line() (header excluded)
    pub n_short_rows:    u64,       // with -O N: data lines with fewer than N fields
    pub n_overflow_rows: u64,       // with -O N: data lines with more than N fields
}

impl LineReader {

    pub fn from_stdin(settings: InputSettings) -> io::Result<LineReader> {
        let reader = open_buffered(io::stdin().lock())?;

        let mut input = LineReader {
            reader,
            settings,
            pending:         false,
            split_order:     Ordering::Equal,
            row:             Row::new(),
            header:          Row::new(),
            header_lineno:   0,
            is_empty:        false,
            n_columns:       0,
            row_column_added: false,
            names:           ColumnNames::numbered(0),
            passthrough_preamble: Vec::new(),
            sam_seqlen:      Vec::new(),
            sam_order:       SamOrder::DEFAULT,
            in_preamble:     true,
            late_sq_warned:  false,
            n_lines:         0,
            n_skipped:       0,
            n_passthrough:   0,
            n_accepted:      0,
            n_short_rows:    0,
            n_overflow_rows: 0,
        };
        input.read_first()?;
        Ok(input)
    }

    // Read the first accepted line and establish the number of columns.
    // With B_header the line is moved to self.header; otherwise it is kept
    // in self.row as the first data line.
    // Pass-through lines before it are stored in self.passthrough_preamble,
    // to be output first.
    fn read_first(&mut self) -> io::Result<()> {
        let result = self.read_first_line();
        self.in_preamble = false;
        result
    }

    fn read_first_line(&mut self) -> io::Result<()> {
        loop {
            match self.accept_line()? {
                Some(LineKind::Data)        => break,
                Some(LineKind::PassThrough) => self.passthrough_preamble.push(self.row.line.clone()),
                None => {
                    self.n_columns = self.settings.omega.unwrap_or(0);   // empty input
                    self.names = ColumnNames::numbered(self.n_columns);
                    self.is_empty = true;
                    return Ok(());
                }
            }
        }
        if self.settings.B_header {
            // Exchange the two Row structs; no bytes are copied. self.row
            // receives the empty buffers of self.header, reused for data lines.
            std::mem::swap(&mut self.header, &mut self.row);
            self.header_lineno = self.n_lines;
            self.n_columns = self.header.n_fields();
            // As in pick, -R has no effect if the first header field is already
            // empty; that field is the row name column.
            if self.settings.add_row_column && !self.header.field(0).is_empty() {
                self.n_columns += 1;
                self.row_column_added = true;
            }
            self.names = ColumnNames::from_header(&self.header, self.row_column_added, self.settings.make_unique)
                .map_err(|msg| io::Error::new(io::ErrorKind::InvalidData, msg))?;
        } else {
            self.n_columns = self.settings.omega.unwrap_or(self.row.n_fields());
            self.names = ColumnNames::numbered(self.n_columns);
            self.pending = true;
        }
        Ok(())
    }

    // Make the next line available in self.row: a data line, split into fields,
    // or a pass-through line. Returns Ok(None) at end of input, and an error if
    // a data line does not have the expected number of fields.
    pub fn next_line(&mut self) -> io::Result<Option<LineKind>> {
        if self.pending {
            self.pending = false;
        } else {
            match self.accept_line()? {
                Some(LineKind::Data)        => (),
                Some(LineKind::PassThrough) => return Ok(Some(LineKind::PassThrough)),
                None                        => return Ok(None),
            }
        }
        self.n_accepted += 1;
        match self.settings.omega {
            Some(_) => {
                // Always valid; the line has been padded or merged to N fields.
                match self.split_order {
                    Ordering::Less    => self.n_short_rows += 1,
                    Ordering::Greater => self.n_overflow_rows += 1,
                    Ordering::Equal   => (),
                }
            }
            None => {
                let nf = self.row.n_fields();
                if nf != self.n_columns {
                    let msg = format!("Header/{} column/{} count difference at line {} (consider -R or -O<N>)",
                                      self.n_columns, nf, self.n_lines);
                    return Err(io::Error::new(io::ErrorKind::InvalidData, msg));
                }
            }
        }
        Ok(Some(LineKind::Data))
    }

    // Read lines until one is accepted as a data line (and split it into fields)
    // or is to be passed through. Returns Ok(None) at end of input.
    fn accept_line(&mut self) -> io::Result<Option<LineKind>> {
        while self.read_line()? {
            if self.settings.sam != SamMode::Off && self.row.line.starts_with(b"@SQ\t") {
                self.note_sq_line();
            }
            // The SAM spec requires @HD, if present, to be the first line; a later
            // @HD line is ignored.
            if self.settings.sam != SamMode::Off && self.n_lines == 1 && self.row.line.starts_with(b"@HD\t") {
                self.note_hd_line();
            }
            // As in pick, every line (the header line included) is tested before it is
            // split: a line to pass through first, then a line to skip. Both count as
            // skipped. SAM header lines ('@', anywhere in the input) are recognised with
            // a prefix test, in addition to the user's patterns.
            let line = &self.row.line;
            let is_sam_header = self.settings.sam != SamMode::Off && line.starts_with(b"@");
            let pass = (is_sam_header && self.settings.sam == SamMode::PassThrough)
                || self.settings.pat_pass.as_ref().is_some_and(|re| re.is_match(line));
            let purge = (is_sam_header && self.settings.sam == SamMode::Skip)
                || self.settings.pat_purge.as_ref().is_some_and(|re| re.is_match(line));
            if pass {
                self.n_skipped += 1;
                self.n_passthrough += 1;
                return Ok(Some(LineKind::PassThrough));
            }
            if purge {
                self.n_skipped += 1;
                continue;
            }
            let mut n_found = 0;
            match self.settings.omega {
                Some(n) => (n_found, self.split_order) = self.row.split_fields_limit(n),
                None    => self.row.split_fields(),
            }
            if self.settings.sam != SamMode::Off && self.n_accepted < 5
                && self.split_order == Ordering::Less && !self.looks_like_sam(n_found)
            {
                eprintln!("Line {} does not look like SAM format [{}]",
                          self.n_lines, String::from_utf8_lossy(&self.row.line));
                self.n_skipped += 1;
                continue;
            }
            return Ok(Some(LineKind::Data));
        }
        Ok(None)
    }

    // A SAM @SQ line: record the sequence name (SN:) and length (LN:) for the
    // dictionary seqlen (plan/step4.txt item 4a). Only lines before the first data
    // line are used; SAM puts @SQ lines in the header. A malformed line, and a later
    // @SQ line (once), are reported and otherwise ignored.
    fn note_sq_line(&mut self) {
        if !self.in_preamble {
            if !self.late_sq_warned {
                eprintln!("pluk: @SQ line {} after the first alignment line not used for sequence lengths",
                          self.n_lines);
                self.late_sq_warned = true;
            }
            return;
        }
        match parse_sq(&self.row.line) {
            Some(entry) => self.sam_seqlen.push(entry),
            None => eprintln!("pluk: @SQ line {} without SN: or valid LN: field, ignored [{}]",
                              self.n_lines, String::from_utf8_lossy(&self.row.line)),
        }
    }

    // A SAM @HD line: record SO: and GO:. A value not in the spec is reported and
    // the default for the tag is kept.
    fn note_hd_line(&mut self) {
        let (order, unrecognised) = parse_hd(&self.row.line);
        self.sam_order = order;
        for field in unrecognised {
            eprintln!("pluk: @HD line {} has unrecognised {}, default used",
                      self.n_lines, String::from_utf8_lossy(field));
        }
    }

    // pick's check on the first 5 data lines of SAM input, applied to lines with
    // fewer than 12 fields: at least the 11 mandatory fields, and a numeric FLAG.
    fn looks_like_sam(&self, n_found: usize) -> bool {
        n_found >= 11 && looks_like_number(self.row.field(1))
    }

    // Read the next physical line into self.row.line. Returns Ok(false) at end of input.
    // A final line without a trailing newline is returned as a normal line.
    fn read_line(&mut self) -> io::Result<bool> {
        let line = &mut self.row.line;
        line.clear();                       // keeps the allocated capacity
        let n = self.reader.read_until(b'\n', line)?;
        if n == 0 {
            return Ok(false);
        }
        if line.last() == Some(&b'\n') {
            line.pop();
        }
        self.n_lines += 1;
        Ok(true)
    }
}


// A buffered reader for source (STDIN or a file), decompressing if the source
// starts with the gzip magic bytes. Used for the input and for dictionary files.
pub fn open_buffered<R: Read + 'static>(source: R) -> io::Result<Box<dyn BufRead>> {
    let mut raw = BufReader::with_capacity(BUFSIZE, source);

    // fill_buf() returns the buffered bytes without consuming them, so the
    // check does not lose any input. A single read may in principle return
    // fewer than two bytes; for a pipe or file at the start of input this
    // does not happen in practice.
    let is_gzip = raw.fill_buf()?.starts_with(&GZIP_MAGIC);

    if is_gzip {
        // MultiGzDecoder continues across concatenated gzip members,
        // which is what bgzip files consist of.
        let decoder = MultiGzDecoder::new(raw);
        Ok(Box::new(BufReader::with_capacity(BUFSIZE, decoder)))
    } else {
        Ok(Box::new(raw))
    }
}


// The SN: and LN: fields of an @SQ line, in any order. None if either is missing,
// or LN is not a number.
fn parse_sq(line: &[u8]) -> Option<(Vec<u8>, u64)> {
    let mut name = None;
    let mut length = None;
    for field in line.split(|&b| b == b'\t').skip(1) {
        if let Some(v) = field.strip_prefix(b"SN:") {
            name = Some(v.to_vec());
        } else if let Some(v) = field.strip_prefix(b"LN:") {
            length = std::str::from_utf8(v).ok().and_then(|s| s.parse::<u64>().ok());
        }
    }
    match (name, length) {
        (Some(n), Some(l)) if !n.is_empty() => Some((n, l)),
        _ => None,
    }
}


// The SO: and GO: fields of an @HD line, with the spec defaults for absent tags,
// and the fields whose value is not in the spec (their tag keeps the default).
// A repeated tag takes the last value.
fn parse_hd(line: &[u8]) -> (SamOrder, Vec<&[u8]>) {
    let mut order = SamOrder::DEFAULT;
    let mut unrecognised = Vec::new();
    for field in line.split(|&b| b == b'\t').skip(1) {
        if let Some(v) = field.strip_prefix(b"SO:") {
            order.sort = match v {
                b"unknown"    => SamSort::Unknown,
                b"unsorted"   => SamSort::Unsorted,
                b"queryname"  => SamSort::QueryName,
                b"coordinate" => SamSort::Coordinate,
                _ => { unrecognised.push(field); SamSort::Unknown }
            };
        } else if let Some(v) = field.strip_prefix(b"GO:") {
            order.group = match v {
                b"none"      => SamGroup::None,
                b"query"     => SamGroup::Query,
                b"reference" => SamGroup::Reference,
                _ => { unrecognised.push(field); SamGroup::None }
            };
        }
    }
    (order, unrecognised)
}


// Compile a -/PAT or -//PAT pattern (already %-decoded). Unicode mode is off, so
// that matching is on bytes as in Perl without 'use utf8': '.' matches any byte
// except newline, and \w, \d, \s and case-insensitive matching are ASCII-only.
// The regex crate takes the pattern as a str, so the pattern must be valid UTF-8.
pub fn compile_pattern(pat: &[u8], what: &str) -> Result<Regex, String> {
    let pat = std::str::from_utf8(pat)
        .map_err(|_| format!("{}: pattern is not valid UTF-8 (use \\xHH escapes for bytes)", what))?;
    RegexBuilder::new(pat)
        .unicode(false)
        .build()
        .map_err(|e| format!("{}: {}", what, e))
}

// Approximation of Perl's looks_like_number: the bytes, with surrounding white
// space removed, parse as a floating point number. This accepts integers,
// decimals, exponents, and inf/infinity/nan in any case, as Perl does.
fn looks_like_number(bytes: &[u8]) -> bool {
    match std::str::from_utf8(bytes) {
        Ok(s)  => s.trim().parse::<f64>().is_ok(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sq_fields() {
        assert_eq!(parse_sq(b"@SQ\tSN:chr1\tLN:1000"), Some((b"chr1".to_vec(), 1000)));
        assert_eq!(parse_sq(b"@SQ\tLN:5\tAS:x\tSN:y"), Some((b"y".to_vec(), 5)));
        assert_eq!(parse_sq(b"@SQ\tSN:x"), None);
        assert_eq!(parse_sq(b"@SQ\tSN:\tLN:5"), None);
        assert_eq!(parse_sq(b"@SQ\tSN:z\tLN:-1"), None);
    }

    #[test]
    fn hd_fields() {
        let order = |sort, group| SamOrder { sort, group };
        let none: Vec<&[u8]> = Vec::new();
        assert_eq!(parse_hd(b"@HD\tVN:1.6"), (SamOrder::DEFAULT, none.clone()));
        assert_eq!(parse_hd(b"@HD\tVN:1.6\tSO:coordinate"), (order(SamSort::Coordinate, SamGroup::None), none.clone()));
        assert_eq!(parse_hd(b"@HD\tGO:query\tVN:1.6"), (order(SamSort::Unknown, SamGroup::Query), none.clone()));
        assert_eq!(parse_hd(b"@HD\tSO:queryname\tSS:queryname:natural"), (order(SamSort::QueryName, SamGroup::None), none.clone()));
        assert_eq!(parse_hd(b"@HD\tSO:sorted\tGO:x"), (SamOrder::DEFAULT, vec![&b"SO:sorted"[..], &b"GO:x"[..]]));
        assert_eq!(parse_hd(b"@HD\tSO:unsorted\tSO:coordinate"), (order(SamSort::Coordinate, SamGroup::None), none.clone()));
    }

    #[test]
    fn grouped_by_read() {
        let grouped = |sort, group| SamOrder { sort, group }.grouped_by_read();
        assert!(grouped(SamSort::Unknown, SamGroup::None));
        assert!(grouped(SamSort::Unsorted, SamGroup::Query));
        assert!(grouped(SamSort::QueryName, SamGroup::Reference));
        assert!(!grouped(SamSort::Coordinate, SamGroup::None));
        assert!(!grouped(SamSort::Coordinate, SamGroup::Query));
        assert!(!grouped(SamSort::Unsorted, SamGroup::Reference));
        assert!(!grouped(SamSort::Unknown, SamGroup::Reference));
    }
}
