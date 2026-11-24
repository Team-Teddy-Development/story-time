//! Hosted issue comments and resolution markers authenticated by the bot's numeric user ID.

use super::{
    caller::Caller,
    fail, forge_auth, invalid,
    registry::{self, Forge, RegistryRepo},
    workspace::Workspace,
    Server, ToolError,
};
use reqwest::{Method, Url};
use serde_json::{json, Map, Value};
use std::{collections::HashMap, num::NonZeroU64};

const MARKER: &str = "<!-- story-time:v1:resolves=";
const RESOLVED: &str = "Resolved: ";

fn unavailable(operation: &str, reason: String) -> ToolError {
    ToolError {
        code: "forge_unavailable",
        message: format!("Could not {operation}. Try again later."),
        details: json!({"operation": operation, "reason": reason}),
    }
}

fn no_issue(id: &str, forge: &str, reason: &str) -> ToolError {
    ToolError {
        code: "no_tracker_issue",
        message: format!("{id} has no {forge} issue that this server can read."),
        details: json!({"id": id, "reason": reason}),
    }
}

/// The owner, name, and number in `https://<host>/<owner>/<name>/issues/<number>`.
fn issue_of(url: &str) -> Option<(&str, &str, NonZeroU64)> {
    let (_, path) = url.strip_prefix("https://")?.split_once('/')?;
    issue_path(path)
}

fn issue_path(path: &str) -> Option<(&str, &str, NonZeroU64)> {
    let mut steps = path.split('/');
    let (owner, name, kind, number) = (steps.next()?, steps.next()?, steps.next()?, steps.next()?);
    (kind == "issues" && steps.next().is_none()).then_some((owner, name, number.parse().ok()?))
}

fn web_url(raw: &str) -> Option<Url> {
    let (scheme, rest) = raw.split_once("://")?;
    let (authority, path) = rest.split_once('/')?;
    if !matches!(scheme, "http" | "https")
        || authority.is_empty()
        || authority.contains('@')
        || raw.contains(|c: char| c.is_whitespace() || c.is_control() || c == '\\')
    {
        return None;
    }
    let url = Url::parse(raw).ok()?;
    (url.host().is_some()
        && url.query().is_none()
        && url.fragment().is_none()
        && url.path().strip_prefix('/') == Some(path))
    .then_some(url)
}

struct CommentTarget<'a> {
    repository: &'a RegistryRepo,
    issue_url: String,
    endpoint: Endpoint,
}

enum Endpoint {
    GitHub(NonZeroU64),
    Gitea { api_base: String, comments: String },
}

impl<'a> CommentTarget<'a> {
    fn new(repository: &'a RegistryRepo, issue_url: String, id: &str) -> Result<Self, ToolError> {
        let forge = repository.forge;
        let endpoint = match forge {
            Forge::GitHub => {
                let (owner, name, number) = issue_of(&issue_url).ok_or_else(|| {
                    no_issue(
                        id,
                        "GitHub",
                        "the story has no GitHub issue URL in remote_url",
                    )
                })?;
                let (own, named) = forge_auth::repository(&repository.clone)
                    .map_err(|reason| unavailable("name the repository", reason))?;
                if !owner.eq_ignore_ascii_case(&own) || !name.eq_ignore_ascii_case(&named) {
                    return Err(no_issue(
                        id,
                        "GitHub",
                        "the issue is not in the registered repository of the story",
                    ));
                }
                Endpoint::GitHub(number)
            }
            Forge::Gitea => Self::gitea_endpoint(repository, &issue_url).ok_or_else(|| {
                no_issue(
                    id,
                    "Gitea",
                    "remote_url must match the registered Gitea origin, deployment path, and repository",
                )
            })?,
        };
        Ok(Self {
            repository,
            issue_url,
            endpoint,
        })
    }

    fn gitea_endpoint(repository: &RegistryRepo, issue_url: &str) -> Option<Endpoint> {
        let clone = web_url(&repository.clone)?;
        if clone.scheme() == "http"
            && !repository.allow_http
            && !registry::is_loopback(clone.host_str()?)
        {
            return None;
        }
        let issue = web_url(issue_url)?;
        if clone.origin() != issue.origin() {
            return None;
        }
        let (own, named) = forge_auth::repository(&repository.clone).ok()?;
        let prefix = clone.path().trim_end_matches('/').rsplitn(3, '/').nth(2)?;
        let path = issue.path().strip_prefix(prefix)?.strip_prefix('/')?;
        let (owner, name, number) = issue_path(path)?;
        if !owner.eq_ignore_ascii_case(&own) || !name.eq_ignore_ascii_case(&named) {
            return None;
        }
        let origin = clone.origin().ascii_serialization();
        let api_base = format!("{origin}{prefix}/api/v1");
        let comments = format!("{api_base}/repos/{own}/{named}/issues/{number}/comments");
        Some(Endpoint::Gitea { api_base, comments })
    }

    fn key(&self) -> &str {
        &self.repository.key
    }

    fn gitea<'w>(&self, workspace: &'w Workspace) -> Result<&'w forge_auth::Gitea, ToolError> {
        workspace.gitea().ok_or_else(|| {
            fail(
                "forge_not_supported",
                "This server has no Gitea token for issue comments.",
            )
        })
    }

    fn comment_url(&self, comment: &Value) -> Value {
        match self.endpoint {
            Endpoint::GitHub(_) => comment["html_url"].clone(),
            Endpoint::Gitea { .. } => {
                json!(format!("{}#issuecomment-{}", self.issue_url, comment["id"]))
            }
        }
    }
}

/// What a bot reply resolves: the comment id, the `updated_at` that the comment had, and the
/// note. A reply is a marker only in the exact form that `resolve` writes.
fn marker(body: &str, forge: Forge) -> Option<(u64, &str, &str)> {
    let mut lines = body.lines().filter(|line| !line.trim().is_empty());
    let note = lines.next()?.strip_prefix(RESOLVED)?;
    let (id, updated_at) = lines
        .next_back()?
        .strip_prefix(MARKER)?
        .strip_prefix(forge.name())?
        .strip_prefix(':')?
        .strip_suffix(" -->")?
        .split_once(':')?;
    Some((id.parse().ok()?, updated_at, note))
}

fn target<'a>(
    server: &Server,
    workspace: &'a Workspace,
    args: &Map<String, Value>,
) -> Result<CommentTarget<'a>, ToolError> {
    let repo = server.open(args)?;
    let id = args["id"].as_str().unwrap_or_default();
    let (story, _) = repo.story(id)?;
    let registered = workspace.repository(&repo.root.key).ok_or_else(|| {
        fail(
            "repository_not_allowed",
            "That repository is not allowlisted.",
        )
    })?;
    CommentTarget::new(registered, story.get("remote_url").string(), id)
}

fn gitea_comments(api: &forge_auth::Gitea, endpoint: &str) -> Result<Vec<Value>, String> {
    let mut comments = Vec::new();
    for page in 1..=100 {
        let items = api.api(
            Method::GET,
            format!("{endpoint}?page={page}&limit=50"),
            None,
        )?;
        let Value::Array(items) = items else {
            return Err("Gitea returned an invalid comment list".into());
        };
        if items.is_empty() {
            return Ok(comments);
        }
        comments.extend(items);
    }
    Err("the issue comment list exceeded the Gitea page limit".into())
}

fn read(workspace: &Workspace, target: &CommentTarget<'_>) -> Result<Vec<Value>, ToolError> {
    let (bot, bot_id, raw) = match &target.endpoint {
        Endpoint::GitHub(issue) => {
            let (bot, bot_id) = workspace
                .bot(target.key())
                .map_err(|reason| unavailable("identify the bot", reason))?;
            let raw = workspace
                .forge(target.key(), |forge, access| {
                    forge.comments(access, issue.get())
                })
                .map_err(|reason| unavailable("read the comments", reason))?;
            (bot, bot_id, raw)
        }
        Endpoint::Gitea { api_base, comments } => {
            let api = target.gitea(workspace)?;
            let user = api
                .api(Method::GET, format!("{api_base}/user"), None)
                .map_err(|reason| unavailable("identify the bot", reason))?;
            let bot_id = user["id"].as_u64().filter(|id| *id > 0);
            let login = user["login"].as_str().filter(|login| !login.is_empty());
            let (Some(bot_id), Some(login)) = (bot_id, login) else {
                return Err(unavailable(
                    "identify the bot",
                    "Gitea returned an invalid token user".into(),
                ));
            };
            let raw = gitea_comments(api, comments)
                .map_err(|reason| unavailable("read the comments", reason))?;
            (login.to_owned(), bot_id, raw)
        }
    };
    let forge = target.repository.forge;
    let text = |value: &Value| value.as_str().unwrap_or_default().to_owned();
    // The last marker for each comment, in the order of the replies.
    let mut resolved: HashMap<u64, (String, Value)> = HashMap::new();
    let mut comments = Vec::new();
    for comment in &raw {
        let body = text(&comment["body"]);
        match marker(&body, forge).filter(|_| comment["user"]["id"].as_u64() == Some(bot_id)) {
            Some((target, updated_at, note)) => {
                let state = json!({"by": bot, "at": comment["created_at"], "note": note});
                resolved.insert(target, (updated_at.to_owned(), state));
            }
            None => comments.push(json!({
                "id": comment["id"],
                "url": target.comment_url(comment),
                "author": comment["user"]["login"],
                "author_association": if forge == Forge::Gitea && comment["author_association"].is_null() {
                    json!("UNKNOWN")
                } else {
                    comment["author_association"].clone()
                },
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
    Ok(comments)
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
    let workspace = workspace(server)?;
    let target = target(server, workspace, args)?;
    let comments = read(workspace, &target)?;
    Ok(json!({
        "repository": target.key(),
        "id": args["id"],
        "issue_url": target.issue_url,
        "comments": comments,
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
    let target = target(server, workspace, args)?;
    let forge = target.repository.forge;
    let _lock = if forge == Forge::Gitea {
        Some(
            workspace
                .lock_write(target.key())
                .map_err(|reason| unavailable("lock the repository", reason))?,
        )
    } else {
        None
    };
    let comments = read(workspace, &target)?;
    let details = json!({"id": id, "comment_id": comment_id});
    let comment = comments
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
        let reason = if forge == Forge::GitHub {
            "GitHub"
        } else {
            "Gitea"
        };
        let reason = format!("{reason} returned no update time for the comment");
        return Err(unavailable("read the comment", reason));
    }
    let client = caller.client_id.as_deref().unwrap_or("local");
    let body = format!(
        "{RESOLVED}{note}\n\nIn reply to {}\nRequested-by: {client}\n\n{MARKER}{}:{comment_id}:{updated_at} -->",
        text("url"), forge.name(),
    );
    let reply = match &target.endpoint {
        Endpoint::GitHub(issue) => workspace
            .forge(target.key(), |forge, access| {
                forge.post_comment(access, issue.get(), &body)
            })
            .map_err(|reason| unavailable("post the reply", reason))?,
        Endpoint::Gitea { comments, .. } => {
            let api = target.gitea(workspace)?;
            api.api(Method::POST, comments.clone(), Some(json!({"body": body})))
                .and_then(|reply| match reply["id"].as_u64() {
                    Some(id) if id > 0 => Ok(reply),
                    _ => Err("Gitea returned no reply id".into()),
                })
                .map_err(|reason| ToolError {
                    code: "reply_not_confirmed",
                    message:
                        "Could not confirm the Gitea reply. Read the comments before retrying."
                            .into(),
                    details: json!({"reason": reason}),
                })?
        }
    };
    Ok(json!({
        "repository": target.key(),
        "id": id,
        "comment_id": comment_id,
        "reply_url": target.comment_url(&reply),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_issue_url_names_the_repository_and_the_number() {
        assert_eq!(
            issue_of("https://github.com/up/core/issues/12"),
            Some(("up", "core", NonZeroU64::new(12).unwrap()))
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
    fn a_remote_http_gitea_comment_target_requires_allow_http() {
        let mut repository = RegistryRepo {
            key: "core".into(),
            org: "up".into(),
            clone: "http://forge.example:3000/up/core.git".into(),
            forge: Forge::Gitea,
            allow_http: false,
            default_branch: None,
            prefixes: vec!["ST".into()],
        };
        let issue = "http://forge.example:3000/up/core/issues/12";
        assert!(CommentTarget::gitea_endpoint(&repository, issue).is_none());
        repository.allow_http = true;
        let Some(Endpoint::Gitea { api_base, comments }) =
            CommentTarget::gitea_endpoint(&repository, issue)
        else {
            panic!("the registry opted in to plain HTTP");
        };
        assert_eq!(api_base, "http://forge.example:3000/api/v1");
        assert_eq!(
            comments,
            "http://forge.example:3000/api/v1/repos/up/core/issues/12/comments"
        );
    }

    #[test]
    fn a_marker_is_only_the_exact_reply_form() {
        let reply = "Resolved: Added a criterion\n\nIn reply to https://x\nRequested-by: a\n\n<!-- story-time:v1:resolves=github:101:2026-11-10T09:00:00Z -->";
        assert_eq!(
            marker(reply, Forge::GitHub),
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
            assert_eq!(marker(other, Forge::GitHub), None, "{other:?}");
        }
    }
}
