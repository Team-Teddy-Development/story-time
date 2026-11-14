//! `story mcp --graph`: `story_graph_sync` projects one repository's stories into Neo4j and
//! `story_graph` reads a bounded part of that projection back (ST-011). The Markdown backlog stays
//! the source of truth; each sync replaces the repository's projection in one transaction. Nodes
//! are `Story` and `Epic`, keyed by `(repository, id)`, where `repository` is the tracker target
//! or a hash of the root, so equal story ids in two repositories never meet.

use super::{
    fail, object, summary, with_redactions, Field, Hints, Kind, Repo, Server, Tool, ToolError, ID,
    REPOSITORY,
};
use crate::{adoption::sha, valid_id, Failure};
use reqwest::{blocking::Client, StatusCode, Url};
use serde_json::{json, Map, Value};
use std::{collections::BTreeSet, time::Duration};

pub(super) const USAGE: &str =
    "[--graph --neo4j-url URL --neo4j-user USER --neo4j-password-env NAME [--neo4j-database NAME]]";

pub(super) struct Neo4j {
    endpoint: Url,
    user: String,
    pub(super) password: String,
    client: Client,
}

/// Splits the graph flags off `args`. `--graph` needs the URL, the user, and the name of the
/// variable that holds the password; the password itself never appears on the command line.
pub(super) fn launch(args: &[String]) -> Result<(Vec<String>, Option<Neo4j>), Failure> {
    let refuse = |message: String| Failure {
        code: 2,
        message: format!("story mcp: {message}"),
    };
    let mut rest = Vec::new();
    let mut graph = false;
    let mut flags = std::collections::HashMap::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--graph" => graph = true,
            "--neo4j-url" | "--neo4j-user" | "--neo4j-password-env" | "--neo4j-database" => {
                let value = args
                    .next()
                    .ok_or_else(|| refuse(format!("{arg} needs a value.")))?;
                flags.insert(&arg[2..], value.clone());
            }
            _ => rest.push(arg.clone()),
        }
    }
    if !graph {
        return match flags.keys().next() {
            Some(flag) => Err(refuse(format!("--{flag} needs --graph."))),
            None => Ok((rest, None)),
        };
    }
    let flag = |name: &str| {
        flags
            .get(name)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| refuse(format!("--graph needs --{name}.")))
    };
    let url = Url::parse(flag("neo4j-url")?)
        .ok()
        .filter(|u| matches!(u.scheme(), "http" | "https"))
        .filter(|u| u.username().is_empty() && u.password().is_none())
        .filter(|u| u.query().is_none() && u.fragment().is_none())
        .ok_or_else(|| {
            refuse(
                "--neo4j-url must be an http or https URL without credentials, query, or fragment."
                    .into(),
            )
        })?;
    let database = flags.get("neo4j-database").map_or("neo4j", String::as_str);
    if !(1..=63).contains(&database.len())
        || !database
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    {
        return Err(refuse(
            "--neo4j-database must be a Neo4j database name.".into(),
        ));
    }
    let variable = flag("neo4j-password-env")?;
    let password = std::env::var(variable)
        .ok()
        .filter(|p| !p.is_empty())
        .ok_or_else(|| {
            refuse(format!(
                "--neo4j-password-env names {variable}, which is not set."
            ))
        })?;
    let endpoint = url
        .join(&format!(
            "{}/db/{database}/tx/commit",
            url.path().trim_end_matches('/')
        ))
        .map_err(|_| refuse("--neo4j-url is not a base URL.".into()))?;
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| refuse("could not initialize the Neo4j HTTP client.".into()))?;
    Ok((
        rest,
        Some(Neo4j {
            endpoint,
            user: flag("neo4j-user")?.clone(),
            password,
            client,
        }),
    ))
}

impl Neo4j {
    /// Runs the statements in one transaction and returns each statement's rows. Errors carry
    /// a code and Neo4j's error code only, never the request, the URL, or a credential.
    fn run(&self, statements: Vec<(&str, Value)>) -> Result<Vec<Vec<Value>>, ToolError> {
        let statements: Vec<Value> = statements
            .into_iter()
            .map(
                |(statement, parameters)| json!({"statement": statement, "parameters": parameters}),
            )
            .collect();
        let unavailable = || {
            fail(
                "graph_unavailable",
                "The Neo4j service did not answer; nothing was changed.",
            )
        };
        let auth = || {
            fail(
                "graph_auth_failed",
                "Neo4j refused the configured credentials.",
            )
        };
        let response = self
            .client
            .post(self.endpoint.clone())
            .basic_auth(&self.user, Some(&self.password))
            .json(&json!({ "statements": statements }))
            .send()
            .map_err(|_| unavailable())?;
        let status = response.status();
        if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
            return Err(auth());
        }
        if status.is_server_error() {
            return Err(unavailable());
        }
        let query_failed = |details: Value| ToolError {
            code: "graph_query_failed",
            message: "Neo4j rejected the graph query; nothing was changed.".into(),
            details,
        };
        let body: Value = response
            .json()
            .map_err(|_| query_failed(json!({ "status": status.as_u16() })))?;
        if let Some(error) = body["errors"].as_array().and_then(|e| e.first()) {
            let code = error["code"].as_str().unwrap_or_default();
            if code.starts_with("Neo.ClientError.Security.") {
                return Err(auth());
            }
            return Err(query_failed(json!({ "neo4j_code": code })));
        }
        if !status.is_success() {
            return Err(query_failed(json!({ "status": status.as_u16() })));
        }
        Ok(body["results"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|result| {
                result["data"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|row| row["row"].clone())
                    .collect()
            })
            .collect())
    }
}

/// `github:<repo>`, `gitea:<host>/<owner>/<repo>`, or `ado:<org>/<project>` from the tracker
/// settings, else `path:<sha256 of the canonical root>`.
fn identity(repo: &Repo) -> String {
    let s = &repo.settings;
    let field = |tracker: &str, key: &str| s[tracker][key].as_str().filter(|v| !v.is_empty());
    let target = match s["tracker"].as_str() {
        Some("github") => field("github", "repo").map(|r| format!("github:{r}")),
        Some("gitea") => match (
            field("gitea", "baseUrl")
                .and_then(|u| Url::parse(u).ok())
                .and_then(|u| {
                    let host = u.host_str()?.to_owned();
                    Some(u.port().map_or(host.clone(), |p| format!("{host}:{p}")))
                }),
            field("gitea", "owner"),
            field("gitea", "repo"),
        ) {
            (Some(host), Some(owner), Some(name)) => Some(format!("gitea:{host}/{owner}/{name}")),
            _ => None,
        },
        Some("ado") => match (field("ado", "organization"), field("ado", "project")) {
            (Some(org), Some(project)) => Some(format!("ado:{org}/{project}")),
            _ => None,
        },
        _ => None,
    };
    target.unwrap_or_else(|| format!("path:{}", sha(repo.root.path.to_string_lossy().as_bytes())))
}

fn neo4j(server: &Server) -> Result<&Neo4j, ToolError> {
    server
        .graph
        .as_ref()
        .ok_or_else(|| fail("graph_not_configured", "Start story mcp with --graph."))
}

fn story_graph_sync(server: &Server, args: &Map<String, Value>) -> Result<Value, ToolError> {
    let repo = server.open(args)?;
    let neo4j = neo4j(server)?;
    let repository = identity(&repo);
    let (stories, _) = repo.stories()?;
    let summaries: Vec<Value> = stories
        .iter()
        .map(|s| summary(s, None))
        .filter(|s| s["id"].as_str().is_some_and(valid_id))
        .collect();
    let ids: BTreeSet<&str> = summaries.iter().filter_map(|s| s["id"].as_str()).collect();
    let (mut depends_on, mut in_epic) = (BTreeSet::new(), BTreeSet::new());
    let mut epics = BTreeSet::new();
    for s in &summaries {
        let id = s["id"].as_str().unwrap_or_default();
        for target in s["depends_on"].as_array().into_iter().flatten() {
            if let Some(target) = target.as_str().map(str::trim).filter(|t| !t.is_empty()) {
                depends_on.insert((id, target));
            }
        }
        if let Some(epic) = s["epic"].as_str().map(str::trim).filter(|e| !e.is_empty()) {
            epics.insert(epic);
            in_epic.insert((id, epic));
        }
    }
    let missing: BTreeSet<&str> = depends_on
        .iter()
        .map(|(_, target)| *target)
        .filter(|target| !ids.contains(target))
        .collect();
    let mut seen = BTreeSet::new();
    let nodes: Vec<Value> = summaries
        .iter()
        .filter(|s| seen.insert(s["id"].as_str().unwrap_or_default()))
        .map(|s| {
            json!({
                "repository": repository, "id": s["id"], "title": s["title"], "status": s["status"],
                "type": s["type"], "remote_url": s["remote_url"], "missing": false,
            })
        })
        .chain(
            missing
                .iter()
                .map(|id| json!({"repository": repository, "id": id, "missing": true})),
        )
        .collect();
    let pairs = |edges: &BTreeSet<(&str, &str)>| -> Value {
        edges.iter().map(|(a, b)| json!([a, b])).collect()
    };
    let r = json!(repository);
    neo4j.run(vec![
        (
            "MERGE (r:StoryRepository {id: $repository}) SET r.syncs = coalesce(r.syncs, 0) + 1",
            json!({"repository": r}),
        ),
        (
            "MATCH (n {repository: $repository}) WHERE n:Story OR n:Epic DETACH DELETE n",
            json!({"repository": r}),
        ),
        ("UNWIND $nodes AS s CREATE (n:Story) SET n = s", json!({"nodes": nodes})),
        (
            "UNWIND $epics AS id CREATE (:Epic {repository: $repository, id: id})",
            json!({"repository": r, "epics": epics}),
        ),
        (
            "UNWIND $edges AS e MATCH (a:Story {repository: $repository, id: e[0]}), (b:Story {repository: $repository, id: e[1]}) CREATE (a)-[:DEPENDS_ON]->(b)",
            json!({"repository": r, "edges": pairs(&depends_on)}),
        ),
        (
            "UNWIND $edges AS e MATCH (a:Story {repository: $repository, id: e[0]}), (b:Epic {repository: $repository, id: e[1]}) CREATE (a)-[:IN_EPIC]->(b)",
            json!({"repository": r, "edges": pairs(&in_epic)}),
        ),
    ])?;
    Ok(json!({
        "repository": repo.root.key,
        "repository_id": repository,
        "stories": ids.len(),
        "missing": missing.len(),
        "epics": epics.len(),
        "depends_on": depends_on.len(),
        "in_epic": in_epic.len(),
    }))
}

/// The tail shared by both graph reads: keep the first `$limit` of `found`, then the edges
/// between kept nodes.
const PROJECT: &str = "WITH collect(n) AS found \
    WITH found[..$limit] AS ns, size(found) > $limit AS truncated \
    CALL (ns) { UNWIND ns AS a MATCH (a)-[r]->(b) WHERE b IN ns \
      RETURN collect([type(r), labels(a)[0], a.id, labels(b)[0], b.id]) AS edges } \
    RETURN [n IN ns | [labels(n)[0], properties(n)]] AS nodes, edges, truncated";

fn story_graph(server: &Server, args: &Map<String, Value>) -> Result<Value, ToolError> {
    let repo = server.open(args)?;
    let neo4j = neo4j(server)?;
    let repository = identity(&repo);
    let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(200);
    let id = args.get("id").and_then(Value::as_str);
    let statement = match id {
        Some(id) => {
            repo.check_id(id)?;
            let depth = args.get("depth").and_then(Value::as_u64).unwrap_or(1);
            format!(
                "MATCH (s:Story {{repository: $repository, id: $id}}) MATCH (s)-[*0..{depth}]-(n) \
                 WITH DISTINCT s, n ORDER BY CASE WHEN n = s THEN 0 ELSE 1 END, n.id, labels(n)[0] {PROJECT}"
            )
        }
        None => format!(
            "MATCH (n {{repository: $repository}}) WHERE n:Story OR n:Epic \
             WITH n ORDER BY n.id, labels(n)[0] {PROJECT}"
        ),
    };
    let rows = neo4j.run(vec![(
        &statement,
        json!({"repository": repository, "id": id, "limit": limit}),
    )])?;
    let row = rows
        .first()
        .and_then(|r| r.first())
        .cloned()
        .unwrap_or(json!([[], [], false]));
    let key = |label: &Value, id: &Value| {
        format!(
            "{}:{}",
            label.as_str().unwrap_or_default(),
            id.as_str().unwrap_or_default()
        )
    };
    let nodes: Vec<Value> = row[0]
        .as_array()
        .into_iter()
        .flatten()
        .map(|node| {
            let (label, p) = (&node[0], &node[1]);
            json!({
                "key": key(label, &p["id"]), "label": label, "id": p["id"],
                "title": p["title"], "status": p["status"], "type": p["type"],
                "remote_url": p["remote_url"], "missing": p["missing"] == true,
            })
        })
        .collect();
    if nodes.is_empty() {
        if let Some(id) = id {
            let mut error = fail(
                "story_not_found",
                "No synchronized story has that id; run story_graph_sync first.",
            );
            error.details = json!({ "id": id });
            return Err(error);
        }
    }
    let mut edges: Vec<Value> = row[1]
        .as_array()
        .into_iter()
        .flatten()
        .map(|e| json!({"type": e[0], "from": key(&e[1], &e[2]), "to": key(&e[3], &e[4])}))
        .collect();
    edges.sort_by_key(Value::to_string);
    Ok(json!({
        "repository": repo.root.key,
        "repository_id": repository,
        "nodes": nodes,
        "edges": edges,
        "truncated": row[2] == true,
    }))
}

pub(super) const TOOLS: [Tool; 2] = [
    Tool {
        name: "story_graph_sync",
        hints: Hints {
            read_only: false,
            destructive: true,
            idempotent: true,
            open_world: true,
        },
        description: "Replace this repository's Neo4j projection with its current stories, depends_on links, and epic membership. Writes Neo4j only, never story files.",
        fields: &[REPOSITORY],
        required: &[],
        output: sync_schema,
        run: story_graph_sync,
    },
    Tool {
        name: "story_graph",
        hints: Hints {
            read_only: true,
            destructive: false,
            idempotent: true,
            open_world: true,
        },
        description: "Story and Epic nodes with DEPENDS_ON and IN_EPIC edges from the Neo4j projection: the stories within depth links of id, or the whole repository, at most limit nodes.",
        fields: &[
            REPOSITORY,
            ID,
            Field {
                name: "depth",
                kind: Kind::Integer {
                    min: 1,
                    max: 3,
                    default: 1,
                },
            },
            Field {
                name: "limit",
                kind: Kind::Integer {
                    min: 1,
                    max: 200,
                    default: 200,
                },
            },
        ],
        required: &[],
        output: graph_schema,
        run: story_graph,
    },
];

fn sync_schema() -> Value {
    let count = json!({"type": "integer", "minimum": 0});
    let text = json!({"type": "string"});
    let keys = [
        "repository",
        "repository_id",
        "stories",
        "missing",
        "epics",
        "depends_on",
        "in_epic",
    ];
    with_redactions(object(
        &keys,
        json!({
            "repository": text, "repository_id": text, "stories": count, "missing": count,
            "epics": count, "depends_on": count, "in_epic": count,
        }),
    ))
}

fn graph_schema() -> Value {
    let text = json!({"type": "string"});
    let nullable = json!({"type": ["string", "null"]});
    let node = object(
        &[
            "key",
            "label",
            "id",
            "title",
            "status",
            "type",
            "remote_url",
            "missing",
        ],
        json!({
            "key": text, "label": {"enum": ["Story", "Epic"]}, "id": text, "title": nullable,
            "status": nullable, "type": nullable, "remote_url": nullable, "missing": {"type": "boolean"},
        }),
    );
    let edge = object(
        &["type", "from", "to"],
        json!({"type": {"enum": ["DEPENDS_ON", "IN_EPIC"]}, "from": text, "to": text}),
    );
    with_redactions(object(
        &["repository", "repository_id", "nodes", "edges", "truncated"],
        json!({
            "repository": text, "repository_id": text,
            "nodes": {"type": "array", "items": node, "maxItems": 200},
            "edges": {"type": "array", "items": edge},
            "truncated": {"type": "boolean"},
        }),
    ))
}
