use std::{fs, path::PathBuf};

fn skill(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../skills")
        .join(name)
        .join("SKILL.md");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn position(text: &str, needle: &str) -> usize {
    text.find(needle)
        .unwrap_or_else(|| panic!("missing instruction: {needle}"))
}

#[test]
fn agile_metrics_takes_every_figure_from_the_rust_command() {
    let text = skill("agile-metrics");
    position(&text, "story metrics --from");
    position(&text, "--json");
    position(&text, "Never compute a figure yourself");
}

#[test]
fn agile_metrics_names_source_window_and_units_for_every_figure() {
    let text = skill("agile-metrics");
    position(&text, "| Metric | Value | Units | Window | Source |");
    position(&text, "Every figure names its source, window, and units");
}

#[test]
fn agile_metrics_reports_gaps_as_unknown_and_never_fills_them() {
    let text = skill("agile-metrics");
    position(&text, "Report a gap as `unknown`");
    position(&text, "Never fill a gap");
    position(&text, "needs at least 3 closed sprints");
}

#[test]
fn agile_metrics_runs_no_write_command_and_uses_no_long_dashes() {
    let text = skill("agile-metrics");
    for line in text.lines().map(str::trim_start) {
        for write in ["story set", "story push", "story new", "story note"] {
            assert!(
                !line.starts_with(write),
                "agile-metrics runs no write command: {line}"
            );
        }
    }
    assert!(!text.contains('\u{2014}') && !text.contains('\u{2013}'));
}

#[test]
fn the_router_sends_metrics_requests_to_agile_metrics() {
    assert!(skill("story-time").contains("**agile-metrics** skill"));
}

fn flat(name: &str) -> String {
    skill(name).split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn the_router_has_a_row_for_burndown_forecast_and_when_will_it_be_done() {
    let text = skill("story-time");
    let row = text
        .lines()
        .find(|line| line.starts_with('|') && line.contains("\"burndown\""))
        .expect("a route row for burndown");
    for phrase in ["\"forecast\"", "\"when will it be done\""] {
        assert!(
            row.contains(phrase),
            "the burndown row names {phrase}: {row}"
        );
    }
    assert!(row.contains("**agile-metrics** skill"), "{row}");
}

#[test]
fn agile_metrics_reports_velocity_burndown_and_forecast() {
    let text = skill("agile-metrics");
    position(&text, "story burndown");
    position(&text, "story forecast");
    position(&text, "story_velocity");
    assert!(
        !text.contains("always `unknown` today"),
        "velocity is no longer always unknown"
    );
}

#[test]
fn sprint_planning_sizes_the_commitment_from_the_velocity_range() {
    let text = flat("sprint-planning");
    position(
        &text,
        "the budget comes from the velocity minimum to maximum over the last 3 closed sprints",
    );
    position(&text, "not the mean");
    position(&text, "`sprint` through `story set`");
}

#[test]
fn sprint_review_shows_the_burndown_and_committed_against_done() {
    let text = flat("sprint-review");
    position(&text, "`story burndown --sprint`");
    position(&text, "committed against done from `story sprint show`");
}
