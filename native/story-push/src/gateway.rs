use reqwest::blocking::Client;
use reqwest::{Method, StatusCode, Url};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

#[derive(Debug, PartialEq, Eq)]
pub struct Remote {
    pub remote_id: String,
    pub url: String,
}

pub struct Gateway {
    config: Value,
    backend: Backend,
}

enum Backend {
    Github {
        repo: String,
        cli: Cli,
        labels: HashSet<String>,
    },
    Ado {
        organization: String,
        project: String,
        process: String,
        cli: Cli,
    },
    Gitea(Gitea),
}

struct Cli {
    program: PathBuf,
    root: PathBuf,
    token: Option<(String, String)>,
}

impl Cli {
    fn run(&self, args: &[String], input: Option<&str>) -> Result<String, String> {
        let mut command = Command::new(&self.program);
        command
            .args(args)
            .current_dir(&self.root)
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some((name, value)) = &self.token {
            command.env(name, value);
        }
        let mut child = command
            .spawn()
            .map_err(|_| "Tracker CLI could not start; check its installation.".to_string())?;
        let write_result = match (input, child.stdin.take()) {
            (Some(text), Some(mut stdin)) => stdin.write_all(text.as_bytes()),
            _ => Ok(()),
        };
        let out = child
            .wait_with_output()
            .map_err(|_| "Tracker CLI could not be read.".to_string())?;
        if !out.status.success() {
            return Err(format!(
                "Tracker CLI failed (exit {}). Check tracker authentication and repository access.",
                out.status
                    .code()
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "signal".into())
            ));
        }
        write_result.map_err(|_| "Tracker CLI did not accept its input.".to_string())?;
        String::from_utf8(out.stdout).map_err(|_| "Tracker CLI returned invalid UTF-8.".to_string())
    }
}

impl Gateway {
    pub fn new(config: &Value, root: &Path) -> Result<Self, String> {
        let root = root
            .canonicalize()
            .map_err(|_| "Could not resolve project root.".to_string())?;
        let root = root.as_path();
        let tracker = required(config, "tracker")?;
        let (variable, file) = match tracker {
            "github" => ("GH_TOKEN", ".github.env"),
            "ado" => ("AZURE_DEVOPS_EXT_PAT", ".ado.env"),
            "gitea" => ("GITEA_TOKEN", ".gitea.env"),
            _ => return Err("Push requires a github, ado, or gitea tracker.".into()),
        };
        let environment = std::env::var(variable).ok().filter(|s| !s.is_empty());
        let stored = if tracker == "gitea" && environment.is_some() {
            None
        } else {
            load_secret(root, file, variable)?
        };
        let token = environment.or(stored);
        if let Some(value) = &token {
            validate_token(value)?;
        }
        let cli = |program: &str| Cli {
            program: PathBuf::from(program),
            root: root.to_path_buf(),
            token: token.clone().map(|value| (variable.into(), value)),
        };
        let backend = match tracker {
            "github" => {
                let repo = required(&config["github"], "repo")?;
                let parts: Vec<_> = repo.split('/').collect();
                if parts.len() != 2 || parts.iter().any(|p| !valid_segment(p)) {
                    return Err("github.repo must be owner/repository.".into());
                }
                Backend::Github {
                    repo: repo.into(),
                    cli: cli("gh"),
                    labels: HashSet::new(),
                }
            }
            "ado" => {
                let ado = &config["ado"];
                let organization = validate_base(required(ado, "organization")?)?;
                let project = required(ado, "project")?;
                let process = ado
                    .get("process")
                    .and_then(Value::as_str)
                    .unwrap_or("agile");
                if !["agile", "scrum", "basic"].contains(&process) {
                    return Err("ado.process must be agile, scrum, or basic.".into());
                }
                Backend::Ado {
                    organization,
                    project: project.into(),
                    process: process.into(),
                    cli: cli("az"),
                }
            }
            _ => {
                let gitea = &config["gitea"];
                let base = validate_base(required(gitea, "baseUrl")?)?;
                let owner = required(gitea, "owner")?;
                let repo = required(gitea, "repo")?;
                if !valid_segment(owner) || !valid_segment(repo) {
                    return Err("Gitea owner and repository must be single path segments.".into());
                }
                let token = token.ok_or("No Gitea token. Store one with: story secret set gitea (or export GITEA_TOKEN).")?;
                let client = Client::builder()
                    .redirect(reqwest::redirect::Policy::none())
                    .timeout(Duration::from_secs(30))
                    .build()
                    .map_err(|_| "Could not initialize Gitea HTTP client.".to_string())?;
                Backend::Gitea(Gitea {
                    base,
                    owner: owner.into(),
                    repo: repo.into(),
                    token,
                    client,
                    labels: None,
                })
            }
        };
        Ok(Self {
            config: config.clone(),
            backend,
        })
    }

    pub fn find(&mut self, story: &Value) -> Result<Option<Remote>, String> {
        let prefix = format!("{}: ", required(story, "id")?);
        let titled = |title: Option<&str>| title.is_some_and(|t| t.starts_with(&prefix));
        let mut found = Vec::new();
        match &mut self.backend {
            Backend::Github { repo, cli, .. } => {
                let issues = parse_json(&cli.run(
                    &strings(&[
                        "issue",
                        "list",
                        "--repo",
                        repo,
                        "--state",
                        "open",
                        "--limit",
                        "1000",
                        "--json",
                        "number,title",
                    ]),
                    None,
                )?)?;
                let issues = issues
                    .as_array()
                    .ok_or("GitHub returned an invalid issue list.")?;
                if issues.len() >= 1000 {
                    return Err("Too many open GitHub issues to search for an earlier create; link the story by hand.".into());
                }
                for issue in issues {
                    if titled(issue["title"].as_str()) {
                        let id = value_id(&issue["number"])?;
                        found.push(Remote {
                            url: format!("https://github.com/{repo}/issues/{id}"),
                            remote_id: id,
                        });
                    }
                }
            }
            Backend::Ado {
                organization,
                project,
                cli,
                ..
            } => {
                let wiql = format!(
                    "SELECT [System.Id], [System.Title] FROM WorkItems WHERE [System.TeamProject] = '{}' AND [System.Title] CONTAINS '{}'",
                    project.replace('\'', "''"),
                    prefix.trim_end()
                );
                let items = parse_json(&cli.run(
                    &strings(&[
                        "boards",
                        "query",
                        "--org",
                        organization,
                        "--project",
                        project,
                        "--wiql",
                        &wiql,
                        "--output",
                        "json",
                    ]),
                    None,
                )?)?;
                for item in items
                    .as_array()
                    .ok_or("Azure DevOps returned an invalid work item list.")?
                {
                    if titled(item["fields"]["System.Title"].as_str()) {
                        found.push(ado_remote(organization, project, value_id(&item["id"])?));
                    }
                }
            }
            Backend::Gitea(gitea) => {
                for page in 1.. {
                    if page > 10000 {
                        return Err("Gitea issue pagination exceeded its safety limit.".into());
                    }
                    let batch = gitea.api(
                        Method::GET,
                        &format!("/issues?state=open&type=issues&page={page}&limit=50"),
                        None,
                    )?;
                    let batch = batch
                        .as_array()
                        .ok_or("Gitea returned an invalid issue list.")?;
                    if batch.is_empty() {
                        break;
                    }
                    for issue in batch {
                        if titled(issue["title"].as_str()) {
                            found.push(gitea.remote(value_id(&issue["number"])?));
                        }
                    }
                }
            }
        }
        if found.len() > 1 {
            return Err(format!(
                "Found {} open tracker items titled \"{prefix}...\"; link the story to one by hand.",
                found.len()
            ));
        }
        Ok(found.pop())
    }

    pub fn create(&mut self, story: &Value) -> Result<Remote, String> {
        let tracker = self.tracker();
        let card = crate::story::render_card(story, tracker, &self.config)?;
        match &mut self.backend {
            Backend::Github { repo, cli, labels } => {
                let names = card_labels(&card)?;
                ensure_github_labels(cli, repo, labels, &names)?;
                let mut args = strings(&[
                    "issue",
                    "create",
                    "--repo",
                    repo,
                    "--title",
                    required(&card, "title")?,
                    "--body-file",
                    "-",
                ]);
                for name in names {
                    args.extend(strings(&["--label", &name]));
                }
                let out = cli.run(&args, Some(required(&card, "body")?))?;
                let id = github_created_id(&out, repo)?;
                github_state(cli, repo, &id, closing(story), "OPEN")?;
                Ok(Remote {
                    url: format!("https://github.com/{repo}/issues/{id}"),
                    remote_id: id,
                })
            }
            Backend::Ado {
                organization,
                project,
                process,
                cli,
            } => {
                let (kind, content) = ado_content(&card)?;
                let mut args = strings(&[
                    "boards",
                    "work-item",
                    "create",
                    "--org",
                    organization,
                    "--project",
                    project,
                    "--type",
                    &kind,
                ]);
                args.extend(content);
                args.extend(strings(&["--output", "json"]));
                let value = parse_json(&cli.run(&args, None)?)?;
                let id = value_id(&value["id"])?;
                if let Some(state) = ado_state(story, process) {
                    cli.run(
                        &strings(&[
                            "boards",
                            "work-item",
                            "update",
                            "--id",
                            &id,
                            "--org",
                            organization,
                            "--state",
                            state,
                            "--output",
                            "json",
                        ]),
                        None,
                    )?;
                }
                Ok(ado_remote(organization, project, id))
            }
            Backend::Gitea(gitea) => {
                let names = gitea_labels(&card, story)?;
                let ids = gitea.ensure_labels(&names)?;
                let created = gitea.api(Method::POST, "/issues", Some(json!({"title":required(&card,"title")?,"body":required(&card,"body")?,"labels":ids})))?;
                let id = value_id(&created["number"])?;
                if closing(story).is_some() {
                    gitea.api(
                        Method::PATCH,
                        &format!("/issues/{id}"),
                        Some(json!({"state":"closed"})),
                    )?;
                }
                Ok(gitea.remote(id))
            }
        }
    }

    pub fn update(&mut self, story: &Value) -> Result<Remote, String> {
        let id = value_id(&story["remote"]).map_err(|_| {
            "Story has no valid positive numeric remote item to update.".to_string()
        })?;
        let card = crate::story::render_card(story, self.tracker(), &self.config)?;
        match &mut self.backend {
            Backend::Github { repo, cli, labels } => {
                let current = parse_json(&cli.run(
                    &strings(&[
                        "issue",
                        "view",
                        &id,
                        "--repo",
                        repo,
                        "--json",
                        "labels,state",
                    ]),
                    None,
                )?)?;
                let current_labels = named_labels(&current)?;
                let state = required(&current, "state")?;
                if !["OPEN", "CLOSED"].contains(&state) {
                    return Err("GitHub returned an invalid issue state.".into());
                }
                let names = card_labels(&card)?;
                ensure_github_labels(cli, repo, labels, &names)?;
                let mut args = strings(&[
                    "issue",
                    "edit",
                    &id,
                    "--repo",
                    repo,
                    "--title",
                    required(&card, "title")?,
                    "--body-file",
                    "-",
                ]);
                for name in &names {
                    args.extend(strings(&["--add-label", name]));
                }
                for name in current_labels {
                    if managed(&name, false) && !names.contains(&name) {
                        args.extend(strings(&["--remove-label", &name]));
                    }
                }
                cli.run(&args, Some(required(&card, "body")?))?;
                github_state(cli, repo, &id, closing(story), state)?;
                Ok(Remote {
                    url: format!("https://github.com/{repo}/issues/{id}"),
                    remote_id: id,
                })
            }
            Backend::Ado {
                organization,
                project,
                process,
                cli,
            } => {
                let (_, content) = ado_content(&card)?;
                let mut args = strings(&[
                    "boards",
                    "work-item",
                    "update",
                    "--id",
                    &id,
                    "--org",
                    organization,
                ]);
                args.extend(content);
                if let Some(state) = ado_state(story, process) {
                    args.extend(strings(&["--state", state]));
                }
                args.extend(strings(&["--output", "json"]));
                cli.run(&args, None)?;
                Ok(ado_remote(organization, project, id))
            }
            Backend::Gitea(gitea) => {
                let current = gitea.api(Method::GET, &format!("/issues/{id}"), None)?;
                let mut ids = Vec::new();
                if let Some(labels) = current.get("labels").filter(|v| !v.is_null()) {
                    for label in labels
                        .as_array()
                        .ok_or("Gitea returned invalid issue labels.")?
                    {
                        if !managed(required(label, "name")?, true) {
                            ids.push(numeric_id(&label["id"])?);
                        }
                    }
                }
                ids.extend(gitea.ensure_labels(&gitea_labels(&card, story)?)?);
                ids.sort_unstable();
                ids.dedup();
                gitea.api(Method::PATCH, &format!("/issues/{id}"), Some(json!({"title":required(&card,"title")?,"body":required(&card,"body")?,"state":if closing(story).is_some(){"closed"}else{"open"}})))?;
                gitea.api(
                    Method::PUT,
                    &format!("/issues/{id}/labels"),
                    Some(json!({"labels":ids})),
                )?;
                Ok(gitea.remote(id))
            }
        }
    }

    pub fn remove(&mut self, remote_id: &str) -> Result<(), String> {
        let id = validate_id(remote_id)?;
        match &mut self.backend {
            Backend::Github { repo, cli, .. } => {
                cli.run(
                    &strings(&[
                        "issue",
                        "close",
                        &id,
                        "--repo",
                        repo,
                        "--reason",
                        "not planned",
                    ]),
                    None,
                )?;
            }
            Backend::Ado {
                organization, cli, ..
            } => {
                cli.run(
                    &strings(&[
                        "boards",
                        "work-item",
                        "delete",
                        "--id",
                        &id,
                        "--org",
                        organization,
                        "--yes",
                    ]),
                    None,
                )?;
            }
            Backend::Gitea(gitea) => {
                let ids = gitea.ensure_labels(&["not-planned".into()])?;
                gitea.api(
                    Method::POST,
                    &format!("/issues/{id}/labels"),
                    Some(json!({"labels":ids})),
                )?;
                gitea.api(
                    Method::PATCH,
                    &format!("/issues/{id}"),
                    Some(json!({"state":"closed"})),
                )?;
            }
        }
        Ok(())
    }

    fn tracker(&self) -> &'static str {
        match self.backend {
            Backend::Github { .. } => "github",
            Backend::Ado { .. } => "ado",
            Backend::Gitea(_) => "gitea",
        }
    }
}

struct Gitea {
    base: String,
    owner: String,
    repo: String,
    token: String,
    client: Client,
    labels: Option<HashMap<String, u64>>,
}

impl Gitea {
    fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<(StatusCode, Value), String> {
        let mut authorization =
            reqwest::header::HeaderValue::from_str(&format!("token {}", self.token))
                .map_err(|_| "Invalid Gitea credential.".to_string())?;
        authorization.set_sensitive(true);
        let mut request = self
            .client
            .request(
                method.clone(),
                format!(
                    "{}/api/v1/repos/{}/{}{}",
                    self.base, self.owner, self.repo, path
                ),
            )
            .header(reqwest::header::AUTHORIZATION, authorization)
            .header(reqwest::header::ACCEPT, "application/json");
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().map_err(|_| {
            format!("Gitea {method} request failed; check endpoint and connectivity.")
        })?;
        let status = response.status();
        if !status.is_success() {
            return Ok((status, Value::Null));
        }
        let mut bytes = Vec::new();
        response
            .take(8 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "Could not read Gitea response.".to_string())?;
        if bytes.len() > 8 * 1024 * 1024 {
            return Err("Gitea response exceeds size limit.".into());
        }
        let value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes)
                .map_err(|_| "Gitea returned invalid JSON.".to_string())?
        };
        Ok((status, value))
    }

    fn api(&self, method: Method, path: &str, body: Option<Value>) -> Result<Value, String> {
        let (status, value) = self.request(method.clone(), path, body)?;
        if !status.is_success() {
            return Err(format!("Gitea {method} failed: HTTP {}.", status.as_u16()));
        }
        Ok(value)
    }

    fn load_labels(&mut self) -> Result<(), String> {
        let mut ids = HashMap::new();
        for page in 1..=10000 {
            let batch = self.api(Method::GET, &format!("/labels?page={page}&limit=50"), None)?;
            let batch = batch
                .as_array()
                .ok_or("Gitea returned invalid repository labels.")?;
            if batch.is_empty() {
                self.labels = Some(ids);
                return Ok(());
            }
            for label in batch {
                ids.insert(required(label, "name")?.into(), numeric_id(&label["id"])?);
            }
        }
        Err("Gitea label pagination exceeded its safety limit.".into())
    }

    fn ensure_labels(&mut self, names: &[String]) -> Result<Vec<u64>, String> {
        if self.labels.is_none() {
            self.load_labels()?;
        }
        for name in names {
            if self.labels.as_ref().is_some_and(|m| m.contains_key(name)) {
                continue;
            }
            let (status, label) = self.request(
                Method::POST,
                "/labels",
                Some(json!({"name":name,"color":"#ededed"})),
            )?;
            if status == StatusCode::CONFLICT || status == StatusCode::UNPROCESSABLE_ENTITY {
                self.load_labels()?;
                if self.labels.as_ref().is_some_and(|m| m.contains_key(name)) {
                    continue;
                }
            }
            if !status.is_success() {
                return Err(format!(
                    "Gitea POST label failed: HTTP {}.",
                    status.as_u16()
                ));
            }
            if required(&label, "name")? != name {
                return Err("Gitea returned an unexpected label name.".into());
            }
            let id = numeric_id(&label["id"])?;
            self.labels
                .as_mut()
                .ok_or("Gitea label cache unavailable.")?
                .insert(name.clone(), id);
        }
        names
            .iter()
            .map(|n| {
                self.labels
                    .as_ref()
                    .and_then(|m| m.get(n))
                    .copied()
                    .ok_or_else(|| "Gitea label missing after creation.".into())
            })
            .collect()
    }

    fn remote(&self, id: String) -> Remote {
        Remote {
            url: format!("{}/{}/{}/issues/{id}", self.base, self.owner, self.repo),
            remote_id: id,
        }
    }
}

fn required<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("Missing or invalid {field}."))
}
fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| (*v).into()).collect()
}
fn parse_json(text: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|_| "Tracker CLI returned invalid JSON.".into())
}
fn validate_id(id: &str) -> Result<String, String> {
    if id.is_empty()
        || !id.bytes().all(|b| b.is_ascii_digit())
        || id.parse::<u64>().ok().filter(|n| *n > 0).is_none()
    {
        return Err("Remote ID must be a positive numeric identifier.".into());
    }
    Ok(id.into())
}
fn value_id(value: &Value) -> Result<String, String> {
    match value {
        Value::String(s) => validate_id(s),
        Value::Number(n) => validate_id(&n.to_string()),
        _ => Err("Tracker returned no valid remote ID.".into()),
    }
}
fn numeric_id(value: &Value) -> Result<u64, String> {
    value
        .as_u64()
        .filter(|n| *n > 0)
        .ok_or_else(|| "Gitea returned an invalid numeric label ID.".into())
}
fn valid_segment(s: &str) -> bool {
    !s.is_empty()
        && s != "."
        && s != ".."
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
fn validate_base(base: &str) -> Result<String, String> {
    let url = Url::parse(base).map_err(|_| "Invalid tracker base URL.".to_string())?;
    if !["http", "https"].contains(&url.scheme())
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(
            "Tracker base URL requires HTTP(S), a host, and no credentials, query, or fragment."
                .into(),
        );
    }
    Ok(base.trim_end_matches('/').into())
}
fn closing(story: &Value) -> Option<&'static str> {
    match story["status"].as_str() {
        Some("done") => Some("completed"),
        Some("archived" | "split") => Some("not planned"),
        _ => None,
    }
}
fn managed(name: &str, gitea: bool) -> bool {
    ["story", "spike", "bug", "chore"].contains(&name)
        || (gitea && name == "not-planned")
        || ["size:", "value:", "epic:"]
            .iter()
            .any(|prefix| name.starts_with(prefix))
}
fn card_labels(card: &Value) -> Result<Vec<String>, String> {
    card["labels"]
        .as_array()
        .ok_or("Rendered issue has invalid labels.")?
        .iter()
        .map(|v| {
            v.as_str()
                .map(String::from)
                .ok_or_else(|| "Rendered issue has invalid label.".into())
        })
        .collect()
}
fn named_labels(issue: &Value) -> Result<Vec<String>, String> {
    issue["labels"]
        .as_array()
        .ok_or("Tracker returned invalid issue labels.")?
        .iter()
        .map(|v| Ok(required(v, "name")?.into()))
        .collect()
}
fn gitea_labels(card: &Value, story: &Value) -> Result<Vec<String>, String> {
    let mut names = card_labels(card)?;
    if closing(story) == Some("not planned") {
        names.push("not-planned".into());
    }
    Ok(names)
}
fn ensure_github_labels(
    cli: &Cli,
    repo: &str,
    known: &mut HashSet<String>,
    names: &[String],
) -> Result<(), String> {
    for name in names {
        if !known.contains(name) {
            cli.run(
                &strings(&["label", "create", name, "--repo", repo, "--force"]),
                None,
            )?;
            known.insert(name.clone());
        }
    }
    Ok(())
}
fn github_created_id(out: &str, repo: &str) -> Result<String, String> {
    let prefix = format!("https://github.com/{repo}/issues/");
    for word in out.split_whitespace() {
        if let Some(id) = word.strip_prefix(&prefix) {
            return validate_id(id);
        }
    }
    Err("gh issue create did not return an issue URL for the configured repository.".into())
}
fn github_state(
    cli: &Cli,
    repo: &str,
    id: &str,
    reason: Option<&str>,
    current: &str,
) -> Result<(), String> {
    if let Some(reason) = reason {
        if current == "OPEN" {
            cli.run(
                &strings(&["issue", "close", id, "--repo", repo, "--reason", reason]),
                None,
            )?;
        }
    } else if current == "CLOSED" {
        cli.run(&strings(&["issue", "reopen", id, "--repo", repo]), None)?;
    }
    Ok(())
}
fn ado_state(story: &Value, process: &str) -> Option<&'static str> {
    match (closing(story), process) {
        (Some("completed"), "agile") => Some("Closed"),
        (Some("not planned"), "agile" | "scrum") => Some("Removed"),
        (Some(_), _) => Some("Done"),
        _ => None,
    }
}
fn encode_component(value: &str) -> String {
    let mut out = String::new();
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}
fn ado_remote(org: &str, project: &str, id: String) -> Remote {
    Remote {
        url: format!("{}/{}/_workitems/edit/{id}", org, encode_component(project)),
        remote_id: id,
    }
}
fn html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
fn to_html(text: &str) -> String {
    let lines: Vec<_> = text.split('\n').collect();
    let items: Vec<_> = lines.iter().filter_map(|l| l.strip_prefix("- ")).collect();
    if !items.is_empty() && items.len() == lines.iter().filter(|l| !l.is_empty()).count() {
        return format!(
            "<ul>{}</ul>",
            items
                .iter()
                .map(|s| format!("<li>{}</li>", html(s)))
                .collect::<String>()
        );
    }
    let mut paragraphs = Vec::new();
    let mut rest = text;
    while let Some(index) = rest.find("\n\n") {
        paragraphs.push(&rest[..index]);
        rest = rest[index..].trim_start_matches('\n');
    }
    paragraphs.push(rest);
    paragraphs
        .iter()
        .map(|p| format!("<p>{}</p>", html(p).replace('\n', "<br>")))
        .collect::<Vec<_>>()
        .join("\n")
}
fn ado_content(card: &Value) -> Result<(String, Vec<String>), String> {
    let fields = card["fields"]
        .as_array()
        .ok_or("Rendered ADO card has invalid fields.")?;
    let mut kind = None;
    let mut args = strings(&["--title", required(card, "title")?]);
    let mut extra = Vec::new();
    for field in fields {
        let name = required(field, "name")?;
        let value = field
            .get("value")
            .and_then(Value::as_str)
            .ok_or("Rendered ADO field has invalid value.")?;
        match name {
            "Work Item Type" => kind = Some(value.to_string()),
            "Description" if !value.is_empty() => {
                args.extend(strings(&["--description", &to_html(value)]));
            }
            "Area Path" if !value.is_empty() => {
                args.extend(strings(&["--area", value]));
            }
            "Iteration Path" if !value.is_empty() => {
                args.extend(strings(&["--iteration", value]));
            }
            _ => {}
        }
        if let Some(reference) = field.get("ref").and_then(Value::as_str) {
            if ![
                "System.Description",
                "System.AreaPath",
                "System.IterationPath",
            ]
            .contains(&reference)
            {
                let value = if reference.contains("AcceptanceCriteria")
                    || reference.contains("ReproSteps")
                {
                    to_html(value)
                } else {
                    value.to_string()
                };
                extra.push(format!("{reference}={value}"));
            }
        }
    }
    if !extra.is_empty() {
        args.push("--fields".into());
        args.extend(extra);
    }
    Ok((
        kind.ok_or("Rendered ADO card has no work item type.")?,
        args,
    ))
}

fn validate_token(token: &str) -> Result<(), String> {
    if token.is_empty() || token.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(
            "Tracker credential is empty or contains whitespace/control characters.".into(),
        );
    }
    Ok(())
}
fn load_secret(root: &Path, filename: &str, variable: &str) -> Result<Option<String>, String> {
    let dir = root.join(".story-time");
    if let Ok(meta) = fs::symlink_metadata(&dir) {
        if meta.file_type().is_symlink() || !meta.is_dir() {
            return Err("Secret directory must be a real directory, not a symlink.".into());
        }
    }
    let path = dir.join(filename);
    let meta = match fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("Could not inspect tracker secret file.".into()),
    };
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err("Tracker secret must be a regular file, not a symlink.".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.mode() & 0o077 != 0 {
            return Err(
                "Tracker secret has unsafe permissions; run chmod 600 on its .story-time env file."
                    .into(),
            );
        }
    }
    let repository = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(root)
        .output()
        .map_err(|_| "Could not verify whether tracker secret is tracked by git.".to_string())?;
    if repository.status.success() {
        let tracked = Command::new("git")
            .args(["ls-files", "--stage", "--"])
            .arg(&path)
            .current_dir(root)
            .output()
            .map_err(|_| "Could not check tracker secret in git.".to_string())?;
        if !tracked.status.success() {
            return Err("Could not check tracker secret in git.".into());
        }
        if !tracked.stdout.is_empty() {
            return Err(
                "Tracker secret is committed to git; rotate it, then remove it from the git index."
                    .into(),
            );
        }
    } else if !String::from_utf8_lossy(&repository.stderr).contains("not a git repository") {
        return Err("Could not verify tracker secret git status.".into());
    }
    let file = File::open(&path).map_err(|_| "Could not open tracker secret file.".to_string())?;
    let opened = file
        .metadata()
        .map_err(|_| "Could not inspect opened tracker secret.".to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if opened.ino() != meta.ino() || opened.dev() != meta.dev() || opened.mode() & 0o077 != 0 {
            return Err("Tracker secret changed while opening it.".into());
        }
    }
    let mut text = String::new();
    file.take(65537)
        .read_to_string(&mut text)
        .map_err(|_| "Could not read tracker secret file.".to_string())?;
    if text.len() > 65536 {
        return Err("Tracker secret file exceeds size limit.".into());
    }
    let mut token = None;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            if key.trim() == variable {
                let mut value = value.trim();
                if value.len() >= 2
                    && ((value.starts_with('\"') && value.ends_with('\"'))
                        || (value.starts_with('\'') && value.ends_with('\'')))
                {
                    value = &value[1..value.len() - 1];
                }
                if value.is_empty() {
                    token = None;
                } else {
                    validate_token(value)?;
                    token = Some(value.into());
                }
            }
        }
    }
    Ok(token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::thread;
    use std::time::Instant;

    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "story-gateway-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    fn story(status: &str) -> Value {
        json!({"id":"ST-001","type":"story","title":"Native tracker","status":status,"size":3,"value":8,
            "sections":{"Story":"As a maintainer\nI push safely.","Acceptance Criteria":"- [ ] Given a story, When pushed, Then it is linked."}})
    }
    fn mock(responses: Vec<(&'static str, u16, Value)>) -> (Gitea, thread::JoinHandle<Vec<Value>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let handle = thread::spawn(move || {
            let mut bodies = Vec::new();
            for (expected, status, response) in responses {
                let deadline = Instant::now() + Duration::from_secs(8);
                let mut socket = loop {
                    match listener.accept() {
                        Ok((socket, _)) => break socket,
                        Err(e)
                            if e.kind() == std::io::ErrorKind::WouldBlock
                                && Instant::now() < deadline =>
                        {
                            thread::sleep(Duration::from_millis(5))
                        }
                        Err(e) => panic!("mock HTTP accept failed: {e}"),
                    }
                };
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut data = Vec::new();
                let mut buf = [0; 4096];
                let (header_end, length) = loop {
                    let n = socket.read(&mut buf).unwrap();
                    assert!(n > 0);
                    data.extend_from_slice(&buf[..n]);
                    if let Some(index) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&data[..index]);
                        assert_eq!(
                            headers.lines().next().unwrap(),
                            format!("{expected} HTTP/1.1")
                        );
                        assert!(headers
                            .to_lowercase()
                            .contains("authorization: token test-private"));
                        let length = headers
                            .lines()
                            .find_map(|line| {
                                line.to_lowercase()
                                    .strip_prefix("content-length: ")
                                    .and_then(|s| s.parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        break (index + 4, length);
                    }
                };
                while data.len() < header_end + length {
                    let n = socket.read(&mut buf).unwrap();
                    assert!(n > 0);
                    data.extend_from_slice(&buf[..n]);
                }
                let body = &data[header_end..header_end + length];
                bodies.push(if body.is_empty() {
                    Value::Null
                } else {
                    serde_json::from_slice(body).unwrap()
                });
                let body = response.to_string();
                write!(socket,"HTTP/1.1 {status} Mock\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
            }
            bodies
        });
        (
            Gitea {
                base,
                owner: "acme".into(),
                repo: "shop".into(),
                token: "test-private".into(),
                client: Client::builder()
                    .no_proxy()
                    .redirect(reqwest::redirect::Policy::none())
                    .timeout(Duration::from_secs(3))
                    .build()
                    .unwrap(),
                labels: None,
            },
            handle,
        )
    }

    #[test]
    fn validates_identifiers_and_urls_before_io() {
        for id in [
            "",
            "0",
            "-1",
            "1/labels",
            "1?token=x",
            "--help",
            "1.5",
            "18446744073709551616",
        ] {
            assert!(validate_id(id).is_err());
        }
        assert_eq!(validate_id("42").unwrap(), "42");
        for url in [
            "file:///etc/passwd",
            "https://token@example.com",
            "https://example.com?token=x",
            "https://example.com/#x",
        ] {
            assert!(validate_base(url).is_err());
        }
        assert_eq!(
            validate_base("http://127.0.0.1:3000/").unwrap(),
            "http://127.0.0.1:3000"
        );
        assert!(github_created_id("https://github.com/wrong/repo/issues/3", "acme/shop").is_err());
        assert_eq!(
            github_created_id("https://github.com/acme/shop/issues/3\n", "acme/shop").unwrap(),
            "3"
        );
    }

    #[test]
    fn ado_maps_html_fields_and_process_states() {
        let card = json!({"title":"Example","fields":[
            {"name":"Work Item Type","value":"Bug"},
            {"name":"Repro Steps","value":"<bad>\nnext","ref":"Microsoft.VSTS.TCM.ReproSteps"},
            {"name":"Acceptance Criteria","value":"- one\n- two","ref":"Microsoft.VSTS.Common.AcceptanceCriteria"},
            {"name":"Area Path","value":"Shop\\UI","ref":"System.AreaPath"}
        ]});
        let (kind, args) = ado_content(&card).unwrap();
        assert_eq!(kind, "Bug");
        assert_eq!(to_html("\ntext\n"), "<p><br>text<br></p>");
        assert_eq!(to_html("\n\ntext\n\n\n"), "<p></p>\n<p>text</p>\n<p></p>");
        assert!(args.contains(&"Microsoft.VSTS.TCM.ReproSteps=<p>&lt;bad&gt;<br>next</p>".into()));
        assert!(args.contains(
            &"Microsoft.VSTS.Common.AcceptanceCriteria=<ul><li>one</li><li>two</li></ul>".into()
        ));
        assert!(args.contains(&"--area".into()));
        for (process, done, archived) in [
            ("agile", "Closed", "Removed"),
            ("scrum", "Done", "Removed"),
            ("basic", "Done", "Done"),
        ] {
            assert_eq!(ado_state(&story("done"), process), Some(done));
            assert_eq!(ado_state(&story("archived"), process), Some(archived));
            assert_eq!(ado_state(&story("ready"), process), None);
        }
        assert_eq!(
            ado_remote("https://dev.azure.com/acme", "My project/one", "7".into()).url,
            "https://dev.azure.com/acme/My%20project%2Fone/_workitems/edit/7"
        );
    }

    #[test]
    fn gitea_create_paginates_and_reuses_cached_labels() {
        let labels = json!([{"id":1,"name":"story"},{"id":2,"name":"size:3"}]);
        let (gitea, handle) = mock(vec![
            (
                "GET /api/v1/repos/acme/shop/labels?page=1&limit=50",
                200,
                labels,
            ),
            (
                "GET /api/v1/repos/acme/shop/labels?page=2&limit=50",
                200,
                json!([{"id":3,"name":"value:8"}]),
            ),
            (
                "GET /api/v1/repos/acme/shop/labels?page=3&limit=50",
                200,
                json!([]),
            ),
            (
                "POST /api/v1/repos/acme/shop/issues",
                201,
                json!({"number":9}),
            ),
            ("PATCH /api/v1/repos/acme/shop/issues/9", 200, json!({})),
            (
                "POST /api/v1/repos/acme/shop/issues",
                201,
                json!({"number":10}),
            ),
        ]);
        let mut gateway = Gateway {
            config: json!({}),
            backend: Backend::Gitea(gitea),
        };
        assert_eq!(gateway.create(&story("done")).unwrap().remote_id, "9");
        assert_eq!(gateway.create(&story("ready")).unwrap().remote_id, "10");
        let bodies = handle.join().unwrap();
        assert_eq!(bodies[3]["labels"], json!([1, 2, 3]));
        assert_eq!(bodies[4], json!({"state":"closed"}));
        assert_eq!(bodies[3]["title"], "ST-001: Native tracker");
        assert_eq!(bodies[5]["title"], "ST-001: Native tracker");
    }

    #[test]
    fn gitea_update_preserves_team_labels_and_reopens() {
        let (gitea, handle) = mock(vec![
            (
                "GET /api/v1/repos/acme/shop/issues/4",
                200,
                json!({"labels":[{"id":99,"name":"team"},{"id":12,"name":"not-planned"},{"id":13,"name":"size:5"}]}),
            ),
            (
                "GET /api/v1/repos/acme/shop/labels?page=1&limit=50",
                200,
                json!([{"id":1,"name":"story"},{"id":2,"name":"size:3"},{"id":3,"name":"value:8"}]),
            ),
            (
                "GET /api/v1/repos/acme/shop/labels?page=2&limit=50",
                200,
                json!([]),
            ),
            ("PATCH /api/v1/repos/acme/shop/issues/4", 200, json!({})),
            (
                "PUT /api/v1/repos/acme/shop/issues/4/labels",
                200,
                json!([]),
            ),
        ]);
        let mut gateway = Gateway {
            config: json!({}),
            backend: Backend::Gitea(gitea),
        };
        let mut s = story("ready");
        s["remote"] = json!(4);
        gateway.update(&s).unwrap();
        let bodies = handle.join().unwrap();
        assert_eq!(bodies[3]["state"], "open");
        assert_eq!(bodies[3]["title"], "ST-001: Native tracker");
        assert_eq!(bodies[4]["labels"], json!([1, 2, 3, 99]));
    }

    #[test]
    fn gitea_remove_adds_not_planned_without_replacing_labels() {
        let (gitea, handle) = mock(vec![
            (
                "GET /api/v1/repos/acme/shop/labels?page=1&limit=50",
                200,
                json!([]),
            ),
            (
                "POST /api/v1/repos/acme/shop/labels",
                201,
                json!({"name":"not-planned","id":8}),
            ),
            (
                "POST /api/v1/repos/acme/shop/issues/3/labels",
                200,
                json!([]),
            ),
            ("PATCH /api/v1/repos/acme/shop/issues/3", 200, json!({})),
        ]);
        let mut gateway = Gateway {
            config: json!({}),
            backend: Backend::Gitea(gitea),
        };
        gateway.remove("3").unwrap();
        let bodies = handle.join().unwrap();
        assert_eq!(bodies[1], json!({"name":"not-planned","color":"#ededed"}));
        assert_eq!(bodies[2], json!({"labels":[8]}));
        assert_eq!(bodies[3], json!({"state":"closed"}));
    }

    #[test]
    fn gitea_label_creation_handles_concurrent_creator() {
        let (mut gitea, handle) = mock(vec![
            (
                "GET /api/v1/repos/acme/shop/labels?page=1&limit=50",
                200,
                json!([]),
            ),
            (
                "POST /api/v1/repos/acme/shop/labels",
                409,
                json!({"message":"already exists"}),
            ),
            (
                "GET /api/v1/repos/acme/shop/labels?page=1&limit=50",
                200,
                json!([{"id":5,"name":"story"}]),
            ),
            (
                "GET /api/v1/repos/acme/shop/labels?page=2&limit=50",
                200,
                json!([]),
            ),
        ]);
        assert_eq!(gitea.ensure_labels(&["story".into()]).unwrap(), vec![5]);
        assert_eq!(gitea.ensure_labels(&["story".into()]).unwrap(), vec![5]);
        handle.join().unwrap();
    }

    #[test]
    fn gitea_errors_do_not_echo_server_secrets_and_redirects_are_not_followed() {
        for status in [401, 302] {
            let (gitea, handle) = mock(vec![(
                "GET /api/v1/repos/acme/shop/issues/1",
                status,
                json!({"message":"test-private"}),
            )]);
            let error = gitea.api(Method::GET, "/issues/1", None).unwrap_err();
            assert!(error.contains(&format!("HTTP {status}")));
            assert!(!error.contains("test-private"));
            handle.join().unwrap();
        }
    }

    #[cfg(unix)]
    #[test]
    fn secret_files_reject_permissions_symlinks_and_tracked_credentials() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let tmp = Temp::new();
        fs::create_dir(tmp.0.join(".story-time")).unwrap();
        let path = tmp.0.join(".story-time/.github.env");
        assert_eq!(
            load_secret(&tmp.0, ".github.env", "GH_TOKEN").unwrap(),
            None
        );
        fs::write(&path, "# local\nGH_TOKEN=\"private-token\"\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(load_secret(&tmp.0, ".github.env", "GH_TOKEN")
            .unwrap_err()
            .contains("permissions"));
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            load_secret(&tmp.0, ".github.env", "GH_TOKEN").unwrap(),
            Some("private-token".into())
        );
        symlink(&path, tmp.0.join(".story-time/.ado.env")).unwrap();
        assert!(load_secret(&tmp.0, ".ado.env", "AZURE_DEVOPS_EXT_PAT")
            .unwrap_err()
            .contains("symlink"));
        assert!(Command::new("git")
            .args(["init", "--quiet"])
            .arg(&tmp.0)
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["add", "--", ".story-time/.github.env"])
            .current_dir(&tmp.0)
            .status()
            .unwrap()
            .success());
        assert!(load_secret(&tmp.0, ".github.env", "GH_TOKEN")
            .unwrap_err()
            .contains("committed to git"));
    }

    #[test]
    fn constructor_credentials_child() {
        let Some(root) = std::env::var_os("STORY_GATEWAY_CREDENTIAL_TEST_ROOT") else {
            return;
        };
        let tracker = std::env::var("STORY_GATEWAY_CREDENTIAL_TEST_TRACKER").unwrap();
        let config = json!({"tracker":tracker,"github":{"repo":"acme/shop"},
            "ado":{"organization":"https://dev.azure.com/acme","project":"Shop"},
            "gitea":{"baseUrl":"http://127.0.0.1:9","owner":"acme","repo":"shop"}});
        let result = Gateway::new(&config, Path::new(&root));
        let expected = std::env::var("STORY_GATEWAY_CREDENTIAL_TEST_EXPECTED").unwrap();
        if expected == "unsafe" {
            assert!(result.err().unwrap().contains("permissions"));
        } else {
            let gateway = result.unwrap();
            let token = match gateway.backend {
                Backend::Gitea(g) => Some(g.token),
                Backend::Github { cli, .. } | Backend::Ado { cli, .. } => {
                    cli.token.map(|(_, value)| value)
                }
            };
            assert_eq!(
                token.as_deref(),
                if expected == "none" {
                    None
                } else {
                    Some(expected.as_str())
                }
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn constructor_prefers_environment_and_checks_local_secrets() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = Temp::new();
        fs::create_dir(tmp.0.join(".story-time")).unwrap();
        for (tracker, variable) in [
            ("github", "GH_TOKEN"),
            ("ado", "AZURE_DEVOPS_EXT_PAT"),
            ("gitea", "GITEA_TOKEN"),
        ] {
            let run = |environment: Option<&str>, expected: &str| {
                let mut command = Command::new(std::env::current_exe().unwrap());
                command
                    .args([
                        "--exact",
                        "gateway::tests::constructor_credentials_child",
                        "--nocapture",
                    ])
                    .env_remove("GH_TOKEN")
                    .env_remove("GITHUB_TOKEN")
                    .env_remove("AZURE_DEVOPS_EXT_PAT")
                    .env_remove("GITEA_TOKEN")
                    .env("STORY_GATEWAY_CREDENTIAL_TEST_ROOT", &tmp.0)
                    .env("STORY_GATEWAY_CREDENTIAL_TEST_TRACKER", tracker)
                    .env("STORY_GATEWAY_CREDENTIAL_TEST_EXPECTED", expected);
                if let Some(value) = environment {
                    command.env(variable, value);
                }
                let out = command.output().unwrap();
                assert!(
                    out.status.success(),
                    "{}{}",
                    String::from_utf8_lossy(&out.stdout),
                    String::from_utf8_lossy(&out.stderr)
                );
            };
            if tracker != "gitea" {
                run(None, "none");
            }
            let file = tmp.0.join(format!(".story-time/.{tracker}.env"));
            fs::write(&file, format!("{variable}=from-file\n")).unwrap();
            fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
            run(None, "from-file");
            run(Some("from-environment"), "from-environment");
            fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
            run(
                Some("from-environment"),
                if tracker == "gitea" {
                    "from-environment"
                } else {
                    "unsafe"
                },
            );
        }
    }

    fn stub_cli(root: &Path, program: &str) -> Cli {
        let executable = root.join(program);
        let source = r#"
            use std::{env,fs::OpenOptions,io::{Read,Write}};
            fn main() {
                let args:Vec<String>=env::args().skip(1).collect();
                assert!(!args.iter().any(|s|s.contains("private-cli-token")));
                assert!(env::var("GH_TOKEN").ok().as_deref()==Some("private-cli-token") || env::var("AZURE_DEVOPS_EXT_PAT").ok().as_deref()==Some("private-cli-token"));
                let mut input=String::new(); std::io::stdin().read_to_string(&mut input).unwrap();
                let mut log=OpenOptions::new().create(true).append(true).open("calls").unwrap();
                writeln!(log,"{:?} stdin={}",args,input).unwrap();
                if args.iter().any(|s|s=="fail") { eprintln!("private-cli-token");std::process::exit(7); }
                if args.first().map(String::as_str)==Some("issue") {
                    match args[1].as_str() {
                        "create"=>println!("https://github.com/acme/shop/issues/11"),
                        "view"=>println!("{{\"labels\":[{{\"name\":\"team\"}},{{\"name\":\"size:5\"}}],\"state\":\"CLOSED\"}}"),
                        "list"=>println!("[{{\"number\":11,\"title\":\"ST-001: Native tracker\"}},{{\"number\":13,\"title\":\"ST-0011: Other\"}},{{\"number\":14,\"title\":\"Re: ST-001: x\"}}]"),
                        _=>{}
                    }
                } else if args.iter().any(|s|s=="create") && args.first().map(String::as_str)==Some("boards") { println!("{{\"id\":12}}"); }
                else if args.get(1).map(String::as_str)==Some("query") { println!("[{{\"id\":12,\"fields\":{{\"System.Title\":\"ST-001: Native tracker\"}}}},{{\"id\":15,\"fields\":{{\"System.Title\":\"ST-0011: Other\"}}}}]"); }
            }
        "#;
        let mut compiler = Command::new("rustc")
            .args(["--crate-name", "tracker_stub", "-o"])
            .arg(&executable)
            .arg("-")
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        compiler
            .stdin
            .take()
            .unwrap()
            .write_all(source.as_bytes())
            .unwrap();
        let out = compiler.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        Cli {
            program: executable,
            root: root.into(),
            token: Some((
                if program == "gh" {
                    "GH_TOKEN"
                } else {
                    "AZURE_DEVOPS_EXT_PAT"
                }
                .into(),
                "private-cli-token".into(),
            )),
        }
    }

    #[test]
    fn github_commands_use_stdin_preserve_team_labels_and_sync_state() {
        let tmp = Temp::new();
        let cli = stub_cli(&tmp.0, "gh");
        assert!(!cli
            .run(&strings(&["fail"]), None)
            .unwrap_err()
            .contains("private-cli-token"));
        let mut gateway = Gateway {
            config: json!({}),
            backend: Backend::Github {
                repo: "acme/shop".into(),
                cli,
                labels: HashSet::new(),
            },
        };
        assert_eq!(
            gateway.find(&story("ready")).unwrap(),
            Some(Remote {
                remote_id: "11".into(),
                url: "https://github.com/acme/shop/issues/11".into()
            })
        );
        assert_eq!(gateway.create(&story("done")).unwrap().remote_id, "11");
        let mut s = story("ready");
        s["remote"] = json!(11);
        gateway.update(&s).unwrap();
        gateway.remove("11").unwrap();
        let calls = fs::read_to_string(tmp.0.join("calls")).unwrap();
        assert!(calls.contains("\"--body-file\", \"-\""));
        assert!(calls.contains("\"list\", \"--repo\", \"acme/shop\", \"--state\", \"open\""));
        assert!(calls.contains("stdin=As a maintainer"));
        assert!(calls.contains("\"--remove-label\", \"size:5\""));
        assert!(!calls.contains("\"--remove-label\", \"team\""));
        assert!(calls.contains("\"reopen\""));
        assert!(calls.contains("\"completed\""));
        assert!(calls.contains("\"not planned\""));
        assert!(!calls.contains("private-cli-token"));
        assert_eq!(
            calls
                .lines()
                .filter(|s| s.contains("\"label\", \"create\", \"story\""))
                .count(),
            1
        );
    }

    #[test]
    fn ado_commands_use_environment_and_process_state_mapping() {
        let tmp = Temp::new();
        let cli = stub_cli(&tmp.0, "az");
        let config = json!({"ado":{"process":"agile","organization":"https://dev.azure.com/acme","project":"Shop"}});
        let mut gateway = Gateway {
            config,
            backend: Backend::Ado {
                organization: "https://dev.azure.com/acme".into(),
                project: "Shop".into(),
                process: "agile".into(),
                cli,
            },
        };
        assert_eq!(
            gateway.find(&story("ready")).unwrap().unwrap().remote_id,
            "12"
        );
        assert_eq!(gateway.create(&story("done")).unwrap().remote_id, "12");
        let mut s = story("archived");
        s["remote"] = json!(12);
        gateway.update(&s).unwrap();
        gateway.remove("12").unwrap();
        let calls = fs::read_to_string(tmp.0.join("calls")).unwrap();
        assert!(calls.contains("\"--type\", \"User Story\""));
        assert!(
            calls.contains("[System.TeamProject] = 'Shop' AND [System.Title] CONTAINS 'ST-001:'")
        );
        assert!(calls.contains("\"--state\", \"Closed\""));
        assert!(calls.contains("\"--state\", \"Removed\""));
        assert!(calls.contains("\"delete\", \"--id\", \"12\""));
        assert!(calls.contains("\"--yes\""));
        assert!(!calls.contains("private-cli-token"));
    }
}
