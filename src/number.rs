use std::{fmt, str::FromStr};

use icu_properties::{CodePointMapData, props::GeneralCategory};
use num_bigint::BigInt;
use num_traits::{Num, ToPrimitive};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Number(f64);

impl Number {
    pub fn from_str(s: &str) -> Self {
        // https://tc39.es/ecma262/2024/multipage/abstract-operations.html#sec-stringtonumber

        // Implementing StringToNumber exactly as written in the spec involves
        // writing a parser, along with the conversion of the parsed AST into the
        // actual value.
        //
        // We've already implemented a number parser in the scanner, but we can't
        // import it here. We also do not have the conversion implemented since we
        // previously just wrote `+literal` and let the runtime handle it.
        //
        // The strategy below is to instead break the number apart and fix it up
        // such that Rust's own parsing functionality can handle it. This won't be
        // the fastest method, but it saves us from writing the full parser and
        // conversion logic.

        let mut s = s.trim_matches(is_str_whitespace);

        match s {
            "" => return Self(0.0),
            "Infinity" | "+Infinity" => return Self(f64::INFINITY),
            "-Infinity" => return Self(f64::NEG_INFINITY),
            _ => {}
        }

        for c in s.chars() {
            if !is_number_char(c) {
                return Number(f64::NAN);
            }
        }

        if let Ok(n) = try_parse_int(s) {
            return Number(n);
        }

        // Cut this off first so we can ensure -0 is returned as -0.
        let negative = if let Some(rest) = s.strip_prefix('-') {
            s = rest;
            true
        } else {
            if let Some(rest) = s.strip_prefix('+') {
                s = rest;
            }
            false
        };

        if s.chars().next().is_none_or(|c| !c.is_ascii_digit() && c != '.') {
            return Number(f64::NAN);
        }

        let f = parse_float_string(s);
        if f.is_nan() {
            return Number(f64::NAN);
        }

        let sign = if negative { -1.0 } else { 1.0 };
        Number(f.copysign(sign))
    }
}

impl fmt::Display for Number {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // https://tc39.es/ecma262/2024/multipage/ecmascript-data-types-and-values.html#sec-numeric-types-number-tostring
        let n = self.0;
        if n.is_nan() {
            return write!(f, "NaN");
        }

        if n.is_infinite() {
            return if n < 0.0 { write!(f, "-Infinity") } else { write!(f, "Infinity") };
        }

        const MAX_EXACT_INTEGER: i64 = (1 << 53) - 1;
        const MIN_EXACT_INTEGER: i64 = -MAX_EXACT_INTEGER;

        // Fast path: for safe integers, directly convert to string.
        if MIN_EXACT_INTEGER as f64 <= n && n <= MAX_EXACT_INTEGER as f64 && n.fract() == 0.0 {
            return write!(f, "{}", n as i64);
        }

        let mut buf = ryu_js::Buffer::new();
        write!(f, "{}", buf.format(n))
    }
}

pub fn parse_pseudo_big_int(mut s: &str) -> String {
    s = s.trim_end_matches('n');
    let (rest, base) = match s.split_at_checked(2) {
        Some(("0b" | "0B", rest)) => (rest, 2),
        Some(("0o" | "0O", rest)) => (rest, 8),
        Some(("0x" | "0X", rest)) => (rest, 16),
        _ => {
            s = s.trim_start_matches('0');
            if s.is_empty() {
                return String::from('0');
            }
            return s.to_string();
        }
    };
    let bigint = BigInt::from_str_radix(rest, base).unwrap();
    bigint.to_string()
}

fn try_parse_int(mut s: &str) -> Result<f64, ()> {
    let mut i = None;
    let mut has_int_result = false;
    if s.len() > 2 {
        let (prefix, rest) = s.split_at(2);
        match prefix {
            "0b" | "0B" => {
                if !is_all_binary_digits(rest) {
                    return Ok(f64::NAN);
                }
                i = i64::from_str_radix(rest, 2).ok();
                has_int_result = true;
            }
            "0o" | "0O" => {
                if !is_all_octal_digits(rest) {
                    return Ok(f64::NAN);
                }
                i = i64::from_str_radix(rest, 8).ok();
                has_int_result = true;
            }
            "0x" | "0X" => {
                if !is_all_hex_digits(rest) {
                    return Ok(f64::NAN);
                }
                i = i64::from_str_radix(rest, 16).ok();
                has_int_result = true;
            }
            _ => {}
        }
    }
    if !has_int_result {
        // StringToNumber does not parse leading zeros as octal.
        s = trim_leading_zeros(s);
        if !is_all_digits(s) {
            return Err(());
        }
        i = i64::from_str_radix(s, 10).ok();
        has_int_result = true;
    }
    if has_int_result && let Some(i) = i {
        return Ok(i as f64);
    }

    // Using this to parse large integers.
    let Ok(bigint) = BigInt::from_str(s) else {
        return Ok(f64::NAN);
    };

    let f = bigint.to_f64().unwrap_or_default();
    Ok(f)
}

fn parse_float_string(s: &str) -> f64 {
    let has_dot;
    let has_exp;

    // <a>
    // <a>.<b>
    // <a>.<b>e<c>
    // <a>e<c>
    let mut a;
    let mut b = "";
    let mut c;
    let rest;

    (a, rest, has_dot) = cut(s, ['.']);

    if has_dot {
        // <a>.<b>
        // <a>.<b>e<c>
        (b, c, has_exp) = cut(rest, ['e', 'E']);
    } else {
        // <a>
        // <a>e<c>
        (a, c, has_exp) = cut(s, ['e', 'E']);
    }

    let mut out = String::with_capacity(a.len() + b.len() + c.len() + 3);
    if a.is_empty() {
        if has_dot && b.is_empty() {
            return f64::NAN;
        }
        if has_exp && c.is_empty() {
            return f64::NAN;
        }
        out.push('0');
    } else {
        a = trim_leading_zeros(a);
        if !is_all_digits(a) {
            return f64::NAN;
        }
        out.push_str(a);
    }

    if has_dot {
        out.push('.');
        if b.is_empty() {
            out.push('0');
        } else {
            b = trim_trailing_zeros(b);
            if !is_all_digits(b) {
                return f64::NAN;
            }
            out.push_str(b);
        }
    }

    if has_exp {
        out.push('e');

        if let Some(rest) = c.strip_prefix('-') {
            c = rest;
            out.push('-');
        } else {
            if let Some(rest) = c.strip_prefix('+') {
                c = rest;
            }
        }
        c = trim_leading_zeros(c);
        if !is_all_digits(c) {
            return f64::NAN;
        }
        out.push_str(c);
    }

    f64::from_str(&out).unwrap_or(f64::NAN)
}

fn cut<const N: usize>(s: &str, p: [char; N]) -> (&str, &str, bool) {
    match s.split_once(p) {
        Some((a, b)) => (a, b, true),
        None => (s, "", false),
    }
}

fn is_str_whitespace(c: char) -> bool {
    // This is different than stringutil.IsWhiteSpaceLike.

    // https://tc39.es/ecma262/2024/multipage/ecmascript-language-lexical-grammar.html#prod-LineTerminator
    // https://tc39.es/ecma262/2024/multipage/ecmascript-language-lexical-grammar.html#prod-WhiteSpace

    match c {
        // LineTerminator
        '\n' | '\r' | '\u{2028}' | '\u{2029}' => true,
        // WhiteSpace
        '\t' | '\u{0B}' | '\u{0C}' | '\u{FEFF}' => true,
        _ => CodePointMapData::<GeneralCategory>::new().get(c) == GeneralCategory::SpaceSeparator,
    }
}

fn is_number_char(c: char) -> bool {
    matches!(c, '.' | '-' | '+' | 'x' | 'X' | 'o' | 'O' | '0'..='9' | 'a'..='f' | 'A'..='F')
}

fn is_all_digits(s: &str) -> bool {
    s.chars().all(|c| c.is_ascii_digit())
}

fn is_all_binary_digits(s: &str) -> bool {
    s.chars().all(|c| matches!(c, '0' | '1'))
}

fn is_all_octal_digits(s: &str) -> bool {
    s.chars().all(|c| matches!(c, '0'..='7'))
}

fn is_all_hex_digits(s: &str) -> bool {
    s.chars().all(|c| c.is_ascii_hexdigit())
}

fn trim_leading_zeros(mut s: &str) -> &str {
    if s.starts_with('0') {
        s = s.trim_start_matches('0');
        if s.is_empty() {
            return "0";
        }
    }
    s
}

fn trim_trailing_zeros(mut s: &str) -> &str {
    if s.ends_with('0') {
        s = s.trim_end_matches('0');
        if s.is_empty() {
            return "0";
        }
    }
    s
}
