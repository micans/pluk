mod bam;
mod colspec;
mod compute;
mod demux;
mod dict;
mod exec;
mod filter;
mod io;
mod names;
mod num;
mod ops;
mod opt;
mod output;
mod row;
// Not yet called; wired in with the first SAM operator step (plan/step5.txt item 3).
#[allow(dead_code)]
mod samseq;
mod select;
mod sink;

use std::io::{BufWriter, ErrorKind, Write};
use std::os::unix::ffi::OsStrExt;
use std::process;

// Output buffer size. Rust's stdout flushes at every newline by default;
// wrapping it in a BufWriter avoids a write system call per line.
const OUT_BUFSIZE: usize = 128 * 1024;

fn run() -> std::io::Result<()> {
    let config = opt::parse_args();

    // Temporary (step10): the parsed computes; the compiled form follows once the
    // header has been read.
    if config.test_compute {
        for spec in &config.computes {
            print!("{}", compute::describe(spec));
        }
    }

    // Temporary step1 output: line number and line (or one field with --test-i).
    let lineno_mode = config.test_lineno || config.test_i.is_some();

    // As in pick: without column names there is nothing to do; checked before reading input.
    if config.columns.is_empty() && !config.computes.iter().any(|c| c.output)
        && !config.all && !config.excise && !config.c
        && !lineno_mode && !config.test_names && !config.test_select && !config.test_dict
        && !config.test_samsort && !config.test_compute
    {
        eprintln!("pluk: no columns given. Use --help for help, -h suppresses header line output");
        eprintln!("pluk only works as a pipe, so invoke e.g. as: pluk foo bar < data.txt");
        return Ok(());
    }

    // Dictionaries are read once all options are known, before any input is read
    // (plan/step4.txt, design considerations).
    // Dictionaries named by a dord record the order of their keys while read (step15).
    let mut dicts = match dict::Dicts::load(&config.dicts, &compute::dord_dictionaries(&config.computes)) {
        Ok(dicts) => dicts,
        Err(msg)  => {
            eprintln!("pluk: {}", msg);
            process::exit(1);
        }
    };

    // --other and --assert-other are created before input is read, so that a bad
    // path is reported early.
    let mut other = config.other.as_deref().map(|n| sink::SideFile::create("--other", n)).transpose()?;
    let mut assert_other = config.assert_other.as_deref()
        .map(|n| sink::SideFile::create("--assert-other", n)).transpose()?;
    if let Some(dir) = &config.demux_dir {
        if !std::path::Path::new(std::ffi::OsStr::from_bytes(dir)).is_dir() {
            return Err(std::io::Error::new(ErrorKind::NotFound,
                       format!("--demux-dir: [{}] is not a directory", String::from_utf8_lossy(dir))));
        }
    }

    // As in pick: header output on by default, off with -k (not with -K); -h and -c always
    // switch it off.
    #[allow(non_snake_case)]
    // -K asks for the output header line that -k would leave out.
    let B_print_header = (!config.k || config.big_k) && !config.no_header && !config.c;
    // The header rule for the files of --other, --assert-other and --demux: as
    // stdout, except that -c concerns stdout only.
    let file_header = (!config.k || config.big_k) && !config.no_header;

    // -/PAT and -//PAT; an invalid pattern is reported like an option error.
    let compile = |pat: &Option<Vec<u8>>, what: &str| match pat {
        None    => None,
        Some(p) => match io::compile_pattern(p, what) {
            Ok(re)   => Some(re),
            Err(msg) => {
                eprintln!("{}", msg);
                process::exit(1);
            }
        },
    };

    let settings = io::InputSettings {
        pat_purge:             compile(&config.pat_purge, "-/PAT"),
        pat_pass:              compile(&config.pat_pass, "-//PAT"),
        B_header:              !config.k,
        omega:                 config.omega.map(|n| n as usize),
        add_row_column:        config.add_row_column,
        make_unique:           config.make_unique,
        threads:               io::bam_threads(config.threads),
        sam:                   if config.sam_h    { io::SamMode::PassThrough }
                               else if config.sam { io::SamMode::Skip }
                               else               { io::SamMode::Off },
    };
    let mut input = io::LineReader::from_stdin(settings)?;

    // SAM sequence lengths are known once the header has been read (plan/step4.txt item 4a).
    if config.sam || config.sam_h {
        dicts.add_sam_lengths(&input.sam_seqlen);
    }

    // Temporary step5 output: the SAM sort order and grouping from the @HD line.
    if config.test_samsort {
        let order = &input.sam_order;
        println!("SO\t{}\nGO\t{}\ngrouped_by_read\t{}", order.sort_name(), order.group_name(),
                 if order.grouped_by_read() { "yes" } else { "no" });
        return Ok(());
    }

    if config.test_dict {
        let mut out = BufWriter::with_capacity(OUT_BUFSIZE, std::io::stdout().lock());
        dicts.write_test(&mut out)?;
        out.flush()?;
        return Ok(());
    }
    let mut out = BufWriter::with_capacity(OUT_BUFSIZE, std::io::stdout().lock());

    // --test-i N is 1-based on the command line, field indexes are 0-based.
    let test_column: Option<usize> = config.test_i.map(|n| n as usize - 1);

    // Column names from the command line, as bytes (see opt.rs).
    let column_args = &config.columns;

    // Computes are compiled once the column names are known (with empty input there
    // is nothing to compile). 'names' are the input names followed by the compute
    // names: output, @ filters and --demux see them; @@ filters only the input names.
    let (computes, names) = if input.is_empty {
        (Vec::new(), input.names.clone())
    } else {
        compute::compile(&config.computes, &input.names, config.inplace, &dicts)
            .map_err(|msg| std::io::Error::new(ErrorKind::InvalidInput, msg))?
    };
    let computed: Vec<(usize, usize)> = computes.iter().filter(|c| c.output).map(|c| (c.position, c.index)).collect();

    // As in pick, the names are checked once the first line has been read;
    // with empty input there is nothing to check against.
    let selection = if input.is_empty {
        select::OutputSelection { output_colnames: Vec::new(), output_offset: Vec::new() }
    } else {
        // As in pick, -x takes precedence over -A.
        let result = if config.excise {
            select::OutputSelection::complement(column_args, &computed, &names)
        } else if config.all {
            select::OutputSelection::all(column_args, &computed, config.insert_offset, &names)
        } else {
            select::OutputSelection::from_names(column_args, &computed, &names)
        };
        result.map_err(|msg| std::io::Error::new(ErrorKind::InvalidInput, msg))?
    };

    // Filters are compiled once the column names are known.
    let filters = if input.is_empty {
        filter::Filters::none()
    } else {
        filter::Filters::compile(&config.filters, &input.names, &names, &dicts, config.or_pre, config.or_post)
            .map_err(|msg| std::io::Error::new(ErrorKind::InvalidInput, msg))?
    };

    if config.test_compute {
        for c in &computes {
            print!("{}", compute::describe_compiled(c, &names));
        }
        return Ok(());
    }
    let mut stages = Stages { filters, computes, machine: exec::Machine::new(config.num_perl, config.seed.unwrap_or_else(exec::clock_seed), &dicts) };

    if config.test_select {
        for (name, i) in selection.output_colnames.iter().zip(&selection.output_offset) {
            out.write_all(name)?;
            writeln!(out, "\t{}", i)?;
        }
        out.flush()?;
        return Ok(());
    }

    if config.test_names {
        for i in 0..input.names.len() {
            let name = input.names.name(i);
            let lookup = input.names.index_of(name).map_or("none".to_string(), |j| j.to_string());
            write!(out, "{}\t", i)?;
            out.write_all(name)?;
            writeln!(out, "\t{}", lookup)?;
        }
        out.flush()?;
        return Ok(());
    }

    // Pass-through lines (--sam-h) are output in all modes, as in pick; those
    // before the first data line come first.
    for line in &input.passthrough_preamble {
        output::write_line(&mut out, line)?;
    }

    // --other and --assert-other: the preamble, then the header following -h and
    // -k (not -c, which concerns stdout), then their rows. With -c and no column
    // arguments the rows are written whole (pick writes empty lines).
    if !input.is_empty {
        let all = config.c && column_args.is_empty() && !config.all && !config.excise;
        let offsets: Vec<usize> = if all { (0..input.n_columns).collect() } else { selection.output_offset.clone() };
        let header = if file_header { Some(&names) } else { None };
        for side in [&mut other, &mut assert_other].into_iter().flatten() {
            let writer = output::RowWriter::new(&offsets, input.n_columns, input.settings.omega.is_some());
            side.start(writer, &input.passthrough_preamble, header)?;
        }
    }

    // --demux: the output rows go to files, each starting with the preamble and
    // the header (following -h and -k), instead of stdout.
    let mut demux = match &config.demux {
        Some(name) if !input.is_empty => {
            let column = names.index_of(name).ok_or_else(|| std::io::Error::new(ErrorKind::InvalidInput,
                format!("--demux: column [{}] not found among {}", String::from_utf8_lossy(name), names.joined())))?;
            let writer = output::RowWriter::new(&selection.output_offset, input.n_columns, input.settings.omega.is_some());
            let mut start = Vec::new();
            for line in &input.passthrough_preamble {
                output::write_line(&mut start, line)?;
            }
            if file_header {
                writer.write_header(&mut start, &names)?;
            }
            let taken = [&config.other, &config.assert_other].into_iter().flatten().cloned().collect();
            Some(demux::Demux::new(column, config.demux_dir.clone(), writer, start, taken))
        }
        _ => None,
    };

    if config.c {
        // -c: output only the number of data rows that pass the filters.
        let mut n_kept: u64 = 0;
        while let Some(kind) = input.next_line()? {
            match kind {
                io::LineKind::Data        => if route(&mut stages, &mut input.row, input.n_lines,
                                                          &mut other, &mut assert_other, &mut None)? {
                    n_kept += 1
                },
                io::LineKind::PassThrough => output::write_line(&mut out, &input.row.line)?,
            }
        }
        writeln!(out, "{}", n_kept)?;
    } else if lineno_mode {
        if input.header_lineno > 0 {
            output::write_numbered(&mut out, input.header_lineno, &input.header, test_column)?;
        }
        while let Some(kind) = input.next_line()? {
            match kind {
                io::LineKind::Data        => output::write_numbered(&mut out, input.n_lines, &input.row, test_column)?,
                io::LineKind::PassThrough => output::write_line(&mut out, &input.row.line)?,
            }
        }
    } else {
        let writer = output::RowWriter::new(&selection.output_offset, input.n_columns,
                                            input.settings.omega.is_some());
        if B_print_header && !input.is_empty && demux.is_none() {
            writer.write_header(&mut out, &names)?;
        }
        while let Some(kind) = input.next_line()? {
            match kind {
                io::LineKind::Data => if route(&mut stages, &mut input.row, input.n_lines,
                                               &mut other, &mut assert_other, &mut demux)? {
                    match &mut demux {
                        Some(d) => d.write(&input.row, input.n_lines)?,
                        None => writer.write_row(&mut out, &input.row)?,
                    }
                },
                io::LineKind::PassThrough => output::write_line(&mut out, &input.row.line)?,
            }
        }
    }
    out.flush()?;
    if let Some(d) = demux {
        d.finish()?;
    }
    if let Some(file) = other {
        file.finish()?;
    }
    if let Some(file) = assert_other {
        if file.n_rows > 0 {
            eprintln!("pluk: {} row{} failed an assertion (written to {})", file.n_rows,
                      if file.n_rows == 1 { "" } else { "s" }, String::from_utf8_lossy(&file.name));
        }
        file.finish()?;
    }
    stages.filters.report_na();

    if config.test_counts {
        eprintln!("columns={} lines={} skipped={} accepted={} short={} overflow={}",
            input.n_columns, input.n_lines, input.n_skipped, input.n_accepted,
            input.n_short_rows, input.n_overflow_rows);
        if input.settings.sam != io::SamMode::Off || input.settings.pat_pass.is_some() {
            eprintln!("passthrough={}", input.n_passthrough);
        }
    }
    Ok(())
}

// What happens to a data row before output: the @@ stage, the computes, the @
// stage (plan/COMPUTE.design.txt section 1).
struct Stages<'d> {
    filters:  filter::Filters<'d>,
    computes: Vec<compute::Compute>,
    machine:  exec::Machine<'d>,
}

impl Stages<'_> {
    // The computed fields of the previous row are removed first, so that a row
    // rejected by the @@ stage has none (they read as empty).
    fn judge(&mut self, row: &mut row::Row) -> Result<filter::Verdict, Failure> {
        row.clear_computed();
        if !self.filters.pre(row).map_err(Failure::Assertion)? {
            return Ok(filter::Verdict::RejectPre);
        }
        if !self.computes.is_empty() {
            self.machine.run(&self.computes, row).map_err(Failure::Fatal)?;
        }
        let kept = self.filters.post(row).map_err(Failure::Assertion)?;
        Ok(if kept { filter::Verdict::Keep } else { filter::Verdict::RejectPost })
    }
}

// Why a row could not be judged: an assertion failed (diverted with
// --assert-other), or an operator failed (map with __EXIT__), which ends the run.
enum Failure {
    Assertion(String),
    Fatal(String),
}

// Whether the current data row is kept. A row rejected by a filter is written to
// --other, and if rejected by an @ filter also counted by --demux (which opens
// its file); a row that fails an assertion is written to --assert-other, or else
// ends the run with an error giving the line number.
fn route(stages: &mut Stages, row: &mut row::Row, lineno: u64,
         other: &mut Option<sink::SideFile>, assert_other: &mut Option<sink::SideFile>,
         demux: &mut Option<demux::Demux>)
    -> std::io::Result<bool>
{
    match stages.judge(row) {
        Ok(filter::Verdict::Keep) => Ok(true),
        Ok(verdict) => {
            if let Some(file) = other {
                file.write_row(row)?;
            }
            if let (filter::Verdict::RejectPost, Some(d)) = (verdict, demux) {
                d.filtered(row, lineno)?;
            }
            Ok(false)
        }
        Err(Failure::Assertion(msg)) => match assert_other {
            Some(file) => {
                file.write_row(row)?;
                Ok(false)
            }
            None => Err(std::io::Error::new(ErrorKind::InvalidData, format!("{} at line {}", msg, lineno))),
        },
        Err(Failure::Fatal(msg)) => Err(std::io::Error::new(ErrorKind::InvalidData, format!("{} at line {}", msg, lineno))),
    }
}

fn main() {
    if let Err(e) = run() {
        // Output closed by the reader, e.g. pluk ... | head. Not an error.
        if e.kind() == ErrorKind::BrokenPipe {
            process::exit(0);
        }
        eprintln!("pluk: {}", e);
        process::exit(1);
    }
}
