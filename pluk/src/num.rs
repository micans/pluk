// Numeric values (plan/NUMERIC.design.txt).
//
// A numeric operand is a Num: an integer (I) or a float (F). It is produced by
// parsing a field when an operator needs a number; a field that is not a number
// gives None (NA). Operations are typed functions per operand combination, chosen
// by a dispatcher; mixed operands promote the integer to f64.

use std::cmp::Ordering;

// Integer width, a compile-time choice (NUMERIC.design.txt section 6). Computed
// values use Int, never i64 directly.
#[cfg(not(feature = "int128"))]
pub type Int = i64;
#[cfg(feature = "int128")]
pub type Int = i128;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Num {
    I(Int),
    F(f64),
}

// Parse a field as a number. Integer syntax (optional sign, digits) that fits Int
// gives I; anything else accepted by Rust's f64 parser gives F (1.5, .5, 5., 1e3,
// inf, nan; an integer too large for Int). Surrounding whitespace and hexadecimal
// are not numbers. None: not a number (NA).
pub fn parse_num(s: &[u8]) -> Option<Num> {
    let text = std::str::from_utf8(s).ok()?;
    if is_integer_syntax(s) {
        if let Ok(i) = text.parse::<Int>() {
            return Some(Num::I(i));
        }
    }
    text.parse::<f64>().ok().map(Num::F)
}

fn is_integer_syntax(s: &[u8]) -> bool {
    let digits = match s.first() {
        Some(b'+' | b'-') => &s[1..],
        _ => s,
    };
    !digits.is_empty() && digits.iter().all(u8::is_ascii_digit)
}

// Comparison, typed per combination. None: unordered (a NaN is involved).
fn cmp_ii(a: Int, b: Int) -> Option<Ordering> {
    Some(a.cmp(&b))
}

fn cmp_ff(a: f64, b: f64) -> Option<Ordering> {
    a.partial_cmp(&b)
}

// The dispatcher for comparison: selects the typed function; mixed operands
// promote the integer to f64 (exact up to 2^53, NUMERIC.design.txt section 8).
pub fn compare(a: Num, b: Num) -> Option<Ordering> {
    match (a, b) {
        (Num::I(x), Num::I(y)) => cmp_ii(x, y),
        (Num::F(x), Num::F(y)) => cmp_ff(x, y),
        (Num::I(x), Num::F(y)) => cmp_ff(x as f64, y),
        (Num::F(x), Num::I(y)) => cmp_ff(x, y as f64),
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use Num::{F, I};

    #[test]
    fn parse() {
        assert_eq!(parse_num(b"42"), Some(I(42)));
        assert_eq!(parse_num(b"-7"), Some(I(-7)));
        assert_eq!(parse_num(b"+3"), Some(I(3)));
        assert_eq!(parse_num(b"007"), Some(I(7)));
        assert_eq!(parse_num(b"3.14"), Some(F(3.14)));
        assert_eq!(parse_num(b".5"), Some(F(0.5)));
        assert_eq!(parse_num(b"5."), Some(F(5.0)));
        assert_eq!(parse_num(b"1e3"), Some(F(1000.0)));
        assert_eq!(parse_num(b"-inf"), Some(F(f64::NEG_INFINITY)));
        assert!(matches!(parse_num(b"nan"), Some(F(x)) if x.is_nan()));
        let big = format!("{}0", Int::MAX);
        assert!(matches!(parse_num(big.as_bytes()), Some(F(_))));
        for bad in [&b""[..], b"fox", b"0x10", b" 4", b"4 ", b"-", b"+", b"1,5", b"\xff"] {
            assert_eq!(parse_num(bad), None, "{:?}", bad);
        }
    }

    #[test]
    fn comparisons() {
        assert_eq!(compare(I(1), I(2)), Some(Ordering::Less));
        assert_eq!(compare(I(2), F(2.0)), Some(Ordering::Equal));
        assert_eq!(compare(F(2.5), I(2)), Some(Ordering::Greater));
        assert_eq!(compare(F(f64::NAN), I(2)), None);
        assert_eq!(compare(I(Int::MAX), I(Int::MAX - 1)), Some(Ordering::Greater));
    }
}
