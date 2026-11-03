// Output column selection: from the column names given on the command line
// to the 0-based column indexes that will be output.
//
// Only plain names are accepted for now; see plan/COLSPEC.design.txt.

use crate::names::ColumnNames;

pub struct OutputSelection {
    pub output_colnames: Vec<Vec<u8>>,  // names to output, duplicates removed, in given order
    pub output_offset:   Vec<usize>,    // the corresponding 0-based column indexes
}

impl OutputSelection {

    // Output the named columns, in the given order. A name given more than once is
    // kept once, at its first position; this is reported on STDERR as pick does.
    pub fn from_names(args: &[Vec<u8>], names: &ColumnNames) -> Result<OutputSelection, String> {
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
    pub fn all(args: &[Vec<u8>], names: &ColumnNames) -> Result<OutputSelection, String> {
        lookup(args, names)?;
        Ok(OutputSelection::from_offsets((0..names.len()).collect(), names))
    }

    // -x: output all columns except the named ones, in input order.
    pub fn complement(args: &[Vec<u8>], names: &ColumnNames) -> Result<OutputSelection, String> {
        let excluded = lookup(args, names)?;
        let output_offset = (0..names.len()).filter(|i| !excluded.contains(i)).collect();
        Ok(OutputSelection::from_offsets(output_offset, names))
    }

    fn from_offsets(output_offset: Vec<usize>, names: &ColumnNames) -> OutputSelection {
        let output_colnames = output_offset.iter().map(|&i| names.name(i).to_vec()).collect();
        OutputSelection { output_colnames, output_offset }
    }
}

// The 0-based index of each name, in the given order, duplicates included.
// Unknown names and not yet supported syntax are errors.
fn lookup(args: &[Vec<u8>], names: &ColumnNames) -> Result<Vec<usize>, String> {
    let mut offsets = Vec::with_capacity(args.len());
    for arg in args {
        check_supported(arg)?;
        match names.index_of(arg) {
            Some(i) => offsets.push(i),
            None    => return Err(unknown_name_message(arg, names)),
        }
    }
    Ok(offsets)
}

// Reject arguments that will acquire a different meaning in later steps
// (plan/step2.txt item 1, plan/COLSPEC.design.txt).
fn check_supported(arg: &[u8]) -> Result<(), String> {
    let what = if arg.starts_with(b"+") {
        Some("regex column names (leading '+')")
    } else if arg.contains(&b':') {
        Some("lists and computes (':')")
    } else if arg.contains(&b'@') {
        Some("selections ('@')")
    } else if arg.contains(&b'%') {
        Some("%-encoding ('%')")
    } else {
        None
    };
    match what {
        Some(w) => Err(format!("[{}]: {} not yet supported", String::from_utf8_lossy(arg), w)),
        None    => Ok(()),
    }
}

// pick's message, listing all column names separated by commas.
fn unknown_name_message(arg: &[u8], names: &ColumnNames) -> String {
    let all: Vec<String> = (0..names.len())
        .map(|i| String::from_utf8_lossy(names.name(i)).into_owned())
        .collect();
    format!("(Output) handle [{}] in output does not reference a known entity among {}",
            String::from_utf8_lossy(arg), all.join(","))
}
