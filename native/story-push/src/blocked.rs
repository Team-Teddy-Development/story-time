//! The `blocked` label (ST-179). A story that an agent cannot finish carries it until the stories
//! that unblock it are done; rank and assignment skip a labeled story whatever its status. Each
//! unblock story `relates_to` the blocked story, and the blocked story `depends_on` it.

use crate::lenient_story::{self, append_changelog, Js};

const BLOCKED: &str = "blocked";

pub(crate) fn labels(story: &Js) -> Vec<String> {
    match story.get("labels") {
        Js::Undefined => Vec::new(),
        Js::Arr(items) => items
            .iter()
            .filter_map(|v| match v {
                Js::Str(s) if !lenient_story::trim(s).is_empty() => Some(s.clone()),
                _ => None,
            })
            .collect(),
        other => {
            let owned = other.string();
            let text = lenient_story::trim(&owned);
            if text.is_empty() {
                Vec::new()
            } else {
                vec![text.to_owned()]
            }
        }
    }
}

pub(crate) fn has_blocked(story: &Js) -> bool {
    labels(story).iter().any(|l| l == BLOCKED)
}

fn set_labels(story: &Js, labels: Vec<String>) -> Js {
    let mut props = story.props().to_vec();
    if labels.is_empty() {
        props.retain(|(k, _)| k != "labels");
    } else {
        Js::set(
            &mut props,
            "labels",
            Js::Arr(labels.into_iter().map(Js::Str).collect()),
        );
    }
    Js::Obj(props)
}

pub(crate) fn ensure_blocked(story: &Js) -> Js {
    let mut labels = labels(story);
    if !labels.iter().any(|l| l == BLOCKED) {
        labels.push(BLOCKED.into());
    }
    set_labels(story, labels)
}

pub(crate) fn remove_blocked(story: &Js) -> Js {
    let labels: Vec<String> = labels(story).into_iter().filter(|l| l != BLOCKED).collect();
    set_labels(story, labels)
}

pub(crate) fn depends_on(story: &Js) -> Vec<String> {
    match story.get("depends_on") {
        Js::Undefined => Vec::new(),
        Js::Arr(items) => items.iter().map(Js::string).collect(),
        other => vec![other.string()],
    }
}

/// Each blocked story that the move of `done` to done leaves with no unfinished dependency,
/// with its label cleared. `stories` may still show `done` in its earlier status.
pub(crate) fn unblocked(stories: &[Js], done: &str) -> Vec<Js> {
    let finished = |id: &String| {
        id == done
            || stories
                .iter()
                .any(|s| s.get("id").string() == *id && s.get("status").string() == "done")
    };
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    stories
        .iter()
        .filter(|s| has_blocked(s) && depends_on(s).iter().any(|d| d == done))
        .filter(|s| depends_on(s).iter().all(finished))
        .map(|s| {
            let note = format!("Unblocked: {done} finished the last dependency");
            append_changelog(&remove_blocked(s), &note, &date)
        })
        .collect()
}
