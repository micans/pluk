// Column names: a list (index -> name) and a dictionary (name -> index).
//
// Names are byte strings, copied from the header line; this happens once per
// run. Indexes are 0-based.

use std::collections::HashMap;

use crate::row::Row;

#[derive(Clone)]
pub struct ColumnNames {
    names: Vec<Vec<u8>>,
    index: HashMap<Vec<u8>, usize>,
    n_input: usize,                 // the input columns; computes are pushed after them
}

// Internal name of the row name column (-R, or an empty first header field).
pub const ROW_NAME: &[u8] = b"_";

impl ColumnNames {

    // Headerless input (-k): the names are 1 2 ... n.
    pub fn numbered(n: usize) -> ColumnNames {
        let names = (1..=n).map(|i| i.to_string().into_bytes()).collect();
        ColumnNames::from_list(names)
    }

    // Names from the header line. With add_row_column (-R) the name '_' is
    // prepended; an empty first header field is also named '_'.
    // Duplicate names are an error, unless make_unique (-u) is set.
    pub fn from_header(header: &Row, add_row_column: bool, make_unique: bool)
        -> Result<ColumnNames, String>
    {
        let mut names: Vec<Vec<u8>> = Vec::with_capacity(header.n_fields() + 1);
        if add_row_column {
            names.push(ROW_NAME.to_vec());
        }
        for i in 0..header.n_fields() {
            names.push(header.field(i).to_vec());
        }
        if names[0].is_empty() {
            names[0] = ROW_NAME.to_vec();
        }

        let dups = duplicates(&names);
        if !dups.is_empty() {
            if !make_unique {
                let list: Vec<String> = dups.iter().map(|d| String::from_utf8_lossy(d).into_owned()).collect();
                return Err(format!("Duplicate column names: {} (use -u to make them unique)", list.join(" ")));
            }
            names = unique_names(&names);
        }
        Ok(ColumnNames::from_list(names))
    }

    // Build the dictionary from a list of names, which must be unique.
    fn from_list(names: Vec<Vec<u8>>) -> ColumnNames {
        let mut index = HashMap::with_capacity(names.len());
        for (i, name) in names.iter().enumerate() {
            index.insert(name.clone(), i);
        }
        let n_input = names.len();
        ColumnNames { names, index, n_input }
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn n_input(&self) -> usize {
        self.n_input
    }

    // Name of column i (0-based). Panics if i is out of range.
    pub fn name(&self, i: usize) -> &[u8] {
        &self.names[i]
    }

    // All current names separated by commas, as pick lists them in messages.
    pub fn joined(&self) -> String {
        let all: Vec<String> = (0..self.names.len()).filter(|&i| self.is_current(i))
            .map(|i| String::from_utf8_lossy(&self.names[i]).into_owned()).collect();
        all.join(",")
    }

    // With in-place computes (plan/INPLACE.design.txt) a name can be bound again: the
    // new column gets a new index and the name refers to it from then on. Column i is
    // current if its name still refers to it.
    pub fn is_current(&self, i: usize) -> bool {
        self.index.get(&self.names[i]) == Some(&i)
    }

    // The column that the name of column i refers to now.
    pub fn current(&self, i: usize) -> usize {
        self.index[&self.names[i]]
    }

    // Add a name (a compute, plan/COMPUTE.design.txt section 6); returns its index.
    // If the name exists, it is bound to the new index (in-place compute).
    pub fn push(&mut self, name: &[u8]) -> usize {
        let i = self.names.len();
        self.names.push(name.to_vec());
        self.index.insert(name.to_vec(), i);
        i
    }

    // Add an input column (a column of a join, plan/JOIN.design.txt section 4); only
    // before any compute name is pushed. The name must be new.
    pub fn push_input(&mut self, name: &[u8]) -> usize {
        assert!(self.n_input == self.names.len() && !self.index.contains_key(name));
        self.n_input += 1;
        self.push(name)
    }

    // Index of the column with the given name, if it exists.
    pub fn index_of(&self, name: &[u8]) -> Option<usize> {
        self.index.get(name).copied()
    }
}


// Names that occur more than once, each listed once, in order of first repetition.
fn duplicates(names: &[Vec<u8>]) -> Vec<Vec<u8>> {
    let mut count: HashMap<&[u8], usize> = HashMap::new();
    let mut dups = Vec::new();
    for name in names {
        let c = count.entry(name.as_slice()).or_insert(0);
        *c += 1;
        if *c == 2 {
            dups.push(name.clone());
        }
    }
    dups
}

// -u: the first occurrence of a name keeps it; the k-th occurrence (k >= 2)
// receives the suffix _v<k>, k written with 'width' digits (initially 2: _v02).
// If the result still contains duplicates (an original name equals a new one),
// start again from the original names with one more digit (_v002), and so on.
fn unique_names(names: &[Vec<u8>]) -> Vec<Vec<u8>> {
    let mut width = 2;
    loop {
        let mut count: HashMap<&[u8], usize> = HashMap::new();
        let mut result = Vec::with_capacity(names.len());
        for name in names {
            let c = count.entry(name.as_slice()).or_insert(0);
            *c += 1;
            let mut new_name = name.clone();
            if *c >= 2 {
                let suffix = format!("_v{:0width$}", *c, width = width);
                new_name.extend_from_slice(suffix.as_bytes());
            }
            result.push(new_name);
        }
        if duplicates(&result).is_empty() {
            return result;
        }
        width += 1;
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(xs: &[&str]) -> Vec<Vec<u8>> {
        xs.iter().map(|x| x.as_bytes().to_vec()).collect()
    }

    #[test]
    fn unique() {
        assert_eq!(unique_names(&bytes(&["a", "b", "c"])),            bytes(&["a", "b", "c"]));
        assert_eq!(unique_names(&bytes(&["a", "b", "a", "a"])),       bytes(&["a", "b", "a_v02", "a_v03"]));
        assert_eq!(unique_names(&bytes(&["foo", "foo", "foo_v02"])),  bytes(&["foo", "foo_v002", "foo_v02"]));
        assert_eq!(unique_names(&bytes(&["x", "x", "x_v02", "x_v002"])),
                   bytes(&["x", "x_v0002", "x_v02", "x_v002"]));
    }

    #[test]
    fn dups() {
        assert!(duplicates(&bytes(&["a", "b"])).is_empty());
        assert_eq!(duplicates(&bytes(&["a", "b", "b", "a", "b"])), bytes(&["b", "a"]));
    }

    #[test]
    fn header() {
        let mut row = Row::new();
        row.line.extend_from_slice(b"\ta\tb");
        row.split_fields();
        let names = ColumnNames::from_header(&row, false, false).unwrap();
        assert_eq!(names.name(0), b"_");
        assert_eq!(names.index_of(b"b"), Some(2));
        assert_eq!(names.index_of(b"zz"), None);

        let mut row = Row::new();
        row.line.extend_from_slice(b"a\tb\ta");
        row.split_fields();
        assert!(ColumnNames::from_header(&row, false, false).is_err());
        let names = ColumnNames::from_header(&row, true, true).unwrap();
        assert_eq!(names.len(), 4);
        assert_eq!(names.name(3), b"a_v02");
        assert_eq!(names.index_of(b"_"), Some(0));
    }
}
