// Command line options.
//
// Parsing is done by the clap crate for now; it will be replaced by bespoke
// parsing later (see plan/guidelines.txt). Other modules only see the Config
// struct, not clap.

use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about = "pluk: a Rust implementation of pick")]
pub struct Config {
    /// Skip lines starting with '#' at the start of input (temporary option)
    #[arg(long = "test-comment", visible_alias = "tc")]
    pub test_comment: bool,

    /// Output only column <N> (1-based), preceded by the line count (temporary option)
    #[arg(long = "test-i", value_name = "N", value_parser = clap::value_parser!(u32).range(1..))]
    pub test_i: Option<u32>,
}

pub fn parse_args() -> Config {
    Config::parse()
}
