// -l: the operator listing (plan/step16.txt decisions 3 and 4).
//
// One line per operator in pick's layout (columns of 14, 12 and 20; a longer entry
// is followed by one space and shifts the rest), the compute operators by category in the
// order of --help and within a category in table order, then the filter operators.
// TERMs (words, or comma-separated within a word) select: a category name selects
// that category, any other term the operators whose name contains it.

use std::io::Write;

use crate::ops::{self, Category};

// The categories in --help order, with their names; "filter" follows.
const CATEGORIES: [(Category, &str); 13] = [
    (Category::Stack, "stack"),
    (Category::String, "string"),
    (Category::Arithmetic, "arithmetic"),
    (Category::Math, "math"),
    (Category::Precision, "precision"),
    (Category::Dictionary, "dictionary"),
    (Category::Branch, "branch"),
    (Category::State, "state"),
    (Category::Time, "time"),
    (Category::Bit, "bit"),
    (Category::Radix, "radix"),
    (Category::Regex, "regex"),
    (Category::Sam, "sam"),
];

const FILTER: &str = "filter";

// The filter operators and predicates (filter.rs), as used in @NAME<op>VALUE and by
// test (x is the column value, y the VALUE): operator, operands, result, description.
const FILTERS: [(&str, &str, &str, &str); 32] = [
    ("/lt/", "x y", "x < y", "Numeric less than"),
    ("/le/", "x y", "x <= y", "Numeric less than or equal"),
    ("/eq/", "x y", "x == y", "Numeric equal"),
    ("/ge/", "x y", "x >= y", "Numeric greater than or equal"),
    ("/gt/", "x y", "x > y", "Numeric greater than"),
    ("/ne/", "x y", "x != y", "Numeric not equal"),
    ("~lt~", "x y", "x lt y", "String (byte) less than"),
    ("~le~", "x y", "x le y", "String (byte) less than or equal"),
    ("~eq~", "x y", "x eq y", "String equal"),
    ("~ge~", "x y", "x ge y", "String (byte) greater than or equal"),
    ("~gt~", "x y", "x gt y", "String (byte) greater than"),
    ("~ne~", "x y", "x ne y", "String not equal"),
    ("=", "x y", "x eq y", "String equal"),
    ("/=", "x y", "x ne y", "String not equal"),
    ("~", "x p", "x =~ p", "Regex p matches x (:NAME: a fixed string)"),
    ("/~", "x p", "x !~ p", "Regex p does not match x"),
    ("~isin~", "x D", "x in D", "x is a key of dictionary D"),
    ("~isnotin~", "x D", "x not in D", "x is not a key of dictionary D"),
    ("~hasval~", "x D~v", "D{x} eq v", "x is a key of D with value v (not in test)"),
    ("~hasnotval~", "x D~v", "D{x} ne v", "x is not a key of D with value v (not in test)"),
    ("/ep/", "x y/e", "|x-y| <= e", "x within e of y (default e 0.0001)"),
    ("/epx/", "x y/e", "|x-y| > e", "x not within e of y"),
    ("/om/", "x y/k", "x ~ y", "|x| within a factor k of |y| (default k 2)"),
    ("/omx/", "x y/k", "x !~ y", "|x| not within a factor k of |y|"),
    ("/all/", "x m", "all m in |x|", "All bits of mask m set in |x|"),
    ("/any/", "x m", "any m in |x|", "At least one bit of mask m set in |x|"),
    ("/notall/", "x m", "!all m in |x|", "Not all bits of mask m set in |x|"),
    ("/none/", "x m", "!any m in |x|", "No bit of mask m set in |x|"),
    ("/ISINT", "x", "B", "x is an integer (no VALUE; not in test)"),
    ("/NOTINT", "x", "B", "x is not an integer (not in test)"),
    ("/ISNUM", "x", "B", "x is a number, not nan (not in test)"),
    ("/NOTNUM", "x", "B", "x is not a number (not in test)"),
];

struct Line {
    name: &'static str,
    operands: &'static str,
    result: &'static str,
    doc: &'static str,
    category: &'static str,
}

// All lines in listing order.
fn lines() -> Vec<Line> {
    let mut all = Vec::new();
    for (category, cname) in CATEGORIES {
        for op in ops::OPERATORS.iter().filter(|op| op.category == category) {
            all.push(Line { name: op.name, operands: op.operands, result: op.result, doc: op.doc, category: cname });
        }
    }
    for (name, operands, result, doc) in FILTERS {
        all.push(Line { name, operands, result, doc, category: FILTER });
    }
    all
}

fn is_category(term: &str) -> bool {
    term == FILTER || CATEGORIES.iter().any(|(_, name)| *name == term)
}

fn selected(line: &Line, terms: &[String]) -> bool {
    terms.is_empty()
        || terms.iter().any(|t| if is_category(t) { line.category == t } else { line.name.contains(t.as_str()) })
}

fn category_names() -> String {
    let mut names: Vec<&str> = CATEGORIES.iter().map(|(_, n)| *n).collect();
    names.push(FILTER);
    names.join(" ")
}

// The listing for the words after -l; the exit status (1: nothing matches).
pub fn run(words: &[Vec<u8>]) -> i32 {
    let terms: Vec<String> = words.iter()
        .flat_map(|w| w.split(|&b| b == b',').map(|t| String::from_utf8_lossy(t).into_owned()).collect::<Vec<_>>())
        .filter(|t| !t.is_empty())
        .collect();
    let chosen: Vec<Line> = lines().into_iter().filter(|l| selected(l, &terms)).collect();
    if chosen.is_empty() {
        eprintln!("pluk: no operator or category matches {}\nCategories: {}", terms.join(","), category_names());
        return 1;
    }
    let mut text = format!("{:<13} {:<11} {:<19} {}\n{}\n", "Operator", "Consumed", "Produced", "Description", "-".repeat(80));
    for l in &chosen {
        text.push_str(&format!("{:<13} {:<11} {:<19} {} [{}]\n", l.name, l.operands, l.result, l.doc, l.category));
    }
    text.push_str(&format!("\nUse -l TERM.. for a category or the operators whose name contains TERM.\nCategories: {}\n",
                           category_names()));
    let mut out = std::io::stdout().lock();
    match out.write_all(text.as_bytes()).and_then(|_| out.flush()) {
        Err(e) if e.kind() != std::io::ErrorKind::BrokenPipe => {
            eprintln!("pluk: {}", e);
            1
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(terms: &[&str]) -> Vec<&'static str> {
        let terms: Vec<String> = terms.iter().map(|t| t.to_string()).collect();
        lines().into_iter().filter(|l| selected(l, &terms)).map(|l| l.name).collect()
    }

    #[test]
    fn every_operator_listed_once() {
        let all = lines();
        assert_eq!(all.len(), ops::OPERATORS.len() + FILTERS.len());
        for op in ops::OPERATORS {
            assert_eq!(all.iter().filter(|l| l.name == op.name).count(), 1, "{}", op.name);
        }
    }

    #[test]
    fn filter_rows_are_filter_operators() {
        for (name, ..) in FILTERS {
            let known = crate::filter::operator(name.as_bytes()).is_some()
                || ["/ISINT", "/NOTINT", "/ISNUM", "/NOTNUM"].contains(&name);
            assert!(known, "{}", name);
        }
    }

    #[test]
    fn selection() {
        assert_eq!(names(&["add"]), ["add", "addall"]);
        assert_eq!(names(&["bit"]), ["bitand", "bitor", "bitxor", "bitoff", "bitlsh", "bitrsh"]);
        assert_eq!(names(&["time", "md5"]), ["md5", "now", "ttu", "rttu", "todate"]);
        assert_eq!(names(&["isin"]), ["~isin~"]);
        assert_eq!(names(&["eq"]), ["/eq/", "~eq~"]);
        assert!(names(&["nosuch"]).is_empty());
        assert_eq!(names(&["filter"]).len(), FILTERS.len());
    }
}
