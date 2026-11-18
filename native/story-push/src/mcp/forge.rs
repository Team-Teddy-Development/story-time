//! The pull requests of a hosted server (ADR 0007). A write of a registry server is a commit on
//! the branch `story-time/<ID>`; this module finds or opens the one pull request of that branch
//! through the REST API of GitHub or of Gitea. Each call carries the token of one repository.

use reqwest::{
    blocking::{Client, RequestBuilder},
    header::{HeaderValue, AUTHORIZATION},
};
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Mutex, time::Duration};

/// The pages of comments that one issue can have: 100 comments for each page.
const PAGE_LIMIT: usize = 30;
const PAGES_KEPT: usize = 256;

/// What differs between the REST APIs of two forges. The calls are the same.
struct Flavor {
    /// The name of the forge in error text.
    name: &'static str,
    /// The word before the token in the `Authorization` header.
    scheme: &'static str,
    headers: &'static [(&'static str, &'static str)],
    lookup: Lookup,
}

/// How to find the open pull request of a branch.
enum Lookup {
    /// The list takes a `head` filter.
    HeadFilter,
    /// The list has no `head` filter (Gitea 1.21): read it page by page.
    Listing,
}

const GITHUB: Flavor = Flavor {
    name: "GitHub",
    scheme: "Bearer",
    headers: &[
        ("Accept", "application/vnd.github+json"),
        ("X-GitHub-Api-Version", "2022-11-28"),
    ],
    lookup: Lookup::HeadFilter,
};

const GITEA: Flavor = Flavor {
    name: "Gitea",
    scheme: "token",
    headers: &[("Accept", "application/json")],
    lookup: Lookup::Listing,
};

/// The open pull requests that one list page holds at most. A server can cap it lower.
const LIST_LIMIT: usize = 50;
/// The pages of open pull requests that are read to find one branch.
const LIST_PAGES: usize = 40;

pub(super) struct Rest {
    flavor: &'static Flavor,
    api: String,
    client: Client,
    /// The last answer for each page of comments, which a `304` answer stands for.
    pages: Mutex<HashMap<String, Page>>,
}

#[derive(Clone)]
struct Page {
    etag: String,
    items: Vec<Value>,
    next: Option<String>,
}

/// One repository at the forge, and the token that reaches it.
pub(super) struct Access {
    pub(super) owner: String,
    pub(super) name: String,
    pub(super) token: String,
}

impl Rest {
    pub(super) fn github(api: &str) -> Result<Self, String> {
        Self::new(&GITHUB, api)
    }

    pub(super) fn gitea(api: &str) -> Result<Self, String> {
        Self::new(&GITEA, api)
    }

    fn new(flavor: &'static Flavor, api: &str) -> Result<Self, String> {
        let client = Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent("story-time-mcp")
            .build()
            .map_err(|_| "could not build the HTTP client")?;
        Ok(Self {
            flavor,
            api: api.trim_end_matches('/').to_owned(),
            client,
            pages: Mutex::default(),
        })
    }

    /// The request with the token and the headers of this forge.
    fn signed(&self, access: &Access, request: RequestBuilder) -> RequestBuilder {
        let mut request = request;
        for (name, value) in self.flavor.headers {
            request = request.header(*name, *value);
        }
        match HeaderValue::from_str(&format!("{} {}", self.flavor.scheme, access.token)) {
            Ok(mut value) => {
                value.set_sensitive(true);
                request.header(AUTHORIZATION, value)
            }
            Err(_) => request,
        }
    }

    fn send(&self, access: &Access, request: RequestBuilder, step: &str) -> Result<Value, String> {
        let name = self.flavor.name;
        let request = self.signed(access, request);
        let response = super::retry::send(request)
            .map_err(|_| format!("{name} did not answer the {step} request"))?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!(
                "{name} answered the {step} request with status {}",
                status.as_u16()
            ));
        }
        response
            .json::<Value>()
            .map_err(|_| format!("{name} answered the {step} request with invalid JSON"))
    }

    fn pulls(&self, access: &Access) -> String {
        format!("{}/repos/{}/{}/pulls", self.api, access.owner, access.name)
    }

    /// The URL of the open pull request whose head is `branch`, when there is one. The branch
    /// name holds only characters that a query takes as they are.
    pub(super) fn open_pull(
        &self,
        access: &Access,
        branch: &str,
    ) -> Result<Option<String>, String> {
        let name = self.flavor.name;
        let list = |url: String| -> Result<Vec<Value>, String> {
            let found = self.send(access, self.client.get(url), "pull request list")?;
            found.as_array().cloned().ok_or_else(|| {
                format!("{name} answered the pull request list request with no list")
            })
        };
        let url_of = |pulls: &[Value], own: &dyn Fn(&Value) -> bool| {
            pulls
                .iter()
                .find(|pull| pull["head"]["ref"] == branch && own(pull))
                .and_then(|pull| pull["html_url"].as_str())
                .map(str::to_owned)
        };
        match self.flavor.lookup {
            Lookup::HeadFilter => {
                let pulls = list(format!(
                    "{}?state=open&per_page=1&head={}:{branch}",
                    self.pulls(access),
                    access.owner
                ))?;
                Ok(url_of(&pulls, &|_| true))
            }
            Lookup::Listing => {
                // A pull request from a fork can have the same head name. Only a head in this
                // repository is the branch of this server.
                let own = format!("{}/{}", access.owner, access.name);
                let from_here = |pull: &Value| {
                    pull["head"]["repo"]["full_name"]
                        .as_str()
                        .is_some_and(|repo| repo.eq_ignore_ascii_case(&own))
                };
                for page in 1..=LIST_PAGES {
                    let pulls = list(format!(
                        "{}?state=open&page={page}&limit={LIST_LIMIT}",
                        self.pulls(access)
                    ))?;
                    if pulls.is_empty() {
                        return Ok(None);
                    }
                    if let Some(url) = url_of(&pulls, &from_here) {
                        return Ok(Some(url));
                    }
                }
                Err(format!(
                    "{name} has more than {} open pull requests",
                    LIST_PAGES * LIST_LIMIT
                ))
            }
        }
    }

    /// Opens a pull request from `head` to `base`, and returns its URL.
    pub(super) fn create_pull(
        &self,
        access: &Access,
        head: &str,
        base: &str,
        title: &str,
        body: &str,
    ) -> Result<String, String> {
        let request = self
            .client
            .post(self.pulls(access))
            .json(&json!({"head": head, "base": base, "title": title, "body": body}));
        let made = self.send(access, request, "pull request")?;
        made["html_url"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| format!("{} returned no pull request URL", self.flavor.name))
    }
}

impl Rest {
    fn issue(&self, access: &Access, issue: u64) -> String {
        format!(
            "{}/repos/{}/{}/issues/{issue}/comments",
            self.api, access.owner, access.name
        )
    }

    /// One page of comments. The request names the ETag of the last answer for this URL, and
    /// a `304` answer returns that answer again.
    fn page(&self, access: &Access, url: &str) -> Result<Page, String> {
        let step = "comment list";
        let known = {
            let pages = self.pages.lock().unwrap_or_else(|e| e.into_inner());
            pages.get(url).cloned()
        };
        let mut request = self.signed(access, self.client.get(url));
        if let Some(known) = &known {
            request = request.header("If-None-Match", &known.etag);
        }
        let name = self.flavor.name;
        let response = super::retry::send(request)
            .map_err(|_| format!("{name} did not answer the {step} request"))?;
        let status = response.status();
        if let (304, Some(known)) = (status.as_u16(), known) {
            return Ok(known);
        }
        if !status.is_success() {
            return Err(format!(
                "{name} answered the {step} request with status {}",
                status.as_u16()
            ));
        }
        let header = |name: &str| {
            let value = response.headers().get(name)?.to_str().ok()?;
            Some(value.to_owned())
        };
        let etag = header("etag");
        // The next page is followed only at the API this server was given, so that the token
        // goes nowhere else.
        let next = header("link").and_then(|link| {
            let part = link.split(',').find(|part| part.contains("rel=\"next\""))?;
            let url = part.split_once('<')?.1.split_once('>')?.0;
            url.starts_with(&format!("{}/", self.api))
                .then(|| url.to_owned())
        });
        let items = response
            .json::<Value>()
            .ok()
            .and_then(|body| body.as_array().cloned())
            .ok_or_else(|| format!("{name} answered the {step} request with no list"))?;
        let page = Page {
            etag: etag.clone().unwrap_or_default(),
            items,
            next,
        };
        if etag.is_some() {
            let mut pages = self.pages.lock().unwrap_or_else(|e| e.into_inner());
            if pages.len() >= PAGES_KEPT {
                pages.clear();
            }
            pages.insert(url.to_owned(), page.clone());
        }
        Ok(page)
    }

    /// Every comment of an issue, oldest first, as GitHub returns them.
    pub(super) fn comments(&self, access: &Access, issue: u64) -> Result<Vec<Value>, String> {
        let mut url = format!("{}?per_page=100", self.issue(access, issue));
        let mut all = Vec::new();
        for _ in 0..PAGE_LIMIT {
            let page = self.page(access, &url)?;
            all.extend(page.items);
            match page.next {
                Some(next) => url = next,
                None => return Ok(all),
            }
        }
        Err(format!(
            "the issue has more than {} comments",
            PAGE_LIMIT * 100
        ))
    }

    /// Posts one comment on an issue, and returns it.
    pub(super) fn post_comment(
        &self,
        access: &Access,
        issue: u64,
        body: &str,
    ) -> Result<Value, String> {
        let request = self
            .client
            .post(self.issue(access, issue))
            .json(&json!({ "body": body }));
        self.send(access, request, "comment")
    }

    /// The numeric id of the user `login`. A login can be copied; the id cannot.
    pub(super) fn user_id(&self, access: &Access, login: &str) -> Result<u64, String> {
        let escaped = login.replace('[', "%5B").replace(']', "%5D");
        let url = format!("{}/users/{escaped}", self.api);
        let user = self.send(access, self.client.get(url), "user")?;
        user["id"]
            .as_u64()
            .ok_or_else(|| "GitHub returned no user id".into())
    }
}
