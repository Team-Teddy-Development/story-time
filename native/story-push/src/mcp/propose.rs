//! The writes of a registry server (ADR 0007). The server has no working tree, so a write does
//! not change a file in place. It makes one commit on the branch `story-time/<ID>`, pushes the
//! branch only when the remote is where the server last saw it, and keeps one pull request to
//! the default branch open for it. The story logic is that of the local write tools, run on a
//! directory that holds the story files of the base commit.

use super::{
    accept_in, blocked,
    caller::Caller,
    create_in, fail, io_error, note_in, refused, registry, update_in,
    workspace::{Pushed, Workspace, Writing},
    Root, Server, ToolError,
};
use crate::{lenient_story::Js, next_id, valid_id};
use serde_json::{json, Map, Value};
use std::path::Path;

/// Whether a registry server proposes the change of this tool.
pub(super) fn handles(tool: &str) -> bool {
    matches!(
        tool,
        "story_update"
            | "story_note"
            | "story_create"
            | "story_accept"
            | "story_mark_blocked"
            | "story_clear_blocked"
        // story_propose_unblocks writes multiple files; local MCP only until multi-file propose lands.
    )
}

/// The branch that holds the proposed changes of story `id`.
pub(super) fn branch(id: &str) -> String {
    format!("story-time/{id}")
}

/// Adds what a proposed write returns to the output schema of a write tool.
pub(super) fn schema(output: &mut Value) {
    let text = json!({"type": "string"});
    for (key, kind) in [
        ("state", json!({"enum": ["proposed", "unchanged"]})),
        ("pr_url", json!({"type": ["string", "null"]})),
        ("branch", text.clone()),
        ("head_sha", text),
    ] {
        output["properties"][key] = kind;
        output["required"].as_array_mut().unwrap().push(json!(key));
    }
}

/// The schema of `proposed` in a `story_show` result.
pub(super) fn proposed_schema() -> Value {
    let text = json!({"type": "string"});
    json!({
        "type": ["object", "null"],
        "additionalProperties": false,
        "required": ["ref", "content_sha256", "pr_url"],
        "properties": {"ref": text, "content_sha256": text, "pr_url": text},
    })
}

fn unavailable(operation: &str, reason: String) -> ToolError {
    ToolError {
        code: "forge_unavailable",
        message: format!("Could not {operation}. Nothing is proposed. Try again later."),
        details: json!({"operation": operation, "reason": reason}),
    }
}

fn git_error(operation: &str, reason: String) -> ToolError {
    let mut error = io_error(operation);
    error.details["reason"] = json!(reason);
    error
}

/// Where one write starts and what it must find at the remote.
struct Base {
    /// The commit that the new commit is on top of.
    commit: String,
    /// The branch that `commit` is the head of, which a conflict names.
    name: String,
    /// Where the story branch is at the remote now, when it exists.
    lease: Option<String>,
    /// The open pull request of the story branch.
    pull: Option<String>,
}

/// The base of a write of story `id`. A story branch with an open pull request is the base, so
/// that the changes of one story collect in one pull request. A branch whose pull request is
/// closed or merged is replaced: the write starts again from the default branch.
fn base(writing: &Writing, id: Option<&str>, default_head: &str) -> Result<Base, ToolError> {
    let fresh = |lease| Base {
        commit: default_head.to_owned(),
        name: writing.default_branch.clone(),
        lease,
        pull: None,
    };
    let Some(branch) = id.map(branch) else {
        return Ok(fresh(None));
    };
    let Some(head) = writing.head(&branch) else {
        return Ok(fresh(None));
    };
    let pull = writing
        .open_pull(&branch)
        .map_err(|reason| unavailable("read the pull request", reason))?;
    Ok(match pull {
        Some(url) => Base {
            commit: head.clone(),
            name: branch,
            lease: Some(head),
            pull: Some(url),
        },
        None => fresh(Some(head)),
    })
}

/// The highest number that a story id with `prefix` has on any branch.
fn floor(writing: &Writing, backlog: &str, prefix: &str) -> Result<f64, ToolError> {
    let ids = writing
        .ids(backlog)
        .map_err(|reason| git_error("list story ids", reason))?;
    Ok(ids
        .iter()
        .filter_map(|id| next_id::number(id, prefix))
        .fold(0.0, f64::max))
}

pub(super) fn run(
    server: &Server,
    workspace: &Workspace,
    caller: &Caller,
    tool: &str,
    args: &Map<String, Value>,
) -> Result<Value, ToolError> {
    let key = server.key(args)?;
    let named = args.get("id").and_then(Value::as_str);
    if named.is_some_and(|id| !valid_id(id) || !registry::branch_ok(&branch(id))) {
        return Err(fail("invalid_story_id", "That is not a story id."));
    }
    let writing = workspace
        .write(&key)
        .map_err(|refusal| refused(refusal, &key, None))?;
    // A new story can lose its id to a push from elsewhere. It then takes the next id, once.
    let attempts = if named.is_some() { 1 } else { 2 };
    for attempt in 1..=attempts {
        let default_head = writing
            .head(&writing.default_branch)
            .ok_or_else(|| git_error("read the default branch", "no head".into()))?;
        let base = base(&writing, named, &default_head)?;
        let tree = writing
            .scratch(&base.commit)
            .map_err(|reason| git_error("read the base commit", reason))?;
        let repo = server.repo_at(Root {
            key: key.clone(),
            path: tree.path.clone(),
            source: None,
        })?;
        let backlog = repo
            .dir
            .strip_prefix(&repo.root.path)
            .unwrap_or(Path::new("backlog"))
            .to_string_lossy()
            .into_owned();
        let written = match tool {
            "story_update" => update_in(server, &repo, args),
            "story_note" => note_in(&repo, args),
            "story_accept" => accept_in(&repo, args),
            "story_mark_blocked" => blocked::mark_in(&repo, args),
            "story_clear_blocked" => blocked::clear_in(&repo, args),
            _ => {
                let prefix = repo.settings["prefix"].as_str().unwrap_or("ST");
                create_in(&repo, args, floor(&writing, &backlog, prefix)?)
            }
        };
        let mut result = written.map_err(|mut error| {
            if error.code == "conflict" {
                error.details["ref"] = json!(base.name);
            }
            error
        })?;
        result["publish"]["next"] = Value::Null;
        let id = result["story"]["id"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let branch = branch(&id);
        if result["status"] == "already_applied" {
            result["state"] = json!("unchanged");
            result["pr_url"] = json!(base.pull);
            result["branch"] = json!(base.name);
            result["head_sha"] = json!(base.commit);
            return Ok(result);
        }
        let (story, bytes) = repo.story(&id)?;
        let client = caller.client_id.as_deref().unwrap_or("local");
        let subject = match tool {
            "story_update" => "update the story",
            "story_note" => "add a note",
            "story_accept" => "record acceptance",
            "story_mark_blocked" => "mark the story blocked",
            "story_clear_blocked" => "clear the blocked label",
            _ => "create the story",
        };
        let message = format!("story({id}): {subject}\n\nRequested-by: {client}\n");
        let path = format!("{backlog}/{id}.md");
        let commit = writing
            .commit(&base.commit, &path, &bytes, &message, &writing.author())
            .map_err(|reason| git_error("make the commit", reason))?;
        let pushed = writing
            .push(&branch, &commit, base.lease.as_deref())
            .map_err(|reason| unavailable("push the story branch", reason))?;
        if matches!(pushed, Pushed::Rejected) {
            if attempt < attempts {
                writing
                    .fetch()
                    .map_err(|refusal| refused(refusal, &key, None))?;
                continue;
            }
            return Err(ToolError {
                code: if named.is_some() {
                    "conflict"
                } else {
                    "id_taken"
                },
                message: format!("The branch of {id} changed at the remote. Read the story again."),
                details: json!({"id": id, "ref": branch}),
            });
        }
        let pull = match base.pull {
            Some(url) => url,
            None => {
                let title = match story.get("title") {
                    Js::Undefined => id.clone(),
                    title => format!("{id}: {}", title.string()),
                };
                let issue = story.get("remote").string();
                let line = if !issue.is_empty() && issue.bytes().all(|b| b.is_ascii_digit()) {
                    format!("Stories: {id} (#{issue})")
                } else {
                    format!("Stories: {id}")
                };
                let body = format!(
                    "{line}\n\nstory-time proposes a change to `{path}`. Client `{client}` asked for it through the story-time MCP server.\n"
                );
                match writing.create_pull(&branch, &title, &body) {
                    Ok(url) => url,
                    Err(reason) => {
                        // A branch with no pull request is not a proposal. Remove it.
                        let _ = writing.delete(&branch, &commit);
                        return Err(unavailable("open the pull request", reason));
                    }
                }
            }
        };
        writing.remember(&branch, &commit, &pull);
        result["state"] = json!("proposed");
        result["pr_url"] = json!(pull);
        result["branch"] = json!(branch);
        result["head_sha"] = json!(commit);
        return Ok(result);
    }
    Err(fail(
        "id_taken",
        "Another writer took each id this call allocated.",
    ))
}
