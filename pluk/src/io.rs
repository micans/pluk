// Input: reading lines from STDIN.
//
// Input may be plain text or gzip-compressed; this is detected from the first
// two bytes. Lines are read into a single reusable byte buffer. See
// plan/step1-notes.txt for the rationale.

use std::io::{self, BufRead, BufReader};

use flate2::bufread::MultiGzDecoder;

// Size of the read buffers. The standard library default is 8 KiB.
const BUFSIZE: usize = 128 * 1024;

// The first two bytes of every gzip (and bgzip) stream.
const GZIP_MAGIC: [u8; 2] = [0x1f, 0x8b];

pub struct LineReader {
    reader:      Box<dyn BufRead>,
    pub line:    Vec<u8>,       // current line, newline removed
    pub n_lines: u64,           // number of lines read so far
}

impl LineReader {

    pub fn from_stdin() -> io::Result<LineReader> {
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

        Ok(LineReader { reader, line: Vec::with_capacity(1024), n_lines: 0 })
    }

    // Read the next line into self.line. Returns Ok(false) at end of input.
    // A final line without a trailing newline is returned as a normal line.
    pub fn next_line(&mut self) -> io::Result<bool> {
        self.line.clear();                  // keeps the allocated capacity
        let n = self.reader.read_until(b'\n', &mut self.line)?;
        if n == 0 {
            return Ok(false);
        }
        if self.line.last() == Some(&b'\n') {
            self.line.pop();
        }
        self.n_lines += 1;
        Ok(true)
    }
}

