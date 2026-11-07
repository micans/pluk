// Command line options.
//
// Bespoke parsing following pick (plan/step3.txt item 2; rationale in
// plan/step3-notes.txt item 1). Words are processed left to right while they
// start with '-'; a bare '--' ends the options. The remaining words are column
// arguments. Options take effect in order; the last one wins.
//
// Arguments are taken as bytes, without requiring UTF-8: on Unix an argument is
// an arbitrary byte sequence (like char *argv[]), and into_vec() from
// std::os::unix::ffi::OsStringExt gives those bytes. pluk, like pick, targets Unix.

use std::os::unix::ffi::OsStringExt;
use std::process;

use crate::colspec::{self, ColSpec};
use crate::compute::{self, ComputeSpec, UnnamedNames};
use crate::dict::{self, DictKind, DictSpec, NotFound};
use crate::filter::{parse_filter, FilterSpec};

#[derive(Debug, Default)]
pub struct Config {
    pub no_header: bool,                // -h
    pub k: bool,                        // -k (also set by -K, --sam, --sam-h)
    pub big_k: bool,                    // -K: as -k, and output a header line of the output names
    pub all: bool,                      // -A
    pub insert_offset: Option<i64>,     // -A<N>
    pub insert_after: Option<Vec<u8>>,  // --A=NAME (%-decoded): as -A<N>, N the position of NAME
    pub list: Option<ListKind>,         // --idx-list, --name-list, --idx-map
    pub progress: Option<(u64, u64)>,   // --progress[=K/N]: a dot every K rows, a count every N dots
    pub excise: bool,                   // -x
    pub c: bool,                        // -c
    pub omega: Option<u32>,             // -O<N>
    pub expect: Option<u64>,            // -E<N>: the number of output rows must be N
    pub squash: bool,                   // -e: remove white space from filters and computes
    pub quiet: bool,                    // -q: no messages other than errors
    pub add_row_column: bool,           // -R
    pub make_unique: bool,              // -u
    pub sam: bool,                      // --sam
    pub sam_h: bool,                    // --sam-h
    pub sam_seqmiss_exit: bool,         // --sam-seqmiss=exit (default skip; SAMSEQ.design.txt section 5)
    pub sam_seqmiss_set: bool,          // --sam-seqmiss given
    pub sam_nonf: bool,                 // --sam-nonf: no normal form (plan/SAMVIEW.design.txt section 3)
    pub sam_aln_prefix: Option<Vec<u8>>, // --sam-aln-prefix=STR (plan/step18.txt decision 4)
    pub sam_aln_mrk: bool,              // --sam-aln-mrk (decision 5)
    pub sam_qlt_sim: bool,              // --sam-qlt-sim (decision 6)
    pub sam_aln_context: Option<u64>,   // --sam-aln-context=N (plan/step19.txt decision 2)
    pub sam_aln_xy: Option<(u64, u64)>, // --sam-aln-xy=X,Y
    pub sam_narrow_xy: Option<(u64, u64)>, // --sam-narrow-xy=X,Y (plan/step19.txt decision 4)
    pub threads: Option<u32>,           // --threads=N
    pub pat_purge: Option<Vec<u8>>,     // -/PAT, %-decoded
    pub pat_pass: Option<Vec<u8>>,      // -//PAT, %-decoded
    pub dicts: Vec<DictSpec>,           // dictionary options, in command line order
    pub test_i: Option<u32>,            // --test-i=N (temporary)
    pub test_lineno: bool,              // --test-lineno (temporary)
    pub test_counts: bool,              // --test-counts (temporary)
    pub test_names: bool,               // --test-names (temporary)
    pub test_select: bool,              // --test-select (temporary)
    pub test_dict: bool,                // --test-dict (temporary)
    pub test_samsort: bool,             // --test-samsort (temporary)
    pub test_compute: bool,             // --test-compute (temporary, step10)
    pub num_perl: bool,                 // --num-perl: write floats as Perl (%.15g)
    pub inplace: bool,                  // -i: computes may redefine names (plan/INPLACE.design.txt)
    pub seed: Option<u64>,              // --seed=N: seed for rand
    pub columns: Vec<ColSpec>,          // column arguments (plan/COLSPEC.design.txt)
    pub computes: Vec<ComputeSpec>,     // computes (plan/COMPUTE.design.txt), in command line order
    pub filters: Vec<FilterSpec>,       // @ and @@ filters, in command line order
    pub or_post: bool,                  // -o: OR the @ filters
    pub or_pre: bool,                   // -s: OR the @@ filters
    pub other: Option<Vec<u8>>,         // --other=FILE: rows rejected by filters
    pub assert_other: Option<Vec<u8>>,  // --assert-other=FILE: rows failing an assertion
    pub demux: Option<Vec<u8>>,         // --demux=NAME (%-decoded column name)
    pub demux_dir: Option<Vec<u8>>,     // --demux-dir=PATH
}

// What --idx-list, --name-list and --idx-map write (plan/step16.txt decision 9).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ListKind {
    Indexes,
    Names,
    Map,
}

// Result of a successful parse: run, or print help or version and exit.
#[derive(Debug)]
pub enum Parsed {
    Run(Config),
    Help,
    Version,
    Encode(Vec<Vec<u8>>),
    Decode(Vec<Vec<u8>>),
    List(Vec<Vec<u8>>),
}

pub fn parse_args() -> Config {
    let args: Vec<Vec<u8>> = std::env::args_os().skip(1).map(|a| a.into_vec()).collect();
    match parse(&args) {
        Ok(Parsed::Run(config)) => config,
        Ok(Parsed::Help) => {
            print_help();
            process::exit(0);
        }
        Ok(Parsed::Version) => {
            println!("pluk {}", env!("CARGO_PKG_VERSION"));
            process::exit(0);
        }
        Ok(Parsed::Encode(words)) => {
            let encoded: Vec<Vec<u8>> = words.iter().map(|w| pct_encode(w)).collect();
            write_list(&encoded, b" ");
        }
        Ok(Parsed::List(words)) => process::exit(crate::list::run(&words)),
        Ok(Parsed::Decode(words)) => {
            let decoded: Vec<Vec<u8>> = words.iter().map(|w| pct_decode(w)).collect();
            write_list(&decoded, b"\n");
        }
        Err(msg) => {
            eprintln!("{}", msg);
            process::exit(1);
        }
    }
}

// Output of -H and --help. Options are listed in order of importance (plan/step3.txt item 5).
const HELP: &str = "\
Usage: pluk [options] <column|filter>+ < FILE
Output the named columns of TAB-separated input (plain or gzip-compressed),
optionally only the rows that pass the filters.

Columns: a name, a list a:b:c, or +REGEX for all columns whose whole name
matches REGEX (Rust regex syntax; \\Q...\\E quotes literal text, in every pluk
regex), e.g. +num\\d or name:+.*_p. Each part is
%-decoded after splitting on ':' (%3A for ':', %2B for a leading '+').

Options:
  -K          as -k, and output a header line with the output column names
              (the numbers, or compute names: -K name::1 len::2)
  -k          input has no header line; columns are named 1 2 .. N, and can be
              given as ranges S-E, S- (to the last), O+S-E (offset), S-E*M
              (multiplier): columns O + i*M for i = S..E
  -h          do not output the header line
  -c          output only the number of data rows (that pass the filters)
  -E<N>       the number of output rows must be N, else an error (exit 1) at
              the end, once the rows have been written
  -e          remove white space from filters and computes before parsing them
              (write a space in a value as %20)
  -q          quiet: no messages other than errors (counts, tallies, warnings)
  -A          output all columns
  -A<N>       as -A, with the new computed columns inserted after the first N
              input columns (N < 0: counted from the right)
  --A=NAME    as -A, with the new computed columns inserted after input column
              NAME
  -x          output all columns except the named ones
  -/<pat>     skip lines matching regular expression <pat>, e.g. -/^#
  -//<pat>    output lines matching <pat> unchanged
  -u          make duplicate input column names unique (suffixes _v02, _v03, ..)
  -F          accepted for compatibility with pick; no effect
  -O<N>       ragged input: <N> columns, merge columns at and after <N> (requires -k)
  -R          header has no name for the first (row name) column; it is named _
  -o          keep rows that pass any @ filter (default: all)
  -s          keep rows that pass any @@ filter (default: all)
  --other=FILE  write the rows rejected by @ and @@ filters to FILE (header and
              columns as the output; .gz or .gzip: gzip-compressed)
  --assert-other=FILE  write rows that fail an @assert@ or @@assert@ to FILE
              instead of stopping; such rows go nowhere else
  --demux=NAME  write each output row to the file named by its value in column
              NAME (a relative path; .gz or .gzip: gzip-compressed); a tally of
              files goes to stderr. Rows rejected by @ filters create their file.
  --demux-dir=PATH  directory for the --demux files (must exist)
  --demux-buffer=N  accepted for compatibility with pick; no effect
  --sam       SAM input: implies -k and -O12, skips lines starting with @.
              BAM input is recognised and read as SAM
  --sam-h     as --sam, but outputs lines starting with @ unchanged
  --sam/FILE, --sam-h/FILE
              as --sam and --sam-h; reference sequences are read from fasta
              FILE into the dictionary SAMFA (default __EXIT__)
  --sam-nonf  keep exterior deletions and insertions; by default a leading or
              trailing nD is removed (POS adjusted) and a leading or trailing nI
              becomes nS, as pick (vsearch writes such alignments)
  --sam-seqmiss=skip|exit  a row with SEQ '*' whose sequence is not found in an
              earlier row of the read (needed by qry_seq qry_qual) is skipped
              (default) or ends pluk with an error
  --sam-aln-prefix=STR  put STR before every line of aln_all aln_allq
              aln_all_pack
  --sam-aln-mrk  aln_ref and aln_qry end in markers and the lengths of the
              5' trail, the matched part and the 3' trail
  --sam-qlt-sim  simplify quality symbols, five values per symbol: .,aAbB..iI
              (qry_qual aln_qlt)
  --sam-aln-context=N  extend alignments by up to N bases of reference and
              soft-clipped query at each end; changes POS and CIGAR (the
              context is written as X, D, I)
  --sam-aln-xy=X,Y  extend alignments to include reference positions X..Y
              (never reduces; with --sam-aln-context the larger per side)
  --sam-narrow-xy=X,Y  reduce alignments to reference positions X..Y (before
              the extension); a record outside X..Y gets FLAG 0x4 and CIGAR *
  --threads=N threads for BAM decompression (default 4, at most the number
              of processors; 1: no separate threads)
  --idx-list  write the input indexes of the output columns (computed columns
              are numbered after the input columns) and stop after the header
  --name-list write the output column names; --idx-map: index and name per line
  --progress  a dot on stderr every 1000000 input rows; --progress=K/N: every
              K rows, and the row count after every N dots

Computes (given among the columns; a stack language, as pick):
  NAME::SPEC      compute column NAME and output it; NAME:=SPEC computes it
                  without output; ::SPEC is output as PLUKAAAAA, PLUKAAAAB, ..
  SPEC is a sequence of ^VALUE (constant), :COLUMN (pushes the column; a
  +REGEX or -k range pushes several) and ,OP (operator). A SPEC that does not
  start with ^ : , starts with a column. The result is what is left on the
  stack, concatenated. Operators so far:
    stack       dup pop xch
    string      cat len lc uc rev rot13 substr head tail skip shed strim ppl
                ppr zp nchar catall joinall join uie urlec urldc md5 fmtall cgsum
                cgmax cgcount
                (on bytes)
    arithmetic  add sub mul div idiv mod pow ipow max min neg abs sign sq incr
                decr rand addall mulall maxall minall meanall hmeanall gmeanall
    math        int floor ceil round sqrt exp exp2 exp10 log log2 log10 sin cos
                tan
    precision   dd sn frac pct pml
    dictionary  map dord
    branch      ifelse test (test x y T: T a filter operator, e.g. ^/gt/)
    state       save fetch (registers, per row) rowno r0wno lineno
    time        now ttu rttu todate
    bit         bitand bitor bitxor bitoff bitlsh bitrsh (on |x| |y|; integers)
    radix       tobin tohex tooct binto hexto octto
    regex       del delg ed edg edbr edgbr get npat (the pattern is a constant)
    sam         (with --sam) isforward sam_get ref_len ref_posx ref_posy qry_len
                qry_posx qry_posy ref_trail5p_N ref_matched_N ref_trail3p_N
                ref_trail5p_V ref_trail3p_V qry_trail5p_N qry_matched_N
                qry_trail3p_N aln_trail5p_cgr aln_matched_cgr aln_trail3p_cgr
                aln_nmatch aln_nedit aln_nmatchx (pluk -l sam)
  Numbers are written exactly (round-trip); NA where a number was needed.
  --num-perl  write numbers as Perl and pick do (%.15g, Inf, NaN)
  --seed=N    seed for rand (default: from the clock), for reproducible output
  -i          computes may redefine a name (in place): an input column once,
              a computed name any number of times. Earlier references see the
              old value, later ones and the output the new value. __ in SPEC is
              the compute's own name (the value being replaced). With -i, NAME
              can be a list, +REGEX or (-k) range of existing columns: the
              compute is applied to each, e.g. -i +num\\d::__^1,add.

Filters (given among the columns; VALUE is %-decoded, e.g. %2F for '/'):
  @NAME/op/VALUE  numeric comparison of column NAME with VALUE, op one of
                  lt le eq ge gt ne. A value that is not a number fails.
  @NAME~op~VALUE  string (byte) comparison, op as above
  @NAME=VALUE     string equal; @NAME/=VALUE not equal
  @NAME~REGEX     REGEX matches (anywhere); @NAME/~REGEX does not match.
                  Rust regex syntax, as -/PAT. With :NAME2 the value of
                  column NAME2 is matched as a fixed string, not a regex.
  @NAME~isin~DICT the value is a key of dictionary DICT; ~isnotin~ is not
  @NAME~hasval~DICT~V  the value is a key of DICT with value V; ~hasnotval~
                  negates. Dictionary defaults play no part.
  @NAME/ep/VALUE/E  value within E of VALUE (default 0.0001, E >= 0)
  @NAME/om/VALUE/K  |value| within a factor K of |VALUE| (default 2, K >= 1)
                  /E and /K are optional; /epx/ and /omx/ negate.
  @NAME/all/MASK  all bits of MASK set in |value|; /any/: at least one;
                  /notall/ and /none/ negate. MASK a non-negative integer;
                  a value that is not an integer fails.
  @NAME/op/:NAME2 compare with column NAME2; ^VALUE is VALUE taken literally
  @NAME/ISINT     the value is an integer; /ISNUM: a number (also 1e3, inf;
                  not nan); /NOTINT and /NOTNUM negate. No VALUE.
  @@NAME...       as @, applied before computes (computes are not yet supported)
  @assert@..., @@assert@...  assertions: as @ and @@, but a row that fails ends
                  pluk with an error. Checked before the filters of the same
                  stage; -o and -s do not apply.
  NAME can be a list, +REGEX or (-k) range, as for columns: the test must
  hold for every column, e.g. @+num\\d/gt/0. A single name follows ':'.

Dictionaries (NAME: letters, digits, _; used by ~isin~ ~hasval~ map dord):
  --kd-NAME=FILE          keys from the first column of FILE, all values 1
  --fd-NAME=FILE          keys and values from the first two columns of FILE
  --cd-NAME=k1:v1,k2:v2   keys and values from the list; k without :v has value 1
  --kd-NAME/DEFAULT=...   as above, DEFAULT is the value for keys not found;
                          __EXIT__ makes a failed lookup an error. Without
                          DEFAULT a key not found maps to itself.
  --fasta-dict-NAME=FILE  sequences from fasta FILE, keyed by the identifier (up
                          to the first white space); DEFAULT is __EXIT__ unless
                          given, '*' maps to the empty sequence.
  --td^KEY/DEFAULT=FILE   table with column names: every column other than KEY
                          becomes a dictionary named after the column, mapping
                          the KEY value to the value in that column. ^KEY and
                          /DEFAULT are optional; KEY defaults to the first column.
  --kdict- --fdict- --cdict- are synonyms, as are --tdict --table-dict for --td.
  A repeated NAME adds to the dictionary. FILE may be gzip-compressed.
  In KEY write / and = as %2F and %3D, in DEFAULT = as %3D; in --cd , and : as
  %2C and %3A.

  -l TERM..   list the operators (compute and filter) in a category, or whose
              name contains TERM; no TERM: all
  -z ARG..    write the %-encoding of each ARG (on one line); no ARG: a table
  -zz ARG..   write the %-decoding of each ARG (one per line)
  -H, --help  print this help
  --version   print the version

Short options combine, e.g. -kO12/^#. Options take effect from left to right.
In <pat>, write / as %2F.
";

// -z and -zz (plan/step16.txt decisions 1, 2): the values separated by sep and a
// newline, or without values the table of encodings; exit 0.
fn write_list(values: &[Vec<u8>], sep: &[u8]) -> ! {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    let result = if values.is_empty() {
        out.write_all(ENCODINGS.as_bytes())
    } else {
        out.write_all(&values.join(sep)).and_then(|_| out.write_all(b"\n"))
    };
    if let Err(e) = result.and_then(|_| out.flush()) {
        if e.kind() != std::io::ErrorKind::BrokenPipe {
            eprintln!("pluk: {}", e);
            process::exit(1);
        }
    }
    process::exit(0);
}

const ENCODINGS: &str = "\
pluk -z ARG.. writes the %-encoding of each ARG, -zz ARG.. the %-decoding. Encoded
are the characters that mean something in a pluk argument, and white space:
  %   %25     ^   %5E     /   %2F     =   %3D     <SPACE>    %20
  :   %3A     ,   %2C     ~   %7E     @   %40     <TAB>      %09
  +   %2B                                         <NEWLINE>  %0A
Other control bytes are encoded as well. For shell meta-characters it is
easiest to quote the argument.
";

fn print_help() {
    print!("{}", HELP);
}

pub fn parse(args: &[Vec<u8>]) -> Result<Parsed, String> {
    let mut config = Config::default();
    let mut i = 0;
    while i < args.len() && args[i].starts_with(b"-") {
        let word = &args[i];
        i += 1;
        if word == b"--" {
            break;
        }
        // -z and -zz are words of their own and take all following words (step16
        // decision 0); -zzz and more are not accepted (pick: odd encodes, even decodes).
        if word.len() >= 2 && word[1..].iter().all(|&b| b == b'z') {
            let rest = args[i..].to_vec();
            return match word.len() {
                2 => Ok(Parsed::Encode(rest)),
                3 => Ok(Parsed::Decode(rest)),
                _ => Err(format!("{}: use -z to encode, -zz to decode", String::from_utf8_lossy(word))),
            };
        }
        // -l lists operators; all following words are terms (step16 decision 3).
        if word == b"-l" {
            return Ok(Parsed::List(args[i..].to_vec()));
        }
        if word.len() > 2 && word[1..].iter().all(|&b| b == b'l') {
            return Err(format!("{}: not supported; -l lists operands and results", String::from_utf8_lossy(word)));
        }
        // As in pick, -H must be a word of its own.
        if word == b"-H" {
            return Ok(Parsed::Help);
        }
        if let Some(long) = word.strip_prefix(b"--") {
            if let Some(parsed) = parse_long(long, &mut config)? {
                return Ok(parsed);
            }
        } else {
            parse_bundle(&word[1..], &mut config)?;
        }
    }
    // The trailing arguments: column names, filters and computes, mixed as in pick.
    let mut unnamed = UnnamedNames::new();
    for arg in &args[i..] {
        // -e: white space is removed from filters and computes (as pick: arguments
        // starting with @ or containing :: or :=), not from column arguments.
        let squashed;
        let arg = if config.squash && (arg.starts_with(b"@") || compute::is_compute(arg)) {
            squashed = arg.iter().copied().filter(|b| !b" \t\n\r\x0b\x0c".contains(b)).collect::<Vec<u8>>();
            &squashed
        } else {
            arg
        };
        if arg.starts_with(b"@") {
            config.filters.push(parse_filter(arg, config.k)?);
        } else if compute::is_compute(arg) {
            let mut spec = compute::parse(arg, config.k, &mut unnamed)?;
            spec.position = config.columns.len();
            config.computes.push(spec);
        } else {
            config.columns.push(colspec::parse(arg, config.k)?);
        }
    }
    check_combinations(&config)?;
    Ok(Parsed::Run(config))
}

// A '--' option, one per word. Returns Some for options that end parsing.
fn parse_long(long: &[u8], config: &mut Config) -> Result<Option<Parsed>, String> {
    if let Some(rest) = table_option(long) {
        config.dicts.push(parse_table_spec(rest, long)?);
        return Ok(None);
    }
    if let Some((kind, rest)) = dict_option(long) {
        config.dicts.push(parse_dict_spec(kind, rest, long)?);
        return Ok(None);
    }
    match long {
        b"help"    => return Ok(Some(Parsed::Help)),
        b"version" => return Ok(Some(Parsed::Version)),
        // --sam and --sam-h imply -k and -O12; a later -O overrides.
        b"sam"   => set_sam(config, false),
        b"sam-h" => set_sam(config, true),
        b"sam-nonf" => config.sam_nonf = true,
        b"sam-aln-mrk" => config.sam_aln_mrk = true,
        b"sam-qlt-sim" => config.sam_qlt_sim = true,
        _ if long.starts_with(b"sam-aln-prefix=") => config.sam_aln_prefix = Some(long[15..].to_vec()),
        _ if long.starts_with(b"sam-aln-context=") => match digits(&long[16..]) {
            Some(n) => config.sam_aln_context = Some(n),
            None => return Err("--sam-aln-context=N: N must be a non-negative integer".to_string()),
        },
        _ if long.starts_with(b"sam-aln-xy=") => config.sam_aln_xy = Some(parse_xy(&long[11..], "--sam-aln-xy")?),
        _ if long.starts_with(b"sam-narrow-xy=") =>
            config.sam_narrow_xy = Some(parse_xy(&long[14..], "--sam-narrow-xy")?),
        b"sam-seqmiss=skip" | b"sam-seqmiss=exit" => {
            config.sam_seqmiss_exit = long == b"sam-seqmiss=exit";
            config.sam_seqmiss_set = true;
        }
        _ if long.starts_with(b"sam-seqmiss") => return Err("--sam-seqmiss=skip|exit".to_string()),
        b"test-lineno"          => config.test_lineno = true,
        b"test-counts"          => config.test_counts = true,
        b"test-names"           => config.test_names = true,
        b"test-select"          => config.test_select = true,
        b"test-dict"            => config.test_dict = true,
        b"test-samsort"         => config.test_samsort = true,
        b"test-compute"         => config.test_compute = true,
        b"num-perl"             => config.num_perl = true,
        b"idx-list" | b"name-list" | b"idx-map" => {
            if config.list.is_some() {
                return Err("--idx-list, --name-list and --idx-map cannot be combined".to_string());
            }
            config.list = Some(match long {
                b"idx-list" => ListKind::Indexes,
                b"name-list" => ListKind::Names,
                _ => ListKind::Map,
            });
        }
        b"progress" => config.progress = Some((1_000_000, 0)),
        _ if long.starts_with(b"progress=") => {
            let bad = || "--progress=K/N: K a positive number of rows, N a number of dots (0: no counts)".to_string();
            let (k, n) = long[9..].split_at(long[9..].iter().position(|&b| b == b'/').ok_or_else(bad)?);
            match (parse_number::<u64>(k), parse_number::<u64>(&n[1..])) {
                (Some(k), Some(n)) if k > 0 => config.progress = Some((k, n)),
                _ => return Err(bad()),
            }
        }
        _ if long.starts_with(b"A=") => {
            if long.len() == 2 {
                return Err("--A=NAME: column name missing".to_string());
            }
            config.all = true;
            config.insert_offset = None;
            config.insert_after = Some(pct_decode(&long[2..]));
        }
        _ if long.starts_with(b"other=") || long.starts_with(b"assert-other=") => {
            let (name, slot) = match long.strip_prefix(b"other=") {
                Some(name) => (name, &mut config.other),
                None => (&long[13..], &mut config.assert_other),
            };
            if name.is_empty() {
                return Err(format!("--{}FILE: file name missing", String::from_utf8_lossy(&long[..long.len() - name.len()])));
            }
            *slot = Some(name.to_vec());
        }
        _ if long.starts_with(b"seed=") => {
            match parse_number::<u64>(&long[5..]) {
                Some(n) => config.seed = Some(n),
                None => return Err("--seed=<N> N must be a non-negative integer".to_string()),
            }
        }
        _ if long.starts_with(b"demux=") => {
            let name = &long[6..];
            if name.is_empty() || name.starts_with(b"+") || name.contains(&b':') {
                return Err("--demux=NAME: NAME must be a single column name".to_string());
            }
            config.demux = Some(pct_decode(name));
        }
        _ if long.starts_with(b"demux-dir=") => {
            if long.len() == 10 {
                return Err("--demux-dir=PATH: path missing".to_string());
            }
            config.demux_dir = Some(long[10..].to_vec());
        }
        // pick buffers N rows per demux file; pluk's files are buffered anyway.
        _ if long.starts_with(b"demux-buffer=") => {
            if parse_number::<u64>(&long[13..]).is_none() {
                return Err("--demux-buffer=<N> N must be a number".to_string());
            }
        }
        _ if long.starts_with(b"threads=") => {
            match parse_number::<u32>(&long[8..]) {
                Some(n) if n > 0 => config.threads = Some(n),
                _ => return Err("--threads=<N> N must be a positive number".to_string()),
            }
        }
        _ if long.starts_with(b"test-i=") => {
            let n = parse_number::<u32>(&long[7..]);
            match n {
                Some(n) if n > 0 => config.test_i = Some(n),
                _ => return Err("--test-i=<N> N must be a positive number".to_string()),
            }
        }
        _ if long.starts_with(b"sam/") || long.starts_with(b"sam-h/") => {
            // --sam/FILE, --sam-h/FILE: as --sam, --sam-h, with FILE read as the fasta
            // dictionary SAMFA, default __EXIT__ (as pick). A repeated --sam/FILE merges.
            let pass_header = long.starts_with(b"sam-h/");
            let file = &long[if pass_header { 6 } else { 4 }..];
            if file.is_empty() {
                return Err(format!("--{}: expected --{}FILE", String::from_utf8_lossy(long),
                                   String::from_utf8_lossy(long)));
            }
            set_sam(config, pass_header);
            config.dicts.push(DictSpec {
                option: [b"--", long].concat(),
                kind: DictKind::Fasta,
                name: dict::SAM_REFERENCE.to_vec(),
                key_column: None,
                not_found: Some(NotFound::Exit),
                source: file.to_vec(),
            });
        }
        _ => return Err(format!("Unrecognised long option --{}", String::from_utf8_lossy(long))),
    }
    Ok(None)
}

// --sam and --sam-h imply -k and -O12 at their position; a later -O overrides.
fn set_sam(config: &mut Config, pass_header: bool) {
    config.sam   = !pass_header;
    config.sam_h = pass_header;
    config.k     = true;
    config.omega = Some(12);
}

// Dictionary options: the kind, and the part after the prefix (NAME[/DEFAULT]=SOURCE).
fn dict_option(long: &[u8]) -> Option<(DictKind, &[u8])> {
    const PREFIXES: [(&[u8], DictKind); 7] = [
        (b"kd-", DictKind::Keys),   (b"kdict-", DictKind::Keys),
        (b"fd-", DictKind::Pairs),  (b"fdict-", DictKind::Pairs),
        (b"cd-", DictKind::Inline), (b"cdict-", DictKind::Inline),
        (b"fasta-dict-", DictKind::Fasta),
    ];
    PREFIXES.iter().find_map(|&(prefix, kind)| long.strip_prefix(prefix).map(|rest| (kind, rest)))
}

// NAME[/DEFAULT]=SOURCE. NAME is [A-Za-z0-9_]+ (pick: \w+). DEFAULT extends to the
// first '=' and is %-decoded (pick does not decode it); it may be empty.
fn parse_dict_spec(kind: DictKind, rest: &[u8], long: &[u8]) -> Result<DictSpec, String> {
    let option = [b"--", long].concat();
    let usage = || {
        let source = if kind == DictKind::Inline { "k:v,.." } else { "FILE" };
        let prefix = &long[..long.len() - rest.len()];
        format!("{}: expected --{}NAME[/DEFAULT]={}", String::from_utf8_lossy(&option),
                String::from_utf8_lossy(prefix), source)
    };
    let name_len = rest.iter().take_while(|&&b| b.is_ascii_alphanumeric() || b == b'_').count();
    if name_len == 0 {
        return Err(usage());
    }
    let name = &rest[..name_len];
    if dict::is_reserved(name) {
        return Err(format!("{}: {} is a reserved dictionary name", String::from_utf8_lossy(&option),
                           String::from_utf8_lossy(name)));
    }
    let (mut not_found, source) = parse_default_source(&rest[name_len..]).ok_or_else(usage)?;
    // As in pick, a fasta dictionary without default fails on a key not found.
    if kind == DictKind::Fasta && not_found.is_none() {
        not_found = Some(NotFound::Exit);
    }
    Ok(DictSpec { option, kind, name: rest[..name_len].to_vec(), key_column: None, not_found, source })
}

// [/DEFAULT]=SOURCE, shared by all dictionary options. DEFAULT extends to the
// first '=' and is %-decoded; SOURCE must be non-empty. None if malformed.
fn parse_default_source(tail: &[u8]) -> Option<(Option<NotFound>, Vec<u8>)> {
    let mut tail = tail;
    let mut not_found = None;
    if let Some(t) = tail.strip_prefix(b"/") {
        let eq = t.iter().position(|&b| b == b'=')?;
        not_found = Some(NotFound::from_default(pct_decode(&t[..eq])));
        tail = &t[eq..];
    }
    let source = tail.strip_prefix(b"=")?;
    if source.is_empty() {
        return None;
    }
    Some((not_found, source.to_vec()))
}

// Table dictionary options --td --tdict --table-dict: the part after the option
// name, if it starts with ^ / = or - (the last for the refused form --td-NAME).
fn table_option(long: &[u8]) -> Option<&[u8]> {
    for prefix in [&b"table-dict"[..], b"tdict", b"td"] {
        if let Some(rest) = long.strip_prefix(prefix) {
            if matches!(rest.first(), Some(b'^' | b'/' | b'=' | b'-')) {
                return Some(rest);
            }
        }
    }
    None
}

// [^KEY][/DEFAULT]=FILE. KEY extends to the first '/' or '=' and is %-decoded.
// The order ^KEY/DEFAULT differs from pick (/DEFAULT^KEY); a '^' in DEFAULT is
// refused, so that pick's order is not silently read as a default containing '^'.
fn parse_table_spec(rest: &[u8], long: &[u8]) -> Result<DictSpec, String> {
    let option = [b"--", long].concat();
    let prefix = String::from_utf8_lossy(&long[..long.len() - rest.len()]).into_owned();
    let what = String::from_utf8_lossy(&option).into_owned();
    let usage = || format!("{}: expected --{}[^KEY][/DEFAULT]=FILE", what, prefix);
    if rest.starts_with(b"-") {
        return Err(format!("{}: named table dictionaries are not supported, use --{}[^KEY][/DEFAULT]=FILE",
                           what, prefix));
    }
    let mut tail = rest;
    let mut key_column = None;
    if let Some(t) = tail.strip_prefix(b"^") {
        let end = t.iter().position(|&b| b == b'/' || b == b'=').ok_or_else(usage)?;
        if end == 0 {
            return Err(usage());
        }
        key_column = Some(pct_decode(&t[..end]));
        tail = &t[end..];
    }
    if let Some(t) = tail.strip_prefix(b"/") {
        let default = &t[..t.iter().position(|&b| b == b'=').unwrap_or(t.len())];
        if default.contains(&b'^') {
            return Err(format!("{}: '^' in DEFAULT; the key column comes first, --{}^KEY/DEFAULT=FILE \
                                (write a literal ^ as %5E)", what, prefix));
        }
    }
    let (not_found, source) = parse_default_source(tail).ok_or_else(usage)?;
    Ok(DictSpec { option, kind: DictKind::Table, name: Vec::new(), key_column, not_found, source })
}

// A bundle of '-' options, without the leading '-', scanned left to right.
fn parse_bundle(chars: &[u8], config: &mut Config) -> Result<(), String> {
    let mut j = 0;
    while j < chars.len() {
        let c = chars[j];
        j += 1;
        match c {
            b'k' => config.k = true,
            b'K' => {
                config.k = true;
                config.big_k = true;
            }
            b'h' => config.no_header = true,
            b'c' => config.c = true,
            b'o' => config.or_post = true,
            b's' => config.or_pre = true,
            b'x' => config.excise = true,
            b'i' => config.inplace = true,
            b'R' => config.add_row_column = true,
            b'u' => config.make_unique = true,
            b'F' => (),     // pick: names are not regexes; pluk: accepted, no effect (COLSPEC.design.txt)
            b'A' => {
                // Optional attached number, possibly negative: -A, -A4, -A-1.
                config.all = true;
                let len = number_len(&chars[j..], true);
                if len > 0 {
                    let n = parse_number::<i64>(&chars[j..j + len])
                        .ok_or("-A<N> N out of range")?;
                    config.insert_offset = Some(n);
                    config.insert_after = None;
                    j += len;
                }
            }
            b'E' => {
                let len = number_len(&chars[j..], false);
                match parse_number::<u64>(&chars[j..j + len]) {
                    Some(n) if len > 0 => config.expect = Some(n),
                    _ => return Err("-E<N> requires an attached non-negative number N, e.g. -E10".to_string()),
                }
                j += len;
            }
            b'e' => config.squash = true,
            b'q' => config.quiet = true,
            b'O' => {
                // Required attached number: -O8, not -O 8.
                let len = number_len(&chars[j..], false);
                if len == 0 {
                    return Err("-O<N> requires an attached number N, e.g. -O12".to_string());
                }
                match parse_number::<u32>(&chars[j..j + len]) {
                    Some(n) if n > 0 => config.omega = Some(n),
                    _ => return Err("-O<N> N must be positive".to_string()),
                }
                j += len;
            }
            // Patterns extend to the end of the word.
            b'/' => return parse_patterns(&chars[j - 1..], config),
            b'z' => return Err("-z and -zz must be words of their own".to_string()),
            b'l' => return Err("-l must be a word of its own".to_string()),
            _ => {
                return Err(format!("Option string not fully parsed: -{} remains",
                                   String::from_utf8_lossy(&chars[j - 1..])));
            }
        }
    }
    Ok(())
}

// s starts with '/'. Accepted forms: /PAT, //PAT, /PAT1//PAT2.
// A '/' inside a pattern must be written as %2F, so that //PAT1/PAT2 is refused
// rather than read as the pass pattern 'PAT1/PAT2'.
fn parse_patterns(s: &[u8], config: &mut Config) -> Result<(), String> {
    let (purge, pass) = if let Some(rest) = s.strip_prefix(b"//") {
        (None, Some(rest))
    } else {
        let rest = &s[1..];
        match rest.windows(2).position(|w| w == b"//") {
            Some(k) => (Some(&rest[..k]), Some(&rest[k + 2..])),
            None    => (Some(rest), None),
        }
    };
    if let Some(p) = purge {
        check_pattern(p, "-/PAT")?;
        config.pat_purge = Some(pct_decode(p));
    }
    if let Some(p) = pass {
        check_pattern(p, "-//PAT")?;
        config.pat_pass = Some(pct_decode(p));
    }
    Ok(())
}

fn check_pattern(p: &[u8], what: &str) -> Result<(), String> {
    if p.is_empty() {
        return Err(format!("{}: empty pattern", what));
    }
    if p.contains(&b'/') {
        return Err(format!("{}: write '/' inside a pattern as %2F [{}]", what,
                           String::from_utf8_lossy(p)));
    }
    Ok(())
}

// Combinations checked after all options are known; messages as in pick.
fn check_combinations(config: &Config) -> Result<(), String> {
    if config.omega.is_some() && !config.k {
        return Err("-O<N> requires -k".to_string());
    }
    if config.add_row_column && config.k {
        return Err("-R cannot be combined with -k".to_string());
    }
    if config.sam_nonf && !(config.sam || config.sam_h) {
        return Err("--sam-nonf requires --sam or --sam-h".to_string());
    }
    if config.sam_seqmiss_set && !(config.sam || config.sam_h) {
        return Err("--sam-seqmiss requires --sam or --sam-h".to_string());
    }
    for (given, name) in [(config.sam_aln_prefix.is_some(), "--sam-aln-prefix"), (config.sam_aln_mrk, "--sam-aln-mrk"),
                          (config.sam_qlt_sim, "--sam-qlt-sim"), (config.sam_aln_context.is_some(), "--sam-aln-context"),
                          (config.sam_aln_xy.is_some(), "--sam-aln-xy"),
                          (config.sam_narrow_xy.is_some(), "--sam-narrow-xy")] {
        if given && !(config.sam || config.sam_h) {
            return Err(format!("{} requires --sam or --sam-h", name));
        }
    }
    if config.big_k && (config.sam || config.sam_h) {
        return Err("-K cannot be combined with --sam or --sam-h".to_string());
    }
    // -x takes precedence over -A, so an insert position would have no effect.
    if config.insert_offset.is_some() && config.excise {
        return Err("-A<N> cannot be combined with -x".to_string());
    }
    if config.insert_after.is_some() && config.excise {
        return Err("--A=NAME cannot be combined with -x".to_string());
    }
    if config.list.is_some() && (config.c || config.demux.is_some() || config.other.is_some()
                                 || config.assert_other.is_some()) {
        return Err("--idx-list, --name-list and --idx-map cannot be combined with -c, --demux, \
                    --other or --assert-other".to_string());
    }
    if config.demux.is_some() && config.c {
        return Err("--demux cannot be combined with -c".to_string());
    }
    if config.demux_dir.is_some() && config.demux.is_none() {
        return Err("--demux-dir requires --demux".to_string());
    }
    if let (Some(a), Some(b)) = (&config.other, &config.assert_other) {
        if a == b {
            return Err(format!("--other and --assert-other name the same file [{}]", String::from_utf8_lossy(a)));
        }
    }
    Ok(())
}

// Length of the number at the start of s: digits, with an optional leading '-'
// if allow_minus is set. Returns 0 if there is no number.
fn number_len(s: &[u8], allow_minus: bool) -> usize {
    let start = if allow_minus && s.first() == Some(&b'-') { 1 } else { 0 };
    let n_digits = s[start..].iter().take_while(|b| b.is_ascii_digit()).count();
    if n_digits == 0 { 0 } else { start + n_digits }
}

// Parses s (ASCII digits, optional '-') as a number; None if malformed or out of range.
fn parse_number<T: std::str::FromStr>(s: &[u8]) -> Option<T> {
    std::str::from_utf8(s).ok()?.parse::<T>().ok()
}

// A non-negative integer written in digits only (no sign).
fn digits(s: &[u8]) -> Option<u64> {
    if s.is_empty() || !s.iter().all(u8::is_ascii_digit) {
        return None;
    }
    parse_number(s)
}

// X,Y for --sam-aln-xy and --sam-narrow-xy: positive integers, X <= Y (plan/step19.txt
// decision 6).
fn parse_xy(s: &[u8], option: &str) -> Result<(u64, u64), String> {
    let mut parts = s.splitn(2, |&b| b == b',');
    match (parts.next().and_then(digits), parts.next().and_then(digits)) {
        (Some(x), Some(y)) if x > 0 && x <= y => Ok((x, y)),
        (Some(x), Some(y)) if x > 0 && y > 0 => Err(format!("{}=X,Y: X is larger than Y", option)),
        _ => Err(format!("{}=X,Y: X and Y must be positive integers", option)),
    }
}

// -z: %-encoding of the bytes that mean something in a pluk argument (% : ^ , / ~
// = @ +), white space and other control bytes (plan/step16.txt decision 1); upper
// case hex, as pick. The urlec operator encodes everything but [A-Za-z0-9_].
pub fn pct_encode(s: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for &b in s {
        if b <= b' ' || b == 0x7f || b"%:^,/~=@+".contains(&b) {
            out.extend_from_slice(format!("%{:02X}", b).as_bytes());
        } else {
            out.push(b);
        }
    }
    out
}

// %-decoding as in pick: %XX (two hex digits) becomes the byte XX; anything
// else, including a malformed % sequence, is kept as is.
pub fn pct_decode(s: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if s[i] == b'%' && i + 2 < s.len() {
            if let (Some(hi), Some(lo)) = (hex_value(s[i + 1]), hex_value(s[i + 2])) {
                out.push(hi * 16 + lo);
                i += 3;
                continue;
            }
        }
        out.push(s[i]);
        i += 1;
    }
    out
}

fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(words: &[&str]) -> Result<Config, String> {
        let args: Vec<Vec<u8>> = words.iter().map(|w| w.as_bytes().to_vec()).collect();
        match parse(&args)? {
            Parsed::Run(config) => Ok(config),
            other => Err(format!("{:?}", other)),
        }
    }

    fn texts(c: &Config) -> Vec<Vec<u8>> {
        c.columns.iter().map(|spec| spec.text.clone()).collect()
    }

    #[test]
    fn bundle_with_numbers_and_pattern() {
        let c = run(&["-A4kO8/^#", "a"]).unwrap();
        assert!(c.all && c.k);
        assert_eq!(c.insert_offset, Some(4));
        assert_eq!(c.omega, Some(8));
        assert_eq!(c.pat_purge, Some(b"^#".to_vec()));
        assert_eq!(texts(&c), vec![b"a".to_vec()]);
    }

    #[test]
    fn negative_insert_offset() {
        let c = run(&["-A-1k"]).unwrap();
        assert_eq!(c.insert_offset, Some(-1));
        assert!(c.k);
    }

    #[test]
    fn order_rule_last_wins() {
        assert_eq!(run(&["--sam", "-O3"]).unwrap().omega, Some(3));
        assert_eq!(run(&["-kO3", "--sam"]).unwrap().omega, Some(12));
        let c = run(&["--sam", "--sam-h"]).unwrap();
        assert!(c.sam_h && !c.sam);
    }

    #[test]
    fn omega_needs_attached_positive_number() {
        assert!(run(&["-kO", "8"]).is_err());
        assert!(run(&["-kO0"]).is_err());
    }

    #[test]
    fn patterns() {
        let c = run(&["-/a//b"]).unwrap();
        assert_eq!(c.pat_purge, Some(b"a".to_vec()));
        assert_eq!(c.pat_pass, Some(b"b".to_vec()));
        let c = run(&["-//x%2Fy"]).unwrap();
        assert_eq!(c.pat_pass, Some(b"x/y".to_vec()));
        assert!(run(&["-//a/b"]).is_err());
        assert!(run(&["-/a/b"]).is_err());
        assert!(run(&["-/"]).is_err());
        assert!(run(&["-//"]).is_err());
    }

    #[test]
    fn words_after_options_are_columns() {
        let c = run(&["-k", "1", "-h"]).unwrap();
        assert!(!c.no_header);
        assert_eq!(texts(&c), vec![b"1".to_vec(), b"-h".to_vec()]);
        let c = run(&["--", "-h"]).unwrap();
        assert_eq!(texts(&c), vec![b"-h".to_vec()]);
    }

    fn parsed(words: &[&str]) -> Result<Parsed, String> {
        parse(&words.iter().map(|w| w.as_bytes().to_vec()).collect::<Vec<_>>())
    }

    #[test]
    fn encode_options() {
        let w = |v: &[&str]| v.iter().map(|s| s.as_bytes().to_vec()).collect::<Vec<_>>();
        let run = parsed;
        assert!(matches!(run(&["-z"]), Ok(Parsed::Encode(v)) if v.is_empty()));
        assert!(matches!(run(&["-z", "-k", "a b"]), Ok(Parsed::Encode(v)) if v == w(&["-k", "a b"])));
        assert!(matches!(run(&["-k", "-zz", "a%3A"]), Ok(Parsed::Decode(v)) if v == w(&["a%3A"])));
        assert_eq!(run(&["-zzz", "a"]).err().unwrap(), "-zzz: use -z to encode, -zz to decode");
        assert_eq!(run(&["-kz", "a"]).err().unwrap(), "-z and -zz must be words of their own");
        assert!(matches!(run(&["-k", "-l", "-h", "a,b"]), Ok(Parsed::List(v)) if v == w(&["-h", "a,b"])));
        assert_eq!(run(&["-kl"]).err().unwrap(), "-l must be a word of its own");
        assert_eq!(run(&["-ll"]).err().unwrap(), "-ll: not supported; -l lists operands and results");
        assert_eq!(pct_encode(b"a.b:c d"), b"a.b%3Ac%20d");
        assert_eq!(pct_encode(b"%^,/~=@+\t\n\x7f-\\\xe9"), b"%25%5E%2C%2F%7E%3D%40%2B%09%0A%7F-\\\xe9");
        for s in [&b"x%y:z^1,2/3~4=5@6+7 \t"[..], b"\x00\x1f\x7f\xff"] {
            assert_eq!(pct_decode(&pct_encode(s)), s);
        }
    }

    #[test]
    fn unknown_options() {
        assert_eq!(run(&["-kw"]).unwrap_err(), "Option string not fully parsed: -w remains");
        assert_eq!(run(&["--nosuch"]).unwrap_err(), "Unrecognised long option --nosuch");
    }

    #[test]
    fn dict_options() {
        let c = run(&["--fd-x=f.txt", "--kdict-y_2/=g", "--cd-z/a%3Db=k:v", "--kd-w/__EXIT__=h"]).unwrap();
        let s = &c.dicts;
        assert_eq!(s.len(), 4);
        assert_eq!((s[0].kind, &s[0].name[..], &s[0].not_found, &s[0].source[..]),
                   (DictKind::Pairs, &b"x"[..], &None, &b"f.txt"[..]));
        assert_eq!((s[1].kind, &s[1].name[..], &s[1].not_found),
                   (DictKind::Keys, &b"y_2"[..], &Some(NotFound::Value(Vec::new()))));
        assert_eq!((s[2].kind, &s[2].not_found, &s[2].source[..]),
                   (DictKind::Inline, &Some(NotFound::Value(b"a=b".to_vec())), &b"k:v"[..]));
        assert_eq!(s[3].not_found, Some(NotFound::Exit));
        assert_eq!(s[0].option, b"--fd-x=f.txt".to_vec());
        for bad in ["--fd-=f", "--fd-x", "--fd-x=", "--fd-x/d", "--fd-x!=f"] {
            assert!(run(&[bad]).is_err(), "{}", bad);
        }
        assert_eq!(run(&["--fd-x"]).unwrap_err(), "--fd-x: expected --fd-NAME[/DEFAULT]=FILE");
        let c = run(&["--fasta-dict-f=a.fa", "--fasta-dict-g/=b.fa"]).unwrap();
        assert_eq!((c.dicts[0].kind, &c.dicts[0].not_found), (DictKind::Fasta, &Some(NotFound::Exit)));
        assert_eq!(c.dicts[1].not_found, Some(NotFound::Value(Vec::new())));
    }

    #[test]
    fn table_options() {
        let c = run(&["--td=f", "--tdict^k%2Fx/d=g", "--table-dict/__EXIT__=h", "--td^k=i"]).unwrap();
        let s = &c.dicts;
        assert!(s.iter().all(|d| d.kind == DictKind::Table && d.name.is_empty()));
        assert_eq!((&s[0].key_column, &s[0].not_found, &s[0].source[..]), (&None, &None, &b"f"[..]));
        assert_eq!((&s[1].key_column, &s[1].not_found),
                   (&Some(b"k/x".to_vec()), &Some(NotFound::Value(b"d".to_vec()))));
        assert_eq!(s[2].not_found, Some(NotFound::Exit));
        assert_eq!((&s[3].key_column, &s[3].not_found), (&Some(b"k".to_vec()), &None));
        for bad in ["--td", "--td-x=f", "--tdict-x=f", "--td^=f", "--td^k", "--td/d^k=f", "--td=", "--td^k/d"] {
            assert!(run(&[bad]).is_err(), "{}", bad);
        }
        assert!(run(&["--tdx=f"]).unwrap_err().starts_with("Unrecognised long option"));
    }

    #[test]
    fn threads() {
        assert_eq!(run(&["--threads=3", "-A"]).unwrap().threads, Some(3));
        assert_eq!(run(&["-A"]).unwrap().threads, None);
        for bad in ["--threads=0", "--threads=", "--threads=x", "--threads=-1", "--threads"] {
            assert!(run(&[bad, "-A"]).is_err(), "{}", bad);
        }
    }

    #[test]
    fn sam_reference() {
        let c = run(&["-O3", "--sam-h/ref.fa", "--sam/r2.fa"]).unwrap();
        assert!(c.sam && !c.sam_h && c.k);
        assert_eq!(c.omega, Some(12));
        assert_eq!(c.dicts.len(), 2);
        assert_eq!((c.dicts[0].kind, &c.dicts[0].name[..], &c.dicts[0].not_found, &c.dicts[0].source[..]),
                   (DictKind::Fasta, &b"SAMFA"[..], &Some(NotFound::Exit), &b"ref.fa"[..]));
        assert_eq!(run(&["--sam/ref.fa", "-O3"]).unwrap().omega, Some(3));
        assert!(run(&["--sam/"]).is_err());
        assert!(run(&["--sam-h/"]).is_err());
        assert!(run(&["--fd-SAMFA=f"]).is_err());
        assert!(run(&["--fasta-dict-SAMFA=f"]).is_err());
    }

    #[test]
    fn decode() {
        assert_eq!(pct_decode(b"a%2Fb%3a"), b"a/b:".to_vec());
        assert_eq!(pct_decode(b"%zz%2"), b"%zz%2".to_vec());
    }
}
