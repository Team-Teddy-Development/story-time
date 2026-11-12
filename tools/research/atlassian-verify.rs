use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
};
fn root() -> PathBuf {
    PathBuf::from(env!("STORY_RESEARCH_ROOT"))
}
fn article_rows(report: &str) -> Vec<Vec<&str>> {
    report
        .lines()
        .filter(|l| l.starts_with("| "))
        .map(|l| l.split('|').map(str::trim).collect::<Vec<_>>())
        .filter(|c| {
            c.len() == 9 && c[2].starts_with("https://www.atlassian.com/agile/project-management")
        })
        .collect()
}
fn coverage(manifest: &str, report: &str) -> Result<usize, String> {
    let mut expected = BTreeMap::new();
    for line in manifest.lines() {
        let c: Vec<_> = line.split('\t').collect();
        if c.len() < 5 || c[2] != "0" || c[3] != "200" || c[1] != c[4] {
            return Err(format!("fetch record invalid: {line}"));
        }
        if expected.insert(c[0], c[1]).is_some() {
            return Err("duplicate manifest slug".into());
        }
    }
    let mut seen = BTreeSet::new();
    let rows = article_rows(report);
    for c in &rows {
        if expected.get(c[1]) != Some(&c[2]) {
            return Err(format!("unexpected URL {}", c[1]));
        }
        if !seen.insert(c[1]) {
            return Err("duplicate report slug".into());
        }
        if c[3..8].iter().any(|s| s.is_empty()) {
            return Err("empty finding cell".into());
        }
    }
    if seen.len() != expected.len() {
        return Err(format!("coverage {} != {}", seen.len(), expected.len()));
    }
    Ok(rows.len())
}
#[test]
fn all_sources_have_findings_and_exact_quotes() {
    let r = root();
    let manifest = fs::read_to_string(r.join("docs/spikes/ST-019-source-manifest.tsv")).unwrap();
    let report = fs::read_to_string(r.join("docs/spikes/ST-019-atlassian-articles.md")).unwrap();
    let count = coverage(&manifest, &report).unwrap();
    assert!(count > 0);
    let mut quotes = 0;
    for row in article_rows(&report) {
        let text =
            fs::read_to_string(format!("/tmp/story-atlassian-research/{}.txt", row[1])).unwrap();
        assert!(text.contains(row[3]), "title {}", row[1]);
        let quote = row[5].trim_matches(|c| c == '“' || c == '”' || c == '\"');
        assert!(text.contains(quote), "quote {}", row[1]);
        quotes += 1;
    }
    assert_eq!(count, quotes);
    println!("SOURCE_COVERAGE={count} EXACT_QUOTES={quotes}");
}
#[test]
fn coverage_rejects_missing_duplicate_and_wrong_url() {
    let m="x\thttps://www.atlassian.com/agile/project-management/x\t0\t200\thttps://www.atlassian.com/agile/project-management/x\t\n";
    let row="| x | https://www.atlassian.com/agile/project-management/x | Title | Finding | Quote | Application | Class |\n";
    assert_eq!(coverage(m, row), Ok(1));
    assert!(coverage(m, "").is_err());
    assert!(coverage(m, &row.repeat(2)).is_err());
    assert!(coverage(m, &row.replace("management/x", "management/wrong")).is_err());
    assert!(coverage(&m.replace("\t200\t", "\t404\t"), row).is_err());
}
#[test]
fn all_repository_skills_have_ownership_and_new_names_are_not_installed() {
    let r = root();
    let report = fs::read_to_string(r.join("docs/spikes/ST-019-atlassian-lifecycle.md")).unwrap();
    let mut count = 0;
    for entry in fs::read_dir(r.join("skills")).unwrap() {
        let entry = entry.unwrap();
        if entry.path().is_dir() {
            let name = entry.file_name().into_string().unwrap();
            assert!(entry.path().join("SKILL.md").is_file());
            assert!(report.contains(&format!("| `{name}` |")), "missing {name}");
            count += 1;
        }
    }
    assert_eq!(count, 17);
    let names = [
        "project-discovery",
        "sprint-planning",
        "flow-review",
        "sprint-review",
        "release-planning",
        "retrospective",
        "agile-metrics",
        "project-status",
    ];
    for name in names {
        assert!(!r.join("skills").join(name).exists());
        assert!(
            report.contains(&format!("| `{name}` | New.")),
            "missing new contract {name}"
        );
    }
    assert!(report.contains("| `refinement` | Replacement"));
    println!(
        "EXISTING_SKILL_OWNERS={count} PROPOSED_NEW_SKILLS={} REPLACEMENT_ENTRY_POINTS=1",
        names.len()
    );
}
