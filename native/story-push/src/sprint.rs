//! Sprint ids, the sprint calendar, and what a sprint committed to, derived from changelog
//! events alone. Nothing here reads a file or the clock.

use chrono::{Datelike, Duration, NaiveDate, Weekday};
use serde_json::Value;

/// `YYYY-Snn`, with `nn` from 01 to 99.
pub fn valid_id(id: &str) -> bool {
    let b = id.as_bytes();
    b.len() == 8
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[4] == b'-'
        && b[5] == b'S'
        && b[6..].iter().all(u8::is_ascii_digit)
        && &id[6..] != "00"
}

/// A real `YYYY-MM-DD` date, with the two-digit month and day spelled out.
pub fn parse_date(raw: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .ok()
        .filter(|d| d.format("%Y-%m-%d").to_string() == raw)
}

/// An IANA-style zone name such as `UTC` or `America/Chicago`. The zone is recorded, not applied:
/// changelog dates are already calendar days.
pub fn valid_tz(tz: &str) -> bool {
    let segments: Vec<&str> = tz.split('/').collect();
    segments.len() <= 3
        && segments.iter().all(|s| {
            !s.is_empty()
                && s.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"_+-".contains(&c))
        })
        && tz.starts_with(|c: char| c.is_ascii_uppercase())
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sprint {
    pub id: String,
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub goal: Option<String>,
    pub overridden: bool,
}

#[derive(Clone, Debug)]
pub struct Calendar {
    anchor: Option<(NaiveDate, String)>,
    working_days: u32,
    overrides: Vec<Sprint>,
}

impl Calendar {
    /// The calendar in validated settings, or `None` when they hold neither an anchor nor a
    /// `team.sprints` list.
    pub fn from_settings(settings: &Value) -> Option<Self> {
        let team = &settings["team"];
        let anchor = team["sprintAnchor"].as_object().and_then(|a| {
            Some((
                parse_date(a.get("date")?.as_str()?)?,
                a.get("tz")?.as_str()?.to_owned(),
            ))
        });
        let overrides: Vec<Sprint> = team["sprints"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| {
                Some(Sprint {
                    id: s["id"].as_str()?.to_owned(),
                    start: parse_date(s["start"].as_str()?)?,
                    end: parse_date(s["end"].as_str()?)?,
                    goal: s["goal"].as_str().map(str::to_owned),
                    overridden: true,
                })
            })
            .collect();
        if anchor.is_none() && overrides.is_empty() {
            return None;
        }
        Some(Self {
            anchor,
            working_days: team["sprintDays"].as_f64().map_or(10, |d| d as u32).max(1),
            overrides,
        })
    }

    pub fn tz(&self) -> Option<&str> {
        self.anchor.as_ref().map(|(_, tz)| tz.as_str())
    }

    pub fn resolve(&self, id: &str) -> Result<Sprint, String> {
        if !valid_id(id) {
            return Err(format!("sprint must be YYYY-Snn, got \"{id}\"."));
        }
        if let Some(found) = self.overrides.iter().find(|s| s.id == id) {
            return Ok(found.clone());
        }
        let Some((anchor, _)) = &self.anchor else {
            return Err(format!(
                "{id} is not in team.sprints and team.sprintAnchor is not set."
            ));
        };
        let year: i32 = id[..4].parse().unwrap();
        let number: u32 = id[6..].parse().unwrap();
        if year < anchor.year() {
            return Err(format!("{id} is before the sprint anchor {anchor}."));
        }
        let (mut start, mut in_year, mut in_year_of) = (*anchor, 0, anchor.year());
        for _ in 0..10_000 {
            if start.year() != in_year_of {
                (in_year, in_year_of) = (0, start.year());
            }
            in_year += 1;
            if in_year_of > year {
                break;
            }
            let end = add_working_days(start, self.working_days - 1);
            if in_year_of == year && in_year == number {
                return Ok(Sprint {
                    id: id.to_owned(),
                    start,
                    end,
                    goal: None,
                    overridden: false,
                });
            }
            start = add_working_days(end, 1);
        }
        Err(format!(
            "{id} is past the last sprint of {year} that starts from the anchor {anchor}."
        ))
    }
}

fn weekend(d: NaiveDate) -> bool {
    matches!(d.weekday(), Weekday::Sat | Weekday::Sun)
}

fn add_working_days(from: NaiveDate, days: u32) -> NaiveDate {
    let mut day = from;
    for _ in 0..days {
        day += Duration::days(1);
        while weekend(day) {
            day += Duration::days(1);
        }
    }
    day
}

/// A story's changelog events in file order, one list per kind. A `None` value is a field
/// removed (`set sprint=`) or a size that is not a number.
#[derive(Clone, Debug, Default)]
pub struct StoryLog {
    pub status: Vec<(NaiveDate, String)>,
    pub sprint: Vec<(NaiveDate, Option<String>)>,
    pub size: Vec<(NaiveDate, Option<f64>)>,
}

#[derive(Debug, PartialEq)]
pub struct Member {
    pub id: String,
    pub points: Option<f64>,
}

#[derive(Debug, PartialEq)]
pub enum Move {
    Add,
    Drop { to: Option<String> },
}

#[derive(Debug, PartialEq)]
pub struct Change {
    pub date: NaiveDate,
    pub id: String,
    pub kind: Move,
    pub points: Option<f64>,
}

#[derive(Debug, PartialEq)]
pub struct Done {
    pub id: String,
    pub date: NaiveDate,
    pub points: Option<f64>,
}

#[derive(Debug, Default, PartialEq)]
pub struct Derived {
    pub committed: Vec<Member>,
    pub changes: Vec<Change>,
    pub done: Vec<Done>,
}

impl Derived {
    /// Whether any story was in the sprint at its start or joined it before its end.
    pub fn had_members(&self) -> bool {
        !self.committed.is_empty() || self.changes.iter().any(|c| c.kind == Move::Add)
    }

    /// Completed points, or the stories whose size at commitment is unknown.
    pub fn completed(&self) -> Result<f64, Vec<&str>> {
        let unknown: Vec<&str> = self
            .done
            .iter()
            .filter(|d| d.points.is_none())
            .map(|d| d.id.as_str())
            .collect();
        if unknown.is_empty() {
            Ok(self.done.iter().filter_map(|d| d.points).sum())
        } else {
            Err(unknown)
        }
    }
}

fn by_date<T: Clone>(events: &[(NaiveDate, T)]) -> Vec<(NaiveDate, T)> {
    let mut sorted = events.to_vec();
    sorted.sort_by_key(|(d, _)| *d);
    sorted
}

/// The last `set size=` on or before `date`. A size set later never reaches back.
fn points_at(log: &StoryLog, date: NaiveDate) -> Option<f64> {
    by_date(&log.size)
        .into_iter()
        .rfind(|(d, _)| *d <= date)
        .and_then(|(_, size)| size)
}

/// The day a story reached done within the sprint: the first of the `done` events that end its
/// status history on or before the sprint end, if that is no earlier than the sprint start.
fn done_date(log: &StoryLog, sprint: &Sprint) -> Option<NaiveDate> {
    let history: Vec<_> = by_date(&log.status)
        .into_iter()
        .filter(|(d, _)| *d <= sprint.end)
        .collect();
    let trailing = history
        .iter()
        .rev()
        .take_while(|(_, s)| s == "done")
        .last()?;
    (trailing.0 >= sprint.start).then_some(trailing.0)
}

/// What `sprint` committed to and did, from the logs of every story. The committed set is the
/// stories in the sprint on its start date, with points as sized on that date. A later
/// `set sprint=` is an add or a drop, with the points as sized on the day it happened. A
/// story's completed points are its points on entry, so a re-size after commitment moves nothing.
pub fn derive<'a>(
    sprint: &Sprint,
    stories: impl IntoIterator<Item = (&'a str, &'a StoryLog)>,
) -> Derived {
    let mut out = Derived::default();
    for (id, log) in stories {
        let events = by_date(&log.sprint);
        let in_sprint = |value: &Option<String>| value.as_deref() == Some(sprint.id.as_str());
        let after_start = events.partition_point(|(d, _)| *d <= sprint.start);
        let mut member = events[..after_start]
            .last()
            .is_some_and(|(_, value)| in_sprint(value));
        let mut points = None;
        if member {
            points = points_at(log, sprint.start);
            out.committed.push(Member {
                id: id.to_owned(),
                points,
            });
        }
        for (date, value) in events[after_start..]
            .iter()
            .take_while(|(d, _)| *d <= sprint.end)
        {
            match (in_sprint(value), member) {
                (true, false) => {
                    member = true;
                    points = points_at(log, *date);
                    out.changes.push(Change {
                        date: *date,
                        id: id.to_owned(),
                        kind: Move::Add,
                        points,
                    });
                }
                (false, true) => {
                    member = false;
                    out.changes.push(Change {
                        date: *date,
                        id: id.to_owned(),
                        kind: Move::Drop { to: value.clone() },
                        points,
                    });
                }
                _ => {}
            }
        }
        if let Some(date) = member.then(|| done_date(log, sprint)).flatten() {
            out.done.push(Done {
                id: id.to_owned(),
                date,
                points,
            });
        }
    }
    out.changes
        .sort_by(|a, b| (a.date, &a.id).cmp(&(b.date, &b.id)));
    out.done
        .sort_by(|a, b| (a.date, &a.id).cmp(&(b.date, &b.id)));
    out
}

/// Mean, minimum and maximum of the last `n` values, or `None` when fewer than `n` exist.
#[derive(Debug, PartialEq)]
pub struct Stat {
    pub sprints: usize,
    pub mean: f64,
    pub min: f64,
    pub max: f64,
}

pub fn stat(points: &[f64], n: usize) -> Option<Stat> {
    let last = points.get(points.len().checked_sub(n)?..)?;
    Some(Stat {
        sprints: n,
        mean: last.iter().sum::<f64>() / n as f64,
        min: last.iter().copied().fold(f64::INFINITY, f64::min),
        max: last.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn d(s: &str) -> NaiveDate {
        parse_date(s).unwrap()
    }

    fn calendar() -> Calendar {
        Calendar::from_settings(&json!({"team": {
            "sprintDays": 10,
            "sprintAnchor": {"date": "2026-01-05", "tz": "UTC"},
            "sprints": [{"id": "2026-S02", "start": "2026-01-19", "end": "2026-01-23", "goal": "short"}],
        }}))
        .unwrap()
    }

    #[test]
    fn ids_are_year_s_and_two_digits() {
        for good in ["2026-S01", "2026-S23", "1999-S99"] {
            assert!(valid_id(good), "{good}");
        }
        for bad in [
            "2026-S1",
            "2026-S00",
            "26-S01",
            "2026-s01",
            "2026-S001",
            "2026S01",
            "",
        ] {
            assert!(!valid_id(bad), "{bad}");
        }
    }

    #[test]
    fn dates_must_be_spelled_out_and_real() {
        assert!(parse_date("2026-02-28").is_some());
        for bad in ["2026-02-30", "2026-2-3", "02/03/2026", ""] {
            assert!(parse_date(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn zones_look_like_iana_names() {
        for good in [
            "UTC",
            "America/Chicago",
            "America/Argentina/Buenos_Aires",
            "Etc/GMT+5",
        ] {
            assert!(valid_tz(good), "{good}");
        }
        for bad in [
            "",
            "chicago",
            "America//Chicago",
            "A/B/C/D",
            "America/New York",
        ] {
            assert!(!valid_tz(bad), "{bad}");
        }
    }

    #[test]
    fn the_anchor_and_working_days_fix_each_sprint_and_an_override_changes_only_its_own() {
        let c = calendar();
        let s1 = c.resolve("2026-S01").unwrap();
        assert_eq!((s1.start, s1.end), (d("2026-01-05"), d("2026-01-16")));
        let s2 = c.resolve("2026-S02").unwrap();
        assert_eq!(
            (s2.start, s2.end, s2.goal.as_deref()),
            (d("2026-01-19"), d("2026-01-23"), Some("short"))
        );
        let s3 = c.resolve("2026-S03").unwrap();
        assert_eq!((s3.start, s3.end), (d("2026-02-02"), d("2026-02-13")));
    }

    #[test]
    fn numbering_restarts_in_a_new_year_and_ids_outside_the_calendar_fail() {
        let c = Calendar::from_settings(&json!({"team": {"sprintDays": 10, "sprintAnchor": {"date": "2026-12-07", "tz": "UTC"}}})).unwrap();
        assert_eq!(c.resolve("2026-S02").unwrap().end, d("2027-01-01"));
        assert_eq!(c.resolve("2027-S01").unwrap().start, d("2027-01-04"));
        assert!(c
            .resolve("2026-S03")
            .unwrap_err()
            .contains("past the last sprint of 2026"));
        assert!(c
            .resolve("2025-S01")
            .unwrap_err()
            .contains("before the sprint anchor"));
        assert!(c.resolve("soon").unwrap_err().contains("YYYY-Snn"));
    }

    #[test]
    fn a_calendar_needs_an_anchor_or_a_list() {
        assert!(Calendar::from_settings(&json!({"team": {"sprintDays": 10}})).is_none());
        let only_list = Calendar::from_settings(&json!({"team": {"sprints": [{"id": "2026-S01", "start": "2026-01-05", "end": "2026-01-16"}]}})).unwrap();
        assert!(only_list.resolve("2026-S01").is_ok());
        assert!(only_list
            .resolve("2026-S02")
            .unwrap_err()
            .contains("team.sprintAnchor is not set"));
    }

    fn log(
        sprint: &[(&str, Option<&str>)],
        size: &[(&str, Option<f64>)],
        status: &[(&str, &str)],
    ) -> StoryLog {
        StoryLog {
            sprint: sprint
                .iter()
                .map(|(date, v)| (d(date), v.map(str::to_owned)))
                .collect(),
            size: size.iter().map(|(date, v)| (d(date), *v)).collect(),
            status: status
                .iter()
                .map(|(date, v)| (d(date), (*v).to_owned()))
                .collect(),
        }
    }

    fn sprint() -> Sprint {
        calendar().resolve("2026-S01").unwrap()
    }

    #[test]
    fn committed_is_the_set_on_the_start_date_with_points_as_sized_then() {
        let early = log(
            &[("2026-01-02", Some("2026-S01"))],
            &[("2026-01-02", Some(3.0)), ("2026-01-08", Some(8.0))],
            &[],
        );
        let on_start = log(
            &[("2026-01-05", Some("2026-S01"))],
            &[("2026-01-05", Some(2.0))],
            &[],
        );
        let other = log(&[("2026-01-02", Some("2026-S09"))], &[], &[]);
        let none = log(&[], &[], &[]);
        let got = derive(
            &sprint(),
            [("A", &early), ("B", &on_start), ("C", &other), ("D", &none)],
        );
        assert_eq!(
            got.committed,
            vec![
                Member {
                    id: "A".into(),
                    points: Some(3.0)
                },
                Member {
                    id: "B".into(),
                    points: Some(2.0)
                },
            ]
        );
        assert!(got.changes.is_empty());
    }

    #[test]
    fn a_set_after_the_start_is_an_add_and_a_move_away_is_a_drop() {
        let add = log(
            &[("2026-01-13", Some("2026-S01"))],
            &[("2026-01-02", Some(1.0))],
            &[],
        );
        let drop = log(
            &[
                ("2026-01-02", Some("2026-S01")),
                ("2026-01-12", Some("2026-S02")),
            ],
            &[("2026-01-02", Some(2.0))],
            &[],
        );
        let cleared = log(
            &[("2026-01-02", Some("2026-S01")), ("2026-01-14", None)],
            &[("2026-01-02", Some(4.0))],
            &[],
        );
        let got = derive(&sprint(), [("A", &add), ("B", &drop), ("C", &cleared)]);
        assert_eq!(
            got.changes,
            vec![
                Change {
                    date: d("2026-01-12"),
                    id: "B".into(),
                    kind: Move::Drop {
                        to: Some("2026-S02".into())
                    },
                    points: Some(2.0)
                },
                Change {
                    date: d("2026-01-13"),
                    id: "A".into(),
                    kind: Move::Add,
                    points: Some(1.0)
                },
                Change {
                    date: d("2026-01-14"),
                    id: "C".into(),
                    kind: Move::Drop { to: None },
                    points: Some(4.0)
                },
            ]
        );
    }

    #[test]
    fn changes_after_the_sprint_end_belong_to_the_next_sprint() {
        let carried = log(
            &[
                ("2026-01-02", Some("2026-S01")),
                ("2026-01-19", Some("2026-S02")),
            ],
            &[("2026-01-02", Some(3.0))],
            &[("2026-01-20", "done")],
        );
        let got = derive(&sprint(), [("A", &carried)]);
        assert!(got.changes.is_empty());
        assert!(
            got.done.is_empty(),
            "done after the end is not this sprint's"
        );
    }

    #[test]
    fn a_re_size_after_commitment_leaves_the_points_alone() {
        let l = log(
            &[("2026-01-02", Some("2026-S01"))],
            &[("2026-01-02", Some(5.0)), ("2026-01-08", Some(8.0))],
            &[("2026-01-14", "done")],
        );
        let got = derive(&sprint(), [("A", &l)]);
        assert_eq!(
            got.done,
            vec![Done {
                id: "A".into(),
                date: d("2026-01-14"),
                points: Some(5.0)
            }]
        );
    }

    #[test]
    fn done_must_fall_inside_the_sprint_and_the_story_must_be_a_member_at_the_end() {
        let before = log(
            &[("2026-01-02", Some("2026-S01"))],
            &[("2026-01-02", Some(1.0))],
            &[("2026-01-03", "done")],
        );
        let reopened = log(
            &[("2026-01-02", Some("2026-S01"))],
            &[("2026-01-02", Some(1.0))],
            &[("2026-01-07", "done"), ("2026-01-09", "in-progress")],
        );
        let dropped = log(
            &[
                ("2026-01-02", Some("2026-S01")),
                ("2026-01-10", Some("2026-S02")),
            ],
            &[("2026-01-02", Some(1.0))],
            &[("2026-01-12", "done")],
        );
        let twice = log(
            &[("2026-01-02", Some("2026-S01"))],
            &[("2026-01-02", Some(1.0))],
            &[("2026-01-07", "done"), ("2026-01-08", "done")],
        );
        let got = derive(
            &sprint(),
            [
                ("A", &before),
                ("B", &reopened),
                ("C", &dropped),
                ("D", &twice),
            ],
        );
        assert_eq!(
            got.done,
            vec![Done {
                id: "D".into(),
                date: d("2026-01-07"),
                points: Some(1.0)
            }]
        );
    }

    #[test]
    fn unsized_completed_stories_make_the_total_unknown() {
        let sized = log(
            &[("2026-01-02", Some("2026-S01"))],
            &[("2026-01-02", Some(2.0))],
            &[("2026-01-07", "done")],
        );
        let late = log(
            &[("2026-01-02", Some("2026-S01"))],
            &[("2026-01-08", Some(2.0))],
            &[("2026-01-07", "done")],
        );
        let ok = derive(&sprint(), [("A", &sized)]);
        assert_eq!(ok.completed(), Ok(2.0));
        let bad = derive(&sprint(), [("A", &sized), ("B", &late)]);
        assert_eq!(bad.completed(), Err(vec!["B"]));
    }

    #[test]
    fn stat_is_the_last_n_or_nothing() {
        assert_eq!(
            stat(&[9.0, 6.0, 10.0], 3),
            Some(Stat {
                sprints: 3,
                mean: 25.0 / 3.0,
                min: 6.0,
                max: 10.0
            })
        );
        assert_eq!(stat(&[1.0, 9.0, 6.0, 10.0], 3).unwrap().min, 6.0);
        assert_eq!(stat(&[9.0, 6.0], 3), None);
        assert_eq!(stat(&[], 3), None);
    }
}
