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
    position(&text, "If the calendar or known closed sprints are missing");
    assert!(!text.contains("always `unknown` today"));
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
    let router = skill("story-time");
    let route = router
        .lines()
        .find(|line| line.contains("**agile-metrics** skill"))
        .unwrap();
    for text in ["burndown", "forecast", "when will it be done"] {
        position(route, text);
    }
}

#[test]
fn planning_and_review_use_native_sprint_figures() {
    let metrics = skill("agile-metrics");
    for term in [
        "story burndown",
        "story forecast",
        "story_velocity",
        "story_burndown",
        "story_forecast",
    ] {
        position(&metrics, term);
    }
    let planning = skill("sprint-planning")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    position(
        &planning,
        "velocity minimum to maximum over the last 3 closed sprints",
    );
    position(&planning, "story set ID sprint=YYYY-Snn");
    let review = skill("sprint-review");
    position(&review, "story burndown --sprint");
    position(
        &review,
        "committed points against done points from `story sprint show`",
    );
}
