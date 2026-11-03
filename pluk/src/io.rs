// Input: reading lines from STDIN.
//
// Input may be plain text or gzip-compressed; this is detected from the first
// two bytes. Lines are read into a single reusable byte buffer. See
// plan/step1-notes.txt for the rationale.

use std::cmp::Ordering;
use std::io::{self, BufRead, BufReader};

use flate2::bufread::MultiGzDecoder;

use crate::names::ColumnNames;
use crate::row::Row;

// Size of the read buffers. The standard library default is 8 KiB.
const BUFSIZE: usize = 128 * 1024;

// The first two bytes of every gzip (and bgzip) stream.
const GZIP_MAGIC: [u8; 2] = [0x1f, 0x8b];

// Settings that affect input processing, filled in from the command line by main.
// B_header keeps the name used in the plan; Rust convention would be b_header.
#[allow(non_snake_case)]
pub struct InputSettings {
    pub skip_comment_preamble: bool,    // --test-comment: skip leading lines starting with '#'
    pub B_header: bool,                 // first accepted line holds column names (no -k)
    pub omega: Option<usize>,           // -O <N>: ragged input, N columns
    pub add_row_column: bool,           // -R: header lacks the name of the first (row name) column
    pub make_unique: bool,              // -u: make duplicate column names unique
}

pub struct LineReader {
    reader:              Box<dyn BufRead>,
    pub settings:        InputSettings,
    in_preamble:         bool,      // no accepted line seen yet
    pending:             bool,      // self.row holds a data line not yet returned by next_line()
    split_order:         Ordering,  // with -O N: current line had fewer/exactly/more than N fields
    pub row:             Row,       // current line and its fields
    pub header:          Row,       // header line and column names, if B_header
    pub header_lineno:   u64,       // physical line number of the header; 0 if none
    pub is_empty:        bool,      // input has no accepted line at all
    pub n_columns:       usize,     // number of columns in the data; 0 for empty input
    pub row_column_added: bool,     // -R took effect: n_columns is one more than the header fields
    pub names:           ColumnNames, // column names and name -> index dictionary
    pub n_lines:         u64,       // number of lines read so far (physical line number)
    pub n_skipped:       u64,       // lines skipped
    pub n_accepted:      u64,       // data lines returned by next_line() (header excluded)
    pub n_short_rows:    u64,       // with -O N: data lines with fewer than N fields
    pub n_overflow_rows: u64,       // with -O N: data lines with more than N fields
}

impl LineReader {

    pub fn from_stdin(settings: InputSettings) -> io::Result<LineReader> {
        let stdin = io::stdin().lock();
        let mut raw = BufReader::with_capacity(BUFSIZE, stdin);

        // fill_buf() returns the buffered bytes without consuming them, so the
        // check does not lose any input. A single read may in principle return
        // fewer than two bytes; for a pipe or file at the start of input this
        // does not happen in practice.
        let is_gzip = raw.fill_buf()?.starts_with(&GZIP_MAGIC);

        let reader: Box<dyn BufRead> = if is_gzip {
            // MultiGzDecoder continues across concatenated gzip members,
            // which is what bgzip files consist of.
            let decoder = MultiGzDecoder::new(raw);
            Box::new(BufReader::with_capacity(BUFSIZE, decoder))
        } else {
            Box::new(raw)
        };

        let mut input = LineReader {
            reader,
            settings,
            in_preamble:     true,
            pending:         false,
            split_order:     Ordering::Equal,
            row:             Row::new(),
            header:          Row::new(),
            header_lineno:   0,
            is_empty:        false,
            n_columns:       0,
            row_column_added: false,
            names:           ColumnNames::numbered(0),
            n_lines:         0,
            n_skipped:       0,
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
    fn read_first(&mut self) -> io::Result<()> {
        if !self.accept_line()? {
            self.n_columns = self.settings.omega.unwrap_or(0);   // empty input
            self.names = ColumnNames::numbered(self.n_columns);
            self.is_empty = true;
            return Ok(());
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

    // Make the next data line available in self.row, split into fields.
    // Returns Ok(false) at end of input, and an error if the line does not
    // have the expected number of fields.
    pub fn next_line(&mut self) -> io::Result<bool> {
        if self.pending {
            self.pending = false;
        } else if !self.accept_line()? {
            return Ok(false);
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
        Ok(true)
    }

    // Read lines until one is accepted (not skipped), and split it into fields.
    // Returns Ok(false) at end of input.
    fn accept_line(&mut self) -> io::Result<bool> {
        while self.read_line()? {
            if self.in_preamble && self.settings.skip_comment_preamble && self.row.line.starts_with(b"#") {
                self.n_skipped += 1;
                continue;
            }
            self.in_preamble = false;
            match self.settings.omega {
                Some(n) => self.split_order = self.row.split_fields_limit(n),
                None    => self.row.split_fields(),
            }
            return Ok(true);
        }
        Ok(false)
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

