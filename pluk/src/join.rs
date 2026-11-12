// Joins: columns from a keyed table appended to each input row (plan/JOIN.design.txt).
//
// opt.rs records a JoinSpec per --join and --join-inner option. The tables are read
// before the input (Joins::load); once the header is known, bind() resolves each
// KEY and adds the joined names as input columns. For each data row apply() appends
// the joined fields to the line, so that everything after it (filters, computes,
// output, the files of --rejected and --demux) sees them as input columns.

use std::collections::HashMap;

use crate::colspec::ColSpec;
use crate::dict::TableFile;
use crate::names::ColumnNames;
use crate::row::Row;

// A --join or --join-inner option as given on the command line.
#[derive(Debug, Clone)]
pub struct JoinSpec {
    pub option: Vec<u8>,            // the whole option, for messages
    pub inner:  bool,               // --join-inner: a row whose key is not found is rejected
    pub key:    Vec<u8>,            // KEY: an input column, or a column of an earlier join (%-decoded)
    pub naming: Option<Naming>,     // KEY/SPEC, KEY/SPEC/S, KEY/P/SPEC/S (step37 item 3)
    pub source: Vec<u8>,            // FILE
}

// The naming part of a join (plan/JOIN.design.txt section 3): SPEC selects among the
// table's columns; with a prefix or suffix (rename) each selected column is named
// P + name + S, where capture group 1 of a '+' regex replaces the name if the regex
// has one, as in a renaming compute (plan/step26.txt decision 3).
#[derive(Debug, Clone)]
pub struct Naming {
    pub spec:   ColSpec,
    pub prefix: Vec<u8>,            // %-decoded
    pub suffix: Vec<u8>,            // %-decoded
    pub rename: bool,               // P/SPEC/S or SPEC/S given, not SPEC alone
}

// The joined columns of a table with header: their indexes in the table, in the order
// of SPEC, each once, never the key (column 0); and their names. Without a naming part
// all columns but the key, under their own names.
fn select(spec: &JoinSpec, option: &str, header: &[Vec<u8>]) -> Result<(Vec<usize>, Vec<Vec<u8>>), String> {
    let Some(naming) = &spec.naming else {
        return Ok(((1..header.len()).collect(), header[1..].to_vec()));
    };
    let table_names = ColumnNames::from_names(header.to_vec());
    // Capture groups name columns only when renaming, as in computes.
    let selected = if naming.rename {
        naming.spec.expand_bases(&table_names, "Join", option)?
    } else {
        naming.spec.expand(&table_names, "Join", option)?.into_iter().map(|c| (c, None)).collect()
    };
    let mut columns = Vec::new();
    let mut names = Vec::new();
    for (c, base) in selected {
        if c == 0 || columns.contains(&c) {
            continue;
        }
        let name = if naming.rename {
            let base = base.unwrap_or_else(|| header[c].clone());
            [&naming.prefix[..], &base, &naming.suffix].concat()
        } else {
            header[c].clone()
        };
        if names.contains(&name) {
            return Err(format!("{}: two columns would be named [{}]", option, String::from_utf8_lossy(&name)));
        }
        columns.push(c);
        names.push(name);
    }
    if columns.is_empty() {
        return Err(format!("{}: [{}] selects no column other than the key (the first column)",
                           option, String::from_utf8_lossy(&naming.spec.text)));
    }
    Ok((columns, names))
}

// The value of a missing cell in a left join: the NA that computes write
// (plan/JOIN.design.txt section 5).
const NA: &[u8] = b"NA";

struct Table {
    option:    String,
    file:      String,                  // FILE, for the report
    inner:     bool,
    key:       Vec<u8>,
    key_input: usize,                   // index of KEY among the row's columns, set by bind()
    key_join:  Option<usize>,           // the earlier left join that KEY is a column of, set by bind()
    names:     Vec<Vec<u8>>,            // the joined columns: the table's columns except its key
    rows:      HashMap<Vec<u8>, usize>, // key -> row number
    text:      Vec<u8>,                 // the joined values of all rows, one after the other
    cells:     Vec<(usize, usize)>,     // offsets into text; row r has cells r*w .. (r+1)*w
    n_missing: u64,                     // rows rejected by an inner join
}

impl Table {
    fn width(&self) -> usize {
        self.names.len()
    }
}

// What a left join gives for a key that is not in the table (plan/JOIN.design.txt
// section 5): NA, or the value of --join-missing=VALUE, or with __EXIT__ an error.
#[derive(Debug, Clone, PartialEq)]
pub enum Missing {
    Na,
    Value(Vec<u8>),
    Exit,
}

impl Missing {
    // --join-missing=VALUE, VALUE %-decoded.
    pub fn from_option(value: Vec<u8>) -> Missing {
        if value == b"__EXIT__" { Missing::Exit } else { Missing::Value(value) }
    }
}

pub struct Joins {
    tables:  Vec<Table>,
    missing: Missing,
    missed:  Vec<bool>,     // per table: the key was not found in the current row
}

impl Joins {
    // Read every table, in command line order. The key is the first column; the
    // joined columns are the others, or those selected by the naming part.
    pub fn load(specs: &[JoinSpec], missing: Missing) -> Result<Joins, String> {
        let mut tables = Vec::with_capacity(specs.len());
        for spec in specs {
            let option = String::from_utf8_lossy(&spec.option).into_owned();
            let mut file = TableFile::open(&option, &spec.source, None)?;
            let (columns, names) = select(spec, &option, &file.header)?;
            if names.iter().any(|n| n.is_empty()) {
                return Err(format!("{}: empty column name in {}", option, file.path.display()));
            }
            let mut table = Table {
                option, file: file.path.display().to_string(), inner: spec.inner, key: spec.key.clone(),
                key_input: 0, key_join: None, names, rows: HashMap::new(), text: Vec::new(), cells: Vec::new(), n_missing: 0,
            };
            while let Some(fields) = file.next_row()? {
                table.rows.insert(fields[0].to_vec(), table.rows.len());
                for &c in &columns {
                    let start = table.text.len();
                    table.text.extend_from_slice(fields[c]);
                    table.cells.push((start, table.text.len()));
                }
            }
            tables.push(table);
        }
        let missed = vec![false; tables.len()];
        Ok(Joins { tables, missing, missed })
    }

    pub fn is_empty(&self) -> bool {
        self.tables.is_empty()
    }

    // The number of joined columns over all tables.
    pub fn width(&self) -> usize {
        self.tables.iter().map(Table::width).sum()
    }

    // Resolve each KEY and add the joined names to the input names, table by table,
    // so that a KEY can name a column of an earlier join. Called before the computes
    // are compiled: KEY cannot be a computed column.
    pub fn bind(&mut self, names: &mut ColumnNames) -> Result<(), String> {
        // The first column of each table among the row's columns.
        let mut first = Vec::with_capacity(self.tables.len());
        for k in 0..self.tables.len() {
            let t = &self.tables[k];
            let key_input = names.index_of(&t.key).ok_or_else(|| format!(
                "{}: no column [{}] among {} (KEY is an input column or a column of an earlier join)",
                t.option, String::from_utf8_lossy(&t.key), names.joined()))?;
            let key_join = (0..k).rev().find(|&j| key_input >= first[j]).filter(|&j| !self.tables[j].inner);
            first.push(names.len());
            let t = &mut self.tables[k];
            (t.key_input, t.key_join) = (key_input, key_join);
            for name in &t.names {
                if names.index_of(name).is_some() {
                    return Err(format!("{}: column [{}] of {} exists already (in the input or an earlier join); \
                                        rename with --join^KEY/P/SPEC/S=FILE", t.option, String::from_utf8_lossy(name), t.file));
                }
                names.push_input(name);
            }
        }
        Ok(())
    }

    // Append the joined fields to row. Returns false if an inner join does not find
    // the key: the row is rejected, and gets empty fields for that join and the later
    // ones (earlier joins keep their values), so that it has every column for
    // --rejected (plan/JOIN.design.txt section 6). A left join that does not find the
    // key with --join-missing=__EXIT__ is an error (the caller adds the line number).
    pub fn apply(&mut self, row: &mut Row) -> Result<bool, String> {
        for k in 0..self.tables.len() {
            let t = &mut self.tables[k];
            let w = t.width();
            // A key that an earlier left join did not find (NA) never matches; a fill
            // value of --join-missing is data, and is looked up.
            let na_key = self.missing == Missing::Na && t.key_join.is_some_and(|j| self.missed[j]);
            let found = if na_key { None } else { t.rows.get(row.field(t.key_input)).copied() };
            self.missed[k] = found.is_none();
            match found {
                Some(r) => for &(start, end) in &t.cells[r * w..(r + 1) * w] {
                    row.push_field(&t.text[start..end]);
                },
                None if t.inner => {
                    t.n_missing += 1;
                    let rest: usize = self.tables[k..].iter().map(Table::width).sum();
                    for _ in 0..rest {
                        row.push_field(b"");
                    }
                    return Ok(false);
                }
                None => match &self.missing {
                    Missing::Na => for _ in 0..w {
                        row.push_field(NA);
                    },
                    Missing::Value(v) => for _ in 0..w {
                        row.push_field(v);
                    },
                    Missing::Exit => return Err(format!("{}: key [{}] not found in {} (--join-missing=__EXIT__)",
                                                        t.option, String::from_utf8_lossy(row.field(t.key_input)), t.file)),
                },
            }
        }
        Ok(true)
    }

    // The end-of-run counts (silenced by -q).
    pub fn report(&self) {
        for t in self.tables.iter().filter(|t| t.inner && t.n_missing > 0) {
            crate::note::note!("pluk: --join-inner: {} row{} with a key not in {}", t.n_missing,
                               if t.n_missing == 1 { "" } else { "s" }, t.file);
        }
    }
}
