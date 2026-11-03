mod io;
mod names;
mod opt;
mod row;
mod select;

use std::io::{BufWriter, ErrorKind, Write};
use std::os::unix::ffi::OsStringExt;
use row::Row;
use std::process;

// Output buffer size. Rust's stdout flushes at every newline by default;
// wrapping it in a BufWriter avoids a write system call per line.
const OUT_BUFSIZE: usize = 128 * 1024;

fn run() -> std::io::Result<()> {
    let config = opt::parse_args();

    let settings = io::InputSettings {
        skip_comment_preamble: config.test_comment,
        B_header:              !config.k,
        omega:                 config.omega.map(|n| n as usize),
        add_row_column:        config.add_row_column,
        make_unique:           config.make_unique,
    };
    let mut input = io::LineReader::from_stdin(settings)?;
    let mut out = BufWriter::with_capacity(OUT_BUFSIZE, std::io::stdout().lock());

    // --test-i N is 1-based on the command line, field indexes are 0-based.
    let test_column: Option<usize> = config.test_i.map(|n| n as usize - 1);

    // Column names from the command line, as bytes. into_vec() gives the bytes of an
    // OsString (Unix only); pluk targets Unix systems, as pick does.
    let column_args: Vec<Vec<u8>> = config.columns.iter().map(|a| a.clone().into_vec()).collect();

    // As in pick, the names are checked once the first line has been read;
    // with empty input there is nothing to check against.
    let selection = if input.is_empty {
        select::OutputSelection { output_colnames: Vec::new(), output_offset: Vec::new() }
    } else {
        select::OutputSelection::from_names(&column_args, &input.names)
            .map_err(|msg| std::io::Error::new(ErrorKind::InvalidInput, msg))?
    };

    if config.test_select {
        for (name, i) in selection.output_colnames.iter().zip(&selection.output_offset) {
            out.write_all(name)?;
            writeln!(out, "\t{}", i)?;
        }
        out.flush()?;
        return Ok(());
    }

    if config.test_names {
        for i in 0..input.names.len() {
            let name = input.names.name(i);
            let lookup = input.names.index_of(name).map_or("none".to_string(), |j| j.to_string());
            write!(out, "{}\t", i)?;
            out.write_all(name)?;
            writeln!(out, "\t{}", lookup)?;
        }
        out.flush()?;
        return Ok(());
    }

    if config.c {
        // -c: output only the number of data rows.
        while input.next_line()? { }
        writeln!(out, "{}", input.n_accepted)?;
    } else {
        if input.header_lineno > 0 {
            write_numbered(&mut out, input.header_lineno, &input.header, test_column)?;
        }
        while input.next_line()? {
            write_numbered(&mut out, input.n_lines, &input.row, test_column)?;
        }
    }
    out.flush()?;

    if config.test_counts {
        eprintln!("columns={} lines={} skipped={} accepted={} short={} overflow={}",
            input.n_columns, input.n_lines, input.n_skipped, input.n_accepted,
            input.n_short_rows, input.n_overflow_rows);
    }
    Ok(())
}

// Output used during step1: line number, TAB, then the line or (--test-i) one field.
// 'out: &mut impl Write' accepts any writer, here the BufWriter on stdout.
fn write_numbered(out: &mut impl Write, lineno: u64, row: &Row, test_column: Option<usize>)
    -> std::io::Result<()>
{
    write!(out, "{}\t", lineno)?;
    match test_column {
        None    => out.write_all(&row.line)?,
        Some(i) => {
            // Lines with too few fields print an empty field; input
            // validation follows in item 5.
            if i < row.n_fields() {
                out.write_all(row.field(i))?;
            }
        }
    }
    out.write_all(b"\n")
}

fn main() {
    if let Err(e) = run() {
        // Output closed by the reader, e.g. pluk ... | head. Not an error.
        if e.kind() == ErrorKind::BrokenPipe {
            process::exit(0);
        }
        eprintln!("pluk: {}", e);
        process::exit(1);
    }
}
