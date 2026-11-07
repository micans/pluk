mod bam;
mod colspec;
mod compute;
mod demux;
mod dict;
mod exec;
mod filter;
mod io;
mod list;
mod names;
mod note;
mod num;
mod ops;
mod opt;
mod output;
mod row;
mod sam;
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
    note::set_quiet(config.quiet);

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
        compute::compile(&config.computes, &input.names, config.inplace, config.sam || config.sam_h, &dicts)
            .map_err(|msg| std::io::Error::new(ErrorKind::InvalidInput, msg))?
    };
    let computed: Vec<(usize, usize)> = computes.iter().filter(|c| c.output).map(|c| (c.position, c.index)).collect();

    // As in pick, the names are checked once the first line has been read;
    // with empty input there is nothing to check against.
    // --A=NAME: as -A<N> with N the position of input column NAME.
    let insert_offset = match &config.insert_after {
        Some(name) if !input.is_empty => {
            let at = (0..names.n_input()).find(|&i| names.name(i) == &name[..]).ok_or_else(||
                std::io::Error::new(ErrorKind::InvalidInput, format!("--A={}: no input column [{}] among {}",
                    String::from_utf8_lossy(name), String::from_utf8_lossy(name), input.names.joined())))?;
            Some(at as i64 + 1)
        }
        _ => config.insert_offset,
    };
    let selection = if input.is_empty {
        select::OutputSelection { output_colnames: Vec::new(), output_offset: Vec::new() }
    } else {
        // As in pick, -x takes precedence over -A.
        let result = if config.excise {
            select::OutputSelection::complement(column_args, &computed, &names)
        } else if config.all {
            select::OutputSelection::all(column_args, &computed, insert_offset, &names)
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
    let mut machine = exec::Machine::new(config.num_perl, config.seed.unwrap_or_else(exec::clock_seed), &dicts);
    machine.set_registers(&computes);
    let normal_form = (config.sam || config.sam_h) && !config.sam_nonf;
    // The fill is active when an operator needs the query sequence (step17 decision 8).
    let fill = if (config.sam || config.sam_h) && compute::needs_query(&computes) {
        if !input.sam_order.grouped_by_read() {
            note::note!("pluk: the SAM input is not grouped by read (@HD SO:{} GO:{}); a SEQ '*' is filled only \
                         from earlier rows of the same read", input.sam_order.sort_name(), input.sam_order.group_name());
        }
        Some(Fill::new(config.sam_seqmiss_exit))
    } else {
        None
    };
    let mut stages = Stages { filters, computes, machine, rowno: 0, normal_form, fill };

    // --idx-list, --name-list, --idx-map: the output columns, without reading the data.
    if let Some(kind) = config.list {
        if !input.is_empty {
            write_list(&mut out, kind, &selection, names.n_input())?;
        }
        out.flush()?;
        return Ok(());
    }

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

    // Output rows: those that pass the filters, to stdout or --demux (for -E<N>).
    let mut n_output: u64 = 0;
    let mut progress = config.progress.map(|(every, per_line)| Progress { every, per_line, rows: 0 });
    if config.c {
        // -c: output only the number of data rows that pass the filters.
        while let Some(kind) = input.next_line()? {
            if let (io::LineKind::Data, Some(p)) = (&kind, &mut progress) {
                p.tick();
            }
            match kind {
                io::LineKind::Data        => if route(&mut stages, &mut input.row, input.n_lines,
                                                          &mut other, &mut assert_other, &mut None)? {
                    n_output += 1
                },
                io::LineKind::PassThrough => output::write_line(&mut out, &input.row.line)?,
            }
        }
        writeln!(out, "{}", n_output)?;
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
            if let (io::LineKind::Data, Some(p)) = (&kind, &mut progress) {
                p.tick();
            }
            match kind {
                io::LineKind::Data => if route(&mut stages, &mut input.row, input.n_lines,
                                               &mut other, &mut assert_other, &mut demux)? {
                    n_output += 1;
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
    if progress.is_some() {
        eprintln!();
    }
    if let Some(d) = demux {
        d.finish()?;
    }
    if let Some(file) = other {
        file.finish()?;
    }
    if let Some(file) = assert_other {
        if file.n_rows > 0 {
            note::note!("pluk: {} row{} failed an assertion (written to {})", file.n_rows,
                      if file.n_rows == 1 { "" } else { "s" }, String::from_utf8_lossy(&file.name));
        }
        file.finish()?;
    }
    stages.filters.report_na();
    stages.machine.sam.report();
    if let Some(fill) = &stages.fill {
        fill.report();
    }

    // -E<N>: checked at the end; the rows have been written (as pick).
    if let Some(n) = config.expect {
        if n != n_output && !lineno_mode {
            return Err(std::io::Error::new(ErrorKind::InvalidData,
                       format!("-E{}: expected {} output row{}, found {}", n, n, if n == 1 { "" } else { "s" }, n_output)));
        }
    }

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

// --idx-list, --name-list, --idx-map (plan/step16.txt decision 9): indexes are
// 1-based; a computed column is numbered after the input columns (as pick), with
// a note.
fn write_list(out: &mut impl Write, kind: opt::ListKind, selection: &select::OutputSelection, n_input: usize)
    -> std::io::Result<()>
{
    let pairs: Vec<(usize, &Vec<u8>)> = selection.output_offset.iter().map(|&i| i + 1)
        .zip(&selection.output_colnames).collect();
    match kind {
        opt::ListKind::Indexes => {
            let line: Vec<String> = pairs.iter().map(|(i, _)| i.to_string()).collect();
            writeln!(out, "{}", line.join("\t"))?;
        }
        opt::ListKind::Names => {
            let line: Vec<&[u8]> = pairs.iter().map(|(_, n)| &n[..]).collect();
            out.write_all(&line.join(&b'\t'))?;
            writeln!(out)?;
        }
        opt::ListKind::Map => for (i, name) in &pairs {
            write!(out, "{}\t", i)?;
            out.write_all(name)?;
            writeln!(out)?;
        },
    }
    if pairs.iter().any(|&(i, _)| i > n_input) {
        note::note!("pluk: the list contains computed columns (numbered after the {} input columns)", n_input);
    }
    Ok(())
}

// --progress (plan/step16.txt decision 10): a dot on stderr every 'every' input data
// rows, rejected or not, and after every 'per_line' dots the count and a newline.
struct Progress {
    every:    u64,
    per_line: u64,
    rows:     u64,
}

impl Progress {
    fn tick(&mut self) {
        self.rows += 1;
        if self.rows % self.every == 0 {
            eprint!(".");
            if self.per_line > 0 && self.rows % self.every.saturating_mul(self.per_line) == 0 {
                eprintln!(" {:9}", self.rows);
            }
        }
    }
}

// What happens to a data row before output: the @@ stage, the computes, the @
// stage (plan/COMPUTE.design.txt section 1).
struct Stages<'d> {
    filters:  filter::Filters<'d>,
    computes: Vec<compute::Compute>,
    machine:  exec::Machine<'d>,
    rowno:    u64,              // data rows seen (rowno, r0wno); rejected rows count too
    normal_form: bool,          // SAM input without --sam-nonf (plan/SAMVIEW.design.txt section 3)
    fill:     Option<Fill>,     // the sequence fill, when an operator needs the query (SAMSEQ.design.txt)
}

// Why a SEQ '*' could not be filled (SAMSEQ.design.txt sections 2-5).
const FILL_REASONS: [&str; 4] = [
    "segment ambiguous (FLAG 0x1 with 0x40 and 0x80 both set or both unset)",
    "CIGAR '*' or not valid, so the part of the read is not known",
    "the stored sequences of the read have another length",
    "no earlier row of the read holds the sequence",
];
const FILL_MESSAGES: u64 = 5;   // per-row messages, then only the counts

// The sequence fill: the cache of the current read, the mode and the counts.
struct Fill {
    cache:  samseq::GroupCache,
    exit:   bool,               // --sam-seqmiss=exit
    filled: u64,
    missed: [u64; 4],           // per reason in FILL_REASONS
}

impl Fill {
    fn new(exit: bool) -> Fill {
        Fill { cache: samseq::GroupCache::new(), exit, filled: 0, missed: [0; 4] }
    }

    // The query for this row, or why the row cannot go on: Ok(None) skip, Err exit.
    fn process(&mut self, row: &row::Row, lineno: u64) -> Result<Option<sam::Query>, String> {
        let flag = sam::flag(row).and_then(|f| u16::try_from(f).ok()).unwrap_or(0);
        let outcome = self.cache.process(row.field(0), flag, row.field(sam::CIGAR), row.field(sam::SEQ),
                                         row.field(sam::QUAL));
        let reason = match outcome {
            samseq::Fill::Present | samseq::Fill::NotSource => return Ok(Some(sam::Query::Row)),
            samseq::Fill::LengthMismatch => return Ok(Some(sam::Query::Mismatch)),
            samseq::Fill::Filled { seq, qual } => {
                self.filled += 1;
                return Ok(Some(sam::Query::Filled { seq, qual }));
            }
            samseq::Fill::Ambiguous => 0,
            samseq::Fill::NoCigar => 1,
            samseq::Fill::LengthConflict { .. } => 2,
            samseq::Fill::NotFound { .. } => 3,
        };
        let qname = String::from_utf8_lossy(row.field(0));
        // An ambiguous segment is skipped whatever the mode (SAMSEQ.design.txt section 2).
        if self.exit && reason != 0 {
            return Err(format!("SEQ '*' of read {} cannot be filled: {} (--sam-seqmiss=exit)",
                               qname, FILL_REASONS[reason]));
        }
        self.missed[reason] += 1;
        if self.missed.iter().sum::<u64>() <= FILL_MESSAGES {
            note::note!("pluk: line {}: SEQ '*' of read {} cannot be filled: {}; row skipped",
                        lineno, qname, FILL_REASONS[reason]);
        }
        Ok(None)
    }

    fn report(&self) {
        if self.filled > 0 {
            note::note!("pluk: SEQ '*' filled from another row of the read: {} row{}", self.filled,
                        if self.filled == 1 { "" } else { "s" });
        }
        for (n, reason) in self.missed.iter().zip(FILL_REASONS) {
            if *n > 0 {
                note::note!("pluk: SEQ '*' not filled, row skipped: {} row{}: {}", n, if *n == 1 { "" } else { "s" },
                            reason);
            }
        }
    }
}

impl Stages<'_> {
    // The computed fields of the previous row are removed first, so that a row
    // rejected by the @@ stage has none (they read as empty).
    fn judge(&mut self, row: &mut row::Row, lineno: u64) -> Result<filter::Verdict, Failure> {
        row.clear_computed();
        self.rowno += 1;
        // The fill comes before every filter (SAMVIEW.design.txt section 2).
        if let Some(fill) = &mut self.fill {
            match fill.process(row, lineno).map_err(Failure::Fatal)? {
                Some(query) => self.machine.sam.query = query,
                None => return Err(Failure::Skipped),
            }
        }
        if !self.filters.pre(row).map_err(Failure::Assertion)? {
            return Ok(filter::Verdict::RejectPre);
        }
        // SAM input rewrites: after the @@ stage, before the computes.
        // As pick, every record, unmapped ones included (CIGAR '*' never changes).
        if self.normal_form && row.n_fields() > sam::CIGAR {
            if let Some((pos, cigar)) = sam::normal_form(row.field(sam::POS), row.field(sam::CIGAR)) {
                row.replace_field(sam::POS, &pos);
                row.replace_field(sam::CIGAR, &cigar);
            }
        }
        if !self.computes.is_empty() {
            self.machine.run(&self.computes, row, self.rowno, lineno).map_err(Failure::Fatal)?;
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
    Skipped,        // a SEQ '*' that cannot be filled, with --sam-seqmiss=skip
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
    match stages.judge(row, lineno) {
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
        // Not a filter rejection: not written to --other (SAMSEQ.design.txt section 5).
        Err(Failure::Skipped) => Ok(false),
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
