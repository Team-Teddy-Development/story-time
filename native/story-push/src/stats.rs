//! `story stats`: Node's `backlogStats` summary of the backlog.

use crate::{
    config::{self, Flags},
    js_space,
    lenient_story::{ordered, Js},
    list, Failure,
};
use chrono::{Local, NaiveDate, Offset, TimeZone, Utc};

const DAY: f64 = 86_400_000.0;

/// `story stats [--stale-days N] [--dir D] [--config C] [--json]`.
pub fn run(args: &[String]) -> Result<String, Failure> {
    let flags = Flags::parse(args)?;
    let loaded = config::load(flags.value("config"))?;
    let settings =
        config::parse(&loaded.raw).map_err(|e| format!("{}: {e}", loaded.path.display()))?;
    let dir = list::backlog_dir(&flags, &loaded.root, &settings)?;
    let stale_days = match flags.value("stale-days") {
        Some(days) if !days.is_empty() => number(days),
        _ => 30.0,
    };
    let stats = stats(&list::stories(&dir, None)?, stale_days);
    if flags.has("json") {
        return Ok(format!("{}\n", stats.json()));
    }
    let ids = |key| match join(stats.get(key)) {
        ids if ids.is_empty() => "none".into(),
        ids => ids,
    };
    let by_status = ordered(stats.get("byStatus").props())
        .into_iter()
        .map(|(status, count)| format!("{status} {}", count.string()))
        .collect::<Vec<_>>()
        .join(", ");
    Ok(format!(
        "Stories: {} total, {} open\nBy status: {}\nPoints: {} ready / {} open\nUnestimated: {}\nStale: {}\n",
        stats.get("total").string(),
        stats.get("open").string(),
        if by_status.is_empty() { "-" } else { &by_status },
        stats.get("readyPoints").string(),
        stats.get("openPoints").string(),
        ids("unestimated"),
        ids("stale"),
    ))
}

fn stats(stories: &[Js], stale_days: f64) -> Js {
    let open: Vec<&Js> = stories
        .iter()
        .filter(|s| !matches!(s.get("status"), Js::Str(st) if ["done", "split", "archived"].contains(&st.as_str())))
        .collect();
    let mut by_status: Vec<(String, Js)> = Vec::new();
    for s in stories {
        let status = s.get("status").string();
        // Node assigns `byStatus.__proto__` to the prototype slot, so it never becomes a key.
        if status == "__proto__" {
            continue;
        }
        match by_status.iter_mut().find(|(k, _)| *k == status) {
            Some((_, Js::Num(n))) => *n += 1.0,
            _ => by_status.push((status, Js::Num(1.0))),
        }
    }
    let points = |xs: &mut dyn Iterator<Item = &&Js>| {
        xs.fold(0.0, |sum, s| match s.get("size") {
            Js::Num(n) => sum + n,
            _ => sum,
        })
    };
    let now = Utc::now().date_naive();
    let now = days_from_epoch(now) * DAY;
    let ids = |keep: &dyn Fn(&Js) -> bool| {
        Js::Arr(
            open.iter()
                .filter(|s| keep(s))
                .map(|s| s.get("id").clone())
                .collect(),
        )
    };
    Js::Obj(vec![
        ("total".into(), Js::Num(stories.len() as f64)),
        ("open".into(), Js::Num(open.len() as f64)),
        ("byStatus".into(), Js::Obj(by_status)),
        (
            "readyPoints".into(),
            Js::Num(points(
                &mut open
                    .iter()
                    .filter(|s| *s.get("status") == Js::Str("ready".into())),
            )),
        ),
        ("openPoints".into(), Js::Num(points(&mut open.iter()))),
        (
            "unestimated".into(),
            ids(&|s| {
                *s.get("type") != Js::Str("spike".into()) && !matches!(s.get("size"), Js::Num(_))
            }),
        ),
        (
            "stale".into(),
            ids(&|s| {
                let updated = s.get("updated");
                truthy(updated) && (now - date_parse(&updated.string())) / DAY > stale_days
            }),
        ),
    ])
}

/// `array.join(', ')`.
fn join(items: &Js) -> String {
    let Js::Arr(items) = items else {
        return String::new();
    };
    items
        .iter()
        .map(|v| match v {
            Js::Undefined => String::new(),
            v => v.string(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn truthy(v: &Js) -> bool {
    match v {
        Js::Undefined => false,
        Js::Bool(b) => *b,
        Js::Num(n) => *n != 0.0 && !n.is_nan(),
        Js::Str(s) => !s.is_empty(),
        Js::Arr(_) | Js::Obj(_) => true,
    }
}

/// `Number(string)`.
fn number(s: &str) -> f64 {
    let s = s.trim_matches(|c| js_space(c) || crate::lenient_story::line_terminator(c));
    if s.is_empty() {
        return 0.0;
    }
    for (prefix, radix) in [
        ("0x", 16),
        ("0X", 16),
        ("0o", 8),
        ("0O", 8),
        ("0b", 2),
        ("0B", 2),
    ] {
        if let Some(digits) = s.strip_prefix(prefix) {
            if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
                return f64::NAN;
            }
            return digits.chars().fold(0.0, |n, c| {
                n * radix as f64 + c.to_digit(radix).unwrap() as f64
            });
        }
    }
    let unsigned = s.strip_prefix(['+', '-']).unwrap_or(s);
    if unsigned == "Infinity" {
        return if s.starts_with('-') {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    let (mantissa, exponent) = match unsigned.find(['e', 'E']) {
        Some(i) => (&unsigned[..i], Some(&unsigned[i + 1..])),
        None => (unsigned, None),
    };
    let digits = |s: &str| s.bytes().all(|c| c.is_ascii_digit());
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let valid = digits(whole)
        && digits(fraction)
        && !(whole.is_empty() && fraction.is_empty())
        && exponent.is_none_or(|e| {
            let e = e.strip_prefix(['+', '-']).unwrap_or(e);
            !e.is_empty() && digits(e)
        });
    if valid {
        s.parse().unwrap_or(f64::NAN)
    } else {
        f64::NAN
    }
}

fn days_from_epoch(date: NaiveDate) -> f64 {
    (date - NaiveDate::from_ymd_opt(1970, 1, 1).unwrap()).num_days() as f64
}

/// `Date.parse(string)` for the ECMAScript date time string format, which V8 tries first; strings
/// only V8's legacy fallback parser accepts give NaN here.
fn date_parse(s: &str) -> f64 {
    iso_date(s).unwrap_or(f64::NAN)
}

struct Cursor<'a>(&'a str);
impl Cursor<'_> {
    fn digits(&mut self, n: usize) -> Option<i64> {
        let (head, tail) = self.0.split_at_checked(n)?;
        if !head.bytes().all(|c| c.is_ascii_digit()) {
            return None;
        }
        self.0 = tail;
        head.parse().ok()
    }
    fn eat(&mut self, c: &[char]) -> bool {
        match self.0.strip_prefix(c) {
            Some(tail) => {
                self.0 = tail;
                true
            }
            None => false,
        }
    }
    /// `-NN` within `range`, or `Some(None)` when no `-` follows.
    fn part(&mut self, range: std::ops::RangeInclusive<i64>) -> Option<Option<i64>> {
        if !self.eat(&['-']) {
            return Some(None);
        }
        let v = self.digits(2)?;
        range.contains(&v).then_some(Some(v))
    }
}

fn iso_date(s: &str) -> Option<f64> {
    let mut c = Cursor(s);
    let year = if c.eat(&['+']) {
        c.digits(6)?
    } else if c.eat(&['-']) {
        match c.digits(6)? {
            0 => return None,
            year => -year,
        }
    } else {
        c.digits(4)?
    };
    let month = c.part(1..=12)?;
    let day = if month.is_some() {
        c.part(1..=31)?
    } else {
        None
    };
    let mut ms = (days_from_civil(year, month.unwrap_or(1)) + day.unwrap_or(1) - 1) as f64 * DAY;
    let mut local = false;
    if c.eat(&['T', 't']) {
        let hour = c.digits(2)?;
        if !c.eat(&[':']) {
            return None;
        }
        let minute = c.digits(2)?;
        let (mut second, mut milli) = (0, 0);
        if c.eat(&[':']) {
            second = c.digits(2)?;
            if c.eat(&['.']) {
                let digits = c.0.bytes().take_while(u8::is_ascii_digit).count();
                if digits == 0 {
                    return None;
                }
                milli = format!("{:0<3}", &c.0[..digits.min(3)]).parse().ok()?;
                c.0 = &c.0[digits..];
            }
        }
        let midnight = hour == 24 && minute == 0 && second == 0 && milli == 0;
        if !(hour < 24 || midnight) || minute > 59 || second > 59 {
            return None;
        }
        ms += ((hour * 60 + minute) * 60 + second) as f64 * 1000.0 + milli as f64;
        local = true;
    }
    if c.eat(&['Z', 'z']) {
        local = false;
    } else if local && c.0.starts_with(['+', '-']) {
        let sign = if c.eat(&['-']) {
            -1.0
        } else {
            c.eat(&['+']);
            1.0
        };
        let hours = c.digits(2)?;
        if !c.eat(&[':']) {
            return None;
        }
        let minutes = c.digits(2)?;
        if hours > 23 || minutes > 59 {
            return None;
        }
        ms -= sign * ((hours * 60 + minutes) * 60_000) as f64;
        local = false;
    }
    if !c.0.is_empty() {
        return None;
    }
    if local {
        ms -= local_offset_ms(ms)?;
    }
    (ms.abs() <= 8.64e15).then_some(ms)
}

/// The local zone's offset for a wall-clock time given as milliseconds since the epoch.
fn local_offset_ms(wall: f64) -> Option<f64> {
    let naive = chrono::DateTime::from_timestamp_millis(wall as i64)?.naive_utc();
    let offset = Local
        .offset_from_local_datetime(&naive)
        .earliest()
        .unwrap_or_else(|| Local.offset_from_utc_datetime(&naive));
    Some(offset.fix().local_minus_utc() as f64 * 1000.0)
}

/// Days from 1970-01-01 to the first of `month` in `year`, proleptic Gregorian.
fn days_from_civil(year: i64, month: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_parse_like_v8() {
        for (s, ms) in [
            ("2026-02-30", 1772409600000.0),
            ("2026", 1767225600000.0),
            ("2026-11", 1793491200000.0),
            ("2026Z", 1767225600000.0),
            ("2026-11-14Z", 1794614400000.0),
            ("2026-11-14T10:00Z", 1794650400000.0),
            ("2026-11-14T10:00:00.5Z", 1794650400500.0),
            ("2026-11-14T10:00:00.123456Z", 1794650400123.0),
            ("2026-11-14T10:00+05:30", 1794630600000.0),
            ("2026-11-14T10:00+23:59", 1794564060000.0),
            ("+002026-11-14", 1794614400000.0),
            ("+275760-09-13", 8.64e15),
            ("-271821-04-20", -8.64e15),
            ("0000-01-01", -62167219200000.0),
        ] {
            assert_eq!(date_parse(s), ms, "{s}");
        }
        for s in [
            "2026-13-01",
            "2026-11-32",
            "2026-11-00",
            "2026-11-14T10",
            "2026-11-14T1:00",
            "2026-11-14T10:60",
            "2026-11-14T24:00:01",
            "2026-11-14T10:00:00.Z",
            "2026-11-14T10:00+24:00",
            "2026-11-14+01:00",
            "+275760-09-14",
            "-271821-04-19",
            "garbage",
            "true",
            "",
        ] {
            assert!(date_parse(s).is_nan(), "{s}");
        }
    }
}
