use crate::{validate_base, Error, Gateway};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Value};
use std::{
    collections::HashSet,
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GiteaSettings {
    pub base_url: String,
    pub owner: String,
    pub repo: String,
}

#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SyncMode {
    #[default]
    Off,
    Ask,
    Auto,
}

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub tracker: String,
    pub gitea: GiteaSettings,
    #[serde(default)]
    pub sync: SyncMode,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Create,
    Update,
    Skip,
}

#[derive(Debug)]
pub struct Story {
    pub original: String,
    pub id: String,
    pub title: String,
    pub status: String,
    pub remote: Option<u64>,
    pub remote_url: Option<String>,
    metadata: Mapping,
    front_start: usize,
    front_end: usize,
    body_start: usize,
}

#[derive(Debug, Serialize)]
pub struct PushPlan {
    pub path: PathBuf,
    pub id: String,
    pub action: Action,
    pub title: String,
    pub body: String,
    pub state: &'static str,
    pub remote: Option<u64>,
    pub remote_url: Option<String>,
    pub labels: Vec<String>,
    pub completed: bool,
    #[serde(skip)]
    story: Option<Story>,
}

#[derive(Debug, Serialize)]
pub struct PushOutput {
    pub status: &'static str,
    pub dry_run: bool,
    pub plan: Vec<PushPlan>,
    pub warnings: Vec<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<&'static str>,
}

#[derive(Deserialize)]
struct StoryIssue {
    number: u64,
    html_url: String,
}

#[derive(Deserialize)]
struct Label {
    id: u64,
    name: String,
}

impl Label {
    fn managed(&self) -> bool {
        ["story", "spike", "bug", "chore"].contains(&self.name.as_str())
            || ["size:", "value:", "epic:"]
                .iter()
                .any(|prefix| self.name.starts_with(prefix))
    }
}

#[derive(Serialize)]
struct LabelIds {
    labels: Vec<u64>,
}

fn labels_url(
    gateway: &Gateway,
    settings: &Settings,
    number: Option<u64>,
) -> Result<Url, &'static str> {
    let mut url = gateway
        .endpoint(&settings.gitea.owner, &settings.gitea.repo, number)
        .map_err(error_code)?;
    let mut parts = url
        .path_segments_mut()
        .map_err(|_| "invalid_configuration")?;
    if number.is_none() {
        parts.pop();
    }
    parts.push("labels");
    drop(parts);
    Ok(url)
}

fn read_labels(gateway: &Gateway, url: Url) -> Result<Vec<Label>, &'static str> {
    let labels: Vec<Label> = gateway
        .send(reqwest::Method::GET, url, &())
        .map_err(error_code)?;
    if labels
        .iter()
        .any(|label| label.id == 0 || label.name.is_empty())
    {
        return Err("invalid_response");
    }
    Ok(labels)
}

fn list_labels(gateway: &Gateway, url: Url) -> Result<Vec<Label>, &'static str> {
    let mut labels = Vec::new();
    for page in 1..=1000 {
        let mut url = url.clone();
        url.query_pairs_mut()
            .append_pair("limit", "50")
            .append_pair("page", &page.to_string());
        let batch = read_labels(gateway, url)?;
        let finished = batch.len() < 50;
        labels.extend(batch);
        if finished {
            return Ok(labels);
        }
    }
    Err("response_too_large")
}

fn desired_label_ids(
    gateway: &Gateway,
    url: &Url,
    known: &mut Vec<Label>,
    desired: &[String],
) -> Result<Vec<u64>, &'static str> {
    let mut ids = Vec::new();
    for name in desired {
        if !known.iter().any(|label| &label.name == name) {
            #[derive(Serialize)]
            struct NewLabel<'a> {
                name: &'a str,
                color: &'static str,
            }
            let label: Label = gateway
                .send(
                    reqwest::Method::POST,
                    url.clone(),
                    &NewLabel {
                        name,
                        color: "ededed",
                    },
                )
                .map_err(error_code)?;
            if label.id == 0 || &label.name != name {
                return Err("invalid_response");
            }
            known.push(label);
        }
        ids.push(
            known
                .iter()
                .find(|label| &label.name == name)
                .ok_or("invalid_response")?
                .id,
        );
    }
    Ok(ids)
}

#[derive(Serialize)]
struct StoryPayload<'a> {
    title: &'a str,
    body: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    state: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    labels: Option<&'a [u64]>,
}

fn text<'a>(metadata: &'a Mapping, key: &str) -> Result<&'a str, &'static str> {
    metadata
        .get(Value::String(key.into()))
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or("invalid_story")
}

impl Settings {
    pub fn validate(&self) -> Result<Url, &'static str> {
        if self.tracker != "gitea"
            || [&self.gitea.owner, &self.gitea.repo].iter().any(|s| {
                s.is_empty()
                    || *s == "."
                    || *s == ".."
                    || !s
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            })
        {
            return Err("invalid_configuration");
        }
        validate_base(&self.gitea.base_url).map_err(|_| "invalid_configuration")
    }

    fn issue_url(&self, number: u64) -> Result<Url, &'static str> {
        let mut url = self.validate()?;
        url.path_segments_mut()
            .map_err(|_| "invalid_configuration")?
            .pop_if_empty()
            .extend([
                &self.gitea.owner,
                &self.gitea.repo,
                "issues",
                &number.to_string(),
            ]);
        Ok(url)
    }

    fn check_link(&self, number: u64, value: &str) -> Result<(), &'static str> {
        let actual = Url::parse(value).map_err(|_| "target_mismatch")?;
        if number == 0 || actual != self.issue_url(number)? || value.contains(['\r', '\n']) {
            return Err("target_mismatch");
        }
        Ok(())
    }
}

fn fence_delimiter(line: &str) -> Option<(u8, usize, &str)> {
    let content = line.trim_start_matches(' ');
    if line.len() - content.len() > 3 {
        return None;
    }
    let marker = *content.as_bytes().first()?;
    if !matches!(marker, b'`' | b'~') {
        return None;
    }
    let width = content.bytes().take_while(|byte| *byte == marker).count();
    (width >= 3).then(|| (marker, width, &content[width..]))
}

impl Story {
    pub fn parse(original: String) -> Result<Self, &'static str> {
        let mut lines = original.split_inclusive('\n');
        let first = lines.next().ok_or("invalid_story")?;
        if first.trim_end_matches(['\r', '\n']) != "---" {
            return Err("invalid_story");
        }
        let front_start = first.len();
        let mut front_end = front_start;
        let mut body_start = None;
        for line in lines {
            if line.trim_end_matches(['\r', '\n']) == "---" {
                body_start = Some(front_end + line.len());
                break;
            }
            front_end += line.len();
        }
        let body_start = body_start.ok_or("invalid_story")?;
        let metadata: Mapping =
            serde_yaml::from_str(&original[front_start..front_end]).map_err(|_| "invalid_story")?;
        let id = text(&metadata, "id")?.to_owned();
        let title = text(&metadata, "title")?.to_owned();
        let status = text(&metadata, "status")?.to_owned();
        if ![
            "draft",
            "refined",
            "ready",
            "in-progress",
            "done",
            "split",
            "archived",
        ]
        .contains(&status.as_str())
        {
            return Err("invalid_story");
        }
        if let Some(kind) = metadata.get(Value::String("type".into())) {
            if !["story", "spike", "bug", "chore"].contains(&kind.as_str().ok_or("invalid_story")?)
            {
                return Err("invalid_story");
            }
        }
        let remote = match metadata.get(Value::String("remote".into())) {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) if s.is_empty() => None,
            Some(Value::String(s)) => Some(s.parse::<u64>().map_err(|_| "invalid_story")?),
            Some(v) => Some(v.as_u64().ok_or("invalid_story")?),
        };
        if remote == Some(0) {
            return Err("invalid_story");
        }
        let remote_url = match metadata.get(Value::String("remote_url".into())) {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) if s.is_empty() => None,
            Some(Value::String(s)) => Some(s.clone()),
            _ => return Err("invalid_story"),
        };
        if remote.is_none() && remote_url.is_some() {
            return Err("invalid_story");
        }
        Ok(Self {
            original,
            id,
            title,
            status,
            remote,
            remote_url,
            metadata,
            front_start,
            front_end,
            body_start,
        })
    }

    fn terminal(&self) -> bool {
        ["done", "split", "archived"].contains(&self.status.as_str())
    }

    fn render(&self) -> (String, Vec<String>) {
        let mut sections: Vec<(String, String)> = Vec::new();
        let mut fence: Option<(u8, usize)> = None;
        for line in self.original[self.body_start..].lines() {
            let was_fenced = fence.is_some();
            if let Some((marker, width, rest)) = fence_delimiter(line) {
                match fence {
                    Some((opening, minimum))
                        if marker == opening
                            && width >= minimum
                            && rest.bytes().all(|byte| matches!(byte, b' ' | b'\t')) =>
                    {
                        fence = None;
                    }
                    None if marker != b'`' || !rest.contains('`') => {
                        fence = Some((marker, width));
                    }
                    _ => {}
                }
            }
            if !was_fenced {
                if let Some(name) = line.strip_prefix("## ") {
                    sections.push((name.trim().into(), String::new()));
                    continue;
                }
            }
            if let Some((_, content)) = sections.last_mut() {
                content.push_str(line);
                content.push('\n');
            }
        }
        let section = |name: &str| {
            sections
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, s)| s.trim())
                .unwrap_or("")
        };
        let spike = self
            .metadata
            .get(Value::String("type".into()))
            .and_then(Value::as_str)
            == Some("spike");
        let mut parts = Vec::new();
        if spike {
            parts.push(format!("### Question\n{}", section("Question")));
            if let Some(v) = self
                .metadata
                .get(Value::String("timebox".into()))
                .and_then(Value::as_str)
            {
                parts.push(format!("Timebox: {v}"));
            }
        } else if !section("Story").is_empty() {
            parts.push(section("Story").into());
        }
        let criteria = if spike {
            "Exit Criteria"
        } else {
            "Acceptance Criteria"
        };
        if !section(criteria).is_empty() {
            parts.push(format!("### {criteria}\n{}", section(criteria)));
        }
        for (name, content) in &sections {
            if ![
                "Story",
                "Question",
                "Exit Criteria",
                "Acceptance Criteria",
                "Changelog",
                "Test Plan",
            ]
            .contains(&name.as_str())
                && !content.trim().is_empty()
            {
                parts.push(format!("### {name}\n{}", content.trim()));
            }
        }
        let number = |key: &str| {
            self.metadata
                .get(Value::String(key.into()))
                .and_then(Value::as_f64)
        };
        let score = match (
            number("value"),
            number("time_criticality"),
            number("risk_reduction"),
            number("size"),
        ) {
            (Some(v), Some(t), Some(r), Some(s)) if s > 0.0 => {
                format!(" · WSJF {:.2}", (v + t + r) / s)
            }
            _ => String::new(),
        };
        parts.push(format!("<sub>story-time {}{score}</sub>", self.id));
        let mut labels = vec![self
            .metadata
            .get(Value::String("type".into()))
            .and_then(Value::as_str)
            .unwrap_or("story")
            .to_owned()];
        for key in ["size", "value"] {
            if let Some(v) = number(key) {
                labels.push(format!("{key}:{v}"));
            }
        }
        if let Some(epic) = self
            .metadata
            .get(Value::String("epic".into()))
            .and_then(Value::as_str)
        {
            if !epic.is_empty() {
                labels.push(format!("epic:{epic}"));
            }
        }
        (parts.join("\n\n"), labels)
    }

    fn linked(&self, number: u64, url: &str) -> Result<String, &'static str> {
        let newline = if self.original.starts_with("---\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        let mut front = String::new();
        for line in self.original[self.front_start..self.front_end].split_inclusive('\n') {
            let key = line.split_once(':').map(|(k, _)| k.trim_end());
            if matches!(key, Some("remote" | "remote_url")) {
                continue;
            }
            front.push_str(line);
        }
        front.push_str(&format!(
            "remote: {number}{newline}remote_url: {}{newline}",
            serde_json::to_string(url).map_err(|_| "invalid_story")?
        ));
        let linked = format!(
            "{}{}{}",
            &self.original[..self.front_start],
            front,
            &self.original[self.front_end..]
        );
        let parsed = Story::parse(linked.clone())?;
        let mut expected = self.metadata.clone();
        expected.insert(Value::String("remote".into()), Value::Number(number.into()));
        expected.insert(
            Value::String("remote_url".into()),
            Value::String(url.into()),
        );
        if parsed.metadata != expected {
            return Err("unsupported_link_format");
        }
        Ok(linked)
    }
}

pub fn prepare(root: &Path, paths: &[PathBuf]) -> Result<(Settings, Vec<PushPlan>), &'static str> {
    if paths.is_empty() {
        return Err("invalid_arguments");
    }
    let root = fs::canonicalize(root).map_err(|_| "input_io")?;
    let bytes = fs::read(root.join(".story-time/settings.json")).map_err(|_| "input_io")?;
    let settings: Settings = serde_json::from_slice(&bytes).map_err(|_| "invalid_configuration")?;
    settings.validate()?;
    let mut seen = HashSet::new();
    let mut ids = HashSet::new();
    let mut remotes = HashSet::new();
    let mut plans = Vec::new();
    for path in paths {
        if path.is_absolute()
            || path
                .components()
                .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
            || path.extension().and_then(|v| v.to_str()) != Some("md")
        {
            return Err("invalid_path");
        }
        let absolute = fs::canonicalize(root.join(path)).map_err(|_| "input_io")?;
        if !absolute.starts_with(&root) || !seen.insert(absolute.clone()) {
            return Err("invalid_path");
        }
        let original = fs::read_to_string(&absolute).map_err(|_| "input_io")?;
        let story = Story::parse(original)?;
        if !ids.insert(story.id.clone()) || story.remote.is_some_and(|n| !remotes.insert(n)) {
            return Err("duplicate_story");
        }
        if let (Some(number), Some(url)) = (story.remote, &story.remote_url) {
            settings.check_link(number, url)?;
        }
        let sample_number = story.remote.unwrap_or(1);
        story.linked(sample_number, settings.issue_url(sample_number)?.as_str())?;
        let action = match (story.remote, story.terminal()) {
            (Some(_), _) => Action::Update,
            (None, true) => Action::Skip,
            (None, false) => Action::Create,
        };
        let (body, labels) = story.render();
        plans.push(PushPlan {
            path: absolute,
            id: story.id.clone(),
            action,
            title: story.title.clone(),
            body,
            state: if story.terminal() { "closed" } else { "open" },
            remote: story.remote,
            remote_url: story.remote_url.clone(),
            labels,
            completed: false,
            story: Some(story),
        });
    }
    Ok((settings, plans))
}

fn save(plan: &PushPlan, number: u64, url: &str) -> Result<(), &'static str> {
    let story = plan.story.as_ref().ok_or("invalid_story")?;
    let updated = story.linked(number, url)?;
    if fs::read_to_string(&plan.path).map_err(|_| "input_io")? != story.original {
        return Err("story_changed");
    }
    let mut temporary = tempfile::NamedTempFile::new_in(plan.path.parent().ok_or("input_io")?)
        .map_err(|_| "output_io")?;
    temporary
        .as_file()
        .set_permissions(
            fs::metadata(&plan.path)
                .map_err(|_| "input_io")?
                .permissions(),
        )
        .map_err(|_| "output_io")?;
    temporary
        .write_all(updated.as_bytes())
        .map_err(|_| "output_io")?;
    temporary.as_file().sync_all().map_err(|_| "output_io")?;
    temporary.persist(&plan.path).map_err(|_| "output_io")?;
    Ok(())
}

fn error_code(error: Error) -> &'static str {
    match error {
        Error::InvalidConfiguration => "invalid_configuration",
        Error::Transport => "transport",
        Error::HttpStatus(_) => "http_status",
        Error::ResponseTooLarge => "response_too_large",
        Error::InvalidResponse => "invalid_response",
    }
}

pub fn push(settings: Settings, plan: Vec<PushPlan>, dry_run: bool) -> PushOutput {
    let mut output = PushOutput {
        status: "ok",
        dry_run,
        plan,
        warnings: vec![],
        code: None,
    };
    let execute = |output: &mut PushOutput| -> Result<(), &'static str> {
        if dry_run {
            return Ok(());
        }
        if settings.sync == SyncMode::Off {
            return Err("sync_off");
        }
        if output.plan.iter().all(|p| p.action == Action::Skip) {
            return Ok(());
        }
        let token = std::env::var("GITEA_TOKEN")
            .ok()
            .filter(|s| !s.is_empty())
            .ok_or("missing_token")?;
        let gateway = Gateway::new(&settings.gitea.base_url, &token).map_err(error_code)?;
        let repository_labels_url = labels_url(&gateway, &settings, None)?;
        let mut known_labels = list_labels(&gateway, repository_labels_url.clone())?;
        for plan in &mut output.plan {
            if plan.action == Action::Skip {
                continue;
            }
            let story = plan.story.as_ref().ok_or("invalid_story")?;
            if fs::read_to_string(&plan.path).map_err(|_| "input_io")? != story.original {
                return Err("story_changed");
            }
            let mut labels = desired_label_ids(
                &gateway,
                &repository_labels_url,
                &mut known_labels,
                &plan.labels,
            )?;
            if plan.action == Action::Update {
                labels.extend(
                    read_labels(&gateway, labels_url(&gateway, &settings, plan.remote)?)?
                        .into_iter()
                        .filter(|label| !label.managed())
                        .map(|label| label.id),
                );
            }
            labels.sort_unstable();
            labels.dedup();
            let payload = StoryPayload {
                title: &plan.title,
                body: &plan.body,
                labels: (plan.action == Action::Create).then_some(labels.as_slice()),
                state: if plan.action == Action::Update {
                    Some(plan.state)
                } else {
                    None
                },
            };
            let issue: StoryIssue = gateway
                .send(
                    if plan.action == Action::Create {
                        reqwest::Method::POST
                    } else {
                        reqwest::Method::PATCH
                    },
                    gateway
                        .endpoint(&settings.gitea.owner, &settings.gitea.repo, plan.remote)
                        .map_err(error_code)?,
                    &payload,
                )
                .map_err(error_code)?;
            if plan.remote.is_some_and(|n| n != issue.number) {
                return Err("target_mismatch");
            }
            settings.check_link(issue.number, &issue.html_url)?;
            if issue.html_url.contains(&token) {
                return Err("invalid_response");
            }
            plan.remote = Some(issue.number);
            plan.remote_url = Some(issue.html_url.clone());
            save(plan, issue.number, &issue.html_url)?;
            if plan.action == Action::Update {
                let _: Vec<Label> = gateway
                    .send(
                        reqwest::Method::PUT,
                        labels_url(&gateway, &settings, plan.remote)?,
                        &LabelIds { labels },
                    )
                    .map_err(error_code)?;
            }
            plan.completed = true;
        }
        Ok(())
    };
    if let Err(code) = execute(&mut output) {
        output.status = "error";
        output.code = Some(code);
    }
    output
}
