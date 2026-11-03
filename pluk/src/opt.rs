// Command line options.
//
// Parsing is done by the clap crate for now; it will be replaced by bespoke
// parsing later (see plan/guidelines.txt). Other modules only see the Config
// struct, not clap.

use clap::Parser;
use std::ffi::OsString;
use std::process;

#[derive(Parser, Debug)]
// disable_help_flag: pick uses -h for "no header"; help stays available as --help.
#[command(version, about = "pluk: a Rust implementation of pick", disable_help_flag = true)]
pub struct Config {
    /// Print help
    #[arg(long = "help", action = clap::ArgAction::Help)]
    pub help: Option<bool>,

    /// Do not output the header line
    #[arg(short = 'h')]
    pub no_header: bool,

    /// Input has no header line; columns are named 1 2 ... N
    #[arg(short = 'k')]
    pub k: bool,

    /// Output all columns
    #[arg(short = 'A')]
    pub all: bool,

    /// Output all columns except the given ones
    #[arg(short = 'x')]
    pub excise: bool,

    /// Only output the count of rows
    #[arg(short = 'c')]
    pub c: bool,

    /// Ragged input: use <N> columns, merge columns at/after position <N> (requires -k)
    #[arg(short = 'O', value_name = "N", value_parser = clap::value_parser!(u32).range(1..))]
    pub omega: Option<u32>,

    /// Header has no name for the first (row name) column
    #[arg(short = 'R')]
    pub add_row_column: bool,

    /// Make duplicate column names unique (suffixes _v02, _v03, ...)
    #[arg(short = 'u')]
    pub make_unique: bool,

    /// SAM input: implies -k and -O12; skip header lines (starting with '@')
    #[arg(long = "sam")]
    pub sam: bool,

    /// As --sam, but output header lines (starting with '@') unchanged
    #[arg(long = "sam-h")]
    pub sam_h: bool,

    /// Skip lines starting with '#' at the start of input (temporary option)
    #[arg(long = "test-comment", visible_alias = "tc")]
    pub test_comment: bool,

    /// Output only column <N> (1-based), preceded by the line count (temporary option)
    #[arg(long = "test-i", value_name = "N", value_parser = clap::value_parser!(u32).range(1..))]
    pub test_i: Option<u32>,

    /// Output line number and line, as in step1 (temporary option)
    #[arg(long = "test-lineno")]
    pub test_lineno: bool,

    /// Print input counters to STDERR at the end (temporary option)
    #[arg(long = "test-counts")]
    pub test_counts: bool,

    /// Print column index, name, and index looked up by name, then exit (temporary option)
    #[arg(long = "test-names")]
    pub test_names: bool,

    /// Print the selected output column names and their 0-based indexes, then exit (temporary option)
    #[arg(long = "test-select")]
    pub test_select: bool,

    /// Column names to output
    // OsString: arguments are taken as bytes, without requiring UTF-8 (see main.rs).
    #[arg(value_name = "COLUMN")]
    pub columns: Vec<OsString>,
}

pub fn parse_args() -> Config {
    let mut config = Config::parse();

    // --sam and --sam-h imply -k and -O12; an explicit -O takes precedence.
    if config.sam || config.sam_h {
        config.k = true;
        if config.omega.is_none() {
            config.omega = Some(12);
        }
    }

    // Combinations clap does not check; messages as in pick.
    if config.omega.is_some() && !config.k {
        eprintln!("-O<N> requires -k");
        process::exit(1);
    }
    if config.add_row_column && config.k {
        eprintln!("-R cannot be combined with -k");
        process::exit(1);
    }
    config
}
