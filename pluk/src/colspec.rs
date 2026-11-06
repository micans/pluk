// Column specifications: how columns are named on the command line
// (plan/COLSPEC.design.txt).
//
// A specification is a ':'-separated list of elements. An element is a plain
// name, or, with a leading '+', a regex that must match the whole column name.
// The argument is split on ':' first and each element is %-decoded afterwards
// (as pick), so a ':' within a name or regex is written %3A; the '+' is tested
// before decoding, so %2Bfoo is the plain name +foo.
//
// parse() runs during option parsing, so that a regex error is an option error
// before any input is read; expand() runs once the column names are known.

use regex::bytes::Regex;

use crate::io::compile_pattern;
use crate::names::ColumnNames;
use crate::opt::pct_decode;

#[derive(Clone, Debug)]
pub enum Element {
    Name(Vec<u8>),                      // %-decoded
    Regex { text: Vec<u8>, re: Regex }, // text: the element as given, for messages
}

#[derive(Clone, Debug)]
pub struct ColSpec {
    pub text:     Vec<u8>,              // the argument as given, for messages
    pub elements: Vec<Element>,
}

// Characters that suggest a name was meant as a regex (pick's REGEXP_CHARS, and
// a few more); used only for the hint on an unknown name.
const REGEX_CHARS: &[u8] = b"[({\\*?^$.|";

pub fn parse(arg: &[u8]) -> Result<ColSpec, String> {
    let shown = String::from_utf8_lossy(arg);
    if arg.windows(2).any(|w| w == b"::" || w == b":=") {
        return Err(format!("[{}]: computes ('::', ':=') not yet supported", shown));
    }
    let mut elements = Vec::new();
    for raw in arg.split(|&b| b == b':') {
        let element = match raw.strip_prefix(b"+") {
            Some(re) => {
                if re.is_empty() {
                    return Err(format!("[{}]: empty regex after '+'", shown));
                }
                let re = pct_decode(re);
                let what = format!("[{}]", String::from_utf8_lossy(raw));
                // Compiled once as given (for a clear message), then anchored to
                // match the whole name: \A(?:re)\z keeps group 1 as the user's group 1.
                compile_pattern(&re, &what)?;
                let mut anchored = b"\\A(?:".to_vec();
                anchored.extend_from_slice(&re);
                anchored.extend_from_slice(b")\\z");
                Element::Regex { text: raw.to_vec(), re: compile_pattern(&anchored, &what)? }
            }
            None => Element::Name(pct_decode(raw)),
        };
        elements.push(element);
    }
    Ok(ColSpec { text: arg.to_vec(), elements })
}

impl ColSpec {
    // The 0-based column indexes selected, in list order across elements and
    // header order within a regex; duplicates are kept. Every element must
    // select at least one column. label and kind make pick's messages, e.g.
    // "(Output) handle [x] in output does not reference ...".
    pub fn expand(&self, names: &ColumnNames, label: &str, kind: &str) -> Result<Vec<usize>, String> {
        let mut offsets = Vec::new();
        for element in &self.elements {
            match element {
                Element::Name(name) => match names.index_of(name) {
                    Some(i) => offsets.push(i),
                    None => return Err(unknown_name(name, names, label, kind)),
                },
                Element::Regex { text, re } => {
                    let before = offsets.len();
                    offsets.extend((0..names.len()).filter(|&i| re.is_match(names.name(i))));
                    if offsets.len() == before {
                        return Err(format!("({}) regex [{}] in {} matches no column among {}",
                                           label, String::from_utf8_lossy(text), kind, names.joined()));
                    }
                }
            }
        }
        Ok(offsets)
    }
}

// pick's message for an unknown name, with a hint if the name looks like a regex.
fn unknown_name(name: &[u8], names: &ColumnNames, label: &str, kind: &str) -> String {
    let shown = String::from_utf8_lossy(name);
    let mut msg = format!("({}) handle [{}] in {} does not reference a known entity among {}",
                          label, shown, kind, names.joined());
    if name.iter().any(|b| REGEX_CHARS.contains(b)) {
        msg.push_str(&format!("\n-- a regex column name starts with '+', e.g. +{} --", shown));
    }
    msg
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::row::Row;

    fn names(header: &str) -> ColumnNames {
        let mut row = Row::new();
        row.line.extend_from_slice(header.as_bytes());
        row.split_fields();
        ColumnNames::from_header(&row, false, false).unwrap()
    }

    fn expand(spec: &str, header: &str) -> Result<Vec<usize>, String> {
        parse(spec.as_bytes())?.expand(&names(header), "Output", "output")
    }

    #[test]
    fn lists_and_regexes() {
        let h = "a\tfoo1\tfoo22\tb\tfoo3";
        assert_eq!(expand("a", h), Ok(vec![0]));
        assert_eq!(expand("b:a", h), Ok(vec![3, 0]));
        assert_eq!(expand("+foo\\d", h), Ok(vec![1, 4]));
        assert_eq!(expand("b:+foo\\d+:a", h), Ok(vec![3, 1, 2, 4, 0]));
        // Anchored: the whole name must match.
        assert_eq!(expand("+oo\\d", h), Err("(Output) regex [+oo\\d] in output matches no column among a,foo1,foo22,b,foo3".to_string()));
        assert_eq!(expand("+.*oo2.*", h), Ok(vec![2]));
        assert_eq!(expand("+a|b", h), Ok(vec![0, 3]));
        // Duplicates are kept here; callers decide.
        assert_eq!(expand("a:a:+a", h), Ok(vec![0, 0, 0]));
    }

    #[test]
    fn decoding() {
        let h = "a:b\t+c\td";
        assert_eq!(expand("a%3Ab", h), Ok(vec![0]));
        assert_eq!(expand("%2Bc:d", h), Ok(vec![1, 2]));
        assert_eq!(expand("+a%3A.", h), Ok(vec![0]));
        assert_eq!(expand("+\\+c", h), Ok(vec![1]));     // in a regex, a literal + is \+
    }

    #[test]
    fn errors() {
        let h = "a\tb";
        assert!(parse(b"+").is_err());
        assert!(parse(b"a:+").is_err());
        assert!(parse(b"+a(").is_err());
        assert!(parse(b"x::y").is_err());
        assert!(parse(b"x:=y").is_err());
        assert!(expand("a:", h).is_err());        // empty element: unknown name ''
        assert!(expand("a:+c.*", h).is_err());    // every element must match
        let msg = expand("num(x)", h).unwrap_err();
        assert!(msg.contains("+num(x)"), "{}", msg);
        assert!(!expand("zz", h).unwrap_err().contains("regex"));
    }
}
