// Column specifications: how columns are named on the command line
// (plan/COLSPEC.design.txt).
//
// A specification is a ':'-separated list of elements. An element is a plain
// name, or, with a leading '+', a regex that must match the whole column name,
// or, with -k, a range of column numbers (as pick's parse_range).
// The argument is split on ':' first and each element is %-decoded afterwards
// (as pick), so a ':' within a name or regex is written %3A; the '+' is tested
// before decoding, so %2Bfoo is the plain name +foo.
//
// parse() runs during option parsing, so that a regex error is an option error
// before any input is read; expand() runs once the column names are known.

use std::sync::LazyLock;

use regex::bytes::Regex;

use crate::io::compile_pattern;
use crate::names::ColumnNames;
use crate::opt::pct_decode;

#[derive(Clone, Debug)]
pub enum Element {
    Name(Vec<u8>),                      // %-decoded
    Regex { text: Vec<u8>, re: Regex }, // text: the element as given, for messages
    Range(Range),                       // -k only
}

// A -k range O+S-E*M: the 1-based columns O + i*M for i = S..E; S- runs to the
// last column (end None).
#[derive(Clone, Debug, PartialEq)]
pub struct Range {
    text:   Vec<u8>,
    offset: usize,
    start:  usize,
    end:    Option<usize>,
    mult:   usize,
}

#[derive(Clone, Debug)]
pub struct ColSpec {
    pub text:     Vec<u8>,              // the argument as given, for messages
    pub elements: Vec<Element>,
}

// Characters that suggest a name was meant as a regex (pick's REGEXP_CHARS, and
// a few more); used only for the hint on an unknown name.
const REGEX_CHARS: &[u8] = b"[({\\*?^$.|";

// headless (-k): elements of range syntax are ranges.
pub fn parse(arg: &[u8], headless: bool) -> Result<ColSpec, String> {
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
                // \Q...\E is expanded before wrapping, so that an unterminated
                // \Q does not quote the wrapper.
                let quoted = std::str::from_utf8(&re).map(crate::io::expand_quotes)
                    .map_or(re.clone(), String::into_bytes);
                let mut anchored = b"\\A(?:".to_vec();
                anchored.extend_from_slice(&quoted);
                anchored.extend_from_slice(b")\\z");
                Element::Regex { text: raw.to_vec(), re: compile_pattern(&anchored, &what)? }
            }
            None => match parse_range(raw) {
                Some(range) if headless => Element::Range(range?),
                _ => Element::Name(pct_decode(raw)),
            },
        };
        elements.push(element);
    }
    Ok(ColSpec { text: arg.to_vec(), elements })
}

// Range syntax, on the raw element, as pick's parse_range: [O+]S-[E][*M]. A
// single number N is not a range but the plain name N (as pick: 007 is not a
// column name under -k, while 1-02 is a range),
// all numbers decimal ASCII digits ([0-9]: \d would include other Unicode
// digits). The regex is compiled once, on first use (LazyLock).
static RANGE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\A(?:([0-9]+)\+)?([0-9]+)-([0-9]+)?(?:\*([0-9]+))?\z").unwrap()
});

// None: not range syntax. Some(Err): range syntax with a number too large for
// usize, or a multiplier of 0.
fn parse_range(raw: &[u8]) -> Option<Result<Range, String>> {
    let caps = RANGE.captures(raw)?;
    let shown = String::from_utf8_lossy(raw);
    // Capture group k as a number; None if the group did not take part.
    let number = |k: usize| -> Result<Option<usize>, String> {
        match caps.get(k) {
            None => Ok(None),
            Some(m) => std::str::from_utf8(m.as_bytes()).unwrap().parse::<usize>().map(Some)
                .map_err(|_| format!("[{}]: number too large", shown)),
        }
    };
    let range = (|| {
        let mult = number(4)?.unwrap_or(1);
        if mult == 0 {
            return Err(format!("[{}]: the multiplier of a range must be at least 1", shown));
        }
        Ok(Range {
            text:   raw.to_vec(),
            offset: number(1)?.unwrap_or(0),
            start:  number(2)?.unwrap_or(0),     // group 2 always takes part
            end:    number(3)?,
            mult,
        })
    })();
    Some(range)
}

impl Range {
    // The 0-based indexes of the columns in range, given n columns, in order, and
    // whether some were dropped as out of range (as pick, which reports this).
    fn indexes(&self, n: usize) -> (Vec<usize>, bool) {
        let end = self.end.unwrap_or(n);
        let mut out = Vec::new();
        let mut dropped = false;
        let mut i = self.start;
        while i <= end {
            // Increasing in i (mult >= 1): once beyond n, the rest is too.
            match i.checked_mul(self.mult).and_then(|x| x.checked_add(self.offset)) {
                Some(c) if c >= 1 && c <= n => out.push(c - 1),
                Some(0) => dropped = true,
                _ => {
                    dropped = true;
                    break;
                }
            }
            i += 1;
        }
        (out, dropped)
    }
}

impl ColSpec {
    // The 0-based column indexes selected, in list order across elements and
    // header order within a regex; duplicates are kept. Names refer to their
    // current binding (in-place computes): a regex skips replaced columns, and a
    // range maps a replaced column to its replacement. Every element must
    // select at least one column. label and kind make pick's messages, e.g.
    // "(Output) handle [x] in output does not reference ...".
    pub fn expand(&self, names: &ColumnNames, label: &str, kind: &str) -> Result<Vec<usize>, String> {
        Ok(self.expand_bases(names, label, kind)?.into_iter().map(|(i, _)| i).collect())
    }

    // As expand, with capture group 1 of the '+' regex element that selected each
    // column, if the regex has a capturing group (plan/step26.txt decision 3: it names
    // the new column of a compute). A group that does not take part in the match, or
    // matches the empty string, is an error: a column cannot be named by nothing.
    pub fn expand_bases(&self, names: &ColumnNames, label: &str, kind: &str)
        -> Result<Vec<(usize, Option<Vec<u8>>)>, String>
    {
        let mut offsets: Vec<(usize, Option<Vec<u8>>)> = Vec::new();
        for element in &self.elements {
            match element {
                Element::Name(name) => match names.index_of(name) {
                    Some(i) => offsets.push((i, None)),
                    None => return Err(unknown_name(name, names, label, kind)),
                },
                Element::Range(range) => {
                    // Ranges number the input columns (as pick); a replaced column
                    // stands for its replacement.
                    let (found, dropped) = range.indexes(names.n_input());
                    let found: Vec<usize> = found.into_iter().map(|i| names.current(i)).collect();
                    if dropped {
                        crate::note::note!("-- range expansion produced some indexes out of range (now dropped)");
                    }
                    if found.is_empty() {
                        return Err(format!("({}) range [{}] in {} selects no column (there are {})",
                                           label, String::from_utf8_lossy(&range.text), kind, names.len()));
                    }
                    offsets.extend(found.into_iter().map(|i| (i, None)));
                }
                Element::Regex { text, re } => {
                    let before = offsets.len();
                    for i in (0..names.len()).filter(|&i| names.is_current(i)) {
                        let Some(caps) = re.captures(names.name(i)) else { continue };
                        let base = if re.captures_len() > 1 {
                            match caps.get(1).map(|g| g.as_bytes()).filter(|g| !g.is_empty()) {
                                Some(g) => Some(g.to_vec()),
                                None => return Err(format!("({}) regex [{}] in {}: group 1 is empty or does not \
                                                            take part in the match for column [{}]", label,
                                                           String::from_utf8_lossy(text), kind,
                                                           String::from_utf8_lossy(names.name(i)))),
                            }
                        } else {
                            None
                        };
                        offsets.push((i, base));
                    }
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
        parse(spec.as_bytes(), false)?.expand(&names(header), "Output", "output")
    }

    fn parse_plain(arg: &[u8]) -> Result<ColSpec, String> {
        parse(arg, false)
    }

    fn expand_k(spec: &str, n: usize) -> Result<Vec<usize>, String> {
        parse(spec.as_bytes(), true)?.expand(&ColumnNames::numbered(n), "Output", "output")
    }

    #[test]
    fn ranges() {
        assert_eq!(expand_k("3", 13), Ok(vec![2]));
        assert!(expand_k("007", 13).is_err());           // a plain name, as pick
        assert_eq!(expand_k("1-02", 13), Ok(vec![0, 1]));
        assert!(matches!(parse(b"3", true).unwrap().elements[0], Element::Name(_)));
        assert_eq!(expand_k("3-5", 13), Ok(vec![2, 3, 4]));
        assert_eq!(expand_k("11-", 13), Ok(vec![10, 11, 12]));
        assert_eq!(expand_k("2+1-3", 13), Ok(vec![2, 3, 4]));
        assert_eq!(expand_k("1-5*2", 13), Ok(vec![1, 3, 5, 7, 9]));
        assert_eq!(expand_k("2+1-3*4", 13), Ok(vec![5, 9]));
        assert_eq!(expand_k("3:5-7:1", 13), Ok(vec![2, 4, 5, 6, 0]));
        assert_eq!(expand_k("12-15", 13), Ok(vec![11, 12]));
        assert_eq!(expand_k("0-2", 13), Ok(vec![0, 1]));
        assert!(parse(b"1-99999999999999999999999", true).is_err());
        assert_eq!(expand_k("2-18446744073709551615", 3), Ok(vec![1, 2]));
        assert!(expand_k("5-3", 13).is_err());
        assert!(expand_k("14", 13).is_err());
        assert!(expand_k("1-5*0", 13).is_err());
        assert!(expand_k("+1-3", 13).is_err());           // '+' starts a regex: '1-3' matches no name
        // Without -k range syntax is a plain name.
        assert_eq!(parse(b"3-5", false).unwrap().elements.len(), 1);
        assert!(matches!(parse(b"3-5", false).unwrap().elements[0], Element::Name(_)));
        assert!(matches!(parse(b"a-5", true).unwrap().elements[0], Element::Name(_)));
        assert!(matches!(parse(b"3-5x", true).unwrap().elements[0], Element::Name(_)));
        assert!(matches!(parse("3-\u{0663}".as_bytes(), true).unwrap().elements[0], Element::Name(_)));
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
        assert!(parse_plain(b"+").is_err());
        assert!(parse_plain(b"a:+").is_err());
        assert!(parse_plain(b"+a(").is_err());
        assert!(parse_plain(b"x::y").is_err());
        assert!(parse_plain(b"x:=y").is_err());
        assert!(expand("a:", h).is_err());        // empty element: unknown name ''
        assert!(expand("a:+c.*", h).is_err());    // every element must match
        let msg = expand("num(x)", h).unwrap_err();
        assert!(msg.contains("+num(x)"), "{}", msg);
        assert!(!expand("zz", h).unwrap_err().contains("regex"));
    }
}
