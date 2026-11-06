// Output files other than stdout (plan/step9.txt): --other, --assert-other and
// demux files. A file whose name ends in .gz or .gzip is gzip-compressed.
//
// Files are created directly, never through a shell: pick opens "|gzip >$name",
// with which a file name taken from the data can run a command.

use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use flate2::write::GzEncoder;
use flate2::Compression;

use crate::names::ColumnNames;
use crate::output::{self, RowWriter};
use crate::row::Row;

const FILE_BUFSIZE: usize = 64 * 1024;

pub enum FileSink {
    Plain(BufWriter<File>),
    Gzip(GzEncoder<BufWriter<File>>),
}

impl FileSink {
    // Create (or truncate, as pick) the file called name; the name is bytes, as
    // command line arguments and data values are.
    pub fn create(name: &[u8]) -> io::Result<FileSink> {
        let path = Path::new(OsStr::from_bytes(name));
        let file = BufWriter::with_capacity(FILE_BUFSIZE, File::create(path)?);
        Ok(if is_gzip_name(name) {
            FileSink::Gzip(GzEncoder::new(file, Compression::default()))
        } else {
            FileSink::Plain(file)
        })
    }

    // Complete the file: flush the buffer and, for gzip, write the trailer.
    // Dropping a FileSink does this too, but ignores errors; finish reports them.
    pub fn finish(self) -> io::Result<()> {
        match self {
            FileSink::Plain(mut w) => w.flush(),
            FileSink::Gzip(g) => g.finish()?.flush(),
        }
    }
}

// FileSink can be used wherever a writer is expected (RowWriter, write_line).
impl Write for FileSink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            FileSink::Plain(w) => w.write(buf),
            FileSink::Gzip(g) => g.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            FileSink::Plain(w) => w.flush(),
            FileSink::Gzip(g) => g.flush(),
        }
    }
}

// A side output (--other, --assert-other): a file that receives some of the data
// rows, in the same columns as stdout, after the preamble and the header.
pub struct SideFile {
    option: &'static str,           // for messages
    pub name: Vec<u8>,
    file:   FileSink,
    writer: Option<RowWriter>,      // None until start(), and with empty input
    pub n_rows: u64,                // data rows written
}

impl SideFile {
    // Created before input is read, so that a bad path is reported early.
    pub fn create(option: &'static str, name: &[u8]) -> io::Result<SideFile> {
        let file = FileSink::create(name).map_err(|e| {
            io::Error::new(e.kind(), format!("{}: cannot create {}: {}", option, String::from_utf8_lossy(name), e))
        })?;
        Ok(SideFile { option, name: name.to_vec(), file, writer: None, n_rows: 0 })
    }

    // Once the header has been read: the preamble, the header if given, and the
    // RowWriter for the data rows.
    pub fn start(&mut self, writer: RowWriter, preamble: &[Vec<u8>], header: Option<&ColumnNames>)
        -> io::Result<()>
    {
        for line in preamble {
            output::write_line(&mut self.file, line)?;
        }
        if let Some(names) = header {
            writer.write_header(&mut self.file, names)?;
        }
        self.writer = Some(writer);
        Ok(())
    }

    pub fn write_row(&mut self, row: &Row) -> io::Result<()> {
        if let Some(writer) = &self.writer {
            writer.write_row(&mut self.file, row)?;
        }
        self.n_rows += 1;
        Ok(())
    }

    pub fn finish(self) -> io::Result<()> {
        let option = self.option;
        self.file.finish().map_err(|e| io::Error::new(e.kind(), format!("{}: {}", option, e)))
    }
}

pub fn is_gzip_name(name: &[u8]) -> bool {
    name.ends_with(b".gz") || name.ends_with(b".gzip")
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn gzip_names() {
        assert!(is_gzip_name(b"a.gz"));
        assert!(is_gzip_name(b"dir/a.txt.gzip"));
        assert!(!is_gzip_name(b"a.gz.txt"));
        assert!(!is_gzip_name(b"agz"));
    }

    #[test]
    fn plain_and_gzip() {
        let dir = std::env::temp_dir().join(format!("pluk-sink-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["x.txt", "x.txt.gz"] {
            let path = dir.join(name);
            let mut sink = FileSink::create(path.as_os_str().as_bytes()).unwrap();
            sink.write_all(b"a\tb\n1\t2\n").unwrap();
            sink.finish().unwrap();
            let raw = std::fs::read(&path).unwrap();
            let mut text = Vec::new();
            if name.ends_with(".gz") {
                flate2::read::GzDecoder::new(&raw[..]).read_to_end(&mut text).unwrap();
            } else {
                text = raw;
            }
            assert_eq!(text, b"a\tb\n1\t2\n");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
