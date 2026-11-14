//! JavaScript Numbers held in a `serde_json::Value`, printed as `String(n)` and
//! `JSON.stringify(n)` print them.

use serde::Serialize;
use serde_json::{
    ser::{CompactFormatter, Formatter, PrettyFormatter},
    Number, Value,
};
use std::io;

const MAX_SAFE: f64 = 9_007_199_254_740_992.0;

/// A `Value` cannot hold a non-finite number, so ±Infinity is stored as ±the smallest subnormal.
/// Model files only hold integers, so they never produce that value themselves.
const INFINITY_MARK: f64 = 5e-324;

/// The Number JavaScript holds, as a `Value`: safe integers as integers, Infinity as its mark.
pub(crate) fn number(n: f64) -> Value {
    if n.is_infinite() {
        return Value::from(INFINITY_MARK.copysign(n));
    }
    if n.fract() == 0.0 && n.abs() < MAX_SAFE {
        return Value::from(n as i64);
    }
    Number::from_f64(n).map_or(Value::Null, Value::Number)
}

fn decode(n: f64) -> f64 {
    if n.abs() == INFINITY_MARK {
        f64::INFINITY.copysign(n)
    } else {
        n
    }
}

/// `Number.prototype.toString()` for a decoded Number.
fn format(n: f64) -> String {
    if n.is_nan() {
        return "NaN".into();
    }
    if n == 0.0 {
        return "0".into();
    }
    if n < 0.0 {
        return format!("-{}", format(-n));
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

/// `String(n)`.
pub(crate) fn to_string(n: &Number) -> String {
    format(decode(n.as_f64().expect("a JSON number is an f64")))
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
        let n = decode(value);
        let text = if n.is_finite() {
            format(n)
        } else {
            "null".into()
        };
        writer.write_all(text.as_bytes())
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
            let Value::Number(n) = &value else {
                panic!("{literal} is not a number")
            };
            assert_eq!(to_string(n), expected, "String({literal})");
            let json = if expected.ends_with("Infinity") {
                "null"
            } else {
                expected
            };
            assert_eq!(stringify(&value, false), json, "JSON.stringify({literal})");
        }
        assert_eq!(format(0.5), "0.5");
        assert_eq!(format(1e-7), "1e-7");
        assert_eq!(format(1.5e-6), "0.0000015");
        assert_eq!(
            stringify(&serde_json::json!({"a": [1, 2.5]}), true),
            "{\n  \"a\": [\n    1,\n    2.5\n  ]\n}"
        );
    }
}
