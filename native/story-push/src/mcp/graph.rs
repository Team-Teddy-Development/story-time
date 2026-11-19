//! `story mcp --graph`: `story_graph_sync` projects one repository's stories into Neo4j and
//! `story_graph` reads a bounded part of that projection back (ST-011). The Markdown backlog stays
//! the source of truth; each sync replaces the repository's projection in one transaction. Nodes
//! are `Story` and `Epic`, keyed by `(repository, id)`, where `repository` is the tracker target
//! or a hash of the root, so equal story ids in two repositories never meet.
//!
//! A server with a registry (ST-132) projects every registered repository in one transaction,
//! with `DEPENDS_ON` and `RELATES_TO` edges between repositories. It has no sync tool: each
//! `story_graph` call compares the commits that Neo4j holds with the commits the server
//! reads, and syncs first when they differ.

use super::{
    fail, object, summary, with_redactions, Field, Hints, Kind, Repo, Server, Tool, ToolError, ID,
    REPOSITORY,
};
use crate::{
    adoption::sha,
    lenient_story::Js,
    org_index::{Located, OrgIndex, OrgRepo},
    valid_id, Failure,
};
use reqwest::{blocking::Client, StatusCode, Url};
use serde_json::{json, Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Mutex,
    time::Duration,
};

pub(super) const USAGE: &str =
    "[--graph --neo4j-url URL [--neo4j-user USER --neo4j-password-env NAME] [--neo4j-database NAME]]";

pub(super) struct Neo4j {
    endpoint: Url,
    /// The user and the password. `None` for a Neo4j that asks for no credentials.
    auth: Option<(String, String)>,
    client: Client,
    /// Held while a registry server compares and replaces the projection, so that two calls
    /// do not sync at once.
    syncing: Mutex<()>,
}

impl Neo4j {
    /// The password, which no output may show.
    pub(super) fn password(&self) -> Option<&String> {
        self.auth.as_ref().map(|(_, password)| password)
    }
}

/// Splits the graph flags off `args`. `--graph` needs the URL. The user and the name of the
/// variable that holds the password go together, or neither for a Neo4j with no
/// authentication; the password itself never appears on the command line.
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
    let auth = match (
        flags.contains_key("neo4j-user"),
        flags.contains_key("neo4j-password-env"),
    ) {
        (false, false) => None,
        _ => {
            let user = flag("neo4j-user")?.clone();
            let variable = flag("neo4j-password-env")?;
            let password = std::env::var(variable)
                .ok()
                .filter(|p| !p.is_empty())
                .ok_or_else(|| {
                    refuse(format!(
                        "--neo4j-password-env names {variable}, which is not set."
                    ))
                })?;
            Some((user, password))
        }
    };
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
            auth,
            client,
            syncing: Mutex::new(()),
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
        let mut request = self.client.post(self.endpoint.clone());
        if let Some((user, password)) = &self.auth {
            request = request.basic_auth(user, Some(password));
        }
        let response = request
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

/// One repository of a sync: its key on this server, its identity in Neo4j, and the commit
/// that its stories were read from, when the server reads commits.
struct Unit {
    key: String,
    identity: String,
    sha: Option<String>,
}

#[derive(Default)]
struct Counts {
    stories: usize,
    missing: usize,
    epics: usize,
    depends_on: usize,
    relates_to: usize,
    in_epic: usize,
    skipped_ambiguous: usize,
}

/// The ids that a link field of a story names.
fn links(story: &Js, field: &str) -> Vec<String> {
    let items: Vec<String> = match story.get(field) {
        Js::Undefined => Vec::new(),
        Js::Arr(items) => items.iter().map(Js::string).collect(),
        other => vec![other.string()],
    };
    items
        .iter()
        .map(|item| item.trim().to_owned())
        .filter(|item| !item.is_empty())
        .collect()
}

/// Replaces the projection of `units` in one transaction and returns the counts of each. The
/// stories come from `org`. With `cross`, a link to a story that its own repository does not
/// hold goes to the repository that `org` locates it in, and a link whose target is ambiguous
/// is skipped and counted. Any other link to a story that its repository does not hold gets a
/// `missing` node in that repository, so a sync of one repository needs no other repository.
fn sync(
    neo4j: &Neo4j,
    units: &[Unit],
    org: &OrgIndex,
    cross: bool,
) -> Result<Vec<Counts>, ToolError> {
    let identities: BTreeMap<&str, &str> = units
        .iter()
        .map(|unit| (unit.key.as_str(), unit.identity.as_str()))
        .collect();
    if identities.values().collect::<BTreeSet<_>>().len() != units.len() {
        let mut error = fail(
            "config_invalid",
            "Two repositories have one tracker target, so their stories cannot be told apart.",
        );
        error.details = json!({ "reason": "duplicate repository identity" });
        return Err(error);
    }
    let (mut nodes, mut epic_nodes) = (Vec::new(), BTreeSet::new());
    let mut edges: BTreeMap<&str, BTreeSet<[String; 4]>> = BTreeMap::new();
    let mut in_epic = BTreeSet::new();
    let mut missing_nodes = BTreeSet::new();
    let mut counts = Vec::new();
    for unit in units {
        let mut count = Counts::default();
        let mut seen = BTreeSet::new();
        let stories: Vec<(&Js, Value)> = org
            .stories(&unit.key)
            .iter()
            .map(|story| (story, summary(story, None)))
            .filter(|(_, s)| s["id"].as_str().is_some_and(valid_id))
            .collect();
        let ids: BTreeSet<&str> = stories
            .iter()
            .filter_map(|(_, s)| s["id"].as_str())
            .collect();
        count.stories = ids.len();
        let mut epics = BTreeSet::new();
        let mut missing = BTreeSet::new();
        for (story, s) in &stories {
            let id = s["id"].as_str().unwrap_or_default();
            if seen.insert(id) {
                nodes.push(json!({
                    "repository": unit.identity, "id": s["id"], "title": s["title"],
                    "status": s["status"], "type": s["type"], "remote_url": s["remote_url"],
                    "missing": false,
                }));
            }
            for (field, kind) in [("depends_on", "DEPENDS_ON"), ("relates_to", "RELATES_TO")] {
                for target in links(story, field) {
                    let home = if ids.contains(target.as_str()) {
                        Some(unit.identity.as_str())
                    } else if cross {
                        match org.locate(&target) {
                            Located::Ambiguous => {
                                count.skipped_ambiguous += 1;
                                continue;
                            }
                            Located::In(key) => identities.get(key).copied(),
                            Located::Missing => None,
                        }
                    } else {
                        None
                    };
                    let home = home.unwrap_or_else(|| {
                        missing.insert(target.clone());
                        &unit.identity
                    });
                    let edge = [
                        unit.identity.clone(),
                        id.to_owned(),
                        home.to_owned(),
                        target,
                    ];
                    if edges.entry(kind).or_default().insert(edge) {
                        match kind {
                            "DEPENDS_ON" => count.depends_on += 1,
                            _ => count.relates_to += 1,
                        }
                    }
                }
            }
            if let Some(epic) = s["epic"].as_str().map(str::trim).filter(|e| !e.is_empty()) {
                epics.insert(epic.to_owned());
                if in_epic.insert([unit.identity.clone(), id.to_owned(), epic.to_owned()]) {
                    count.in_epic += 1;
                }
            }
        }
        count.missing = missing.len();
        count.epics = epics.len();
        missing_nodes.extend(missing.into_iter().map(|id| (unit.identity.clone(), id)));
        epic_nodes.extend(epics.into_iter().map(|id| [unit.identity.clone(), id]));
        counts.push(count);
    }
    nodes.extend(
        missing_nodes
            .iter()
            .map(|(repository, id)| json!({"repository": repository, "id": id, "missing": true})),
    );
    let repositories: Vec<Value> = units
        .iter()
        .zip(&counts)
        .map(|(unit, count)| {
            json!({
                "id": unit.identity, "key": unit.key, "sha": unit.sha,
                "skipped": count.skipped_ambiguous,
            })
        })
        .collect();
    let ids: Vec<&str> = units.iter().map(|unit| unit.identity.as_str()).collect();
    let empty = BTreeSet::new();
    let link = |kind: &str| json!(edges.get(kind).unwrap_or(&empty));
    neo4j.run(vec![
        (
            "UNWIND $repositories AS x MERGE (r:StoryRepository {id: x.id}) \
             SET r.syncs = coalesce(r.syncs, 0) + 1, r.key = x.key, r.sha = x.sha, \
             r.skipped_ambiguous = x.skipped",
            json!({ "repositories": repositories }),
        ),
        (
            "MATCH (n) WHERE (n:Story OR n:Epic) AND n.repository IN $ids DETACH DELETE n",
            json!({ "ids": ids }),
        ),
        ("UNWIND $nodes AS s CREATE (n:Story) SET n = s", json!({ "nodes": nodes })),
        (
            "UNWIND $epics AS e CREATE (:Epic {repository: e[0], id: e[1]})",
            json!({ "epics": epic_nodes }),
        ),
        (
            "UNWIND $edges AS e MATCH (a:Story {repository: e[0], id: e[1]}), (b:Story {repository: e[2], id: e[3]}) CREATE (a)-[:DEPENDS_ON]->(b)",
            json!({ "edges": link("DEPENDS_ON") }),
        ),
        (
            "UNWIND $edges AS e MATCH (a:Story {repository: e[0], id: e[1]}), (b:Story {repository: e[2], id: e[3]}) CREATE (a)-[:RELATES_TO]->(b)",
            json!({ "edges": link("RELATES_TO") }),
        ),
        (
            "UNWIND $edges AS e MATCH (a:Story {repository: e[0], id: e[1]}), (b:Epic {repository: e[0], id: e[2]}) CREATE (a)-[:IN_EPIC]->(b)",
            json!({ "edges": in_epic }),
        ),
    ])?;
    Ok(counts)
}

fn story_graph_sync(server: &Server, args: &Map<String, Value>) -> Result<Value, ToolError> {
    let repo = server.open(args)?;
    let neo4j = neo4j(server)?;
    let unit = Unit {
        key: repo.root.key.clone(),
        identity: identity(&repo),
        sha: None,
    };
    let org = OrgIndex::new(vec![OrgRepo {
        key: unit.key.clone(),
        prefixes: repo.prefixes.clone(),
        stories: repo.stories()?.0,
    }]);
    let counts = sync(neo4j, std::slice::from_ref(&unit), &org, false)?;
    let count = &counts[0];
    Ok(json!({
        "repository": unit.key,
        "repository_id": unit.identity,
        "stories": count.stories,
        "missing": count.missing,
        "epics": count.epics,
        "depends_on": count.depends_on,
        "relates_to": count.relates_to,
        "in_epic": count.in_epic,
        "skipped_ambiguous": count.skipped_ambiguous,
    }))
}

/// For a registry server: makes the projection of the org of the repository `of` match the
/// commits the server reads now, and returns each repository of that org with its identity and
/// commit, plus the count of skipped links. A repository that the server cannot read, or that
/// has no story-time settings, is left out. Each org is projected by itself, so no link and no
/// answer crosses from one org to another.
fn ensure_synced(server: &Server, neo4j: &Neo4j, of: &str) -> Result<(Vec<Unit>, u64), ToolError> {
    let mut units = Vec::new();
    let mut repos = Vec::new();
    let in_org = server.org_keys(of);
    let others: Vec<String> = server
        .keys()
        .into_iter()
        .filter(|key| !in_org.contains(key))
        .collect();
    for key in in_org {
        let named = Map::from_iter([("repository".to_owned(), json!(key))]);
        let repo = match server.open(&named) {
            Ok(repo) => repo,
            Err(error) if matches!(error.code, "not_configured" | "repository_unavailable") => {
                continue
            }
            Err(error) => return Err(error),
        };
        units.push(Unit {
            key,
            identity: identity(&repo),
            sha: repo.root.source.as_ref().map(|source| source.sha.clone()),
        });
        repos.push(repo);
    }
    let _syncing = neo4j.syncing.lock().unwrap_or_else(|e| e.into_inner());
    let keys: Vec<&str> = units.iter().map(|unit| unit.key.as_str()).collect();
    let rows = neo4j.run(vec![(
        "MATCH (r:StoryRepository) WHERE r.key IN $keys \
         RETURN r.id, r.key, r.sha, coalesce(r.skipped_ambiguous, 0)",
        json!({ "keys": keys }),
    )])?;
    let rows = rows.into_iter().next().unwrap_or_default();
    let held: BTreeSet<(&str, &str, &str)> = rows
        .iter()
        .filter_map(|row| Some((row[0].as_str()?, row[1].as_str()?, row[2].as_str()?)))
        .collect();
    let wanted: BTreeSet<(&str, &str, &str)> = units
        .iter()
        .map(|unit| {
            (
                unit.identity.as_str(),
                unit.key.as_str(),
                unit.sha.as_deref().unwrap_or_default(),
            )
        })
        .collect();
    if held == wanted && rows.len() == units.len() {
        let skipped = rows.iter().filter_map(|row| row[3].as_u64()).sum();
        return Ok((units, skipped));
    }
    // A node is keyed by the tracker target of its repository. Two orgs that register one
    // target would share nodes, and a read of one org would follow the links of the other.
    let ids: Vec<&str> = units.iter().map(|unit| unit.identity.as_str()).collect();
    let shared = neo4j.run(vec![(
        "MATCH (r:StoryRepository) WHERE r.id IN $ids AND r.key IN $others RETURN r.key",
        json!({ "ids": ids, "others": others }),
    )])?;
    if let Some(other) = shared.first().and_then(|rows| rows.first()) {
        let mut error = fail(
            "config_invalid",
            "A repository of another org has the same tracker target, so their stories cannot be told apart.",
        );
        error.details =
            json!({ "reason": "duplicate repository identity", "repository": other[0] });
        return Err(error);
    }
    let mut org = Vec::new();
    for (unit, repo) in units.iter().zip(&repos) {
        org.push(OrgRepo {
            key: unit.key.clone(),
            prefixes: repo.prefixes.clone(),
            stories: repo.stories()?.0,
        });
    }
    let counts = sync(neo4j, &units, &OrgIndex::new(org), true)?;
    let skipped = counts.iter().map(|c| c.skipped_ambiguous as u64).sum();
    Ok((units, skipped))
}

/// The tail shared by both graph reads: keep the first `$limit` of `found`, then the edges
/// between kept nodes.
const PROJECT: &str = "WITH collect(n) AS found \
    WITH found[..$limit] AS ns, size(found) > $limit AS truncated \
    CALL (ns) { UNWIND ns AS a MATCH (a)-[r]->(b) WHERE b IN ns \
      RETURN collect([type(r), labels(a)[0], a.id, labels(b)[0], b.id, a.repository, b.repository]) AS edges } \
    RETURN [n IN ns | [labels(n)[0], properties(n)]] AS nodes, edges, truncated";

const DIRECTIONS: &[&str] = &["both", "dependents", "dependencies"];

fn story_graph(server: &Server, args: &Map<String, Value>) -> Result<Value, ToolError> {
    let repo = server.open(args)?;
    let neo4j = neo4j(server)?;
    let repository = identity(&repo);
    let synced = match server.workspace {
        Some(_) => Some(ensure_synced(server, neo4j, &repo.root.key)?),
        None => None,
    };
    let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(200);
    let id = args.get("id").and_then(Value::as_str);
    let statement = match id {
        Some(id) => {
            repo.check_id(id)?;
            let depth = args.get("depth").and_then(Value::as_u64).unwrap_or(1);
            // Dependency and epic links are followed to `depth`. A RELATES_TO link is a
            // neighbor at depth 1 only, in every direction.
            let pattern = match args.get("direction").and_then(Value::as_str) {
                Some("dependents") => format!("<-[:DEPENDS_ON*0..{depth}]-"),
                Some("dependencies") => format!("-[:DEPENDS_ON*0..{depth}]->"),
                _ => format!("-[:DEPENDS_ON|IN_EPIC*0..{depth}]-"),
            };
            format!(
                "MATCH (s:Story {{repository: $repository, id: $id}}) \
                 CALL (s) {{ MATCH (s){pattern}(n) RETURN n UNION MATCH (s)-[:RELATES_TO]-(n) RETURN n }} \
                 WITH DISTINCT s, n ORDER BY CASE WHEN n = s THEN 0 ELSE 1 END, n.repository, n.id, labels(n)[0] {PROJECT}"
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
    // A registry server names the repository of each node, because a graph spans several.
    let keys: Option<BTreeMap<&str, &str>> = synced.as_ref().map(|(units, _)| {
        units
            .iter()
            .map(|unit| (unit.identity.as_str(), unit.key.as_str()))
            .collect()
    });
    let home = |identity: &Value| {
        let keys = keys.as_ref()?;
        Some(keys.get(identity.as_str()?).copied().unwrap_or("unknown"))
    };
    let key = |label: &Value, id: &Value, identity: &Value| {
        let (label, id) = (
            label.as_str().unwrap_or_default(),
            id.as_str().unwrap_or_default(),
        );
        match home(identity) {
            Some(repository) => format!("{label}:{repository}/{id}"),
            None => format!("{label}:{id}"),
        }
    };
    let nodes: Vec<Value> = row[0]
        .as_array()
        .into_iter()
        .flatten()
        .map(|node| {
            let (label, p) = (&node[0], &node[1]);
            let mut node = json!({
                "key": key(label, &p["id"], &p["repository"]), "label": label, "id": p["id"],
                "title": p["title"], "status": p["status"], "type": p["type"],
                "remote_url": p["remote_url"], "missing": p["missing"] == true,
            });
            if let Some(repository) = home(&p["repository"]) {
                node["repository"] = json!(repository);
            }
            node
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
        .map(|e| {
            json!({"type": e[0], "from": key(&e[1], &e[2], &e[5]), "to": key(&e[3], &e[4], &e[6])})
        })
        .collect();
    edges.sort_by_key(Value::to_string);
    let mut result = json!({
        "repository": repo.root.key,
        "repository_id": repository,
        "nodes": nodes,
        "edges": edges,
        "truncated": row[2] == true,
    });
    if let Some((units, skipped)) = &synced {
        let repositories: Vec<Value> = units
            .iter()
            .map(|unit| json!({"repository": unit.key, "sha": unit.sha}))
            .collect();
        result["sync"] = json!({"repositories": repositories, "skipped_ambiguous": skipped});
    }
    Ok(result)
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
        description: "Replace this repository's Neo4j projection with its current stories, depends_on and relates_to links, and epic membership. Writes Neo4j only, never story files.",
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
        description: "Story and Epic nodes with DEPENDS_ON, RELATES_TO, and IN_EPIC edges from the Neo4j projection: the stories within depth links of id, or the whole repository, at most limit nodes. direction limits the links from id to its dependents (the stories that wait on it) or its dependencies; RELATES_TO neighbors come at depth 1 only.",
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
            Field {
                name: "direction",
                kind: Kind::Choice(DIRECTIONS),
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
        "relates_to",
        "in_epic",
        "skipped_ambiguous",
    ];
    with_redactions(object(
        &keys,
        json!({
            "repository": text, "repository_id": text, "stories": count, "missing": count,
            "epics": count, "depends_on": count, "relates_to": count, "in_epic": count,
            "skipped_ambiguous": count,
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
        json!({"type": {"enum": ["DEPENDS_ON", "RELATES_TO", "IN_EPIC"]}, "from": text, "to": text}),
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

/// The fields that `story_graph` has only on a registry server: the repository of each node,
/// and the commits that the projection was synced from.
pub(super) fn registry_schema(output: &mut Value) {
    let text = json!({"type": "string"});
    let node = &mut output["properties"]["nodes"]["items"];
    node["properties"]["repository"] = text.clone();
    node["required"]
        .as_array_mut()
        .unwrap()
        .push(json!("repository"));
    let synced = object(
        &["repository", "sha"],
        json!({"repository": text, "sha": text}),
    );
    output["properties"]["sync"] = object(
        &["repositories", "skipped_ambiguous"],
        json!({
            "repositories": {"type": "array", "items": synced},
            "skipped_ambiguous": {"type": "integer", "minimum": 0},
        }),
    );
    output["required"]
        .as_array_mut()
        .unwrap()
        .push(json!("sync"));
}
