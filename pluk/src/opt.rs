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

#[derive(Debug, Default)]
pub struct Config {
    pub no_header: bool,                // -h
    pub k: bool,                        // -k
    pub all: bool,                      // -A
    pub insert_offset: Option<i64>,     // -A<N>; not yet acted upon
    pub excise: bool,                   // -x
    pub c: bool,                        // -c
    pub omega: Option<u32>,             // -O<N>
    pub add_row_column: bool,           // -R
    pub make_unique: bool,              // -u
    pub sam: bool,                      // --sam
    pub sam_h: bool,                    // --sam-h
    pub pat_purge: Option<Vec<u8>>,     // -/PAT, %-decoded
    pub pat_pass: Option<Vec<u8>>,      // -//PAT, %-decoded
    pub test_i: Option<u32>,            // --test-i=N (temporary)
    pub test_lineno: bool,              // --test-lineno (temporary)
    pub test_counts: bool,              // --test-counts (temporary)
    pub test_names: bool,               // --test-names (temporary)
    pub test_select: bool,              // --test-select (temporary)
    pub columns: Vec<Vec<u8>>,          // column arguments
}

// Result of a successful parse: run, or print help or version and exit.
#[derive(Debug)]
pub enum Parsed {
    Run(Config),
    Help,
    Version,
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
        Err(msg) => {
            eprintln!("{}", msg);
            process::exit(1);
        }
    }
}

// Placeholder; the option listing follows in step3 item 5.
fn print_help() {
    println!("Usage: pluk [options] <column>+ < FILE");
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
    config.columns = args[i..].to_vec();
    check_combinations(&config)?;
    Ok(Parsed::Run(config))
}

// A '--' option, one per word. Returns Some for options that end parsing.
fn parse_long(long: &[u8], config: &mut Config) -> Result<Option<Parsed>, String> {
    match long {
        b"help"    => return Ok(Some(Parsed::Help)),
        b"version" => return Ok(Some(Parsed::Version)),
        // --sam and --sam-h imply -k and -O12; a later -O overrides.
        b"sam" | b"sam-h" => {
            config.sam   = long == b"sam";
            config.sam_h = long == b"sam-h";
            config.k     = true;
            config.omega = Some(12);
        }
        b"test-lineno"          => config.test_lineno = true,
        b"test-counts"          => config.test_counts = true,
        b"test-names"           => config.test_names = true,
        b"test-select"          => config.test_select = true,
        _ if long.starts_with(b"test-i=") => {
            let n = parse_number::<u32>(&long[7..]);
            match n {
                Some(n) if n > 0 => config.test_i = Some(n),
                _ => return Err("--test-i=<N> N must be a positive number".to_string()),
            }
        }
        _ if long.starts_with(b"sam/") || long.starts_with(b"sam-h/") => {
            return Err("--sam/<fastafile> not yet supported".to_string());
        }
        _ => return Err(format!("Unrecognised long option --{}", String::from_utf8_lossy(long))),
    }
    Ok(None)
}

// A bundle of '-' options, without the leading '-', scanned left to right.
fn parse_bundle(chars: &[u8], config: &mut Config) -> Result<(), String> {
    let mut j = 0;
    while j < chars.len() {
        let c = chars[j];
        j += 1;
        match c {
            b'k' => config.k = true,
            b'h' => config.no_header = true,
            b'c' => config.c = true,
            b'x' => config.excise = true,
            b'R' => config.add_row_column = true,
            b'u' => config.make_unique = true,
            b'A' => {
                // Optional attached number, possibly negative: -A, -A4, -A-1.
                config.all = true;
                let len = number_len(&chars[j..], true);
                if len > 0 {
                    let n = parse_number::<i64>(&chars[j..j + len])
                        .ok_or("-A<N> N out of range")?;
                    config.insert_offset = Some(n);
                    j += len;
                }
            }
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

    #[test]
    fn bundle_with_numbers_and_pattern() {
        let c = run(&["-A4kO8/^#", "a"]).unwrap();
        assert!(c.all && c.k);
        assert_eq!(c.insert_offset, Some(4));
        assert_eq!(c.omega, Some(8));
        assert_eq!(c.pat_purge, Some(b"^#".to_vec()));
        assert_eq!(c.columns, vec![b"a".to_vec()]);
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
        assert_eq!(c.columns, vec![b"1".to_vec(), b"-h".to_vec()]);
        let c = run(&["--", "-h"]).unwrap();
        assert_eq!(c.columns, vec![b"-h".to_vec()]);
    }

    #[test]
    fn unknown_options() {
        assert_eq!(run(&["-kw"]).unwrap_err(), "Option string not fully parsed: -w remains");
        assert_eq!(run(&["--nosuch"]).unwrap_err(), "Unrecognised long option --nosuch");
    }

    #[test]
    fn decode() {
        assert_eq!(pct_decode(b"a%2Fb%3a"), b"a/b:".to_vec());
        assert_eq!(pct_decode(b"%zz%2"), b"%zz%2".to_vec());
    }
}
