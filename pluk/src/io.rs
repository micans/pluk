// Input: reading lines from STDIN.
//
// Input may be plain text or gzip-compressed; this is detected from the first
// two bytes. Lines are read into a single reusable byte buffer. See
// plan/step1-notes.txt for the rationale.

use std::io::{self, BufRead, BufReader};

use flate2::bufread::MultiGzDecoder;

use crate::row::Row;

// Size of the read buffers. The standard library default is 8 KiB.
const BUFSIZE: usize = 128 * 1024;

// The first two bytes of every gzip (and bgzip) stream.
const GZIP_MAGIC: [u8; 2] = [0x1f, 0x8b];

// Settings that affect input processing, filled in from the command line by main.
pub struct InputSettings {
    pub skip_comment_preamble: bool,    // --test-comment: skip leading lines starting with '#'
}

pub struct LineReader {
    reader:         Box<dyn BufRead>,
    settings:       InputSettings,
    in_preamble:    bool,           // no accepted line seen yet
    pub row:        Row,            // current line and its fields
    pub n_lines:    u64,            // number of lines read so far (physical line number)
    pub n_skipped:  u64,            // lines skipped
    pub n_accepted: u64,            // lines returned by next_line()
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

        Ok(LineReader {
            reader,
            settings,
            in_preamble: true,
            row:         Row::new(),
            n_lines:     0,
            n_skipped:   0,
            n_accepted:  0,
        })
    }

    // Read the next accepted line into self.row and split it into fields,
    // skipping lines as the settings require. Returns Ok(false) at end of input.
    pub fn next_line(&mut self) -> io::Result<bool> {
        while self.read_line()? {
            if self.in_preamble && self.settings.skip_comment_preamble && self.row.line.starts_with(b"#") {
                self.n_skipped += 1;
                continue;
            }
            self.in_preamble = false;
            self.n_accepted += 1;
            self.row.split_fields();
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

