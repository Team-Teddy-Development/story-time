//! `story forecast (--sprint ID | --epic ID | --ids A,B) [--today D] [--seed N] [--dir D]
//! [--config C] [--json]`: the dates by which 50% and 85% of simulated futures finish the
//! remaining points, drawn from closed-sprint velocity. Read-only.

use crate::metrics::{self, number, History};
use crate::sprint::{self, Calendar, Move};
use crate::{config, list, Failure};
use chrono::{Datelike, NaiveDate, Utc};
use serde_json::{json, Value};

const USAGE: &str = "Usage: story forecast (--sprint YYYY-Snn | --epic ID | --ids A,B) [--today YYYY-MM-DD] [--seed N] [--json]";
const SOURCE: &str = "closed-sprint velocity from changelog `set sprint=`, `set size=` and `set status=` lines in the backlog story files, day resolution, with sprint dates from settings team.sprintAnchor and team.sprints";
const DEFINITION: &str = "dates by which 50% and 85% of 10,000 simulated runs finish the remaining points; each run draws one closed-sprint velocity, with replacement, per future sprint, the sprint holding --today counting only its share of calendar days after --today; a run's date is the end of the sprint where its draws reach the remaining points; a percentile is the nearest-rank run (the 5,000th and 8,500th of 10,000)";
const SAMPLE_SOURCE: &str = "closed-sprint velocity";
const RUNS: usize = 10_000;
pub(crate) const DEFAULT_SEED: u64 = 1;
const MIN_SPRINTS: usize = 3;
const PERCENTILES: [usize; 2] = [50, 85];
const EPSILON: f64 = 1e-9;

/// SplitMix64. Written out so the sequence for a seed never depends on a crate version.
pub(crate) struct SplitMix64(u64);

impl SplitMix64 {
    pub(crate) fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub(crate) fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n` by rejection, so no value is likelier than another.
    pub(crate) fn below(&mut self, n: u64) -> u64 {
        assert!(n > 0, "below(0)");
        let zone = u64::MAX - (u64::MAX - n + 1) % n;
        loop {
            let x = self.next_u64();
            if x <= zone {
                return x % n;
            }
        }
    }
}

/// One sprint still to come: where its draw lands and how much of it is left to use.
#[derive(Debug, PartialEq)]
pub(crate) struct Ahead {
    pub(crate) end: NaiveDate,
    pub(crate) share: f64,
}

/// The sprints a run draws for, in order, from the one holding `today`. That sprint counts the
/// share of its calendar days after `today`, so a sprint with none left is skipped. A sprint
/// that starts before the previous one ends is left out, so overlapping `team.sprints` entries
/// never give a run two draws for the same days.
pub(crate) fn sprints_ahead(calendar: &Calendar, today: NaiveDate) -> Vec<Ahead> {
    let mut found: Vec<sprint::Sprint> = (today.year() - 1..=today.year() + 5)
        .flat_map(|year| (1..=99).map(move |n| format!("{year}-S{n:02}")))
        .filter_map(|id| calendar.resolve(&id).ok())
        .filter(|s| s.end > today)
        .collect();
    found.sort_by(|a, b| (a.start, &a.id).cmp(&(b.start, &b.id)));
    let mut kept: Vec<sprint::Sprint> = Vec::new();
    for sprint in found {
        if kept.last().is_none_or(|last| sprint.start > last.end) {
            kept.push(sprint);
        }
    }
    kept.into_iter()
        .map(|s| {
            let share = if s.start > today {
                1.0
            } else {
                let total = (s.end - s.start).num_days() + 1;
                (s.end - today).num_days() as f64 / total as f64
            };
            Ahead { end: s.end, share }
        })
        .collect()
}

/// One date per run, earliest first, with a run that never reaches `remaining` before the last
/// sprint as `None`, last. `sample` is where ST-193 swaps in calibrated draws.
pub(crate) fn simulate(
    remaining: f64,
    today: NaiveDate,
    ahead: &[Ahead],
    runs: usize,
    rng: &mut SplitMix64,
    mut sample: impl FnMut(&mut SplitMix64) -> f64,
) -> Vec<Option<NaiveDate>> {
    let mut dates: Vec<Option<NaiveDate>> = (0..runs)
        .map(|_| {
            if remaining <= EPSILON {
                return Some(today);
            }
            let mut done = 0.0;
            for sprint in ahead {
                done += sample(rng) * sprint.share;
                if done + EPSILON >= remaining {
                    return Some(sprint.end);
                }
            }
            None
        })
        .collect();
    dates.sort_by_key(|d| (d.is_none(), *d));
    dates
}

/// The nearest-rank percentile of `sorted`: the `ceil(p * n / 100)`th value, counting from one.
pub(crate) fn nearest_rank<T>(sorted: &[T], percentile: usize) -> &T {
    assert!(!sorted.is_empty(), "no runs to rank");
    &sorted[(percentile * sorted.len()).div_ceil(100).max(1) - 1]
}

/// A draw of one closed-sprint velocity, with replacement.
pub(crate) fn velocity_sample(points: &[f64]) -> impl FnMut(&mut SplitMix64) -> f64 + '_ {
    move |rng| points[rng.below(points.len() as u64) as usize]
}

pub(crate) enum Target {
    Sprint(String),
    Epic(String),
    Ids(Vec<String>),
}

/// The open points a target holds today, and the stories left out because they carry no size.
struct Scope {
    remaining: f64,
    unknown: Vec<Value>,
}

fn is_open(status: &str) -> bool {
    !matches!(status, "done" | "split" | "archived")
}

/// A story's status on `date`: its last status event on or before that day.
fn status_on(story: &History, date: NaiveDate) -> Option<&str> {
    story
        .log
        .status
        .iter()
        .rfind(|(d, _)| *d <= date)
        .map(|(_, status)| status.as_str())
}

fn open_stories(stories: Vec<&History>) -> Scope {
    let mut scope = Scope {
        remaining: 0.0,
        unknown: Vec::new(),
    };
    for story in stories {
        if let Some(gap) = story.gap.as_ref().filter(|_| story.status.is_empty()) {
            scope.unknown.push(json!({"id": story.id, "reason": gap}));
        } else if is_open(&story.status) {
            match story.size {
                Some(points) => scope.remaining += points,
                None => scope
                    .unknown
                    .push(json!({"id": story.id, "reason": "not sized"})),
            }
        }
    }
    scope
}

/// A sprint's remaining scope on `today`: committed, plus adds and minus drops dated on or
/// before `today`, minus the stories that are done, split or archived on `today` by their own
/// status history, so a replay with a past `--today` ignores what happened later. Points are as
/// sized on entry to the sprint.
fn sprint_scope(sprint: &sprint::Sprint, stories: &[History], today: NaiveDate) -> Scope {
    let mut snapshot = sprint.clone();
    snapshot.end = sprint.end.min(today);
    let trusted = stories
        .iter()
        .filter(|s| s.gap.is_none())
        .map(|s| (s.id.as_str(), &s.log));
    let derived = sprint::derive(&snapshot, trusted);
    let mut open: Vec<(&str, Option<f64>)> = derived
        .committed
        .iter()
        .map(|m| (m.id.as_str(), m.points))
        .collect();
    for change in derived.changes.iter().filter(|c| c.date <= today) {
        match change.kind {
            Move::Add => open.push((change.id.as_str(), change.points)),
            Move::Drop { .. } => open.retain(|(id, _)| *id != change.id),
        }
    }
    open.retain(|(id, _)| {
        let status = stories
            .iter()
            .find(|s| s.id == *id)
            .and_then(|s| status_on(s, today));
        status.is_none_or(is_open)
    });
    let mut scope = Scope {
        remaining: open.iter().filter_map(|(_, p)| *p).sum(),
        unknown: open
            .iter()
            .filter(|(_, p)| p.is_none())
            .map(|(id, _)| json!({"id": id, "reason": "not sized"}))
            .collect(),
    };
    scope.unknown.extend(
        stories
            .iter()
            .filter(|s| {
                s.log
                    .sprint
                    .iter()
                    .any(|(_, v)| v.as_deref() == Some(&sprint.id))
            })
            .filter_map(|s| Some(json!({"id": s.id, "reason": s.gap.as_ref()?}))),
    );
    scope
}

fn scope_of(
    target: &Target,
    stories: &[History],
    calendar: &Calendar,
    today: NaiveDate,
) -> Result<Scope, Failure> {
    match target {
        Target::Sprint(id) => Ok(sprint_scope(&calendar.resolve(id)?, stories, today)),
        Target::Epic(id) => {
            let members: Vec<&History> = stories
                .iter()
                .filter(|s| s.epic.as_deref() == Some(id))
                .collect();
            if members.is_empty() {
                return Err(format!("no story names epic {id}.").into());
            }
            Ok(open_stories(members))
        }
        Target::Ids(ids) => {
            let mut named = Vec::new();
            for id in ids {
                named.push(
                    stories
                        .iter()
                        .find(|s| &s.id == id)
                        .ok_or_else(|| format!("no story {id} in the backlog."))?,
                );
            }
            Ok(open_stories(named))
        }
    }
}

fn parse_target(flags: &config::Flags) -> Result<Target, String> {
    let sprint = flags.value("sprint");
    let epic = flags.value("epic");
    let ids = flags.value("ids");
    match (sprint, epic, ids) {
        (Some(id), None, None) if sprint::valid_id(id) => Ok(Target::Sprint(id.to_owned())),
        (Some(id), None, None) => Err(format!("sprint must be YYYY-Snn, got \"{id}\".")),
        (None, Some(id), None) => Ok(Target::Epic(id.to_owned())),
        (None, None, Some(list)) => {
            let mut ids: Vec<String> = Vec::new();
            for id in list.split(',').map(str::trim).filter(|id| !id.is_empty()) {
                if !ids.iter().any(|seen| seen == id) {
                    ids.push(id.to_owned());
                }
            }
            if ids.is_empty() {
                Err("--ids names no story.".to_owned())
            } else {
                Ok(Target::Ids(ids))
            }
        }
        _ => Err("Give exactly one of --sprint, --epic or --ids.".to_owned()),
    }
}

pub fn run(args: &[String]) -> Result<String, Failure> {
    let usage = |message: String| Failure {
        code: 2,
        message: format!("{message}\n{USAGE}"),
    };
    let flags = config::Flags::parse(args)?;
    let target = parse_target(&flags).map_err(usage)?;
    let today = match flags.value("today") {
        Some(raw) => sprint::parse_date(raw)
            .ok_or_else(|| usage(format!("--today must be a YYYY-MM-DD date: {raw}")))?,
        None => Utc::now().date_naive(),
    };
    let seed = match flags.value("seed") {
        Some(raw) => raw
            .parse::<u64>()
            .map_err(|_| usage(format!("--seed must be a non-negative integer: {raw}")))?,
        None => DEFAULT_SEED,
    };
    let loaded = config::load(flags.value("config"))?;
    let settings =
        config::parse(&loaded.raw).map_err(|e| format!("{}: {e}", loaded.path.display()))?;
    let calendar = Calendar::from_settings(&settings)
        .ok_or("no sprint calendar in settings (team.sprintAnchor or team.sprints).")?;
    let dir = list::backlog_dir(flags.value("dir"), &loaded.root, &settings)?;
    let stories = metrics::histories(&dir)?;
    let report = report(&stories, &calendar, &target, today, seed)?;
    Ok(if flags.has("json") {
        format!("{}\n", serde_json::to_string_pretty(&report).unwrap())
    } else {
        text(&report)
    })
}

pub(crate) fn report(
    stories: &[History],
    calendar: &Calendar,
    target: &Target,
    today: NaiveDate,
    seed: u64,
) -> Result<Value, Failure> {
    let scope = scope_of(target, stories, calendar, today)?;
    let (closed, _) = metrics::closed_velocities(stories, Some(calendar), today);
    let points: Vec<f64> = closed.iter().map(|(_, p)| *p).collect();

    let rows = if points.len() < MIN_SPRINTS {
        let plural = if points.len() == 1 { "" } else { "s" };
        let reason = format!(
            "{} closed sprint{plural} with known points; the forecast needs at least {MIN_SPRINTS}",
            points.len()
        );
        unknown_rows(&reason)
    } else if scope.remaining <= EPSILON && !scope.unknown.is_empty() {
        unknown_rows("no sized points remain in the target, so there is nothing to forecast")
    } else {
        let ahead = sprints_ahead(calendar, today);
        let mut rng = SplitMix64::new(seed);
        let runs = simulate(
            scope.remaining,
            today,
            &ahead,
            RUNS,
            &mut rng,
            velocity_sample(&points),
        );
        PERCENTILES
            .iter()
            .map(|&p| match nearest_rank(&runs, p) {
                Some(date) => json!({"percentile": p, "date": date.to_string()}),
                None => json!({"percentile": p, "date": "unknown", "reason": format!(
                    "the {p}th-percentile run does not finish before the sprint calendar ends{}",
                    ahead.last().map_or(String::new(), |a| format!(" on {}", a.end)),
                )}),
            })
            .collect()
    };

    let window_start = closed.first().map_or(today, |(s, _)| s.start);
    Ok(json!({
        "metric": "forecast",
        "units": "date",
        "window": format!("{window_start}..{today}"),
        "source": SOURCE,
        "definition": DEFINITION,
        "target": match &target {
            Target::Sprint(id) => json!({"sprint": id}),
            Target::Epic(id) => json!({"epic": id}),
            Target::Ids(ids) => json!({"ids": ids}),
        },
        "today": today.to_string(),
        "remaining_points": number(scope.remaining),
        "sample_source": SAMPLE_SOURCE,
        "sample_count": points.len(),
        "runs": RUNS,
        "seed": seed,
        "rows": rows,
        "unknown": scope.unknown,
    }))
}

fn unknown_rows(reason: &str) -> Vec<Value> {
    PERCENTILES
        .iter()
        .map(|&p| json!({"percentile": p, "date": "unknown", "reason": reason}))
        .collect()
}

fn text(report: &Value) -> String {
    let plain = |v: &Value| v.as_str().map_or_else(|| v.to_string(), str::to_owned);
    let mut out = format!(
        "Forecast from {}, {} points remaining, {} {} samples, {} runs, seed {}\n",
        plain(&report["today"]),
        plain(&report["remaining_points"]),
        plain(&report["sample_count"]),
        plain(&report["sample_source"]),
        plain(&report["runs"]),
        plain(&report["seed"]),
    );
    for row in report["rows"].as_array().unwrap() {
        out += &format!(
            "  {}% by {}",
            plain(&row["percentile"]),
            plain(&row["date"])
        );
        if let Some(reason) = row["reason"].as_str() {
            out += &format!(" ({reason})");
        }
        out += "\n";
    }
    for gap in report["unknown"].as_array().unwrap() {
        out += &format!("unknown {}: {}\n", plain(&gap["id"]), plain(&gap["reason"]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        sprint::parse_date(s).unwrap()
    }

    #[test]
    fn splitmix64_matches_the_reference_sequence_for_seed_zero() {
        let mut rng = SplitMix64::new(0);
        assert_eq!(rng.next_u64(), 0xE220_A839_7B1D_CDAF);
        assert_eq!(rng.next_u64(), 0x6E78_9E6A_A1B9_65F4);
        assert_eq!(rng.next_u64(), 0x06C4_5D18_8009_454F);
    }

    #[test]
    fn the_same_seed_repeats_and_another_seed_differs() {
        let take = |seed| {
            let mut rng = SplitMix64::new(seed);
            (0..5).map(|_| rng.next_u64()).collect::<Vec<_>>()
        };
        assert_eq!(take(7), take(7));
        assert_ne!(take(7), take(8));
    }

    #[test]
    fn below_stays_in_range_and_visits_every_value_evenly() {
        let mut rng = SplitMix64::new(3);
        let mut counts = [0u32; 5];
        for _ in 0..50_000 {
            counts[rng.below(5) as usize] += 1;
        }
        for c in counts {
            assert!((9_500..=10_500).contains(&c), "{counts:?}");
        }
        assert_eq!(SplitMix64::new(1).below(1), 0);
    }

    #[test]
    fn below_accepts_only_a_whole_number_of_laps_of_n() {
        for n in [3, 5, 1 << 40, u64::MAX / 3 * 2, u64::MAX] {
            let zone = u64::MAX - (u64::MAX - n + 1) % n;
            assert_eq!((u128::from(zone) + 1) % u128::from(n), 0, "n {n}");
        }
    }

    #[test]
    fn nearest_rank_is_the_ceiling_of_p_n_over_100_counting_from_one() {
        let runs: Vec<usize> = (1..=10_000).collect();
        assert_eq!(*nearest_rank(&runs, 50), 5_000);
        assert_eq!(*nearest_rank(&runs, 85), 8_500);
        assert_eq!(*nearest_rank(&runs, 100), 10_000);
        assert_eq!(*nearest_rank(&runs, 1), 100);
        let three = [1, 2, 3];
        assert_eq!(*nearest_rank(&three, 50), 2);
        assert_eq!(*nearest_rank(&three, 85), 3);
        assert_eq!(*nearest_rank(&[9], 50), 9);
    }

    fn calendar() -> Calendar {
        Calendar::from_settings(&serde_json::json!({"team": {
            "sprintDays": 10,
            "sprintAnchor": {"date": "2026-11-02", "tz": "UTC"},
        }}))
        .unwrap()
    }

    #[test]
    fn the_sprint_holding_today_counts_its_share_of_days_after_today() {
        // 2026-S01 is 11-02..11-13, twelve calendar days.
        let c = calendar();
        let ahead = sprints_ahead(&c, d("2026-11-06"));
        assert_eq!(ahead[0].end, d("2026-11-13"));
        assert!((ahead[0].share - 7.0 / 12.0).abs() < 1e-12);
        assert_eq!(ahead[1].end, d("2026-11-27"));
        assert_eq!(ahead[1].share, 1.0);
    }

    #[test]
    fn a_forecast_on_a_sprints_last_day_starts_with_the_next_sprint() {
        let ahead = sprints_ahead(&calendar(), d("2026-11-13"));
        assert_eq!(ahead[0].end, d("2026-11-27"));
        assert_eq!(ahead[0].share, 1.0);
    }

    #[test]
    fn a_weekend_today_counts_the_calendar_days_left_in_the_sprint() {
        let ahead = sprints_ahead(&calendar(), d("2026-11-08"));
        assert_eq!(ahead[0].end, d("2026-11-13"));
        assert!((ahead[0].share - 5.0 / 12.0).abs() < 1e-12);
    }

    #[test]
    fn a_day_before_the_first_sprint_counts_the_whole_of_it() {
        let c = Calendar::from_settings(&serde_json::json!({"team": {"sprintDays": 10, "sprintAnchor": {"date": "2026-11-02", "tz": "UTC"}}})).unwrap();
        let ahead = sprints_ahead(&c, d("2026-11-01"));
        assert_eq!(
            ahead[0],
            Ahead {
                end: d("2026-11-13"),
                share: 1.0
            }
        );
    }

    #[test]
    fn numbering_crosses_a_year_boundary_and_stays_in_order() {
        let ahead = sprints_ahead(&calendar(), d("2026-12-15"));
        let ends: Vec<NaiveDate> = ahead.iter().take(3).map(|a| a.end).collect();
        assert_eq!(ends, [d("2026-12-25"), d("2027-01-08"), d("2027-01-22")]);
        assert!(ahead.windows(2).all(|w| w[0].end < w[1].end));
    }

    #[test]
    fn constant_velocity_lands_every_run_on_the_second_sprint() {
        let ahead = sprints_ahead(&calendar(), d("2026-11-13"));
        let mut rng = SplitMix64::new(1);
        let runs = simulate(20.0, d("2026-11-13"), &ahead, 100, &mut rng, |_| 10.0);
        assert!(runs.iter().all(|r| *r == Some(d("2026-12-11"))));
    }

    #[test]
    fn nothing_remaining_finishes_today_and_an_endless_backlog_never_finishes() {
        let ahead = sprints_ahead(&calendar(), d("2026-11-13"));
        let mut rng = SplitMix64::new(1);
        let none = simulate(0.0, d("2026-11-13"), &ahead[..1], 5, &mut rng, |_| 10.0);
        assert!(none.iter().all(|r| *r == Some(d("2026-11-13"))));
        let never = simulate(1e9, d("2026-11-13"), &ahead[..2], 5, &mut rng, |_| 10.0);
        assert!(never.iter().all(Option::is_none));
    }

    #[test]
    fn the_default_seed_splits_ten_thousand_runs_by_the_pairs_it_draws() {
        // Counts from an independent Python re-implementation of the generator and the draw.
        let ends = [
            d("2026-11-29"),
            d("2026-12-13"),
            d("2026-12-25"),
            d("2027-01-08"),
        ];
        let ahead: Vec<Ahead> = ends.iter().map(|&end| Ahead { end, share: 1.0 }).collect();
        let points = [8.0, 10.0, 12.0, 9.0, 11.0];
        let mut rng = SplitMix64::new(1);
        let runs = simulate(
            20.0,
            d("2026-11-15"),
            &ahead,
            RUNS,
            &mut rng,
            velocity_sample(&points),
        );
        let on = |end: &str| runs.iter().filter(|r| **r == Some(d(end))).count();
        assert_eq!((on("2026-12-13"), on("2026-12-25")), (5_918, 4_082));
        assert_eq!(*nearest_rank(&runs, 50), Some(d("2026-12-13")));
        assert_eq!(*nearest_rank(&runs, 85), Some(d("2026-12-25")));
    }

    #[test]
    fn overlapping_sprints_give_one_draw_for_the_days_they_share() {
        let c = Calendar::from_settings(&serde_json::json!({"team": {
            "sprintDays": 10,
            "sprintAnchor": {"date": "2026-11-02", "tz": "UTC"},
            "sprints": [{"id": "2026-S02", "start": "2026-11-16", "end": "2026-12-04"}],
        }}))
        .unwrap();
        let ends: Vec<NaiveDate> = sprints_ahead(&c, d("2026-11-13"))
            .iter()
            .take(3)
            .map(|a| a.end)
            .collect();
        assert_eq!(ends, [d("2026-12-04"), d("2026-12-25"), d("2027-01-08")]);
    }

    #[test]
    #[should_panic(expected = "no runs to rank")]
    fn ranking_no_runs_is_a_bug() {
        nearest_rank::<u8>(&[], 50);
    }

    #[test]
    fn dates_are_sorted_earliest_first_with_unfinished_runs_last() {
        let ahead = sprints_ahead(&calendar(), d("2026-11-13"));
        let mut rng = SplitMix64::new(5);
        let runs = simulate(
            20.0,
            d("2026-11-13"),
            &ahead[..3],
            500,
            &mut rng,
            velocity_sample(&[1.0, 10.0, 12.0]),
        );
        assert!(runs
            .windows(2)
            .all(|w| (w[0].is_none(), w[0]) <= (w[1].is_none(), w[1])));
        assert!(runs.first().unwrap().is_some() && runs.last().unwrap().is_none());
    }
}
