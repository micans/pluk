// Dictionaries: reading and storage (plan/step4.txt).
//
// A dictionary maps keys to values; both are byte strings. Dictionaries are
// kept in a list, and a second map gives the list index of each name. Lookup
// is implemented in a later step, where a constant dictionary name in a compute
// is to be replaced by its index at compile time.
//
// opt.rs records a DictSpec for each dictionary option; Dicts::load() reads
// them in command line order once option parsing is complete.

use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, BufRead, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use crate::io::open_buffered;
use crate::opt::pct_decode;

// The source of a dictionary.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DictKind {
    Keys,       // --kd-NAME=FILE: keys from the first column, every value 1
    Pairs,      // --fd-NAME=FILE: key TAB value
    Inline,     // --cd-NAME=k:v,k:v
    Table,      // --td^KEY=FILE: one dictionary per column, named after the column
}

// Result of a failing lookup.
#[derive(Debug, Clone, PartialEq)]
pub enum NotFound {
    Key,                // no default given: the key itself, as in pick
    Value(Vec<u8>),     // NAME/default
    Exit,               // NAME/__EXIT__: lookup failure is an error
}

impl NotFound {
    // The meaning of a default, already %-decoded.
    pub fn from_default(default: Vec<u8>) -> NotFound {
        if default == b"__EXIT__" { NotFound::Exit } else { NotFound::Value(default) }
    }
}

// A dictionary option as given on the command line.
#[derive(Debug, Clone, PartialEq)]
pub struct DictSpec {
    pub option: Vec<u8>,                // the whole option, for messages
    pub kind: DictKind,
    pub name: Vec<u8>,                  // empty for Table
    pub key_column: Option<Vec<u8>>,    // Table: ^KEY; None for the first column
    pub not_found: Option<NotFound>,    // None: no default given
    pub source: Vec<u8>,                // file name, or k:v list for Inline
}

pub struct Dict {
    pub name: Vec<u8>,
    pub map: HashMap<Vec<u8>, Vec<u8>>,
    pub not_found: NotFound,
}

pub struct Dicts {
    list: Vec<Dict>,
    index: HashMap<Vec<u8>, usize>,
}

impl Dicts {

    pub fn new() -> Dicts {
        Dicts { list: Vec::new(), index: HashMap::new() }
    }

    // Read all dictionaries, in the order given.
    pub fn load(specs: &[DictSpec]) -> Result<Dicts, String> {
        let mut dicts = Dicts::new();
        for spec in specs {
            match spec.kind {
                DictKind::Keys | DictKind::Pairs => {
                    let dict = dicts.get_or_create(&spec.name, spec);
                    read_file(spec, &mut dict.map)?;
                }
                DictKind::Inline => {
                    let dict = dicts.get_or_create(&spec.name, spec);
                    parse_inline(spec, &mut dict.map)?;
                }
                DictKind::Table => dicts.read_table(spec)?,
            }
        }
        Ok(dicts)
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    // The dictionary called name, for spec. A new one is created if the name is
    // not yet in use; otherwise spec merges into the existing one, with a message.
    // A default replaces an earlier one, except that __EXIT__ is kept.
    fn get_or_create(&mut self, name: &[u8], spec: &DictSpec) -> &mut Dict {
        match self.index.get(name) {
            Some(&i) => {
                eprintln!("pluk: {}: merging into existing dictionary {}",
                          String::from_utf8_lossy(&spec.option), String::from_utf8_lossy(name));
                let dict = &mut self.list[i];
                if let Some(nf) = &spec.not_found {
                    if dict.not_found != NotFound::Exit {
                        dict.not_found = nf.clone();
                    }
                }
                dict
            }
            None => {
                self.index.insert(name.to_vec(), self.list.len());
                self.list.push(Dict {
                    name: name.to_vec(),
                    map: HashMap::new(),
                    not_found: spec.not_found.clone().unwrap_or(NotFound::Key),
                });
                self.list.last_mut().unwrap()
            }
        }
    }

    // --td: the first line holds the column names. Every column other than the key
    // column becomes a dictionary named after the column, mapping the key to the
    // value in that column; the default applies to all of them. As in pick, a
    // repeated key and a line whose number of fields differs from the header are
    // errors. Unlike pick, a repeated column name is an error (pick: the last wins).
    fn read_table(&mut self, spec: &DictSpec) -> Result<(), String> {
        let option = String::from_utf8_lossy(&spec.option);
        let path = Path::new(OsStr::from_bytes(&spec.source));
        let error = |e: io::Error| format!("{}: cannot read {}: {}", option, path.display(), e);
        let mut reader = open_buffered(File::open(path).map_err(error)?).map_err(error)?;

        let mut line = Vec::new();
        if !read_line(&mut reader, &mut line).map_err(error)? {
            return Err(format!("{}: table {} is empty", option, path.display()));
        }
        let header: Vec<Vec<u8>> = line.split(|&b| b == b'\t').map(|f| f.to_vec()).collect();
        let key_index = match &spec.key_column {
            None      => 0,
            Some(key) => header.iter().position(|name| name == key).ok_or_else(||
                format!("{}: no column [{}] in {}", option, String::from_utf8_lossy(key), path.display()))?,
        };
        let mut seen = HashSet::new();
        for name in &header {
            if !seen.insert(name) {
                return Err(format!("{}: repeated column name [{}] in {}", option,
                                   String::from_utf8_lossy(name), path.display()));
            }
        }

        // Dictionary index for each column; None for the key column.
        let mut targets: Vec<Option<usize>> = Vec::with_capacity(header.len());
        for (i, name) in header.iter().enumerate() {
            if i == key_index {
                targets.push(None);
            } else {
                self.get_or_create(name, spec);
                targets.push(Some(self.index[name.as_slice()]));
            }
        }

        let mut keys = HashSet::new();
        let mut lineno = 1;
        while read_line(&mut reader, &mut line).map_err(error)? {
            lineno += 1;
            let fields: Vec<&[u8]> = line.split(|&b| b == b'\t').collect();
            if fields.len() != header.len() {
                return Err(format!("{}: {} fields on line {} of {}, header has {}", option,
                                   fields.len(), lineno, path.display(), header.len()));
            }
            let key = fields[key_index];
            if !keys.insert(key.to_vec()) {
                return Err(format!("{}: repeated key [{}] on line {} of {}", option,
                                   String::from_utf8_lossy(key), lineno, path.display()));
            }
            for (field, target) in fields.iter().zip(&targets) {
                if let Some(d) = *target {
                    self.list[d].map.insert(key.to_vec(), field.to_vec());
                }
            }
        }
        Ok(())
    }

    // --test-dict: per dictionary a line with name, not-found setting and number of
    // keys, followed by the keys (sorted) and values.
    pub fn write_test(&self, out: &mut impl Write) -> io::Result<()> {
        for dict in &self.list {
            out.write_all(b"dict\t")?;
            out.write_all(&dict.name)?;
            match &dict.not_found {
                NotFound::Key      => out.write_all(b"\tnot-found=key")?,
                NotFound::Exit     => out.write_all(b"\tnot-found=__EXIT__")?,
                NotFound::Value(v) => {
                    out.write_all(b"\tnot-found=value:")?;
                    out.write_all(v)?;
                }
            }
            writeln!(out, "\tkeys={}", dict.map.len())?;
            let mut keys: Vec<&Vec<u8>> = dict.map.keys().collect();
            keys.sort();
            for key in keys {
                out.write_all(key)?;
                out.write_all(b"\t")?;
                out.write_all(&dict.map[key])?;
                out.write_all(b"\n")?;
            }
        }
        Ok(())
    }
}


// --kd and --fd, as in pick: the key is the first field of a line. With --fd the
// value is the second field; a line with a single field gets value 1, further
// fields are ignored. With --kd every value is 1. A repeated key takes the last value.
// The file may be gzip-compressed.
fn read_file(spec: &DictSpec, map: &mut HashMap<Vec<u8>, Vec<u8>>) -> Result<(), String> {
    let path = Path::new(OsStr::from_bytes(&spec.source));
    let error = |e: io::Error| format!("{}: cannot read {}: {}",
                                       String::from_utf8_lossy(&spec.option), path.display(), e);
    let file = File::open(path).map_err(error)?;
    let mut reader = open_buffered(file).map_err(error)?;
    let mut line = Vec::new();
    while read_line(&mut reader, &mut line).map_err(error)? {
        let mut fields = line.splitn(3, |&b| b == b'\t');
        let key = fields.next().unwrap_or(b"");     // splitn always yields at least one item
        let value: &[u8] = match (spec.kind, fields.next()) {
            (DictKind::Pairs, Some(v)) => v,
            _                          => b"1",
        };
        map.insert(key.to_vec(), value.to_vec());
    }
    Ok(())
}

// Read the next line into line, without its newline. Returns Ok(false) at end of input.
fn read_line(reader: &mut Box<dyn BufRead>, line: &mut Vec<u8>) -> io::Result<bool> {
    line.clear();
    if reader.read_until(b'\n', line)? == 0 {
        return Ok(false);
    }
    if line.last() == Some(&b'\n') {
        line.pop();
    }
    Ok(true)
}

// --cd: comma-separated key:value pairs, keys and values %-decoded as in pick.
// A pair without ':' gets value 1. Empty pairs (e.g. from a trailing comma) are
// ignored; a pair with more than one ':' is an error (pick ignores the rest).
fn parse_inline(spec: &DictSpec, map: &mut HashMap<Vec<u8>, Vec<u8>>) -> Result<(), String> {
    for pair in spec.source.split(|&b| b == b',') {
        if pair.is_empty() {
            continue;
        }
        let mut parts = pair.split(|&b| b == b':');
        let key = parts.next().unwrap_or(b"");
        let value = parts.next();
        if parts.next().is_some() {
            return Err(format!("{}: more than one ':' in [{}] (write ':' as %3A)",
                               String::from_utf8_lossy(&spec.option), String::from_utf8_lossy(pair)));
        }
        map.insert(pct_decode(key), value.map_or(b"1".to_vec(), pct_decode));
    }
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;

    fn spec(kind: DictKind, name: &str, not_found: Option<NotFound>, source: &str) -> DictSpec {
        DictSpec { option: b"--test".to_vec(), kind, name: name.as_bytes().to_vec(),
                   key_column: None, not_found, source: source.as_bytes().to_vec() }
    }

    fn value<'a>(dicts: &'a Dicts, name: &str, key: &str) -> Option<&'a [u8]> {
        let i = dicts.index[name.as_bytes()];
        dicts.list[i].map.get(key.as_bytes()).map(|v| v.as_slice())
    }

    #[test]
    fn inline() {
        let d = Dicts::load(&[spec(DictKind::Inline, "x", None, "a:A,b,,c%3A:%2C,")]).unwrap();
        assert_eq!(value(&d, "x", "a"), Some(&b"A"[..]));
        assert_eq!(value(&d, "x", "b"), Some(&b"1"[..]));
        assert_eq!(value(&d, "x", "c:"), Some(&b","[..]));
        assert_eq!(d.list[0].map.len(), 3);
        assert!(Dicts::load(&[spec(DictKind::Inline, "x", None, "a:b:c")]).is_err());
    }

    #[test]
    fn merge_and_sticky_exit() {
        let v = |s: &str| Some(NotFound::Value(s.as_bytes().to_vec()));
        let d = Dicts::load(&[
            spec(DictKind::Inline, "x", v("p"), "a:1,b:2"),
            spec(DictKind::Inline, "x", None, "b:3"),
            spec(DictKind::Inline, "y", Some(NotFound::Exit), "a"),
            spec(DictKind::Inline, "y", v("q"), "b"),
        ]).unwrap();
        assert_eq!(d.len(), 2);
        assert_eq!(value(&d, "x", "b"), Some(&b"3"[..]));
        assert_eq!(d.list[0].not_found, NotFound::Value(b"p".to_vec()));
        assert_eq!(d.list[1].not_found, NotFound::Exit);
        assert_eq!(d.list[1].map.len(), 2);
    }

    #[test]
    fn missing_file() {
        assert!(Dicts::load(&[spec(DictKind::Pairs, "x", None, "/nonexistent/file")]).is_err());
    }
}
