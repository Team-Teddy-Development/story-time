//! Writes every assigned placement as one saga: each step saves one story after taking its
//! original bytes; when a step fails, every earlier step is compensated, newest first, by
//! restoring those bytes. The caller holds the backlog write lock.

use super::classifier::Source;
use super::placement::{round2, Outcome, Placement};
use super::repository::StoryRepository;
use crate::lenient_story::{append_changelog, Js};

/// The changelog line for one placement, in `story set`'s `set k=v, k=v — note` form.
pub(crate) fn line(p: &Placement) -> Option<String> {
    let engine = match p.source.as_ref()? {
        Source::Jev { model } => model.clone(),
        Source::Code { .. } => "code keywords".to_owned(),
    };
    let epic = p.epic.as_ref()?;
    let feature = p.feature.as_ref()?;
    Some(format!(
        "set epic={}, feature={} — grouped by {engine} (epic {}, feature {})",
        epic.name.as_deref()?,
        feature.name.as_deref()?,
        round2(epic.confidence),
        round2(feature.confidence)
    ))
}

/// The story with the placement's epic and feature and its changelog line.
pub(crate) fn placed(story: &Js, p: &Placement, date: &str) -> Option<Js> {
    let line = line(p)?;
    let mut next = story.props().to_vec();
    Js::set(&mut next, "epic", Js::Str(p.epic.as_ref()?.name.clone()?));
    Js::set(
        &mut next,
        "feature",
        Js::Str(p.feature.as_ref()?.name.clone()?),
    );
    Some(append_changelog(&Js::Obj(next), &line, date))
}

/// Applies every `Assigned` placement. Returns the written ids, or the failed step's error
/// after compensation, naming any story it could not restore.
pub(crate) fn apply(
    repo: &mut dyn StoryRepository,
    work: &[(Js, Placement)],
    date: &str,
) -> Result<Vec<String>, String> {
    let mut done: Vec<(String, Vec<u8>)> = Vec::new();
    for (story, p) in work.iter().filter(|(_, p)| p.outcome == Outcome::Assigned) {
        let step = format!("write {}", p.id);
        let result = repo.snapshot(&p.id).and_then(|original| {
            let next = placed(story, p, date)
                .ok_or_else(|| format!("{} has no complete placement", p.id))?;
            repo.save(&next)?;
            done.push((p.id.clone(), original));
            Ok(())
        });
        if let Err(error) = result {
            return Err(compensate(repo, &step, &error, &done));
        }
    }
    Ok(done.into_iter().map(|(id, _)| id).collect())
}

/// Restores the written stories newest first and reports which were and were not restored.
fn compensate(
    repo: &mut dyn StoryRepository,
    step: &str,
    error: &str,
    done: &[(String, Vec<u8>)],
) -> String {
    let (mut restored, mut lost) = (Vec::new(), Vec::new());
    for (id, bytes) in done.iter().rev() {
        match repo.restore(id, bytes) {
            Ok(()) => restored.push(id.clone()),
            Err(e) => lost.push(format!("{id} ({e})")),
        }
    }
    let mut message = format!("Grouping failed at \"{step}\": {error}");
    if !restored.is_empty() {
        message.push_str(&format!("; restored {}", restored.join(", ")));
    }
    if !lost.is_empty() {
        message.push_str(&format!("\nCould not restore: {}", lost.join("; ")));
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::group::questions::Pick;
    use std::collections::BTreeMap;

    struct Memory {
        files: BTreeMap<String, Vec<u8>>,
        fail_on_save: Option<usize>,
        saves: usize,
    }
    impl StoryRepository for Memory {
        fn snapshot(&self, id: &str) -> Result<Vec<u8>, String> {
            self.files
                .get(id)
                .cloned()
                .ok_or_else(|| format!("{id} missing"))
        }
        fn save(&mut self, story: &Js) -> Result<(), String> {
            self.saves += 1;
            if self.fail_on_save == Some(self.saves) {
                return Err("disk full".into());
            }
            let id = story.get("id").string();
            self.files
                .insert(id, crate::lenient_story::serialize(story).into_bytes());
            Ok(())
        }
        fn restore(&mut self, id: &str, bytes: &[u8]) -> Result<(), String> {
            self.files.insert(id.into(), bytes.to_vec());
            Ok(())
        }
    }

    fn file(id: &str) -> String {
        format!("---\nid: {id}\ntitle: Work {id}\nstatus: refined\ntype: story\nepic: VC-CORE\n---\n\n## Story\nAs a maintainer, I want it.\n\n## Changelog\n- 2026-11-01 created\n")
    }
    fn placement(id: &str, outcome: Outcome) -> Placement {
        Placement {
            id: id.into(),
            title: format!("Work {id}"),
            current_epic: Some("VC-CORE".into()),
            current_feature: None,
            epic: Some(Pick {
                name: Some("Core cutover".into()),
                confidence: 0.991,
            }),
            feature: Some(Pick {
                name: Some("Engine call cutovers".into()),
                confidence: 0.876,
            }),
            source: Some(Source::Jev {
                model: "jev-1.14.0".into(),
            }),
            outcome,
        }
    }
    fn setup(fail_on_save: Option<usize>) -> (Memory, Vec<(Js, Placement)>) {
        let mut files = BTreeMap::new();
        let mut work = Vec::new();
        for (id, outcome) in [
            ("VC-001", Outcome::Assigned),
            ("VC-002", Outcome::Unresolved("low".into())),
            ("VC-003", Outcome::Assigned),
        ] {
            files.insert(id.to_owned(), file(id).into_bytes());
            work.push((
                crate::lenient_story::parse(&file(id)).unwrap(),
                placement(id, outcome),
            ));
        }
        (
            Memory {
                files,
                fail_on_save,
                saves: 0,
            },
            work,
        )
    }

    #[test]
    fn assigned_placements_gain_names_and_a_changelog_line_others_are_untouched() {
        let (mut repo, work) = setup(None);
        let before = repo.files["VC-002"].clone();
        let written = apply(&mut repo, &work, "2026-11-27").unwrap();
        assert_eq!(written, ["VC-001", "VC-003"]);
        let text = String::from_utf8(repo.files["VC-001"].clone()).unwrap();
        assert!(
            text.contains("\nepic: Core cutover\nfeature: Engine call cutovers\n"),
            "{text}"
        );
        assert!(text.contains("- 2026-11-27 set epic=Core cutover, feature=Engine call cutovers — grouped by jev-1.14.0 (epic 0.99, feature 0.88)"), "{text}");
        assert_eq!(repo.files["VC-002"], before);
    }

    #[test]
    fn a_failed_write_restores_every_earlier_story_byte_for_byte() {
        let (mut repo, work) = setup(Some(2));
        let before = repo.files.clone();
        let error = apply(&mut repo, &work, "2026-11-27").unwrap_err();
        assert_eq!(
            error,
            "Grouping failed at \"write VC-003\": disk full; restored VC-001"
        );
        assert_eq!(repo.files, before);
    }
}
