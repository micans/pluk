// Command line options.
//
// Parsing is done by the clap crate for now; it will be replaced by bespoke
// parsing later (see plan/guidelines.txt). Other modules only see the Config
// struct, not clap.

use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about = "pluk: a Rust implementation of pick")]
pub struct Config {
}

pub fn parse_args() -> Config {
    Config::parse()
}
