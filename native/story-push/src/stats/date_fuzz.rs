use super::{date_parse_in, LocalOffset};
use std::io::Write;
use std::process::{Command, Stdio};

const SEED: u64 = 0x5354_2d30_3833;
const CASES: usize = 20_000;

const NODE_SCRIPT: &str = r#"
const inputs = JSON.parse(require("fs").readFileSync(0, "utf8"));
const out = {};
for (const tz of ["UTC", "Etc/GMT+6"]) {
  process.env.TZ = tz;
  out[tz] = inputs.map((s) => {
    const t = Date.parse(s);
    return Number.isNaN(t) ? null : t;
  });
}
process.stdout.write(JSON.stringify(out));
"#;

const WORDS: &[&str] = &[
    "Jan", "February", "mar", "APR", "May", "june", "Jul", "aug", "Sept", "oct", "Nov", "December",
    "Mon", "Tuesday", "wed", "Thu", "Friday", "sat", "Sun", "AM", "pm", "a.m.", "UTC", "GMT", "UT",
    "Z", "z", "CST", "cdt", "EST", "EDT", "MST", "MDT", "PST", "pdt", "T", "t", "foo", "Noon",
    "januaryx", "decx",
];

const SEPARATORS: &[&str] = &[" ", "/", "-", ".", ",", ":", ", ", "  ", "\t", "T"];

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len())]
    }
}

fn number(rng: &mut Rng) -> String {
    let width = 1 + rng.below(6);
    let mut s: String = (0..width)
        .map(|_| char::from(b'0' + rng.below(10) as u8))
        .collect();
    if rng.below(8) == 0 {
        s.insert(0, '0');
    }
    s
}

fn offset(rng: &mut Rng) -> String {
    let sign = if rng.below(2) == 0 { '+' } else { '-' };
    let hours = rng.below(26);
    let minutes = rng.pick(&["00", "30", "45", "60"]);
    match rng.below(4) {
        0 => format!("{sign}{hours:02}{minutes}"),
        1 => format!("{sign}{hours:02}:{minutes}"),
        2 => format!("{sign}{hours}"),
        _ => format!("GMT{sign}{hours:02}{minutes}"),
    }
}

fn token(rng: &mut Rng) -> String {
    match rng.below(10) {
        0..=3 => number(rng),
        4..=6 => rng.pick(WORDS).to_string(),
        7 => offset(rng),
        8 => format!("({})", rng.pick(WORDS)),
        _ => format!("{}:{}", number(rng), number(rng)),
    }
}

fn legacy(rng: &mut Rng) -> String {
    let mut s = String::new();
    for i in 0..1 + rng.below(7) {
        if i > 0 {
            s.push_str(rng.pick(SEPARATORS));
        }
        s.push_str(&token(rng));
    }
    s
}

fn iso(rng: &mut Rng) -> String {
    let date = format!(
        "{}-{:02}-{:02}",
        rng.pick(&["2026", "1970", "0000", "+002026", "-000001", "99999"]),
        rng.below(14),
        rng.below(33)
    );
    let mut s = date;
    if rng.below(3) > 0 {
        s.push(if rng.below(4) == 0 { ' ' } else { 'T' });
        s.push_str(&format!("{:02}:{:02}", rng.below(26), rng.below(62)));
        if rng.below(2) == 0 {
            s.push_str(&format!(":{:02}", rng.below(62)));
            if rng.below(2) == 0 {
                s.push('.');
                s.push_str(&number(rng));
            }
        }
        match rng.below(4) {
            0 => s.push('Z'),
            1 => s.push_str(&offset(rng)),
            _ => {}
        }
    }
    if rng.below(5) == 0 {
        let at = rng.below(s.len() + 1);
        let insert = token(rng);
        s.insert_str(at, &insert);
    }
    s
}

fn inputs() -> Vec<String> {
    let mut rng = Rng(SEED);
    (0..CASES)
        .map(|i| {
            if i % 3 == 0 {
                iso(&mut rng)
            } else {
                legacy(&mut rng)
            }
        })
        .collect()
}

fn node_results(inputs: &[String]) -> serde_json::Value {
    let mut child = Command::new("node")
        .args(["-e", NODE_SCRIPT])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("node runs");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(inputs).unwrap().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "node failed");
    serde_json::from_slice(&out.stdout).expect("node prints JSON")
}

#[test]
fn seeded_strings_parse_as_node_does() {
    let inputs = inputs();
    let node = node_results(&inputs);
    let zones: [(LocalOffset, &str); 2] = [
        (|_| Some(0.0), "UTC"),
        (|_| Some(-6.0 * 3_600_000.0), "Etc/GMT+6"),
    ];
    let mut wrong = Vec::new();
    for (offset, zone) in zones {
        let want = node[zone].as_array().expect("one result per input");
        for (s, want) in inputs.iter().zip(want) {
            let want = want.as_f64().unwrap_or(f64::NAN);
            let got = date_parse_in(s, offset);
            if got != want && !(got.is_nan() && want.is_nan()) {
                wrong.push(format!("{s:?} in {zone}: native {got}, Node {want}"));
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "{} of {} differ, first:\n{}",
        wrong.len(),
        2 * inputs.len(),
        wrong[..wrong.len().min(20)].join("\n")
    );
}
