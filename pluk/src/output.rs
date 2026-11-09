// Output of the header line and of data rows.
//
// Fields are written directly from the line buffer of the Row; nothing is
// copied except into the output buffer (a BufWriter, see main.rs).
// See plan/step2-notes.txt, item 2, for the efficiency discussion.

use std::io::{self, Write};

use crate::names::{ColumnNames, ROW_NAME};
use crate::row::Row;

pub struct RowWriter {
    offsets:    Vec<usize>,     // 0-based indexes of the columns to output, in output order
    whole_line: bool,           // offsets are 0, 1, ..., n-1 and the line can be written as is
}

impl RowWriter {

    // 'ragged' is true with -O: a padded line lacks the TABs of the added empty
    // fields, so the line cannot be written as is.
    pub fn new(offsets: &[usize], n_columns: usize, ragged: bool) -> RowWriter {
        let all_in_order = offsets.len() == n_columns && offsets.iter().enumerate().all(|(k, &i)| k == i);
        RowWriter { offsets: offsets.to_vec(), whole_line: all_in_order && !ragged }
    }

    // Column names of the selected columns. The row name column '_' is written
    // as an empty name, as pick does.
    pub fn write_header(&self, out: &mut impl Write, names: &ColumnNames) -> io::Result<()> {
        for (k, &i) in self.offsets.iter().enumerate() {
            if k > 0 {
                out.write_all(b"\t")?;
            }
            let name = names.name(i);
            if name != ROW_NAME {
                out.write_all(name)?;
            }
        }
        out.write_all(b"\n")
    }

    pub fn write_row(&self, out: &mut impl Write, row: &Row) -> io::Result<()> {
        if self.whole_line {
            out.write_all(&row.line)?;
        } else {
            for (k, &i) in self.offsets.iter().enumerate() {
                if k > 0 {
                    out.write_all(b"\t")?;
                }
                out.write_all(row.field(i))?;
            }
        }
        out.write_all(b"\n")
    }
}


// A line output unchanged (pass-through lines, --sam-header).
pub fn write_line(out: &mut impl Write, line: &[u8]) -> io::Result<()> {
    out.write_all(line)?;
    out.write_all(b"\n")
}


// Temporary output used by the step1 tests (--test-lineno, --test-i): line number,
// TAB, then the line or, with --test-i, a single field.
pub fn write_numbered(out: &mut impl Write, lineno: u64, row: &Row, test_column: Option<usize>)
    -> io::Result<()>
{
    write!(out, "{}\t", lineno)?;
    match test_column {
        None    => out.write_all(&row.line)?,
        Some(i) => {
            // A column beyond the last field prints an empty field.
            if i < row.n_fields() {
                out.write_all(row.field(i))?;
            }
        }
    }
    out.write_all(b"\n")
}
