//! The comment tools of a registry server (ADR 0007). `story_comments` lists the comments on
//! the GitHub issue that a story names in `remote_url`. `story_resolve_comment` posts one reply
//! that marks a comment resolved. GitHub issue comments have no resolved state, so the state is
//! a marker in a reply that only the App's bot user can write: the server matches the numeric
//! user id of the reply, not its login.

use super::{caller::Caller, fail, invalid, workspace::Workspace, Server, ToolError};
use serde_json::{json, Map, Value};
use std::collections::HashMap;

const MARKER: &str = "<!-- story-time:v1:resolves=github:";
const RESOLVED: &str = "Resolved: ";

fn unavailable(operation: &str, reason: String) -> ToolError {
    ToolError {
        code: "forge_unavailable",
        message: format!("Could not {operation}. Try again later."),
        details: json!({"operation": operation, "reason": reason}),
    }
}

fn no_issue(id: &str, reason: &str) -> ToolError {
    ToolError {
        code: "no_tracker_issue",
        message: format!("{id} has no GitHub issue that this server can read."),
        details: json!({"id": id, "reason": reason}),
    }
}

/// The owner, name, and number in `https://<host>/<owner>/<name>/issues/<number>`.
fn issue_of(url: &str) -> Option<(&str, &str, u64)> {
    let mut steps = url.strip_prefix("https://")?.split('/');
    let (_host, owner, name) = (steps.next()?, steps.next()?, steps.next()?);
    let (kind, number) = (steps.next()?, steps.next()?);
    let number = number.parse().ok().filter(|n| *n > 0)?;
    (kind == "issues" && steps.next().is_none()).then_some((owner, name, number))
}

/// What a bot reply resolves: the comment id, the `updated_at` that the comment had, and the
/// note. A reply is a marker only in the exact form that `resolve` writes.
fn marker(body: &str) -> Option<(u64, &str, &str)> {
    let mut lines = body.lines().filter(|line| !line.trim().is_empty());
    let note = lines.next()?.strip_prefix(RESOLVED)?;
    let (id, updated_at) = lines
        .next_back()?
        .strip_prefix(MARKER)?
        .strip_suffix(" -->")?
        .split_once(':')?;
    Some((id.parse().ok()?, updated_at, note))
}

struct Thread {
    repository: String,
    issue_url: String,
    issue: u64,
    comments: Vec<Value>,
}

/// The comments on the issue of the story that `args` names, each with its resolved state.
fn read(
    server: &Server,
    workspace: &Workspace,
    args: &Map<String, Value>,
) -> Result<Thread, ToolError> {
    let repo = server.open(args)?;
    let id = args["id"].as_str().unwrap_or_default();
    let (story, _) = repo.story(id)?;
    let key = repo.root.key.as_str();
    let issue_url = story.get("remote_url").string();
    let (owner, name, issue) = issue_of(&issue_url)
        .ok_or_else(|| no_issue(id, "the story has no GitHub issue URL in remote_url"))?;
    // The token of a repository covers only that repository's issues.
    let (own, named) = workspace
        .forge_repository(key)
        .map_err(|reason| unavailable("name the repository", reason))?;
    if !owner.eq_ignore_ascii_case(&own) || !name.eq_ignore_ascii_case(&named) {
        return Err(no_issue(
            id,
            "the issue is not in the registered repository of the story",
        ));
    }
    let (bot, bot_id) = workspace
        .bot(key)
        .map_err(|reason| unavailable("identify the bot", reason))?;
    let raw = workspace
        .forge(key, |forge, access| forge.comments(access, issue))
        .map_err(|reason| unavailable("read the comments", reason))?;
    let text = |value: &Value| value.as_str().unwrap_or_default().to_owned();
    // The last marker for each comment, in the order of the replies.
    let mut resolved: HashMap<u64, (String, Value)> = HashMap::new();
    let mut comments = Vec::new();
    for comment in &raw {
        let body = text(&comment["body"]);
        match marker(&body).filter(|_| comment["user"]["id"].as_u64() == Some(bot_id)) {
            Some((target, updated_at, note)) => {
                let state = json!({"by": bot, "at": comment["created_at"], "note": note});
                resolved.insert(target, (updated_at.to_owned(), state));
            }
            None => comments.push(json!({
                "id": comment["id"],
                "url": comment["html_url"],
                "author": comment["user"]["login"],
                "author_association": comment["author_association"],
                "created_at": comment["created_at"],
                "updated_at": comment["updated_at"],
                "body": body,
                "resolved": null,
            })),
        }
    }
    for comment in &mut comments {
        // A marker stands only while the comment is as it was when the bot answered it.
        let state = comment["id"]
            .as_u64()
            .and_then(|id| resolved.get(&id))
            .filter(|(updated_at, _)| comment["updated_at"] == updated_at.as_str());
        if let Some((_, state)) = state {
            comment["resolved"] = state.clone();
        }
    }
    Ok(Thread {
        repository: key.to_owned(),
        issue_url,
        issue,
        comments,
    })
}

fn workspace(server: &Server) -> Result<&Workspace, ToolError> {
    server
        .workspace
        .as_deref()
        .ok_or_else(|| fail("io_error", "This server has no registry."))
}

pub(super) fn story_comments(
    server: &Server,
    args: &Map<String, Value>,
) -> Result<Value, ToolError> {
    let thread = read(server, workspace(server)?, args)?;
    Ok(json!({
        "repository": thread.repository,
        "id": args["id"],
        "issue_url": thread.issue_url,
        "comments": thread.comments,
    }))
}

pub(super) fn resolve(
    server: &Server,
    caller: &Caller,
    args: &Map<String, Value>,
) -> Result<Value, ToolError> {
    let workspace = workspace(server)?;
    let id = args["id"].as_str().unwrap_or_default();
    let note = args["note"].as_str().unwrap_or_default().trim();
    if note.is_empty() {
        return Err(fail(
            "note_required",
            "A resolve needs a note that says what changed.",
        ));
    }
    // The note is one line of the reply. It cannot hold a second marker.
    if note.chars().any(char::is_control) || note.contains("<!--") || note.contains("-->") {
        let message = "/note is one line with no HTML comment.".into();
        return Err(invalid("/note", message));
    }
    let comment_id = args["comment_id"].as_u64().unwrap_or_default();
    let thread = read(server, workspace, args)?;
    let details = json!({"id": id, "comment_id": comment_id});
    let comment = thread
        .comments
        .iter()
        .find(|comment| comment["id"].as_u64() == Some(comment_id))
        .ok_or_else(|| ToolError {
            code: "comment_not_found",
            message: format!("The issue of {id} has no comment with that id."),
            details: details.clone(),
        })?;
    if !comment["resolved"].is_null() {
        return Err(ToolError {
            code: "already_resolved",
            message: "That comment is resolved.".into(),
            details,
        });
    }
    let text = |key: &str| comment[key].as_str().unwrap_or_default();
    let updated_at = text("updated_at");
    if updated_at.is_empty() || updated_at.contains(|c: char| c.is_whitespace() || c == '>') {
        let reason = "GitHub returned no update time for the comment".into();
        return Err(unavailable("read the comment", reason));
    }
    let client = caller.client_id.as_deref().unwrap_or("local");
    let body = format!(
        "{RESOLVED}{note}\n\nIn reply to {}\nRequested-by: {client}\n\n{MARKER}{comment_id}:{updated_at} -->",
        text("url")
    );
    let reply = workspace
        .forge(&thread.repository, |forge, access| {
            forge.post_comment(access, thread.issue, &body)
        })
        .map_err(|reason| unavailable("post the reply", reason))?;
    Ok(json!({
        "repository": thread.repository,
        "id": id,
        "comment_id": comment_id,
        "reply_url": reply["html_url"],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_issue_url_names_the_repository_and_the_number() {
        assert_eq!(
            issue_of("https://github.com/up/core/issues/12"),
            Some(("up", "core", 12))
        );
        for other in [
            "",
            "http://github.com/up/core/issues/12",
            "https://github.com/up/core/pull/12",
            "https://github.com/up/core/issues/0",
            "https://github.com/up/core/issues/12/extra",
            "https://github.com/up/core/issues/x",
        ] {
            assert_eq!(issue_of(other), None, "{other}");
        }
    }

    #[test]
    fn a_marker_is_only_the_exact_reply_form() {
        let reply = "Resolved: Added a criterion\n\nIn reply to https://x\nRequested-by: a\n\n<!-- story-time:v1:resolves=github:101:2026-11-10T09:00:00Z -->";
        assert_eq!(
            marker(reply),
            Some((101, "2026-11-10T09:00:00Z", "Added a criterion"))
        );
        for other in [
            "",
            "Thanks!",
            "<!-- story-time:v1:resolves=github:101:2026-11-10T09:00:00Z -->",
            "Resolved: yes\n<!-- story-time:v1:resolves=github:x:y -->",
            "Resolved: yes\n<!-- story-time:v1:resolves=github:101:t -->\nmore text",
            "Resolved: yes\n<!-- story-time:v2:resolves=github:101:t -->",
        ] {
            assert_eq!(marker(other), None, "{other:?}");
        }
    }
}
