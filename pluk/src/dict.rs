// Dictionaries: reading and storage (plan/step4.txt).
//
// A dictionary maps keys to values; both are byte strings. Dictionaries are
// kept in a list, and a second map gives the list index of each name. The
// filters ~isin~ ~hasval~ (filter.rs) look a dictionary up by name once, when
// they are compiled; lookup in computes comes in a later step.
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
use crate::num::Num;
use crate::opt::pct_decode;

// The dictionary holding the SAM reference sequences (--sam/FILE), named as in pick.
pub const SAM_REFERENCE: &[u8] = b"SAMFA";

// The dictionary of SAM sequence lengths, from @SQ header lines (item 4a), named as in pick.
pub const SEQ_LENGTHS: &[u8] = b"seqlen";

// Names that dictionary options and table columns cannot define; pluk fills these
// dictionaries itself.
pub fn is_reserved(name: &[u8]) -> bool {
    name == SAM_REFERENCE || name == SEQ_LENGTHS
}

// The source of a dictionary.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DictKind {
    Keys,       // --kd-NAME=FILE: keys from the first column, every value 1
    Pairs,      // --fd-NAME=FILE: key TAB value
    Inline,     // --cd-NAME=k:v,k:v
    Table,      // --td^KEY=FILE: one dictionary per column, named after the column
    Fasta,      // --fasta-dict-NAME=FILE: identifier -> sequence
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
    // For dord (plan/step15.txt decision 0): the 1-based position of each key's first
    // appearance; only for dictionaries named by a dord with a constant.
    pub order: Option<HashMap<Vec<u8>, usize>>,
}

impl Dict {
    // Every reader stores keys through insert, so that the order is recorded.
    fn insert(&mut self, key: Vec<u8>, value: Vec<u8>) {
        if let Some(order) = &mut self.order {
            let next = order.len() + 1;
            order.entry(key.clone()).or_insert(next);
        }
        self.map.insert(key, value);
    }
}

pub struct Dicts {
    list: Vec<Dict>,
    index: HashMap<Vec<u8>, usize>,
    ordered: HashSet<Vec<u8>>,     // names of the dictionaries that record their order
    quants: Vec<Quant>,            // --quant-NAME (plan/step24.txt), apart from the dictionaries
}

// The quantile steps of --quant-NAME=FILE, in non-decreasing order.
pub struct Quant {
    pub name:  Vec<u8>,
    pub steps: Vec<Num>,
}

impl Dicts {

    pub fn new() -> Dicts {
        Dicts { list: Vec::new(), index: HashMap::new(), ordered: HashSet::new(), quants: Vec::new() }
    }

    // --quant-NAME=FILE (plan/step24.txt decision 0): one finite number per line, in
    // non-decreasing order, at least one.
    pub fn load_quant(&mut self, name: &[u8], source: &[u8]) -> Result<(), String> {
        let option = format!("--quant-{}", String::from_utf8_lossy(name));
        let path = Path::new(OsStr::from_bytes(source));
        let error = |e: io::Error| format!("{}: cannot read {}: {}", option, path.display(), e);
        let mut reader = open_buffered(File::open(path).map_err(error)?).map_err(error)?;
        let mut line = Vec::new();
        let mut steps: Vec<Num> = Vec::new();
        let mut lineno = 0;
        while read_line(&mut reader, &mut line).map_err(error)? {
            lineno += 1;
            let at = || format!("{}: {} line {}", option, path.display(), lineno);
            let x = match crate::num::parse_num(&line) {
                Some(Num::F(f)) if !f.is_finite() => None,
                x => x,
            };
            let Some(x) = x else {
                return Err(format!("{}: [{}] is not a number", at(), String::from_utf8_lossy(&line)));
            };
            if let Some(&prev) = steps.last() {
                if crate::num::compare(prev, x) == Some(std::cmp::Ordering::Greater) {
                    return Err(format!("{}: [{}] is smaller than the step before it", at(),
                                       String::from_utf8_lossy(&line)));
                }
            }
            steps.push(x);
        }
        if steps.is_empty() {
            return Err(format!("{}: {} has no steps", option, path.display()));
        }
        self.quants.push(Quant { name: name.to_vec(), steps });
        Ok(())
    }

    // The position of the quantile steps called name, for qnl and qnu.
    pub fn quant_index(&self, name: &[u8]) -> Option<usize> {
        self.quants.iter().position(|q| q.name == name)
    }

    pub fn quant(&self, i: usize) -> &Quant {
        &self.quants[i]
    }

    // Read all dictionaries, in the order given. The dictionaries named in ordered
    // record the order of their keys (dord).
    pub fn load(specs: &[DictSpec], ordered: &[Vec<u8>]) -> Result<Dicts, String> {
        let mut dicts = Dicts::new();
        dicts.ordered = ordered.iter().cloned().collect();
        for spec in specs {
            match spec.kind {
                DictKind::Keys | DictKind::Pairs => {
                    let dict = dicts.get_or_create(&spec.name, spec);
                    read_file(spec, dict)?;
                }
                DictKind::Inline => {
                    let dict = dicts.get_or_create(&spec.name, spec);
                    parse_inline(spec, dict)?;
                }
                DictKind::Fasta => {
                    let dict = dicts.get_or_create(&spec.name, spec);
                    read_fasta(spec, dict)?;
                }
                DictKind::Table => dicts.read_table(spec)?,
            }
        }
        Ok(dicts)
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    // The dictionary called name, if there is one.
    pub fn get(&self, name: &[u8]) -> Option<&Dict> {
        self.index.get(name).map(|&i| &self.list[i])
    }

    // The position of the dictionary called name, for compiled operators (map, dord),
    // which then use at().
    pub fn index_of(&self, name: &[u8]) -> Option<usize> {
        self.index.get(name).copied()
    }

    pub fn at(&self, i: usize) -> &Dict {
        &self.list[i]
    }

    // The dictionary called name, for spec. A new one is created if the name is
    // not yet in use; otherwise spec merges into the existing one, with a message.
    // A default replaces an earlier one, except that __EXIT__ is kept.
    fn get_or_create(&mut self, name: &[u8], spec: &DictSpec) -> &mut Dict {
        match self.index.get(name) {
            Some(&i) => {
                crate::note::note!("pluk: {}: merging into existing dictionary {}",
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
                let order = self.ordered.contains(name).then(HashMap::new);
                self.list.push(Dict {
                    name: name.to_vec(),
                    map: HashMap::new(),
                    not_found: spec.not_found.clone().unwrap_or(NotFound::Key),
                    order,
                });
                self.list.last_mut().unwrap()
            }
        }
    }

    // SAM input: create the dictionary seqlen from the @SQ header lengths (a repeated
    // name takes the last length), not-found value 0, as in pick. With a SAM
    // reference (--sam/FILE) the lengths are compared as in pick's sam_check_reflen:
    // a sequence missing from the header gets its fasta length, a disagreement is
    // reported and the header length is kept, and a summary line follows.
    // The key '*' of the reference (unmapped reads) is not compared.
    pub fn add_sam_lengths(&mut self, header: &[(Vec<u8>, u64)]) {
        let mut lengths: HashMap<Vec<u8>, u64> = header.iter().cloned().collect();
        if let Some(&r) = self.index.get(SAM_REFERENCE) {
            let reference = &self.list[r].map;
            let n_header = lengths.len();
            let mut names: Vec<&Vec<u8>> = reference.keys().filter(|k| k.as_slice() != b"*").collect();
            names.sort();
            let (mut n_shared, mut n_conflict) = (0, 0);
            for name in &names {
                let fasta_length = reference[*name].len() as u64;
                match lengths.get(*name) {
                    None => {
                        lengths.insert(name.to_vec(), fasta_length);
                    }
                    Some(&header_length) => {
                        n_shared += 1;
                        if header_length != fasta_length {
                            n_conflict += 1;
                            crate::note::note!("pluk: sequence {} has length {} in the SAM header and {} in the \
                                       reference; using {}", String::from_utf8_lossy(name),
                                      header_length, fasta_length, header_length);
                        }
                    }
                }
            }
            crate::note::note!("pluk: SAM reference lengths: header/fasta/shared/conflict {}/{}/{}/{}",
                      n_header, names.len(), n_shared, n_conflict);
        }
        let map = lengths.into_iter().map(|(k, v)| (k, v.to_string().into_bytes())).collect();
        self.index.insert(SEQ_LENGTHS.to_vec(), self.list.len());
        // The lengths come from the header as a set: seqlen has no order (dord gives NA).
        self.list.push(Dict { name: SEQ_LENGTHS.to_vec(), map, not_found: NotFound::Value(b"0".to_vec()), order: None });
    }

    // --td: the first line holds the column names. Every column other than the key
    // column becomes a dictionary named after the column, mapping the key to the
    // value in that column; the default applies to all of them. As in pick, a
    // repeated key and a line whose number of fields differs from the header are
    // errors. Unlike pick, a repeated column name is an error (pick: the last wins).
    fn read_table(&mut self, spec: &DictSpec) -> Result<(), String> {
        let option = String::from_utf8_lossy(&spec.option);
        let mut table = TableFile::open(&option, &spec.source, spec.key_column.as_deref())?;
        for (i, name) in table.header.iter().enumerate() {
            if i != table.key_index && is_reserved(name) {
                return Err(format!("{}: column name [{}] in {} is a reserved dictionary name", option,
                                   String::from_utf8_lossy(name), table.path.display()));
            }
        }

        // Dictionary index for each column; None for the key column.
        let mut targets: Vec<Option<usize>> = Vec::with_capacity(table.header.len());
        for (i, name) in table.header.iter().enumerate() {
            if i == table.key_index {
                targets.push(None);
            } else {
                self.get_or_create(name, spec);
                targets.push(Some(self.index[name.as_slice()]));
            }
        }

        let key_index = table.key_index;
        while let Some(fields) = table.next_row()? {
            let key = fields[key_index];
            for (field, target) in fields.iter().zip(&targets) {
                if let Some(d) = *target {
                    self.list[d].insert(key.to_vec(), field.to_vec());
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
fn read_file(spec: &DictSpec, dict: &mut Dict) -> Result<(), String> {
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
        dict.insert(key.to_vec(), value.to_vec());
    }
    Ok(())
}

// --fasta-dict. A line starting with '>' starts a record; the identifier is the
// text after '>' up to the first white space (pick: >(\S+)). The sequence is the
// concatenation of the lines that follow, without newlines; each sequence is stored
// as a value of its own (plan/step4-notes.txt item 3). As in pick, the key '*' is
// added with the empty sequence (the reference name of unmapped SAM reads).
// Unlike pick, a '>' line without identifier, a repeated identifier, and
// sequence data before the first '>' are errors.
fn read_fasta(spec: &DictSpec, dict: &mut Dict) -> Result<(), String> {
    let option = String::from_utf8_lossy(&spec.option);
    let path = Path::new(OsStr::from_bytes(&spec.source));
    let error = |e: io::Error| format!("{}: cannot read {}: {}", option, path.display(), e);
    let mut reader = open_buffered(File::open(path).map_err(error)?).map_err(error)?;

    let mut line = Vec::new();
    let mut lineno = 0;
    let mut id: Option<Vec<u8>> = None;     // identifier of the current record
    let mut seq: Vec<u8> = Vec::new();      // its sequence so far
    let mut ids = HashSet::new();           // identifiers in this file
    while read_line(&mut reader, &mut line).map_err(error)? {
        lineno += 1;
        if let Some(rest) = line.strip_prefix(b">") {
            let len = rest.iter().take_while(|b| !b.is_ascii_whitespace()).count();
            if len == 0 {
                return Err(format!("{}: no identifier on line {} of {}", option, lineno, path.display()));
            }
            let new_id = rest[..len].to_vec();
            if !ids.insert(new_id.clone()) {
                return Err(format!("{}: repeated identifier [{}] on line {} of {}", option,
                                   String::from_utf8_lossy(&new_id), lineno, path.display()));
            }
            if let Some(prev) = id.replace(new_id) {
                store_sequence(dict, prev, &mut seq);
            }
        } else if id.is_some() {
            seq.extend_from_slice(&line);
        } else if !line.is_empty() {
            return Err(format!("{}: sequence data before the first '>' on line {} of {}",
                               option, lineno, path.display()));
        }
    }
    if let Some(prev) = id {
        store_sequence(dict, prev, &mut seq);
    }
    dict.insert(b"*".to_vec(), Vec::new());
    Ok(())
}

// Move the sequence into the dictionary, leaving seq empty. The stored value is
// trimmed to its length; a Vec grows by doubling, so it may have up to twice the
// capacity needed.
fn store_sequence(dict: &mut Dict, id: Vec<u8>, seq: &mut Vec<u8>) {
    let mut value = std::mem::take(seq);
    value.shrink_to_fit();
    dict.insert(id, value);
}

// Read the next line into line, without its newline. Returns Ok(false) at end of input.
// A keyed table file with a header line, read row by row: shared by --td and
// --join (plan/JOIN.design.txt section 2). The header must have distinct names; a
// row with another number of fields than the header, and a repeated key, are errors.
// option names the option in messages.
pub struct TableFile {
    option:        String,
    pub path:      std::path::PathBuf,
    reader:        Box<dyn BufRead>,
    line:          Vec<u8>,
    pub header:    Vec<Vec<u8>>,
    pub key_index: usize,
    keys:          HashSet<Vec<u8>>,
    lineno:        u64,
}

impl TableFile {
    // Open the table and read its header. key_column names the key column; None: the first.
    pub fn open(option: &str, source: &[u8], key_column: Option<&[u8]>) -> Result<TableFile, String> {
        let path = Path::new(OsStr::from_bytes(source)).to_path_buf();
        let error = |e: io::Error| format!("{}: cannot read {}: {}", option, path.display(), e);
        let mut reader = open_buffered(File::open(&path).map_err(error)?).map_err(error)?;
        let mut line = Vec::new();
        if !read_line(&mut reader, &mut line).map_err(error)? {
            return Err(format!("{}: table {} is empty", option, path.display()));
        }
        let header: Vec<Vec<u8>> = line.split(|&b| b == b'\t').map(|f| f.to_vec()).collect();
        let key_index = match key_column {
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
        Ok(TableFile { option: option.to_string(), path, reader, line, header, key_index,
                       keys: HashSet::new(), lineno: 1 })
    }

    // The fields of the next row, borrowed until the next call; None at the end.
    pub fn next_row(&mut self) -> Result<Option<Vec<&[u8]>>, String> {
        let (option, path) = (&self.option, &self.path);
        if !read_line(&mut self.reader, &mut self.line)
            .map_err(|e| format!("{}: cannot read {}: {}", option, path.display(), e))?
        {
            return Ok(None);
        }
        self.lineno += 1;
        let fields: Vec<&[u8]> = self.line.split(|&b| b == b'\t').collect();
        if fields.len() != self.header.len() {
            return Err(format!("{}: {} fields on line {} of {}, header has {}", option,
                               fields.len(), self.lineno, path.display(), self.header.len()));
        }
        let key = fields[self.key_index];
        if !self.keys.insert(key.to_vec()) {
            return Err(format!("{}: repeated key [{}] on line {} of {}", option,
                               String::from_utf8_lossy(key), self.lineno, path.display()));
        }
        Ok(Some(fields))
    }
}

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
fn parse_inline(spec: &DictSpec, dict: &mut Dict) -> Result<(), String> {
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
        dict.insert(pct_decode(key), value.map_or(b"1".to_vec(), pct_decode));
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
        let d = Dicts::load(&[spec(DictKind::Inline, "x", None, "a:A,b,,c%3A:%2C,")], &[]).unwrap();
        assert_eq!(value(&d, "x", "a"), Some(&b"A"[..]));
        assert_eq!(value(&d, "x", "b"), Some(&b"1"[..]));
        assert_eq!(value(&d, "x", "c:"), Some(&b","[..]));
        assert_eq!(d.list[0].map.len(), 3);
        assert!(Dicts::load(&[spec(DictKind::Inline, "x", None, "a:b:c")], &[]).is_err());
    }

    #[test]
    fn merge_and_sticky_exit() {
        let v = |s: &str| Some(NotFound::Value(s.as_bytes().to_vec()));
        let d = Dicts::load(&[
            spec(DictKind::Inline, "x", v("p"), "a:1,b:2"),
            spec(DictKind::Inline, "x", None, "b:3"),
            spec(DictKind::Inline, "y", Some(NotFound::Exit), "a"),
            spec(DictKind::Inline, "y", v("q"), "b"),
        ], &[]).unwrap();
        assert_eq!(d.len(), 2);
        assert_eq!(value(&d, "x", "b"), Some(&b"3"[..]));
        assert_eq!(d.list[0].not_found, NotFound::Value(b"p".to_vec()));
        assert_eq!(d.list[1].not_found, NotFound::Exit);
        assert_eq!(d.list[1].map.len(), 2);
    }

    #[test]
    fn missing_file() {
        assert!(Dicts::load(&[spec(DictKind::Pairs, "x", None, "/nonexistent/file")], &[]).is_err());
    }
}
