mod io;
mod names;
mod opt;
mod output;
mod row;
mod select;

use std::io::{BufWriter, ErrorKind, Write};
use std::os::unix::ffi::OsStringExt;
use std::process;

// Output buffer size. Rust's stdout flushes at every newline by default;
// wrapping it in a BufWriter avoids a write system call per line.
const OUT_BUFSIZE: usize = 128 * 1024;

fn run() -> std::io::Result<()> {
    let config = opt::parse_args();

    // Temporary step1 output: line number and line (or one field with --test-i).
    let lineno_mode = config.test_lineno || config.test_i.is_some();

    // As in pick: without column names there is nothing to do; checked before reading input.
    if config.columns.is_empty() && !config.all && !config.excise && !config.c
        && !lineno_mode && !config.test_names && !config.test_select
    {
        eprintln!("pluk: no columns given. Use --help for help, -h suppresses header line output");
        eprintln!("pluk only works as a pipe, so invoke e.g. as: pluk foo bar < data.txt");
        return Ok(());
    }

    // As in pick: header output on by default, off with -k; -h and -c always switch it off.
    #[allow(non_snake_case)]
    let B_print_header = !config.k && !config.no_header && !config.c;

    let settings = io::InputSettings {
        skip_comment_preamble: config.test_comment,
        B_header:              !config.k,
        omega:                 config.omega.map(|n| n as usize),
        add_row_column:        config.add_row_column,
        make_unique:           config.make_unique,
        sam:                   if config.sam_h    { io::SamMode::PassThrough }
                               else if config.sam { io::SamMode::Skip }
                               else               { io::SamMode::Off },
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
        // As in pick, -x takes precedence over -A.
        let names = &input.names;
        let result = if config.excise {
            select::OutputSelection::complement(&column_args, names)
        } else if config.all {
            select::OutputSelection::all(&column_args, names)
        } else {
            select::OutputSelection::from_names(&column_args, names)
        };
        result.map_err(|msg| std::io::Error::new(ErrorKind::InvalidInput, msg))?
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

    // Pass-through lines (--sam-h) are output in all modes, as in pick; those
    // before the first data line come first.
    for line in &input.passthrough_preamble {
        output::write_line(&mut out, line)?;
    }

    if config.c {
        // -c: output only the number of data rows.
        while let Some(kind) = input.next_line()? {
            if kind == io::LineKind::PassThrough {
                output::write_line(&mut out, &input.row.line)?;
            }
        }
        writeln!(out, "{}", input.n_accepted)?;
    } else if lineno_mode {
        if input.header_lineno > 0 {
            output::write_numbered(&mut out, input.header_lineno, &input.header, test_column)?;
        }
        while let Some(kind) = input.next_line()? {
            match kind {
                io::LineKind::Data        => output::write_numbered(&mut out, input.n_lines, &input.row, test_column)?,
                io::LineKind::PassThrough => output::write_line(&mut out, &input.row.line)?,
            }
        }
    } else {
        let writer = output::RowWriter::new(&selection.output_offset, input.n_columns,
                                            input.settings.omega.is_some());
        if B_print_header && !input.is_empty {
            writer.write_header(&mut out, &input.names)?;
        }
        while let Some(kind) = input.next_line()? {
            match kind {
                io::LineKind::Data        => writer.write_row(&mut out, &input.row)?,
                io::LineKind::PassThrough => output::write_line(&mut out, &input.row.line)?,
            }
        }
    }
    out.flush()?;

    if config.test_counts {
        eprintln!("columns={} lines={} skipped={} accepted={} short={} overflow={}",
            input.n_columns, input.n_lines, input.n_skipped, input.n_accepted,
            input.n_short_rows, input.n_overflow_rows);
        if input.settings.sam != io::SamMode::Off {
            eprintln!("passthrough={}", input.n_passthrough);
        }
    }
    Ok(())
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
