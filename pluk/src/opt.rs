// Command line options.
//
// Parsing is done by the clap crate for now; it will be replaced by bespoke
// parsing later (see plan/guidelines.txt). Other modules only see the Config
// struct, not clap.

use clap::Parser;
use std::process;

#[derive(Parser, Debug)]
#[command(version, about = "pluk: a Rust implementation of pick")]
pub struct Config {
    /// Input has no header line; columns are named 1 2 ... N
    #[arg(short = 'k')]
    pub k: bool,

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

    /// Skip lines starting with '#' at the start of input (temporary option)
    #[arg(long = "test-comment", visible_alias = "tc")]
    pub test_comment: bool,

    /// Output only column <N> (1-based), preceded by the line count (temporary option)
    #[arg(long = "test-i", value_name = "N", value_parser = clap::value_parser!(u32).range(1..))]
    pub test_i: Option<u32>,

    /// Print input counters to STDERR at the end (temporary option)
    #[arg(long = "test-counts")]
    pub test_counts: bool,

    /// Print column index, name, and index looked up by name, then exit (temporary option)
    #[arg(long = "test-names")]
    pub test_names: bool,
}

pub fn parse_args() -> Config {
    let config = Config::parse();

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
