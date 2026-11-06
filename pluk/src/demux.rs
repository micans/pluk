// --demux=NAME: each output row is written to the file named by its value in
// column NAME (plan/step9.txt item 3). Files are opened on first use and get the
// preamble and the header; rows rejected by @ filters open their file too and are
// tallied as filtered (as pick); a tally is written to stderr at the end.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use crate::output::RowWriter;
use crate::row::Row;
use crate::sink::FileSink;

struct DemuxFile {
    path:     Vec<u8>,
    sink:     FileSink,
    written:  u64,
    filtered: u64,
}

pub struct Demux {
    column:  usize,
    dir:     Option<Vec<u8>>,           // --demux-dir
    writer:  RowWriter,                 // the output columns, as stdout
    start:   Vec<u8>,                   // preamble and header, written to each new file
    taken:   Vec<Vec<u8>>,              // --other and --assert-other names
    files:   Vec<DemuxFile>,
    index:   HashMap<Vec<u8>, usize>,   // value -> position in files
}

impl Demux {
    // start: the bytes each file begins with (preamble lines and header line).
    pub fn new(column: usize, dir: Option<Vec<u8>>, writer: RowWriter, start: Vec<u8>, taken: Vec<Vec<u8>>)
        -> Demux
    {
        Demux { column, dir, writer, start, taken, files: Vec::new(), index: HashMap::new() }
    }

    // A row that is output: written to its file.
    pub fn write(&mut self, row: &Row, lineno: u64) -> io::Result<()> {
        let k = self.file_for(row, lineno)?;
        let file = &mut self.files[k];
        self.writer.write_row(&mut file.sink, row)?;
        file.written += 1;
        Ok(())
    }

    // A row rejected by an @ filter: its file exists (as pick), the row is counted.
    pub fn filtered(&mut self, row: &Row, lineno: u64) -> io::Result<()> {
        let k = self.file_for(row, lineno)?;
        self.files[k].filtered += 1;
        Ok(())
    }

    // The position of the file for the row's value, opening it if needed.
    fn file_for(&mut self, row: &Row, lineno: u64) -> io::Result<usize> {
        let value = row.field(self.column);
        if let Some(&k) = self.index.get(value) {
            return Ok(k);
        }
        let error = |msg: String| io::Error::new(io::ErrorKind::InvalidData, msg);
        if let Err(why) = check_name(value) {
            return Err(error(format!("--demux: value [{}] at line {} is not a valid file name ({})",
                                     String::from_utf8_lossy(value), lineno, why)));
        }
        let path = match &self.dir {
            Some(dir) => Path::new(OsStr::from_bytes(dir)).join(OsStr::from_bytes(value)).as_os_str().as_bytes().to_vec(),
            None => value.to_vec(),
        };
        if self.taken.contains(&path) {
            return Err(error(format!("--demux: value [{}] at line {} names the file of --other or --assert-other",
                                     String::from_utf8_lossy(value), lineno)));
        }
        let mut sink = FileSink::create(&path).map_err(|e| {
            // EMFILE (process) and ENFILE (system): too many open files.
            if matches!(e.raw_os_error(), Some(24) | Some(23)) {
                error(format!("--demux: cannot open more files ({} open, at value [{}], line {}); \
                               split the run with @@ filters on the demux column",
                              self.files.len(), String::from_utf8_lossy(value), lineno))
            } else {
                io::Error::new(e.kind(), format!("--demux: cannot create {}: {}", String::from_utf8_lossy(&path), e))
            }
        })?;
        io::Write::write_all(&mut sink, &self.start)?;
        self.files.push(DemuxFile { path, sink, written: 0, filtered: 0 });
        self.index.insert(value.to_vec(), self.files.len() - 1);
        Ok(self.files.len() - 1)
    }

    // Complete all files and write the tally to stderr, sorted by file name.
    pub fn finish(mut self) -> io::Result<()> {
        self.files.sort_by(|a, b| a.path.cmp(&b.path));
        eprintln!("File\tWritten\tFiltered");
        for file in self.files {
            eprintln!("{}\t{}\t{}", String::from_utf8_lossy(&file.path), file.written, file.filtered);
            file.sink.finish()?;
        }
        Ok(())
    }
}

// A value must be a relative path without empty, '.' or '..' components, so that
// it stays below the demux directory and distinct values are distinct files.
fn check_name(value: &[u8]) -> Result<(), &'static str> {
    if value.is_empty() {
        return Err("empty");
    }
    if value.starts_with(b"/") {
        return Err("absolute path");
    }
    for component in value.split(|&b| b == b'/') {
        match component {
            b"" => return Err("empty path component"),
            b"." | b".." => return Err("'.' or '..' path component"),
            _ => (),
        }
    }
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        for ok in ["a", "a.txt", "sub/a.gz", "..a", "a..", ".hidden"] {
            assert!(check_name(ok.as_bytes()).is_ok(), "{}", ok);
        }
        for bad in ["", "/a", "a//b", "a/", "./a", "a/./b", "../a", "a/..", ".", ".."] {
            assert!(check_name(bad.as_bytes()).is_err(), "{}", bad);
        }
    }
}
