//! `story stats`: Node's `backlogStats` summary of the backlog.

use crate::{
    config::{self, Flags},
    js_space,
    lenient_story::{ordered, Js},
    list, Failure,
};
use chrono::{Local, NaiveDate, NaiveDateTime, Offset, TimeZone, Utc};

const DAY: f64 = 86_400_000.0;

/// `story stats [--stale-days N] [--dir D] [--config C] [--json]`.
pub fn run(args: &[String]) -> Result<String, Failure> {
    let flags = Flags::parse(args)?;
    let loaded = config::load(flags.value("config"))?;
    let settings =
        config::parse(&loaded.raw).map_err(|e| format!("{}: {e}", loaded.path.display()))?;
    let dir = list::backlog_dir(flags.value("dir"), &loaded.root, &settings)?;
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
pub(crate) fn number(s: &str) -> f64 {
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

/// `Date.parse(string)` as V8 computes it: a port of V8's `DateParser`, which reads the ECMAScript
/// date time string format first and hands whatever is left to its legacy fallback parser.
fn date_parse(s: &str) -> f64 {
    date_parse_in(s, local_offset_ms)
}

/// The local zone's offset in milliseconds for a wall-clock time.
type LocalOffset = fn(f64) -> Option<f64>;

fn date_parse_in(s: &str, local_offset: LocalOffset) -> f64 {
    let Some((wall, offset)) = parse(s) else {
        return f64::NAN;
    };
    let utc = match offset {
        Some(seconds) => wall - seconds * 1000.0,
        None => match local_offset(wall) {
            Some(local) => wall - local,
            None => return f64::NAN,
        },
    };
    if utc.abs() <= 8.64e15 {
        utc
    } else {
        f64::NAN
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Token {
    End,
    Number { value: i64, len: usize },
    Symbol(char),
    Word(Word, usize),
    Space,
    Other,
}

#[derive(Clone, Copy, PartialEq)]
enum Word {
    Month(i64),
    AmPm(i64),
    Zone(i64),
    TimeSeparator,
    Unknown,
}

impl Token {
    fn number_of_len(self, n: usize) -> Option<i64> {
        match self {
            Token::Number { value, len } if len == n => Some(value),
            _ => None,
        }
    }
    fn sign(self) -> Option<i64> {
        match self {
            Token::Symbol('+') => Some(1),
            Token::Symbol('-') => Some(-1),
            _ => None,
        }
    }
    fn is_z(self) -> bool {
        self == Token::Word(Word::Zone(0), 1)
    }
}

struct Scanner {
    chars: Vec<char>,
    pos: usize,
    next: Token,
}

impl Scanner {
    fn new(s: &str) -> Self {
        let chars = s.chars().take_while(|&c| c != '\0').collect();
        let mut scanner = Scanner {
            chars,
            pos: 0,
            next: Token::End,
        };
        scanner.next = scanner.scan();
        scanner
    }
    fn peek(&self) -> Token {
        self.next
    }
    fn next(&mut self) -> Token {
        let next = self.scan();
        std::mem::replace(&mut self.next, next)
    }
    fn next_value(&mut self) -> i64 {
        match self.next() {
            Token::Number { value, .. } => value,
            _ => 0,
        }
    }
    fn skip(&mut self, c: char) -> bool {
        let hit = self.next == Token::Symbol(c);
        if hit {
            self.next();
        }
        hit
    }
    fn char(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }
    fn scan(&mut self) -> Token {
        let start = self.pos;
        let Some(c) = self.char() else {
            return Token::End;
        };
        if c.is_ascii_digit() {
            while self.char() == Some('0') {
                self.pos += 1;
            }
            let mut value = 0;
            let mut significant = 0;
            while let Some(d) = self.char().and_then(|c| c.to_digit(10)) {
                if significant < 9 {
                    value = value * 10 + i64::from(d);
                }
                significant += 1;
                self.pos += 1;
            }
            return Token::Number {
                value,
                len: self.pos - start,
            };
        }
        self.pos += 1;
        if matches!(c, ':' | '-' | '+' | '.' | ')') {
            return Token::Symbol(c);
        }
        if c >= 'A' && !js_space(c) {
            while self.char().is_some_and(|c| c >= 'A' && !js_space(c)) {
                self.pos += 1;
            }
            let word: String = self.chars[start..self.pos]
                .iter()
                .map(char::to_ascii_lowercase)
                .collect();
            return Token::Word(keyword(&word), self.pos - start);
        }
        if js_space(c) {
            return Token::Space;
        }
        if c == '(' {
            let mut depth = 1;
            while let Some(c) = self.char().filter(|_| depth > 0) {
                depth += match c {
                    '(' => 1,
                    ')' => -1,
                    _ => 0,
                };
                self.pos += 1;
            }
        }
        Token::Other
    }
}

/// V8's keyword table: a month matches on its first three letters, every other keyword exactly.
fn keyword(word: &str) -> Word {
    const MONTHS: [&str; 12] = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let prefix: String = word.chars().take(3).collect();
    if let Some(i) = MONTHS.iter().position(|&m| m == prefix) {
        return Word::Month(i as i64 + 1);
    }
    match word {
        "am" => Word::AmPm(0),
        "pm" => Word::AmPm(12),
        "ut" | "utc" | "z" | "gmt" => Word::Zone(0),
        "cdt" => Word::Zone(-5),
        "cst" => Word::Zone(-6),
        "edt" => Word::Zone(-4),
        "est" => Word::Zone(-5),
        "mdt" => Word::Zone(-6),
        "mst" => Word::Zone(-7),
        "pdt" => Word::Zone(-7),
        "pst" => Word::Zone(-8),
        "t" => Word::TimeSeparator,
        _ => Word::Unknown,
    }
}

#[derive(Default)]
struct Day {
    comp: Vec<i64>,
    named_month: Option<i64>,
    iso: bool,
}

impl Day {
    fn add(&mut self, n: i64) -> bool {
        let room = self.comp.len() < 3;
        if room {
            self.comp.push(n);
        }
        room
    }
    /// Year, 1-based month, and day.
    fn write(mut self) -> Option<(i64, i64, i64)> {
        if self.comp.is_empty() {
            return None;
        }
        self.comp.resize(3, 1);
        let c = &self.comp;
        let is_day = |n: i64| (1..=31).contains(&n);
        let (mut year, month, day) = match self.named_month {
            None if self.iso || !is_day(c[0]) => (c[0], c[1], c[2]),
            None => (c[2], c[0], c[1]),
            Some(month) if !is_day(c[0]) => (c[0], month, c[1]),
            Some(month) => (c[1], month, c[0]),
        };
        if !self.iso {
            match year {
                0..=49 => year += 2000,
                50..=99 => year += 1900,
                _ => {}
            }
        }
        ((1..=12).contains(&month) && is_day(day)).then_some((year, month, day))
    }
}

#[derive(Default)]
struct Time {
    comp: Vec<i64>,
    hour_offset: Option<i64>,
}

impl Time {
    fn is_empty(&self) -> bool {
        self.comp.is_empty()
    }
    fn add(&mut self, n: i64) -> bool {
        let room = self.comp.len() < 4;
        if room {
            self.comp.push(n);
        }
        room
    }
    fn add_final(&mut self, n: i64) {
        self.add(n);
        self.comp.resize(4, 0);
    }
    fn is_expecting(&self, n: i64) -> bool {
        match self.comp.len() {
            1 | 2 => (0..=59).contains(&n),
            3 => (0..=999).contains(&n),
            _ => false,
        }
    }
    /// Milliseconds since midnight.
    fn write(mut self) -> Option<i64> {
        self.comp.resize(4, 0);
        let [mut hour, minute, second, milli] = self.comp[..] else {
            unreachable!()
        };
        if let Some(offset) = self.hour_offset {
            if !(0..=12).contains(&hour) {
                return None;
            }
            hour = hour % 12 + offset;
        }
        let valid = (0..=23).contains(&hour)
            && (0..=59).contains(&minute)
            && (0..=59).contains(&second)
            && (0..=999).contains(&milli);
        let midnight = hour == 24 && minute == 0 && second == 0 && milli == 0;
        (valid || midnight).then_some(((hour * 60 + minute) * 60 + second) * 1000 + milli)
    }
}

#[derive(Default)]
struct Zone {
    sign: Option<i64>,
    hour: Option<i64>,
    minute: Option<i64>,
}

impl Zone {
    fn set(&mut self, hours: i64) {
        self.sign = Some(if hours < 0 { -1 } else { 1 });
        self.hour = Some(hours.abs());
        self.minute = Some(0);
    }
    fn is_expecting(&self, n: i64) -> bool {
        self.hour.is_some() && self.minute.is_none() && (0..=59).contains(&n)
    }
    fn is_utc(&self) -> bool {
        self.hour == Some(0) && self.minute == Some(0)
    }
    /// Seconds east of UTC, `None` for local time, or `Err` when V8 rejects the offset.
    fn write(self) -> Result<Option<f64>, ()> {
        let Some(sign) = self.sign else {
            return Ok(None);
        };
        let seconds = (self.hour.unwrap_or(0) as u32)
            .wrapping_mul(3600)
            .wrapping_add((self.minute.unwrap_or(0) as u32).wrapping_mul(60));
        if seconds > i32::MAX as u32 {
            return Err(());
        }
        Ok(Some((sign * i64::from(seconds)) as f64))
    }
}

/// V8's `ReadMilliseconds`: the first three digits of a fraction, leading zeros included.
fn milliseconds(token: Token) -> i64 {
    let Token::Number { value, len } = token else {
        return 0;
    };
    match len {
        1 => value * 100,
        2 => value * 10,
        3 => value,
        _ => value / 10_i64.pow(len.min(9) as u32 - 3),
    }
}

/// Wall-clock milliseconds since the epoch, and the UTC offset in seconds when the string names one.
fn parse(s: &str) -> Option<(f64, Option<f64>)> {
    let mut scanner = Scanner::new(s);
    let (mut day, mut time, mut zone) = (Day::default(), Time::default(), Zone::default());
    let mut token = es5(&mut scanner, &mut day, &mut time, &mut zone)?;
    let mut read_number = !day.comp.is_empty();
    while token != Token::End {
        match token {
            Token::Number { value: n, .. } => {
                read_number = true;
                if scanner.skip(':') {
                    if scanner.skip(':') {
                        if !time.is_empty() {
                            return None;
                        }
                        time.add(n);
                        time.add(0);
                    } else {
                        if !time.add(n) {
                            return None;
                        }
                        scanner.skip('.');
                    }
                } else if scanner.skip('.') && time.is_expecting(n) {
                    time.add(n);
                    if !matches!(scanner.peek(), Token::Number { .. }) {
                        return None;
                    }
                    time.add_final(milliseconds(scanner.next()));
                } else if zone.is_expecting(n) {
                    zone.minute = Some(n);
                } else if time.is_expecting(n) {
                    time.add_final(n);
                    let peek = scanner.peek();
                    if !matches!(peek, Token::End | Token::Space)
                        && !peek.is_z()
                        && peek.sign().is_none()
                    {
                        return None;
                    }
                } else {
                    if !day.add(n) {
                        return None;
                    }
                    scanner.skip('-');
                }
            }
            Token::Word(word, _) => match word {
                Word::AmPm(offset) if !time.is_empty() => time.hour_offset = Some(offset),
                Word::Month(month) => {
                    day.named_month = Some(month);
                    scanner.skip('-');
                }
                Word::Zone(hours) if read_number => zone.set(hours),
                _ => {
                    if read_number || matches!(scanner.peek(), Token::Number { .. }) {
                        return None;
                    }
                }
            },
            Token::Symbol(_) if token.sign().is_some() && (zone.is_utc() || !time.is_empty()) => {
                zone.sign = token.sign();
                let (n, len) = match scanner.peek() {
                    Token::Number { value, len } => {
                        scanner.next();
                        (value, len)
                    }
                    _ => (0, 0),
                };
                read_number = true;
                if scanner.peek() == Token::Symbol(':') {
                    zone.hour = Some(n);
                    zone.minute = None;
                } else if (1..=2).contains(&len) {
                    zone.hour = Some(n);
                    zone.minute = Some(0);
                } else if (3..=4).contains(&len) {
                    zone.hour = Some(n / 100);
                    zone.minute = Some(n % 100);
                } else {
                    return None;
                }
            }
            Token::Symbol('+' | '-' | ')') if read_number => return None,
            _ => {}
        }
        token = scanner.next();
    }
    let (year, month, date) = day.write()?;
    let ms = time.write()?;
    let offset = zone.write().ok()?;
    if year.abs() > 1_000_000 {
        return None;
    }
    let days = days_from_civil(year, month) + date - 1;
    Some((days as f64 * DAY + ms as f64, offset))
}

/// V8's `ParseES5DateTime`: the ECMAScript format, or the first token it could not use.
fn es5(scanner: &mut Scanner, day: &mut Day, time: &mut Time, zone: &mut Zone) -> Option<Token> {
    if let Some(sign) = scanner.peek().sign() {
        let sign_token = scanner.next();
        if scanner.peek().number_of_len(6).is_none() {
            return Some(sign_token);
        }
        let year = scanner.next_value();
        if sign < 0 && year == 0 {
            return Some(sign_token);
        }
        day.add(sign * year);
    } else if scanner.peek().number_of_len(4).is_some() {
        day.add(scanner.next_value());
    } else {
        return Some(scanner.next());
    }
    for range in [1..=12, 1..=31] {
        if !scanner.skip('-') {
            break;
        }
        match scanner.peek().number_of_len(2) {
            Some(n) if range.contains(&n) => day.add(scanner.next_value()),
            _ => return Some(scanner.next()),
        };
    }
    if scanner.peek() != Token::Word(Word::TimeSeparator, 1) {
        if scanner.peek() != Token::End {
            return Some(scanner.next());
        }
    } else {
        scanner.next();
        let hour = scanner.peek().number_of_len(2).filter(|h| *h <= 24)?;
        time.add(scanner.next_value());
        let fields_zero = |n: i64| hour < 24 || n == 0;
        if !scanner.skip(':') {
            return None;
        }
        scanner
            .peek()
            .number_of_len(2)
            .filter(|&m| m <= 59 && fields_zero(m))?;
        time.add(scanner.next_value());
        if scanner.skip(':') {
            scanner
                .peek()
                .number_of_len(2)
                .filter(|&s| s <= 59 && fields_zero(s))?;
            time.add(scanner.next_value());
            if scanner.skip('.') {
                match scanner.peek() {
                    Token::Number { value, .. } if fields_zero(value) => {
                        time.add(milliseconds(scanner.next()));
                    }
                    _ => return None,
                }
            }
        }
        if scanner.peek().is_z() {
            scanner.next();
            zone.set(0);
        } else if let Some(sign) = scanner.peek().sign() {
            scanner.next();
            zone.sign = Some(sign);
            if let Some(hhmm) = scanner.peek().number_of_len(4) {
                scanner.next();
                let (h, m) = (hhmm / 100, hhmm % 100);
                if h > 23 || m > 59 {
                    return None;
                }
                zone.hour = Some(h);
                zone.minute = Some(m);
            } else {
                zone.hour = Some(scanner.peek().number_of_len(2).filter(|h| *h <= 23)?);
                scanner.next();
                if !scanner.skip(':') {
                    return None;
                }
                zone.minute = Some(scanner.peek().number_of_len(2).filter(|m| *m <= 59)?);
                scanner.next();
            }
        }
        if scanner.peek() != Token::End {
            return None;
        }
    }
    if zone.hour.is_none() && time.is_empty() {
        zone.set(0);
    }
    day.iso = true;
    Some(Token::End)
}

/// The local zone's offset for a wall-clock time given as milliseconds since the epoch.
/// Wall times past chrono's range (years beyond about 262,000) take the offset at its nearest end,
/// as V8 still answers for them.
fn local_offset_ms(wall: f64) -> Option<f64> {
    let range = NaiveDateTime::MIN.and_utc().timestamp_millis()
        ..=NaiveDateTime::MAX.and_utc().timestamp_millis();
    let wall = (wall as i64).clamp(*range.start(), *range.end());
    let naive = chrono::DateTime::from_timestamp_millis(wall)?.naive_utc();
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
mod date_fuzz;

#[cfg(test)]
mod tests {
    use super::*;

    const NAN: f64 = f64::NAN;

    /// `Date.parse(s)` as Node 24 returned it with `TZ=UTC` and with `TZ=Etc/GMT+6` (UTC-6, no DST).
    const NODE: &[(&str, f64, f64)] = &[
        ("2026-11-14", 1794614400000.0, 1794614400000.0),
        ("2026-02-30", 1772409600000.0, 1772409600000.0),
        ("2026", 1767225600000.0, 1767225600000.0),
        ("2026-11", 1793491200000.0, 1793491200000.0),
        ("2026-11-14Z", 1794614400000.0, 1794614400000.0),
        ("2026-11-14T10:00", 1794650400000.0, 1794672000000.0),
        ("2026-11-14T10:00Z", 1794650400000.0, 1794650400000.0),
        ("2026-11-14T10:00:00.5Z", 1794650400500.0, 1794650400500.0),
        (
            "2026-11-14T10:00:00.123456Z",
            1794650400123.0,
            1794650400123.0,
        ),
        ("2026-11-14T10:00+05:30", 1794630600000.0, 1794630600000.0),
        ("2026-11-14T10:00+23:59", 1794564060000.0, 1794564060000.0),
        ("+002026-11-14", 1794614400000.0, 1794614400000.0),
        ("+275760-09-13", 8.64e15, 8.64e15),
        ("-271821-04-20", -8.64e15, -8.64e15),
        ("0000-01-01", -62167219200000.0, -62167219200000.0),
        ("-000000-01-01", 978307200000.0, 978328800000.0),
        ("2026-13-01", NAN, NAN),
        ("2026-11-32", NAN, NAN),
        ("2026-11-00", NAN, NAN),
        ("2026-11-14T10", NAN, NAN),
        ("2026-11-14T1:00", NAN, NAN),
        ("2026-11-14T10:60", NAN, NAN),
        ("2026-11-14T24:00:01", NAN, NAN),
        ("2026-11-14T10:00+24:00", NAN, NAN),
        ("2026-11-14+01:00", NAN, NAN),
        ("+275760-09-14", NAN, NAN),
        ("-271821-04-19", NAN, NAN),
        ("garbage", NAN, NAN),
        ("true", NAN, NAN),
        ("", NAN, NAN),
        ("0", 946684800000.0, 946706400000.0),
        ("2026/11/14", 1794614400000.0, 1794636000000.0),
        ("2026/11/14 10:00", 1794650400000.0, 1794672000000.0),
        ("2026/1/2", 1767312000000.0, 1767333600000.0),
        ("Nov 14 2026", 1794614400000.0, 1794636000000.0),
        ("14 Nov 2026", 1794614400000.0, 1794636000000.0),
        ("November 14, 2026", 1794614400000.0, 1794636000000.0),
        ("11/14/2026", 1794614400000.0, 1794636000000.0),
        ("11/14/26", 1794614400000.0, 1794636000000.0),
        ("11-14-2026", 1794614400000.0, 1794636000000.0),
        ("2026-11-14 10:00", 1794650400000.0, 1794672000000.0),
        ("2026-11-14 10:00:30", 1794650430000.0, 1794672030000.0),
        ("2026-11-14 10:00:30.5", 1794650430500.0, 1794672030500.0),
        ("2026-11-14 10:00 PM", 1794693600000.0, 1794715200000.0),
        ("2026-11-14 10:00Z", 1794650400000.0, 1794650400000.0),
        (
            "2026-11-14 10:00 GMT+0530",
            1794630600000.0,
            1794630600000.0,
        ),
        ("2026-11-14T10:00 GMT", NAN, NAN),
        ("Sat Nov 14 2026", 1794614400000.0, 1794636000000.0),
        (
            "Sat, 14 Nov 2026 10:00:00 GMT",
            1794650400000.0,
            1794650400000.0,
        ),
        (
            "Sat Nov 14 2026 10:00:00 GMT-0600 (Central Standard Time)",
            1794672000000.0,
            1794672000000.0,
        ),
        ("Nov 14", 1005696000000.0, 1005717600000.0),
        ("11/14", 1005696000000.0, 1005717600000.0),
        ("2026.11.14", 1794614400000.0, 1794636000000.0),
        ("14.11.2026", NAN, NAN),
        ("2026-1-2", 1767312000000.0, 1767333600000.0),
        ("2026-11-5", 1793836800000.0, 1793858400000.0),
        ("26-11-14", NAN, NAN),
        ("2026 11 14", 1794614400000.0, 1794636000000.0),
        ("Nov-14-2026", 1794614400000.0, 1794636000000.0),
        ("14-Nov-2026", 1794614400000.0, 1794636000000.0),
        ("2026-Nov-14", 1794614400000.0, 1794636000000.0),
        ("2026/11/14 25:00", NAN, NAN),
        ("2026/11/14 24:00", 1794700800000.0, 1794722400000.0),
        ("2026/11/14 12:00 AM", 1794614400000.0, 1794636000000.0),
        ("2026/11/14 12:00 PM", 1794657600000.0, 1794679200000.0),
        ("2026/11/14 13:00 PM", NAN, NAN),
        ("2026/11/14 10:00 EST", 1794668400000.0, 1794668400000.0),
        ("2026/11/14 10:00 PDT", 1794675600000.0, 1794675600000.0),
        ("2026/11/14 10:00 UTC+2", 1794643200000.0, 1794643200000.0),
        ("2026/11/14 10:00 +02:30", 1794641400000.0, 1794641400000.0),
        ("2026/11/14 10:00 -2", 1794657600000.0, 1794657600000.0),
        ("2026/11/14 10:00 +12345", NAN, NAN),
        ("2026/11/14 10:00 +", NAN, NAN),
        ("2026/11/14 10:00::", NAN, NAN),
        ("10:: 2026/11/14", NAN, NAN),
        ("2026/11/14 10.30", NAN, NAN),
        ("2026/11/14 10:30.5", NAN, NAN),
        ("Nov 14 2026 garbage", NAN, NAN),
        ("garbage Nov 14 2026", 1794614400000.0, 1794636000000.0),
        ("foo2026/11/14", NAN, NAN),
        ("(note) 2026/11/14", 1794614400000.0, 1794636000000.0),
        ("2026/11/14 (note)", 1794614400000.0, 1794636000000.0),
        ("2026/11/14)", NAN, NAN),
        ("Nov 2026", 1793491200000.0, 1793512800000.0),
        ("2026", 1767225600000.0, 1767225600000.0),
        ("2026Z", 1767225600000.0, 1767225600000.0),
        ("1/2/3", 1041465600000.0, 1041487200000.0),
        ("1/2/3/4", NAN, NAN),
        ("50/1/1", -631152000000.0, -631130400000.0),
        ("1/1/50", -631152000000.0, -631130400000.0),
        ("1/1/49", 2493072000000.0, 2493093600000.0),
        ("1/1/0", 946684800000.0, 946706400000.0),
        ("1/1/100", -59011459200000.0, -59011437600000.0),
        ("Nov 14 99", 942537600000.0, 942559200000.0),
        ("Nov 14 1999", 942537600000.0, 942559200000.0),
        ("2026-11-14T10:00:00 Z", NAN, NAN),
        ("2026-11-14 z", 1794614400000.0, 1794614400000.0),
        ("2026-11-14T", NAN, NAN),
        ("t2026", NAN, NAN),
        ("Novembre 14 2026", 1794614400000.0, 1794636000000.0),
        ("nov14 2026", 1794614400000.0, 1794636000000.0),
        ("Nov 14 2026 10am", NAN, NAN),
        ("Nov 14 2026 10 AM", NAN, NAN),
        ("Nov 14 2026 10:00 am", 1794650400000.0, 1794672000000.0),
        ("2026/02/30", 1772409600000.0, 1772431200000.0),
        ("2026/2/29", 1772323200000.0, 1772344800000.0),
        ("2026/11/31", 1796083200000.0, 1796104800000.0),
        ("2026/11/32", NAN, NAN),
        ("2026/13/1", NAN, NAN),
        ("0/1/2026", NAN, NAN),
        ("Nov 0 2026", NAN, NAN),
        ("Nov 32 2026", NAN, NAN),
        ("2026/11/14 10:00 GMT-08", 1794679200000.0, 1794679200000.0),
        (
            "2026/11/14 10:00 GMT-0800",
            1794679200000.0,
            1794679200000.0,
        ),
        (
            "2026/11/14 10:00 GMT-8:30",
            1794681000000.0,
            1794681000000.0,
        ),
        ("2026/11/14 10:00 UT", 1794650400000.0, 1794650400000.0),
        ("2026/11/14 10:00 CST", 1794672000000.0, 1794672000000.0),
        ("2026/11/14 10:00 MDT", 1794672000000.0, 1794672000000.0),
        ("  2026/11/14  ", 1794614400000.0, 1794636000000.0),
        ("2026/11/14\u{9}", 1794614400000.0, 1794636000000.0),
        ("2026/11/14\u{a0}", 1794614400000.0, 1794636000000.0),
        ("\u{feff}2026/11/14", 1794614400000.0, 1794636000000.0),
        ("2026/11/14\u{85}", NAN, NAN),
        ("2026,11,14", 1794614400000.0, 1794636000000.0),
        ("2026_11_14", NAN, NAN),
        ("2026/11/14 T10:00", NAN, NAN),
        ("+2026/11/14", 1794614400000.0, 1794636000000.0),
        ("-2026/11/14", 1794614400000.0, 1794636000000.0),
        ("2026/11/14 +0100", NAN, NAN),
        ("2026/11/14 -0100", NAN, NAN),
        ("10:00 2026/11/14", 1794650400000.0, 1794672000000.0),
        (
            "10:00:00.123456 2026/11/14",
            1794650400123.0,
            1794672000123.0,
        ),
        ("000002026/11/14", 1794614400000.0, 1794636000000.0),
        ("1234567890/1/1", NAN, NAN),
        ("2026/11/14 10:00:00.", NAN, NAN),
        (
            "2026/11/14 10:00:00.1234567890123",
            1794650400123.0,
            1794672000123.0,
        ),
        ("2026/11/14 10:00:60", NAN, NAN),
        ("2026/11/14 10:60", NAN, NAN),
        (
            "Nov 14 2026 00:00:00 GMT+2400",
            1794528000000.0,
            1794528000000.0,
        ),
        ("Nov 14 2026 GMT", 1794614400000.0, 1794614400000.0),
        ("GMT Nov 14 2026", 1794614400000.0, 1794636000000.0),
        ("UTC", NAN, NAN),
        ("Z", NAN, NAN),
        ("Nov", NAN, NAN),
        ("2026-11-14T10:00:00+0100", 1794646800000.0, 1794646800000.0),
        ("2026-11-14T10:00:00+01", NAN, NAN),
        ("2026-11-14T10:00:00.Z", NAN, NAN),
        ("2026-11-14T24:00", 1794700800000.0, 1794722400000.0),
        ("2026-11-14T24:00:00.000", 1794700800000.0, 1794722400000.0),
        ("2026-11-14T24:00:00.001", NAN, NAN),
        ("2026-11-14T10:00:00,5", NAN, NAN),
        ("02026-11-14", 1794614400000.0, 1794636000000.0),
        ("2026-011-14", 1794614400000.0, 1794636000000.0),
        ("2026-11-014", 1794614400000.0, 1794636000000.0),
        ("275760/9/13", 8.64e15, NAN),
        ("275760/9/14", NAN, NAN),
        ("-271821/4/20 UTC", -8.64e15, -8.64e15),
        ("271821/4/20", 8.5156844832e15, 8.5156845048e15),
        ("Nov 14 2026 12:00 a.m.", NAN, NAN),
        ("2026-11-14x", NAN, NAN),
        ("x2026-11-14", NAN, NAN),
        ("2026-11-14 x", NAN, NAN),
        ("Thu, 01 Jan 1970 00:00:00 GMT", 0.0, 0.0),
        ("Jan 1 1970 UTC", 0.0, 0.0),
        ("January 1, 1970 00:00:00 UTC", 0.0, 0.0),
        ("1 January 1970 UTC", 0.0, 0.0),
        ("1970/1/1 0:0:0 GMT", 0.0, 0.0),
        ("0:00 AM 2026/11/14", 1794614400000.0, 1794636000000.0),
        ("12:30 AM 2026/11/14", 1794616200000.0, 1794637800000.0),
        ("2026/11/14 10:00 am pm", 1794693600000.0, 1794715200000.0),
        ("2026/11/14 PM", NAN, NAN),
        ("Nov Dec 14 2026", 1797206400000.0, 1797228000000.0),
        ("14 2026 Nov", 1794614400000.0, 1794636000000.0),
        ("2026 Nov 14", 1794614400000.0, 1794636000000.0),
        ("2026 14 Nov", 1794614400000.0, 1794636000000.0),
        ("Dec 25, 1999 23:59:59.999", 946166399999.0, 946187999999.0),
        (
            "2026/11/14 10:00 GMT+5 PST",
            1794679200000.0,
            1794679200000.0,
        ),
        ("2026/11/14 10:00 Z", 1794650400000.0, 1794650400000.0),
        ("2026/11/14 10::", 1794650400000.0, 1794672000000.0),
        ("2026/11/14 10:00 +01:", 1794646800000.0, 1794646800000.0),
        ("2026/11/14 10 :00", NAN, NAN),
        ("2026/11/14 1:2:3", 1794618123000.0, 1794639723000.0),
        ("2026/11/14 1:2:3:4", 1794618123004.0, 1794639723004.0),
        ("2026/11/14 1:2:3.4", 1794618123400.0, 1794639723400.0),
        ("2026/11/14 23:59:59.9999", 1794700799999.0, 1794722399999.0),
        ("Mon Nov 14 2026 garbage", NAN, NAN),
        (
            "Monday, November 14, 2026",
            1794614400000.0,
            1794636000000.0,
        ),
        ("2026/11/14 10:00 (CST)", 1794650400000.0, 1794672000000.0),
        ("2026/11/14 ((a) b)", 1794614400000.0, 1794636000000.0),
        ("2026/11/14\u{0}junk", 1794614400000.0, 1794636000000.0),
    ];

    #[test]
    fn dates_parse_like_v8() {
        let zones: [(LocalOffset, &str); 2] = [
            (|_| Some(0.0), "UTC"),
            (|_| Some(-6.0 * 3_600_000.0), "UTC-6"),
        ];
        let mut wrong = Vec::new();
        for &(s, utc, minus_six) in NODE {
            for ((offset, zone), want) in zones.iter().zip([utc, minus_six]) {
                let got = date_parse_in(s, *offset);
                if got.to_bits() != want.to_bits() && !(got.is_nan() && want.is_nan()) {
                    wrong.push(format!("{s:?} in {zone}: native {got}, Node {want}"));
                }
            }
        }
        assert!(
            wrong.is_empty(),
            "{} differ:\n{}",
            wrong.len(),
            wrong.join("\n")
        );
    }
}
