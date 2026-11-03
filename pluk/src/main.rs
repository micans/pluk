
mod cl;

use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::Path;
use std::any::type_name_of_val;

use std::env;


fn main() {

    let config = cl::parse_args();
    let colid  = config.colid;

    let reader: Box<dyn BufRead> = match config.input {
        Some(path) => {
            let file = File::open(&path).unwrap_or_else(|err| {
                eprintln!("Error opening file '{}': {}", path, err);
                std::process::exit(1);
            });
            Box::new(BufReader::new(file))
        }
        None => Box::new(BufReader::new(io::stdin())),
    };


    for line in reader.lines() {
        match line {
            Ok(content) => {
               println!("line: {}", content);
            }
            Err(e) => eprintln!("Error reading line: {}", e),
        }
    }
}


