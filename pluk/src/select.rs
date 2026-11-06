// Output column selection: from the column specifications given on the command
// line (plain names, ':' lists, '+' regexes; plan/COLSPEC.design.txt) to the
// 0-based column indexes that will be output.

use crate::colspec::ColSpec;
use crate::names::ColumnNames;

pub struct OutputSelection {
    pub output_colnames: Vec<Vec<u8>>,  // names to output, duplicates removed, in given order
    pub output_offset:   Vec<usize>,    // the corresponding 0-based column indexes
}

impl OutputSelection {

    // Output the named columns, in the given order. A name given more than once is
    // kept once, at its first position; this is reported on STDERR as pick does.
    pub fn from_names(args: &[ColSpec], names: &ColumnNames) -> Result<OutputSelection, String> {
        let mut output_offset: Vec<usize> = Vec::with_capacity(args.len());
        let mut have_duplicate = false;
        for i in lookup(args, names)? {
            if output_offset.contains(&i) {
                have_duplicate = true;
            } else {
                output_offset.push(i);
            }
        }
        if have_duplicate {
            eprintln!("-- Identical output column names exist (will make unique)");
        }
        Ok(OutputSelection::from_offsets(output_offset, names))
    }

    // -A: output all columns, in input order. Names given as well must exist, but
    // add nothing (as in pick, where they are duplicates of columns already present).
    pub fn all(args: &[ColSpec], names: &ColumnNames) -> Result<OutputSelection, String> {
        lookup(args, names)?;
        Ok(OutputSelection::from_offsets((0..names.len()).collect(), names))
    }

    // -x: output all columns except the named ones, in input order.
    pub fn complement(args: &[ColSpec], names: &ColumnNames) -> Result<OutputSelection, String> {
        let excluded = lookup(args, names)?;
        let output_offset = (0..names.len()).filter(|i| !excluded.contains(i)).collect();
        Ok(OutputSelection::from_offsets(output_offset, names))
    }

    fn from_offsets(output_offset: Vec<usize>, names: &ColumnNames) -> OutputSelection {
        let output_colnames = output_offset.iter().map(|&i| names.name(i).to_vec()).collect();
        OutputSelection { output_colnames, output_offset }
    }
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
