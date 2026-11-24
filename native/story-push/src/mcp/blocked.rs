//! Blocked-label lifecycle: mark, propose unblock children, clear, and auto-clear.

use super::{
    conflict, fail, field_error, invalid, io_error, sha, summary, Repo, Server, ToolError, TYPES,
};
use crate::{
    blocked::{depends_on, ensure_blocked, has_blocked, labels, remove_blocked},
    lenient_story::Js,
    list, new_story, note as note_mod,
};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};

pub(super) const MAX_DEPTH: usize = 2;
const BLOCKER_KINDS: &[&str] = &["dependency", "access", "ambiguity", "external", "other"];

fn relates_to(story: &Js) -> Vec<String> {
    match story.get("relates_to") {
        Js::Undefined => Vec::new(),
        Js::Arr(items) => items.iter().map(Js::string).collect(),
        other => vec![other.string()],
    }
}

fn by_id<'a>(stories: &'a [Js], id: &str) -> Option<&'a Js> {
    stories.iter().rev().find(|s| s.get("id").string() == id)
}

/// How deep this story sits in a blocked relates_to chain (0 = not under a blocked parent).
pub(super) fn unblock_depth(id: &str, stories: &[Js]) -> Result<usize, ToolError> {
    fn walk(
        id: &str,
        stories: &[Js],
        active: &mut HashSet<String>,
        memo: &mut HashMap<String, usize>,
    ) -> Result<usize, ToolError> {
        if let Some(d) = memo.get(id) {
            return Ok(*d);
        }
        if !active.insert(id.to_owned()) {
            return Err(fail(
                "dependency_cycle",
                "A depends_on / relates_to cycle blocks depth checks.",
            ));
        }
        let Some(story) = by_id(stories, id) else {
            active.remove(id);
            memo.insert(id.to_owned(), 0);
            return Ok(0);
        };
        let mut depth = 0;
        for parent in relates_to(story) {
            if by_id(stories, &parent).is_some_and(has_blocked) {
                depth = depth.max(1 + walk(&parent, stories, active, memo)?);
            }
        }
        active.remove(id);
        memo.insert(id.to_owned(), depth);
        Ok(depth)
    }
    walk(id, stories, &mut HashSet::new(), &mut HashMap::new())
}

fn would_cycle(stories: &[Js], from: &str, new_deps: &[String]) -> bool {
    let mut graph: HashMap<String, Vec<String>> = HashMap::new();
    for story in stories {
        let id = story.get("id").string();
        let deps = if id == from {
            new_deps.to_vec()
        } else {
            depends_on(story)
        };
        graph.insert(id, deps);
    }
    fn visit(
        id: &str,
        graph: &HashMap<String, Vec<String>>,
        active: &mut HashSet<String>,
        done: &mut HashSet<String>,
    ) -> bool {
        if done.contains(id) {
            return false;
        }
        if !active.insert(id.to_owned()) {
            return true;
        }
        if let Some(deps) = graph.get(id) {
            for dep in deps {
                if graph.contains_key(dep) && visit(dep, graph, active, done) {
                    return true;
                }
            }
        }
        active.remove(id);
        done.insert(id.to_owned());
        false
    }
    let mut done = HashSet::new();
    graph
        .keys()
        .any(|id| visit(id, &graph, &mut HashSet::new(), &mut done))
}

fn open_unblock_deps(parent: &Js, stories: &[Js]) -> Vec<String> {
    let parent_id = parent.get("id").string();
    depends_on(parent)
        .into_iter()
        .filter(|dep| {
            by_id(stories, dep).is_some_and(|child| {
                relates_to(child).iter().any(|r| r == &parent_id)
                    && child.get("status").string() != "done"
            })
        })
        .collect()
}

fn next_steps_mark(id: &str, kind: &str) -> Vec<String> {
    vec![
        format!(
            "Call story_propose_unblocks on {id} with 1..N unblock specs that name the concrete {kind} blocker outcome. Do this yourself; do not wait idle for a groomer."
        ),
        format!(
            "After propose returns, claim and implement the created child ids next. Leave {id} until story_clear_blocked removes the blocked label."
        ),
        "If you cannot name unblock work, escalate to the owner with a note; still keep WIP released on this labeled parent.".into(),
    ]
}

fn next_steps_propose(parent: &str, created: &[String]) -> Vec<String> {
    let mut steps = Vec::new();
    if let Some(first) = created.first() {
        steps.push(format!(
            "Claim and implement {first} next (do not resume blocked parent {parent} yet)."
        ));
    }
    if created.len() > 1 {
        steps.push(format!(
            "Then implement the remaining unblock children: {}.",
            created[1..].join(", ")
        ));
    }
    steps.push(format!(
        "When all unblock deps of {parent} are done, call story_clear_blocked on {parent}, then pull {parent} again."
    ));
    steps
}

fn next_steps_clear(id: &str) -> Vec<String> {
    vec![
        format!("Pull {id} again (rank/assignment now treat it as eligible if status is ready)."),
        format!("Resume implementation on {id}; the blocked label is gone."),
    ]
}

fn next_steps_depth(id: &str) -> Vec<String> {
    vec![
        format!(
            "Blocked-unblock depth for {id} already reached the cap of {MAX_DEPTH}. Do not propose more unblock children."
        ),
        "Escalate to the owner with a story note describing the remaining blocker.".into(),
    ]
}

/// Mark a story blocked: ensure label, optionally release WIP, append note, return next_steps.
pub(super) fn mark_blocked(server: &Server, args: &Map<String, Value>) -> Result<Value, ToolError> {
    let (repo, _lock) = server.lock(args)?;
    mark_in(&repo, args)
}

pub(super) fn mark_in(repo: &Repo, args: &Map<String, Value>) -> Result<Value, ToolError> {
    let id = args["id"].as_str().unwrap_or_default();
    let expected = args["expected_sha256"].as_str().unwrap_or_default();
    let reason = args["reason"].as_str().unwrap_or_default();
    let kind = args
        .get("blocker_kind")
        .and_then(Value::as_str)
        .unwrap_or("other");
    if !BLOCKER_KINDS.contains(&kind) {
        return Err(invalid(
            "/blocker_kind",
            format!("blocker_kind must be one of: {}.", BLOCKER_KINDS.join(", ")),
        ));
    }
    let release_wip = args
        .get("release_wip")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let (current, bytes) = repo.story(id)?;
    let before = sha(&bytes);
    if before != expected {
        return Err(conflict(id, expected, &before));
    }
    let mut next = ensure_blocked(&current);
    let mut fields = vec!["labels".into()];
    if release_wip && next.get("status").string() == "in-progress" {
        let mut props = next.props().to_vec();
        Js::set(&mut props, "status", Js::Str("ready".into()));
        next = Js::Obj(props);
        fields.push("status".into());
    }
    let line = format!("Blocked: {kind} — {reason}");
    let next =
        note_mod::append(&next, &line).map_err(|m| field_error("invalid_value", "reason", m))?;
    let after = repo.save(&next)?;
    let mut result = repo.written(
        "story_mark_blocked",
        "updated",
        &next,
        &before,
        &after,
        fields,
        Some(line),
    );
    result["labels"] = json!(labels(&next));
    result["groomer_hint"] = json!("story_propose_unblocks");
    result["next_steps"] = json!(next_steps_mark(id, kind));
    Ok(result)
}

/// Propose N unblock children and wire depends_on / relates_to.
pub(super) fn propose_unblocks(
    server: &Server,
    args: &Map<String, Value>,
) -> Result<Value, ToolError> {
    let (repo, _lock) = server.lock(args)?;
    propose_in(&repo, args, 0.0)
}

pub(super) fn propose_in(
    repo: &Repo,
    args: &Map<String, Value>,
    floor: f64,
) -> Result<Value, ToolError> {
    let id = args["id"].as_str().unwrap_or_default();
    let expected = args["expected_sha256"].as_str().unwrap_or_default();
    let max_children = args
        .get("max_children")
        .and_then(Value::as_u64)
        .unwrap_or(5) as usize;
    let replace = args
        .get("replace_existing_unblocks")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let unblocks = args
        .get("unblocks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if unblocks.is_empty() {
        return Err(invalid(
            "/unblocks",
            "Provide at least one unblock spec.".into(),
        ));
    }
    if unblocks.len() > max_children {
        return Err(fail(
            "over_split",
            &format!(
                "Refusing {} unblock stories; max_children is {max_children}.",
                unblocks.len()
            ),
        ));
    }
    let (parent, bytes) = repo.story(id)?;
    let before = sha(&bytes);
    if before != expected {
        return Err(conflict(id, expected, &before));
    }
    if !has_blocked(&parent) {
        return Err(fail(
            "not_blocked",
            "Parent must carry the blocked label before proposing unblocks.",
        ));
    }
    let (stories, _) = repo.stories()?;
    let depth = unblock_depth(id, &stories)?;
    if depth >= MAX_DEPTH {
        let mut error = fail(
            "depth_exceeded",
            &format!("Blocked-unblock depth cap is {MAX_DEPTH}; escalate instead of proposing more children."),
        );
        error.details = json!({
            "id": id,
            "depth": depth,
            "max_depth": MAX_DEPTH,
            "next_steps": next_steps_depth(id),
        });
        return Err(error);
    }
    let open = open_unblock_deps(&parent, &stories);
    if !replace && !open.is_empty() {
        return Err(ToolError {
            code: "duplicate_unblocks",
            message: "Parent already depends on open unblock children; set replace_existing_unblocks true to replace that subset.".into(),
            details: json!({ "id": id, "open_unblocks": open }),
        });
    }

    let prefix = repo.settings["prefix"].as_str().unwrap_or("ST");
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let epic = match parent.get("epic") {
        Js::Undefined => None,
        e => Some(e.string()),
    };

    let mut created = Vec::new();
    let mut created_ids = Vec::new();
    let mut next_floor = floor;
    for (i, spec) in unblocks.iter().enumerate() {
        let title = spec["title"].as_str().unwrap_or_default();
        let kind = spec.get("type").and_then(Value::as_str).unwrap_or("story");
        if !TYPES.contains(&kind) {
            return Err(invalid(
                &format!("/unblocks/{i}/type"),
                format!("Unknown type \"{kind}\"."),
            ));
        }
        let mut argv = vec![
            "--title".into(),
            title.to_owned(),
            "--type".into(),
            kind.to_owned(),
        ];
        if let Some(statement) = spec.get("statement").and_then(Value::as_str) {
            argv.extend(["--statement".into(), statement.to_owned()]);
        }
        if let Some(criteria) = spec.get("acceptance_criteria").and_then(Value::as_array) {
            for ac in criteria.iter().filter_map(Value::as_str) {
                argv.extend(["--ac".into(), ac.to_owned()]);
            }
        }
        if let Some(size) = spec.get("size").and_then(Value::as_f64) {
            argv.extend(["--size".into(), crate::js_number::to_string(size)]);
        }
        if let Some(epic) = &epic {
            argv.extend(["--epic".into(), epic.clone()]);
        }
        let flags = crate::config::Flags::parse(&argv).map_err(|e| invalid("", e.message))?;
        let made = new_story::create(&flags, &repo.dir, prefix, &date, 2, next_floor).map_err(
            |e| match e {
                list::CreateError::Taken(_) => fail(
                    "id_taken",
                    "Another process took each id this call allocated.",
                ),
                list::CreateError::Other(_) => io_error("write story file"),
            },
        )?;
        let child_id = made.get("id").string();
        next_floor = next_floor.max(crate::next_id::number(&child_id, prefix).unwrap_or(0.0));
        let mut props = made.props().to_vec();
        Js::set(
            &mut props,
            "relates_to",
            Js::Arr(vec![Js::Str(id.to_owned())]),
        );
        let child = Js::Obj(props);
        list::save(&repo.dir, &child).map_err(|_| io_error("write story file"))?;
        created.push(json!({
            "id": child_id,
            "title": child.get("title").string(),
            "type": kind,
        }));
        created_ids.push(child_id);
    }

    let mut deps = depends_on(&parent);
    if replace {
        let drop: HashSet<String> = open.into_iter().collect();
        deps.retain(|d| !drop.contains(d));
    }
    for child_id in &created_ids {
        if !deps.iter().any(|d| d == child_id) {
            deps.push(child_id.clone());
        }
    }
    if would_cycle(&stories, id, &deps) {
        for child_id in &created_ids {
            let path = repo.dir.join(format!("{child_id}.md"));
            let _ = std::fs::remove_file(path);
        }
        return Err(fail(
            "dependency_cycle",
            "Proposing these unblocks would create a depends_on cycle; nothing was written.",
        ));
    }

    let mut props = parent.props().to_vec();
    Js::set(
        &mut props,
        "depends_on",
        Js::Arr(deps.iter().cloned().map(Js::Str).collect()),
    );
    let parent_next = ensure_blocked(&Js::Obj(props));
    let line = format!("Proposed unblocks: {}", created_ids.join(", "));
    let parent_next =
        note_mod::append(&parent_next, &line).map_err(|m| fail("invalid_value", &m))?;
    let after = repo.save(&parent_next)?;

    Ok(json!({
        "status": "updated",
        "parent": {
            "id": id,
            "depends_on": deps,
            "labels": labels(&parent_next),
            "sha256": after,
        },
        "created": created,
        "sha256_before": before,
        "sha256_after": after,
        "story": summary(&parent_next, Some(&after)),
        "audit": {
            "operation": "story_propose_unblocks",
            "repository": repo.root.key,
            "id": id,
            "fields": ["depends_on", "labels"],
            "changelog": line,
            "at": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            "server_version": env!("CARGO_PKG_VERSION"),
        },
        "publish": {
            "sync": repo.settings["sync"].as_str().unwrap_or("off"),
            "tracker": repo.settings["tracker"],
            "pushed": false,
            "next": (repo.settings["sync"].as_str().unwrap_or("off") != "off")
                .then(|| format!("story push {id}")),
        },
        "next_steps": next_steps_propose(id, &created_ids),
        "redactions": 0,
    }))
}

/// Remove the blocked label when deps are done (or force).
pub(super) fn clear_blocked(
    server: &Server,
    args: &Map<String, Value>,
) -> Result<Value, ToolError> {
    let (repo, _lock) = server.lock(args)?;
    clear_in(&repo, args)
}

pub(super) fn clear_in(repo: &Repo, args: &Map<String, Value>) -> Result<Value, ToolError> {
    let id = args["id"].as_str().unwrap_or_default();
    let expected = args["expected_sha256"].as_str().unwrap_or_default();
    let force = args.get("force").and_then(Value::as_bool).unwrap_or(false);
    let (current, bytes) = repo.story(id)?;
    let before = sha(&bytes);
    if before != expected {
        return Err(conflict(id, expected, &before));
    }
    if !has_blocked(&current) {
        let after = before.clone();
        let mut result = repo.written(
            "story_clear_blocked",
            "already_applied",
            &current,
            &before,
            &after,
            vec!["labels".into()],
            None,
        );
        result["next_steps"] = json!(next_steps_clear(id));
        return Ok(result);
    }
    if !force {
        let (stories, _) = repo.stories()?;
        let open: Vec<String> = depends_on(&current)
            .into_iter()
            .filter(|dep| !by_id(&stories, dep).is_some_and(|s| s.get("status").string() == "done"))
            .collect();
        if !open.is_empty() {
            return Err(ToolError {
                code: "still_blocked",
                message: format!("{id} still waits on unfinished depends_on."),
                details: json!({ "id": id, "open_depends_on": open }),
            });
        }
    }
    let mut next = remove_blocked(&current);
    let note_text = args
        .get("note")
        .and_then(Value::as_str)
        .map(|n| format!("Unblocked: {n}"))
        .unwrap_or_else(|| "Unblocked: blocked label cleared".into());
    next =
        note_mod::append(&next, &note_text).map_err(|m| field_error("invalid_value", "note", m))?;
    let after = repo.save(&next)?;
    let mut result = repo.written(
        "story_clear_blocked",
        "updated",
        &next,
        &before,
        &after,
        vec!["labels".into()],
        Some(note_text),
    );
    result["labels"] = json!(labels(&next));
    result["next_steps"] = json!(next_steps_clear(id));
    Ok(result)
}
