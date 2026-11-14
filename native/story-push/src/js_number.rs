//! JavaScript Numbers held in a `serde_json::Value`, as `JSON.parse` builds them and `String(n)` and
//! `JSON.stringify(n)` print them.

use serde::Serialize;
use serde_json::{
    ser::{CompactFormatter, Formatter, PrettyFormatter},
    Number, Value,
};
use std::io;

const MAX_SAFE: f64 = 9_007_199_254_740_992.0;

/// A `Value` cannot hold a non-finite number, so ±Infinity is stored as an integer that no JavaScript
/// Number becomes: `number` stores every other integer beyond ±2^53 as an f64.
const INFINITY: u64 = u64::MAX;
const NEGATIVE_INFINITY: i64 = i64::MIN;

/// The Number JavaScript holds, as a `Value`: safe integers as integers, ±Infinity as its mark.
pub(crate) fn number(n: f64) -> Value {
    if n.is_infinite() {
        return if n > 0.0 {
            Value::from(INFINITY)
        } else {
            Value::from(NEGATIVE_INFINITY)
        };
    }
    if n.fract() == 0.0 && n.abs() < MAX_SAFE {
        return Value::from(n as i64);
    }
    Number::from_f64(n).map_or(Value::Null, Value::Number)
}

/// The Number a `Value` number holds.
pub(crate) fn value(n: &Number) -> f64 {
    if n.as_u64() == Some(INFINITY) {
        f64::INFINITY
    } else if n.as_i64() == Some(NEGATIVE_INFINITY) {
        f64::NEG_INFINITY
    } else {
        n.as_f64().expect("a JSON number is an f64")
    }
}

/// `JSON.parse(text)`'s Numbers: every literal is read as the nearest f64, so integers beyond 2^53
/// round and literals beyond the f64 range become ±Infinity, where serde_json keeps the exact integer
/// or rejects the text.
pub(crate) fn parse(text: &str) -> serde_json::Result<Value> {
    let mut numbers = Vec::new();
    let mut rewritten = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(|c: char| c == '"' || c == '-' || c.is_ascii_digit()) {
        rewritten.push_str(&rest[..start]);
        rest = &rest[start..];
        let end = if rest.starts_with('"') {
            string_end(rest)
        } else {
            rest.find(|c: char| !matches!(c, '0'..='9' | '-' | '+' | '.' | 'e' | 'E'))
                .unwrap_or(rest.len())
        };
        let token = &rest[..end];
        if is_number(token) {
            let index = numbers.len().to_string();
            numbers.push(
                token
                    .parse::<f64>()
                    .expect("a JSON number is an f64 literal"),
            );
            rewritten.push_str(&index);
            rewritten.push_str(&" ".repeat(token.len().saturating_sub(index.len())));
        } else {
            rewritten.push_str(token);
        }
        rest = &rest[end..];
    }
    rewritten.push_str(rest);
    let mut value = serde_json::from_str(&rewritten)?;
    restore(&mut value, &numbers);
    Ok(value)
}

/// The length of the string literal `text` starts with, or all of `text` when it is unterminated.
fn string_end(text: &str) -> usize {
    let mut escaped = false;
    for (i, c) in text.char_indices().skip(1) {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '"' => return i + 1,
            _ => (),
        }
    }
    text.len()
}

/// `-?(0|[1-9]\d*)(\.\d+)?([eE][+-]?\d+)?`.
fn is_number(token: &str) -> bool {
    fn digits(s: &str) -> (&str, &str) {
        let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
        s.split_at(end)
    }
    let unsigned = token.strip_prefix('-').unwrap_or(token);
    let (whole, rest) = digits(unsigned);
    if whole.is_empty() || (whole.len() > 1 && whole.starts_with('0')) {
        return false;
    }
    let rest = match rest.strip_prefix('.') {
        Some(fraction) => match digits(fraction) {
            ("", _) => return false,
            (_, rest) => rest,
        },
        None => rest,
    };
    match rest.strip_prefix(['e', 'E']) {
        Some(exponent) => {
            let exponent = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
            matches!(digits(exponent), (d, "") if !d.is_empty())
        }
        None => rest.is_empty(),
    }
}

fn restore(value: &mut Value, numbers: &[f64]) {
    match value {
        Value::Number(n) => {
            let index = n.as_u64().expect("every number was rewritten to its index") as usize;
            *value = number(numbers[index]);
        }
        Value::Array(items) => items.iter_mut().for_each(|v| restore(v, numbers)),
        Value::Object(entries) => entries.values_mut().for_each(|v| restore(v, numbers)),
        _ => (),
    }
}

/// `String(n)`.
pub(crate) fn to_string(n: f64) -> String {
    if n.is_nan() {
        return "NaN".into();
    }
    if n == 0.0 {
        return "0".into();
    }
    if n < 0.0 {
        return format!("-{}", to_string(-n));
    }
    if n.is_infinite() {
        return "Infinity".into();
    }
    let scientific = format!("{n:e}");
    let (mantissa, exponent) = scientific.split_once('e').expect("{:e} has an exponent");
    let digits = mantissa.replace('.', "");
    let k = digits.len() as i32;
    let point: i32 = exponent.parse::<i32>().expect("an integer exponent") + 1;
    if k <= point && point <= 21 {
        format!("{digits}{}", "0".repeat((point - k) as usize))
    } else if 0 < point && point <= 21 {
        let (whole, fraction) = digits.split_at(point as usize);
        format!("{whole}.{fraction}")
    } else if -6 < point && point <= 0 {
        format!("0.{}{digits}", "0".repeat(-point as usize))
    } else {
        let e = point - 1;
        let sign = if e < 0 { '-' } else { '+' };
        let (first, rest) = digits.split_at(1);
        let rest = if rest.is_empty() {
            String::new()
        } else {
            format!(".{rest}")
        };
        format!("{first}{rest}e{sign}{}", e.abs())
    }
}

/// `JSON.stringify(value)`, or `JSON.stringify(value, null, 2)` when `pretty`.
pub(crate) fn stringify(value: &impl Serialize, pretty: bool) -> String {
    let mut out = Vec::new();
    let result = if pretty {
        value.serialize(&mut serde_json::Serializer::with_formatter(
            &mut out,
            JsFormatter(PrettyFormatter::new()),
        ))
    } else {
        value.serialize(&mut serde_json::Serializer::with_formatter(
            &mut out,
            JsFormatter(CompactFormatter),
        ))
    };
    result.expect("a JSON value serializes");
    String::from_utf8(out).expect("JSON is UTF-8")
}

struct JsFormatter<F>(F);

impl<F: Formatter> Formatter for JsFormatter<F> {
    fn write_f64<W: ?Sized + io::Write>(&mut self, writer: &mut W, value: f64) -> io::Result<()> {
        writer.write_all(to_string(value).as_bytes())
    }
    fn write_u64<W: ?Sized + io::Write>(&mut self, writer: &mut W, value: u64) -> io::Result<()> {
        if value == INFINITY {
            writer.write_all(b"null")
        } else {
            self.0.write_u64(writer, value)
        }
    }
    fn write_i64<W: ?Sized + io::Write>(&mut self, writer: &mut W, value: i64) -> io::Result<()> {
        if value == NEGATIVE_INFINITY {
            writer.write_all(b"null")
        } else {
            self.0.write_i64(writer, value)
        }
    }
    fn begin_array<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.0.begin_array(w)
    }
    fn end_array<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.0.end_array(w)
    }
    fn begin_array_value<W: ?Sized + io::Write>(
        &mut self,
        w: &mut W,
        first: bool,
    ) -> io::Result<()> {
        self.0.begin_array_value(w, first)
    }
    fn end_array_value<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.0.end_array_value(w)
    }
    fn begin_object<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.0.begin_object(w)
    }
    fn end_object<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.0.end_object(w)
    }
    fn begin_object_key<W: ?Sized + io::Write>(
        &mut self,
        w: &mut W,
        first: bool,
    ) -> io::Result<()> {
        self.0.begin_object_key(w, first)
    }
    fn begin_object_value<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.0.begin_object_value(w)
    }
    fn end_object_value<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.0.end_object_value(w)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_print_as_javascript_prints_them() {
        let cases = [
            ("0", "0"),
            ("-0", "0"),
            ("9007199254740991", "9007199254740991"),
            ("9007199254740992", "9007199254740992"),
            ("9007199254740993", "9007199254740992"),
            ("100000000000000000000", "100000000000000000000"),
            ("1000000000000000000000", "1e+21"),
            ("123456789012345678901234567", "1.2345678901234568e+26"),
            ("-123456789012345678901234567", "-1.2345678901234568e+26"),
            (&"9".repeat(400), "Infinity"),
            (&format!("-{}", "9".repeat(400)), "-Infinity"),
        ];
        for (literal, expected) in cases {
            let value = number(literal.parse().unwrap());
            assert!(value.is_number(), "{literal} is not a number");
            assert_eq!(
                crate::config::js_string(&value),
                expected,
                "String({literal})"
            );
            let json = if expected.ends_with("Infinity") {
                "null"
            } else {
                expected
            };
            assert_eq!(stringify(&value, false), json, "JSON.stringify({literal})");
        }
        assert_eq!(to_string(0.5), "0.5");
        assert_eq!(to_string(1e-7), "1e-7");
        assert_eq!(to_string(1.5e-6), "0.0000015");
        assert_eq!(to_string(5e-324), "5e-324");
        assert_eq!(to_string(-5e-324), "-5e-324");
        assert_eq!(
            stringify(&serde_json::json!({"a": [1, 2.5]}), true),
            "{\n  \"a\": [\n    1,\n    2.5\n  ]\n}"
        );
    }

    #[test]
    fn parse_reads_numbers_as_json_parse_does() {
        let value = parse(
            r#"{"a\"1": "-2 \\\"3", "n": [5e-324, 18446744073709551615, -9223372036854775808, 1e400, -1E+400, 9007199254740993, -0, 0.5]}"#,
        )
        .unwrap();
        assert_eq!(value["a\"1"], "-2 \\\"3");
        let printed: Vec<_> = value["n"]
            .as_array()
            .unwrap()
            .iter()
            .map(crate::config::js_string)
            .collect();
        assert_eq!(
            printed,
            [
                "5e-324",
                "18446744073709552000",
                "-9223372036854776000",
                "Infinity",
                "-Infinity",
                "9007199254740992",
                "0",
                "0.5"
            ]
        );
        assert_eq!(
            stringify(&value["n"], false),
            "[5e-324,18446744073709552000,-9223372036854776000,null,null,9007199254740992,0,0.5]"
        );
        for text in [
            "01", "1.", ".5", "-", "1e", "1e+", "+1", "1.5.5", "--1", "[1,]",
        ] {
            assert!(parse(text).is_err(), "{text} is not JSON");
        }
        assert_eq!(
            parse("[1, 2,\n 3 x]").unwrap_err().to_string(),
            serde_json::from_str::<Value>("[1, 2,\n 3 x]")
                .unwrap_err()
                .to_string()
        );
    }
}
