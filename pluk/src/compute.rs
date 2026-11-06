// Computes: NAME::SPEC, NAME:=SPEC and ::SPEC (plan/COMPUTE.design.txt).
//
// This file has the parsing phase: an argument is split into NAME and SPEC, and
// SPEC into tokens, each introduced by a character:
//    ^VALUE    a constant, %-decoded
//    :COLSPEC  a column specification (a name, a '+' regex, a -k range)
//    ,OP       an operator, looked up in the table of ops.rs
// A SPEC not starting with one of ^ : , starts with a column (as pick).

use crate::colspec::{self, ColSpec, Element};
use crate::ops::{self, OpDef};
use crate::opt::pct_decode;

#[derive(Debug)]
pub enum Token {
    Const(Vec<u8>),
    Column(ColSpec),
    Op(&'static OpDef),
}

#[derive(Debug)]
pub struct ComputeSpec {
    pub text:   Vec<u8>,        // the argument, for messages
    pub name:   Vec<u8>,        // %-decoded; PLUKAAAAA, ... for an unnamed compute
    pub output: bool,           // '::' (true) or ':=' (false)
    pub tokens: Vec<Token>,
}

// Whether an argument is a compute.
pub fn is_compute(arg: &[u8]) -> bool {
    arg.windows(2).any(|w| w == b"::" || w == b":=")
}

// Names for unnamed computes: PLUKAAAAA, PLUKAAAAB, ..., PLUKAAAAZ, PLUKAAABA, ...
// (pick counts PICKAAAAA, ... with Perl's string increment).
pub struct UnnamedNames {
    letters: [u8; 5],
}

impl UnnamedNames {
    pub fn new() -> UnnamedNames {
        UnnamedNames { letters: *b"AAAAA" }
    }

    fn next(&mut self) -> Vec<u8> {
        let mut name = b"PLUK".to_vec();
        name.extend_from_slice(&self.letters);
        // Increment from the right, Z wrapping to A with a carry.
        for b in self.letters.iter_mut().rev() {
            if *b == b'Z' {
                *b = b'A';
            } else {
                *b += 1;
                break;
            }
        }
        name
    }
}

// headless (-k): ranges are allowed in column tokens.
pub fn parse(arg: &[u8], headless: bool, unnamed: &mut UnnamedNames) -> Result<ComputeSpec, String> {
    let shown = String::from_utf8_lossy(arg).into_owned();
    let k = arg.windows(2).position(|w| w == b"::" || w == b":=")
        .ok_or_else(|| format!("[{}]: not a compute", shown))?;
    let (raw_name, output, spec) = (&arg[..k], arg[k + 1] == b':', &arg[k + 2..]);

    let name = if raw_name.is_empty() {
        unnamed.next()
    } else {
        check_name(raw_name, headless).map_err(|e| format!("[{}]: {}", shown, e))?;
        pct_decode(raw_name)
    };
    if spec.is_empty() {
        return Err(format!("[{}]: compute is empty", shown));
    }

    let mut tokens = Vec::new();
    let starts_with_token = matches!(spec[0], b'^' | b':' | b',');
    let mut rest = spec;
    let mut kind = if starts_with_token { None } else { Some(b':') };
    loop {
        // The current token runs to the next ^ : , (or the end).
        let (kind_char, body_start) = match kind {
            Some(c) => (c, 0),
            None => (rest[0], 1),
        };
        let body_len = rest[body_start..].iter().position(|b| matches!(b, b'^' | b':' | b','))
            .unwrap_or(rest.len() - body_start);
        let body = &rest[body_start..body_start + body_len];
        tokens.push(token(kind_char, body, headless).map_err(|e| format!("[{}]: {}", shown, e))?);
        rest = &rest[body_start + body_len..];
        if rest.is_empty() {
            break;
        }
        kind = None;
    }
    Ok(ComputeSpec { text: arg.to_vec(), name, output, tokens })
}

fn token(kind: u8, body: &[u8], headless: bool) -> Result<Token, String> {
    match kind {
        b'^' => Ok(Token::Const(pct_decode(body))),
        b':' => {
            if body.is_empty() {
                return Err("column name missing after ':'".to_string());
            }
            Ok(Token::Column(colspec::parse(body, headless)?))
        }
        _ => {
            if body.is_empty() {
                return Err("operator missing after ','".to_string());
            }
            match ops::lookup(body) {
                Some(op) => Ok(Token::Op(op)),
                None => Err(format!("unknown operator [{}]", String::from_utf8_lossy(body))),
            }
        }
    }
}

// NAME is a single plain name. A list, '+' regex or range (map) and '/' (pick's
// prefix/suffix names) are deferred (COMPUTE.design.txt section 8).
fn check_name(raw: &[u8], headless: bool) -> Result<(), String> {
    if raw.contains(&b'/') {
        return Err("prefix/suffix names ('/' in NAME) not yet supported".to_string());
    }
    match colspec::parse(raw, headless)?.elements.as_slice() {
        [Element::Name(_)] => Ok(()),
        _ => Err("a list, regex or range as compute NAME (map) not yet supported".to_string()),
    }
}

// The parsed form, for --test-compute.
pub fn describe(spec: &ComputeSpec) -> String {
    let mut out = format!("compute {} ({})\n", String::from_utf8_lossy(&spec.name),
                          if spec.output { "output" } else { "no output" });
    for token in &spec.tokens {
        let line = match token {
            Token::Const(c) => format!("  const [{}]\n", String::from_utf8_lossy(c)),
            Token::Column(c) => format!("  column [{}]\n", String::from_utf8_lossy(&c.text)),
            Token::Op(op) => format!("  op {}\n", op.name),
        };
        out.push_str(&line);
    }
    out
}


#[cfg(test)]
mod tests {
    use super::*;

    fn parse1(arg: &str) -> Result<ComputeSpec, String> {
        parse(arg.as_bytes(), false, &mut UnnamedNames::new())
    }

    fn kinds(spec: &ComputeSpec) -> String {
        spec.tokens.iter().map(|t| match t {
            Token::Const(c) => format!("^{}", String::from_utf8_lossy(c)),
            Token::Column(c) => format!(":{}", String::from_utf8_lossy(&c.text)),
            Token::Op(op) => format!(",{}", op.name),
        }).collect()
    }

    #[test]
    fn syntax() {
        let s = parse1("x::fib^1,add").unwrap();
        assert_eq!((s.name.as_slice(), s.output, kinds(&s).as_str()), (&b"x"[..], true, ":fib^1,add"));
        let s = parse1("y:=^a:fib,cat").unwrap();
        assert_eq!((s.output, kinds(&s).as_str()), (false, "^a:fib,cat"));
        assert_eq!(kinds(&parse1("z::,dup").unwrap()), ",dup");
        assert_eq!(kinds(&parse1("z::^").unwrap()), "^");
        let s = parse1("a%2Fb::^x%2Cy%3Az%5E").unwrap();
        assert_eq!((s.name.as_slice(), kinds(&s).as_str()), (&b"a/b"[..], "^x,y:z^"));
        assert_eq!(kinds(&parse1("s::+num\\d,addall").unwrap()), ":+num\\d,addall");
        // The first '::' or ':=' ends NAME; later ones are ordinary ':' tokens.
        assert_eq!(kinds(&parse1("x::a:=b").unwrap()), ":a:=b");
    }

    #[test]
    fn unnamed() {
        let mut names = UnnamedNames::new();
        let a = parse(b"::^1", false, &mut names).unwrap();
        let b = parse(b"::^2", false, &mut names).unwrap();
        assert_eq!((a.name.as_slice(), b.name.as_slice()), (&b"PLUKAAAAA"[..], &b"PLUKAAAAB"[..]));
        let mut names = UnnamedNames { letters: *b"AAAAZ" };
        names.next();
        assert_eq!(names.next(), b"PLUKAAABA");
    }

    #[test]
    fn errors() {
        for bad in ["x::", "x::,", "x::^a:", "x::^a,nosuch", "+x.*::^1", "a:b::^1", "p/x/s::^1", "x::+(a"] {
            assert!(parse1(bad).is_err(), "{}", bad);
        }
        assert!(parse(b"2-3::^1", true, &mut UnnamedNames::new()).is_err());
        assert!(parse(b"2-3::^1", false, &mut UnnamedNames::new()).is_ok());
    }
}
