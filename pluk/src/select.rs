// Output column selection: from the column specifications given on the command
// line (plain names, ':' lists, '+' regexes; plan/COLSPEC.design.txt) to the
// 0-based column indexes that will be output.
//
// With computes (plan/COMPUTE.design.txt), names are the extended names (input
// columns, then computes), so a column argument can name a compute; computes with
// '::' are output at their place among the column arguments ('computed': pairs of
// that place and the column index), or after the input columns with -A and -x.

use crate::colspec::ColSpec;
use crate::names::ColumnNames;

pub struct OutputSelection {
    pub output_colnames: Vec<Vec<u8>>,  // names to output, duplicates removed, in given order
    pub output_offset:   Vec<usize>,    // the corresponding 0-based column indexes
}

impl OutputSelection {

    // Output the named columns and '::' computes, in the given order. A column
    // given more than once is kept once, at its first position; this is reported
    // on STDERR as pick does. A compute shows its name's final binding
    // (INPLACE.design.txt section 2), as do names given as column arguments.
    pub fn from_names(args: &[ColSpec], computed: &[(usize, usize)], names: &ColumnNames)
        -> Result<OutputSelection, String>
    {
        let mut wanted = Vec::with_capacity(args.len() + computed.len());
        for (position, spec) in args.iter().enumerate() {
            wanted.extend(computed.iter().filter(|(p, _)| *p == position).map(|&(_, i)| names.current(i)));
            wanted.extend(spec.expand(names, "Output", "output")?);
        }
        wanted.extend(computed.iter().filter(|(p, _)| *p >= args.len()).map(|&(_, i)| names.current(i)));
        let mut output_offset: Vec<usize> = Vec::with_capacity(wanted.len());
        let mut have_duplicate = false;
        for i in wanted {
            if output_offset.contains(&i) {
                have_duplicate = true;
            } else {
                output_offset.push(i);
            }
        }
        if have_duplicate {
            crate::note::note!("-- Identical output column names exist (will make unique)");
        }
        Ok(OutputSelection::from_offsets(output_offset, names))
    }

    // -A: output all input columns, in input order, and the '::' computes that do
    // not redefine an input column: after the input columns, or with -A<N> inserted
    // after the first N input columns (N < 0 counts from the right; clamped to the
    // ends, as pick). A redefined input column shows its final binding at its own
    // position. Names given as well must exist, but add nothing (as in pick, where
    // they are duplicates of columns already present).
    pub fn all(args: &[ColSpec], computed: &[(usize, usize)], insert: Option<i64>, names: &ColumnNames)
        -> Result<OutputSelection, String>
    {
        lookup(args, names)?;
        let mut output_offset: Vec<usize> = (0..names.n_input()).map(|i| names.current(i)).collect();
        let new = new_columns(computed, &output_offset, names);
        let n = output_offset.len() as i64;
        let at = match insert {
            None => n,
            Some(k) if k >= 0 => k.min(n),
            Some(k) => (n + k).max(0),
        } as usize;
        output_offset.splice(at..at, new);
        Ok(OutputSelection::from_offsets(output_offset, names))
    }

    // -x: output all input columns except the named ones, in input order, then the
    // new '::' computes as with -A. A named column that a '::' compute redefines is
    // kept (as pick: '::' asks for output).
    pub fn complement(args: &[ColSpec], computed: &[(usize, usize)], names: &ColumnNames)
        -> Result<OutputSelection, String>
    {
        let excluded = lookup(args, names)?;
        let recomputed: Vec<usize> = computed.iter().map(|&(_, i)| names.current(i)).collect();
        let input: Vec<usize> = (0..names.n_input()).map(|i| names.current(i))
            .filter(|f| !excluded.contains(f) || recomputed.contains(f)).collect();
        let all_input: Vec<usize> = (0..names.n_input()).map(|i| names.current(i)).collect();
        let output_offset = input.iter().copied().chain(new_columns(computed, &all_input, names)).collect();
        Ok(OutputSelection::from_offsets(output_offset, names))
    }

    fn from_offsets(output_offset: Vec<usize>, names: &ColumnNames) -> OutputSelection {
        let output_colnames = output_offset.iter().map(|&i| names.name(i).to_vec()).collect();
        OutputSelection { output_colnames, output_offset }
    }
}

// The '::' computes that are new columns (not redefinitions of the input columns
// 'input', given as their current indexes), as current indexes, without repeats.
fn new_columns(computed: &[(usize, usize)], input: &[usize], names: &ColumnNames) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::new();
    for &(_, i) in computed {
        let f = names.current(i);
        if !input.contains(&f) && !out.contains(&f) {
            out.push(f);
        }
    }
    out
}

// The 0-based indexes selected by the specifications, in the given order,
// duplicates included. Unknown names and regexes without a match are errors.
fn lookup(args: &[ColSpec], names: &ColumnNames) -> Result<Vec<usize>, String> {
    let mut offsets = Vec::with_capacity(args.len());
    for spec in args {
        offsets.extend(spec.expand(names, "Output", "output")?);
    }
    Ok(offsets)
}
