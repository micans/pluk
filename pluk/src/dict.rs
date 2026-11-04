// Dictionaries: reading and storage (plan/step4.txt).
//
// A dictionary maps keys to values; both are byte strings. Dictionaries are
// kept in a list, and a second map gives the list index of each name. Lookup
// is implemented in a later step, where a constant dictionary name in a compute
// is to be replaced by its index at compile time.
//
// opt.rs records a DictSpec for each dictionary option; Dicts::load() reads
// them in command line order once option parsing is complete.

use std::collections::HashMap;
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
    pub name: Vec<u8>,
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
            let dict = dicts.get_or_create(spec);
            match spec.kind {
                DictKind::Keys | DictKind::Pairs => read_file(spec, &mut dict.map)?,
                DictKind::Inline                 => parse_inline(spec, &mut dict.map)?,
            }
        }
        Ok(dicts)
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    // The dictionary named in spec. A new one is created if the name is not yet
    // in use; otherwise spec merges into the existing one, with a message.
    // A default replaces an earlier one, except that __EXIT__ is kept.
    fn get_or_create(&mut self, spec: &DictSpec) -> &mut Dict {
        match self.index.get(&spec.name) {
            Some(&i) => {
                eprintln!("pluk: {}: merging into existing dictionary {}",
                          String::from_utf8_lossy(&spec.option), String::from_utf8_lossy(&spec.name));
                let dict = &mut self.list[i];
                if let Some(nf) = &spec.not_found {
                    if dict.not_found != NotFound::Exit {
                        dict.not_found = nf.clone();
                    }
                }
                dict
            }
            None => {
                self.index.insert(spec.name.clone(), self.list.len());
                self.list.push(Dict {
                    name: spec.name.clone(),
                    map: HashMap::new(),
                    not_found: spec.not_found.clone().unwrap_or(NotFound::Key),
                });
                self.list.last_mut().unwrap()
            }
        }
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
    loop {
        line.clear();
        if reader.read_until(b'\n', &mut line).map_err(error)? == 0 {
            break;
        }
        if line.last() == Some(&b'\n') {
            line.pop();
        }
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
                   not_found, source: source.as_bytes().to_vec() }
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
