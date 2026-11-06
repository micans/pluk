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

impl Num {
    fn to_f64(self) -> f64 {
        match self {
            Num::I(i) => i as f64,
            Num::F(f) => f,
        }
    }
}

// /ep/: whether x lies within eps of y. Integers x and y are compared exactly
// (|x - y| as an unsigned value); if x - y overflows, or for float operands, the
// test is done in f64 with pick's formula. NaN anywhere gives false.
pub fn within_eps(x: Num, y: Num, eps: Num) -> bool {
    if let (Num::I(a), Num::I(b)) = (x, y) {
        if let Some(d) = a.checked_sub(b) {
            let d = d.unsigned_abs();
            return match eps {
                Num::I(e) => e >= 0 && d <= e.unsigned_abs(),
                Num::F(e) => (d as f64) <= e,
            };
        }
    }
    eps_ff(x.to_f64(), y.to_f64(), eps.to_f64())
}

fn eps_ff(x: f64, y: f64, eps: f64) -> bool {
    x <= y + eps && x >= y - eps
}

// /om/: whether |x| lies within a factor k of |y|. All integers: |x| <= |y|*k and
// |x|*k >= |y| on unsigned values (no division); a product that overflows exceeds
// every Int, which decides its half of the test. Otherwise f64 with pick's formula.
pub fn within_oom(x: Num, y: Num, k: Num) -> bool {
    match (x, y, k) {
        (Num::I(a), Num::I(b), Num::I(k)) => {
            let (a, b, k) = (a.unsigned_abs(), b.unsigned_abs(), k.unsigned_abs());
            b.checked_mul(k).map_or(true, |bk| a <= bk) && a.checked_mul(k).map_or(true, |ak| ak >= b)
        }
        _ => oom_ff(x.to_f64(), y.to_f64(), k.to_f64()),
    }
}

fn oom_ff(x: f64, y: f64, k: f64) -> bool {
    x.abs() <= y.abs() * k && x.abs() >= y.abs() / k
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

    #[test]
    fn eps() {
        assert!(within_eps(I(5), I(3), I(2)));
        assert!(!within_eps(I(5), I(3), I(1)));
        assert!(within_eps(I(-3), I(-5), F(2.0)));
        assert!(within_eps(F(1.00005), I(1), F(0.0001)));
        assert!(!within_eps(F(1.0002), I(1), F(0.0001)));
        // Exact beyond 2^53: as f64 these two are equal.
        assert!(!within_eps(I(9007199254740993), I(9007199254740992), I(0)));
        assert!(within_eps(I(9007199254740993), I(9007199254740992), I(1)));
        // x - y overflows: decided in f64.
        assert!(!within_eps(I(Int::MAX), I(Int::MIN), I(1)));
        assert!(within_eps(I(Int::MAX), I(Int::MIN), F(f64::INFINITY)));
        assert!(!within_eps(F(f64::NAN), I(1), I(1)));
    }

    #[test]
    fn oom() {
        assert!(within_oom(I(4), I(2), I(2)));
        assert!(within_oom(I(1), I(2), I(2)));
        assert!(!within_oom(I(5), I(2), I(2)));
        assert!(!within_oom(I(0), I(1), I(2)));
        assert!(within_oom(I(0), I(0), I(2)));
        assert!(within_oom(I(-4), I(3), I(2)));
        assert!(within_oom(F(2.9), I(2), F(1.5)));
        assert!(!within_oom(F(3.1), I(2), F(1.5)));
        // Products that overflow, and Int::MIN.
        assert!(within_oom(I(Int::MAX), I(Int::MAX / 2 + 1), I(2)));
        assert!(within_oom(I(Int::MIN), I(Int::MAX), I(2)));
        assert!(!within_oom(I(Int::MAX), I(1), I(2)));
        assert!(!within_oom(F(f64::NAN), I(1), I(2)));
    }
}
