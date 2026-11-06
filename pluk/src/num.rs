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
    pub fn is_nan(self) -> bool {
        matches!(self, Num::F(x) if x.is_nan())
    }

    pub fn to_f64(self) -> f64 {
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

// Bit tests on absolute values (as pick), on the unsigned type of the same width,
// so that Int::MIN has an absolute value. /all/: every bit of the mask is set in
// x; /any/: at least one is.
pub fn bits_all(x: Int, mask: Int) -> bool {
    let m = mask.unsigned_abs();
    x.unsigned_abs() & m == m
}

pub fn bits_any(x: Int, mask: Int) -> bool {
    x.unsigned_abs() & mask.unsigned_abs() != 0
}

// Addition, typed per combination (NUMERIC.design.txt section 3). Generic add:
// integer overflow promotes to F, as Perl and pick (section 4).
fn add_ii(a: Int, b: Int) -> Num {
    match a.checked_add(b) {
        Some(c) => Num::I(c),
        None => Num::F(a as f64 + b as f64),
    }
}

fn add_ff(a: f64, b: f64) -> Num {
    Num::F(a + b)
}

pub fn add(a: Num, b: Num) -> Num {
    match (a, b) {
        (Num::I(x), Num::I(y)) => add_ii(x, y),
        _ => add_ff(a.to_f64(), b.to_f64()),
    }
}

// The other arithmetic operators (plan/step12.txt, decisions 1-5 and 8). Generic
// operators promote to F on integer overflow; the explicit integer operators
// (idiv, imod, ipow) return None (NA) when there is no integer result: a float
// operand, division by zero, overflow (NUMERIC.design.txt sections 4, 5 and 9).

pub fn sub(a: Num, b: Num) -> Num {
    match (a, b) {
        (Num::I(x), Num::I(y)) => x.checked_sub(y).map_or(Num::F(x as f64 - y as f64), Num::I),
        _ => Num::F(a.to_f64() - b.to_f64()),
    }
}

pub fn mul(a: Num, b: Num) -> Num {
    match (a, b) {
        (Num::I(x), Num::I(y)) => x.checked_mul(y).map_or(Num::F(x as f64 * y as f64), Num::I),
        _ => Num::F(a.to_f64() * b.to_f64()),
    }
}

// Always a float, as Perl (6/3 is written 2); division by zero follows IEEE.
pub fn div(a: Num, b: Num) -> Num {
    Num::F(a.to_f64() / b.to_f64())
}

// Floored integer division, as pick's floor(x/y): idiv(-7, 2) is -4.
pub fn idiv(a: Num, b: Num) -> Option<Num> {
    let (Num::I(x), Num::I(y)) = (a, b) else { return None };
    let q = x.checked_div(y)?;                      // None for y = 0 and Int::MIN / -1
    Some(Num::I(if x % y != 0 && (x < 0) != (y < 0) { q - 1 } else { q }))
}

// Floored modulo, as Perl's %: the result has the sign of y (-7 mod 3 is 2,
// 7 mod -3 is -2), so that x = y * idiv(x, y) + imod(x, y).
pub fn imod(a: Num, b: Num) -> Option<Num> {
    let (Num::I(x), Num::I(y)) = (a, b) else { return None };
    if y == 0 {
        return None;
    }
    let r = x.checked_rem(y).unwrap_or(0);          // Int::MIN % -1 overflows; it is 0
    Some(Num::I(if r != 0 && (r < 0) != (y < 0) { r + y } else { r }))
}

// Always a float.
pub fn pow(a: Num, b: Num) -> Num {
    Num::F(a.to_f64().powf(b.to_f64()))
}

// Integer power; a negative exponent behaves as pow (pick); overflow is NA.
pub fn ipow(a: Num, b: Num) -> Option<Num> {
    let (Num::I(x), Num::I(e)) = (a, b) else { return None };
    if e < 0 {
        return Some(pow(a, b));
    }
    match u32::try_from(e) {
        Ok(e) => x.checked_pow(e).map(Num::I),
        Err(_) => match x {                         // a huge exponent: only 0, 1, -1 survive
            0 | 1 => Some(Num::I(x)),
            -1 => Some(Num::I(if e % 2 == 0 { 1 } else { -1 })),
            _ => None,
        },
    }
}

// max and min choose an operand: whether y replaces x, as pick: if y > x (max)
// or y < x (min). With a NaN the result is x (max(nan, 3) is nan, max(3, nan) is 3).
pub fn replaces(x: Num, y: Num, max: bool) -> bool {
    compare(y, x) == Some(if max { Ordering::Greater } else { Ordering::Less })
}

pub fn neg(a: Num) -> Num {
    match a {
        Num::I(x) => x.checked_neg().map_or(Num::F(-(x as f64)), Num::I),
        Num::F(x) => Num::F(-x),
    }
}

pub fn abs(a: Num) -> Num {
    match a {
        Num::I(x) => x.checked_abs().map_or(Num::F((x as f64).abs()), Num::I),
        Num::F(x) => Num::F(x.abs()),
    }
}

// -1, 0 or 1 as an integer; NaN stays NaN (pick gives 0).
pub fn sign(a: Num) -> Num {
    match a {
        Num::I(x) => Num::I(x.signum()),
        Num::F(x) if x.is_nan() => a,
        Num::F(x) => Num::I(if x > 0.0 { 1 } else if x < 0.0 { -1 } else { 0 }),
    }
}

pub fn sq(a: Num) -> Num {
    mul(a, a)
}

pub fn incr(a: Num) -> Num {
    add(a, Num::I(1))
}

pub fn decr(a: Num) -> Num {
    sub(a, Num::I(1))
}

// Rounding: the explicit float -> integer conversions (NUMERIC.design.txt section
// 5). An integer stays as it is; a float that is not finite or beyond the integer
// range stays a (whole) float (plan/step12.txt decision 5).
pub fn round_with(a: Num, f: fn(f64) -> f64) -> Num {
    match a {
        Num::I(_) => a,
        Num::F(x) => to_int(f(x)),
    }
}

fn to_int(x: f64) -> Num {
    // -(Int::MIN as f64) is 2^63 (2^127 with int128), exactly representable.
    if x.is_finite() && x >= Int::MIN as f64 && x < -(Int::MIN as f64) {
        Num::I(x as Int)
    } else {
        Num::F(x)
    }
}


// Writing numbers as text (COMPUTE.design.txt section 7). Integers are written as
// integers. Floats, by default, with the shortest digits that read back as the
// same f64, in plain notation for 1e-4 <= |x| < 1e16 and in exponent notation
// otherwise (exponent with sign and at least two digits, as C and Perl); with
// perl (--num-perl) as Perl writes them, %.15g with Inf and NaN.
pub fn write_num(out: &mut Vec<u8>, n: Num, perl: bool) {
    use std::io::Write;
    match n {
        Num::I(i) => { let _ = write!(out, "{}", i); }
        Num::F(x) if x.is_nan() => out.extend_from_slice(if perl { b"NaN" } else { b"nan" }),
        Num::F(x) if x.is_infinite() => {
            let text: &[u8] = match (x > 0.0, perl) {
                (true, false) => b"inf",
                (false, false) => b"-inf",
                (true, true) => b"Inf",
                (false, true) => b"-Inf",
            };
            out.extend_from_slice(text);
        }
        Num::F(x) if perl => write_g15(out, x),
        Num::F(x) => {
            let a = x.abs();
            if a == 0.0 || (1e-4..1e16).contains(&a) {
                let _ = write!(out, "{}", x);       // shortest round-trip, plain notation
            } else {
                let text = format!("{:e}", x);      // shortest round-trip, e.g. 1.5e-7
                write_exponent_form(out, &text);
            }
        }
    }
}

// As Perl prints a number. Perl's arithmetic preserves integers: when operands
// hold whole values in the 64-bit range, the result is an integer (signed, or
// unsigned up to 2^64) and is printed as one. This is approximated here: a whole
// float in [-2^63, 2^64) is written as an integer. Otherwise C's %.15g: 15
// significant digits; exponent notation if the exponent (after rounding) is below
// -4 or at least 15; trailing zeros removed.
fn write_g15(out: &mut Vec<u8>, x: f64) {
    if x.fract() == 0.0 && x >= -9.223372036854775808e18 && x < 1.8446744073709551616e19 {
        out.extend_from_slice((x as i128).to_string().as_bytes());
        return;
    }
    let e_form = format!("{:.14e}", x);             // d.dddddddddddddde<exp>
    let exp: i32 = e_form[e_form.find('e').unwrap() + 1..].parse().unwrap();
    if exp < -4 || exp >= 15 {
        let (mantissa, _) = e_form.split_once('e').unwrap();
        let mantissa = trim_zeros(mantissa);
        write_exponent_form(out, &format!("{}e{}", mantissa, exp));
    } else {
        let fixed = format!("{:.*}", (14 - exp) as usize, x);
        out.extend_from_slice(trim_zeros(&fixed).as_bytes());
    }
}

// Remove trailing zeros after a decimal point, and the point if nothing follows.
fn trim_zeros(s: &str) -> &str {
    if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.') } else { s }
}

// Rust writes 1.5e-7 and 1e21; C and Perl write 1.5e-07 and 1e+21.
fn write_exponent_form(out: &mut Vec<u8>, text: &str) {
    let (mantissa, exp) = text.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    out.extend_from_slice(mantissa.as_bytes());
    out.extend_from_slice(format!("e{}{:02}", if exp < 0 { '-' } else { '+' }, exp.abs()).as_bytes());
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

    fn text(n: Num, perl: bool) -> String {
        let mut out = Vec::new();
        write_num(&mut out, n, perl);
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn formatting() {
        for (x, plain, perl) in [
            (0.1 + 0.2, "0.30000000000000004", "0.3"), (3.0, "3", "3"), (0.5, "0.5", "0.5"),
            (1e15, "1000000000000000", "1000000000000000"), (1e16, "1e+16", "10000000000000000"),
            (1e21, "1e+21", "1e+21"), (9223372036854775808.0, "9.223372036854776e+18", "9223372036854775808"),
            (-2.996432245616634e18, "-2.996432245616634e+18", "-2996432245616633856"),
            (1.5e15, "1500000000000000", "1500000000000000"), (1.25e15 + 0.5, "1250000000000000.5", "1.25e+15"),
            (1.5e-7, "1.5e-07", "1.5e-07"), (0.0001, "0.0001", "0.0001"), (0.00001, "1e-05", "1e-05"),
            (-2.5, "-2.5", "-2.5"), (1.0 / 3.0, "0.3333333333333333", "0.333333333333333"),
            (123456789.123456789, "123456789.12345679", "123456789.123457"), (0.0, "0", "0"),
            (f64::INFINITY, "inf", "Inf"), (f64::NEG_INFINITY, "-inf", "-Inf"), (f64::NAN, "nan", "NaN"),
            (999999999999999.9, "999999999999999.9", "1e+15"),
        ] {
            assert_eq!(text(F(x), false), plain, "{}", x);
            assert_eq!(text(F(x), true), perl, "{}", x);
        }
        assert_eq!(text(I(-42), false), "-42");
        assert_eq!(text(I(Int::MAX), true), Int::MAX.to_string());
    }

    #[test]
    fn adding() {
        assert_eq!(add(I(2), I(3)), I(5));
        assert_eq!(add(I(2), F(0.5)), F(2.5));
        assert_eq!(add(I(Int::MAX), I(1)), F(Int::MAX as f64 + 1.0));
    }

    #[test]
    fn arithmetic() {
        assert_eq!(sub(I(2), I(5)), I(-3));
        assert_eq!(sub(I(Int::MIN), I(1)), F(Int::MIN as f64 - 1.0));
        assert_eq!(mul(I(3), F(0.5)), F(1.5));
        assert_eq!(mul(I(Int::MAX), I(2)), F(Int::MAX as f64 * 2.0));
        assert_eq!(div(I(6), I(3)), F(2.0));
        assert_eq!(div(I(1), I(0)), F(f64::INFINITY));
        assert!(matches!(div(I(0), I(0)), F(x) if x.is_nan()));
        for (x, y, q, r) in [(7, 2, 3, 1), (-7, 2, -4, 1), (7, -2, -4, -1), (-7, -2, 3, -1), (6, 3, 2, 0), (-6, 3, -2, 0)] {
            assert_eq!(idiv(I(x), I(y)), Some(I(q)), "{} {}", x, y);
            assert_eq!(imod(I(x), I(y)), Some(I(r)), "{} {}", x, y);
            assert_eq!(x, y * q + r);
        }
        assert_eq!((idiv(I(1), I(0)), imod(I(1), I(0))), (None, None));
        assert_eq!((idiv(F(7.0), I(2)), imod(I(7), F(2.0))), (None, None));
        assert_eq!(idiv(I(Int::MIN), I(-1)), None);
        assert_eq!(imod(I(Int::MIN), I(-1)), Some(I(0)));
        assert_eq!(pow(I(2), I(10)), F(1024.0));
        assert_eq!(ipow(I(2), I(10)), Some(I(1024)));
        assert_eq!(ipow(I(2), I(-1)), Some(F(0.5)));
        assert_eq!(ipow(I(2), I(200)), None);
        assert_eq!(ipow(I(-1), I(Int::MAX)), Some(I(-1)));
        assert_eq!(ipow(F(2.0), I(2)), None);
        assert!(replaces(I(2), F(2.5), true) && !replaces(I(2), F(2.5), false));
        assert!(!replaces(I(3), F(f64::NAN), true) && !replaces(F(f64::NAN), I(3), true));
        assert_eq!((neg(I(Int::MIN)), abs(I(Int::MIN))), (F(-(Int::MIN as f64)), F(-(Int::MIN as f64))));
        assert_eq!((sign(F(-0.5)), sign(I(0)), sign(F(2.0))), (I(-1), I(0), I(1)));
        assert_eq!((sq(I(-4)), incr(I(Int::MAX)), decr(F(0.5))), (I(16), F(Int::MAX as f64 + 1.0), F(-0.5)));
    }

    #[test]
    fn rounding() {
        assert_eq!(round_with(F(2.5), f64::round_ties_even), I(2));
        assert_eq!(round_with(F(3.5), f64::round_ties_even), I(4));
        assert_eq!(round_with(F(-2.5), f64::round_ties_even), I(-2));
        assert_eq!(round_with(F(-2.7), f64::trunc), I(-2));
        assert_eq!(round_with(F(-2.2), f64::floor), I(-3));
        assert_eq!(round_with(F(-2.7), f64::ceil), I(-2));
        assert_eq!(round_with(I(7), f64::floor), I(7));
        assert_eq!(round_with(F(1e300), f64::floor), F(1e300));
        assert!(matches!(round_with(F(f64::NAN), f64::floor), F(x) if x.is_nan()));
        assert_eq!(round_with(F(-(Int::MIN as f64)), f64::floor), F(-(Int::MIN as f64)));
        assert_eq!(round_with(F(Int::MIN as f64), f64::floor), I(Int::MIN));
    }

    #[test]
    fn bits() {
        assert!(bits_all(7, 5));
        assert!(!bits_all(6, 5));
        assert!(bits_all(-7, 5));
        assert!(bits_all(7, -5));
        assert!(bits_all(0, 0));
        assert!(bits_any(6, 5));
        assert!(!bits_any(2, 5));
        assert!(!bits_any(7, 0));
        assert!(bits_all(Int::MIN, Int::MIN));
        assert!(!bits_any(Int::MIN, Int::MAX));
    }
}
