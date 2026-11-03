mod io;
mod opt;

use std::io::{BufWriter, ErrorKind, Write};
use std::process;

// Output buffer size. Rust's stdout flushes at every newline by default;
// wrapping it in a BufWriter avoids a write system call per line.
const OUT_BUFSIZE: usize = 128 * 1024;

fn run() -> std::io::Result<()> {
    let _config = opt::parse_args();

    let mut input = io::LineReader::from_stdin()?;
    let mut out = BufWriter::with_capacity(OUT_BUFSIZE, std::io::stdout().lock());

    while input.next_line()? {
        write!(out, "{}\t", input.n_lines)?;
        out.write_all(&input.line)?;
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
