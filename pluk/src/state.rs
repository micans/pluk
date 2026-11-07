// Previous-row and group state (plan/step23.txt): --pstore, --group,
// --group-first-ref. The machine holds the kept values that pload reads and the
// groupi and groupno values; Pstore decides, after each row's computes, what is kept
// and whether the row starts a group.

use crate::exec;
use crate::names;
use crate::opt;
use crate::row;

// --pstore: the column of each kept slot (the slots of pload first, then the other
// LIST names); with --group the group column and the state of the groups.
pub struct Pstore {
    columns: Vec<usize>,
    group:   Option<Group>,
}

// --group=COL, --group-first-ref=COL (plan/step23.txt decision 3).
struct Group {
    column:    usize,
    first_ref: bool,
    value:     Option<Vec<u8>>,     // the group column's value in the current group
    n:         u64,                 // groups so far
    i:         u64,                 // position of the next row in the current group
}

impl Pstore {
    // The columns and their values before the first row; the group column.
    pub fn new(spec: &opt::Pstore, pload: &[Vec<u8>], names: &names::ColumnNames, group: Option<&(Vec<u8>, bool)>)
        -> Result<(Pstore, Vec<Option<Vec<u8>>>), String>
    {
        let group = match group {
            None => None,
            Some((col, first_ref)) => {
                let column = names.index_of(col).ok_or_else(|| format!("--group: [{}] is not a column name among {}",
                                                                       String::from_utf8_lossy(col), names.joined()))?;
                Some(Group { column, first_ref: *first_ref, value: None, n: 0, i: 0 })
            }
        };
        let mut kept: Vec<Vec<u8>> = pload.to_vec();
        for (k, _) in &spec.list {
            if !kept.contains(k) {
                kept.push(k.clone());
            }
        }
        let mut columns = Vec::with_capacity(kept.len());
        let mut initial = Vec::with_capacity(kept.len());
        for name in &kept {
            let i = names.index_of(name).ok_or_else(|| format!("--pstore/pload: [{}] is not a column name among {}",
                                                               String::from_utf8_lossy(name), names.joined()))?;
            columns.push(i);
            let given = spec.list.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone());
            initial.push(given.or_else(|| spec.default.clone()));
        }
        Ok((Pstore { columns, group }, initial))
    }

    // After the computes, as pick's do_cache: with groups, a row whose group column
    // differs from the previous row's starts a group and is skipped (true); the kept
    // columns are then taken from it. Within a group they are taken from every row
    // (--group) or not at all (--group-first-ref: the first row stays the reference).
    // groupi and groupno are set for the next row.
    pub fn keep(&mut self, row: &row::Row, machine: &mut exec::Machine) -> bool {
        let mut skip = false;
        if let Some(g) = &mut self.group {
            let value = row.field(g.column);
            if g.value.as_deref() != Some(value) {
                g.value = Some(value.to_vec());
                g.n += 1;
                g.i = 2;
                skip = true;
            } else {
                g.i += 1;
                if g.first_ref {
                    (machine.groupi, machine.groupno) = (g.i, g.n);
                    return false;
                }
            }
            (machine.groupi, machine.groupno) = (g.i, g.n);
        }
        let kept = &mut machine.kept;
        for (slot, &i) in self.columns.iter().enumerate() {
            let value = kept[slot].get_or_insert_with(Vec::new);
            value.clear();
            value.extend_from_slice(row.field(i));
        }
        skip
    }

    // The number of groups (each first row skipped), for the report at the end.
    pub fn groups(&self) -> Option<u64> {
        self.group.as_ref().map(|g| g.n)
    }
}
