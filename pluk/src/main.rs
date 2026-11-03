mod io;
mod opt;
mod row;

use std::io::{BufWriter, ErrorKind, Write};
use std::process;

// Output buffer size. Rust's stdout flushes at every newline by default;
// wrapping it in a BufWriter avoids a write system call per line.
const OUT_BUFSIZE: usize = 128 * 1024;

fn run() -> std::io::Result<()> {
    let config = opt::parse_args();

    let settings = io::InputSettings {
        skip_comment_preamble: config.test_comment,
    };
    let mut input = io::LineReader::from_stdin(settings)?;
    let mut out = BufWriter::with_capacity(OUT_BUFSIZE, std::io::stdout().lock());

    // --test-i N is 1-based on the command line, field indexes are 0-based.
    let test_column: Option<usize> = config.test_i.map(|n| n as usize - 1);

    while input.next_line()? {
        write!(out, "{}\t", input.n_lines)?;
        match test_column {
            None    => out.write_all(&input.row.line)?,
            Some(i) => {
                // Lines with too few fields print an empty field; input
                // validation follows in item 5.
                if i < input.row.n_fields() {
                    out.write_all(input.row.field(i))?;
                }
            }
        }
        out.write_all(b"\n")?;
    }
    out.flush()?;
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
