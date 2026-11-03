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

    // Look up each name. Unknown names and not yet supported syntax are errors.
    // A name given more than once is kept once, at its first position; this is
    // reported on STDERR as pick does.
    pub fn from_names(args: &[Vec<u8>], names: &ColumnNames) -> Result<OutputSelection, String> {
        let mut output_colnames: Vec<Vec<u8>> = Vec::with_capacity(args.len());
        let mut output_offset:   Vec<usize>   = Vec::with_capacity(args.len());
        let mut have_duplicate = false;

        for arg in args {
            check_supported(arg)?;
            let i = match names.index_of(arg) {
                Some(i) => i,
                None    => return Err(unknown_name_message(arg, names)),
            };
            if output_offset.contains(&i) {
                have_duplicate = true;
                continue;
            }
            output_colnames.push(arg.clone());
            output_offset.push(i);
        }
        if have_duplicate {
            eprintln!("-- Identical output column names exist (will make unique)");
        }
        Ok(OutputSelection { output_colnames, output_offset })
    }
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
