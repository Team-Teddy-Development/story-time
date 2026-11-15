//! The pull requests of a hosted server (ADR 0007). A write of a registry server is a commit on
//! the branch `story-time/<ID>`; this module finds or opens the one pull request of that branch
//! through the GitHub REST API. Each call carries the installation token of one repository.

use reqwest::blocking::{Client, RequestBuilder};
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Mutex, time::Duration};

/// The pages of comments that one issue can have: 100 comments for each page.
const PAGE_LIMIT: usize = 30;
const PAGES_KEPT: usize = 256;

pub(super) struct GitHub {
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

impl GitHub {
    pub(super) fn new(api: &str) -> Result<Self, String> {
        let client = Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent("story-time-mcp")
            .build()
            .map_err(|_| "could not build the HTTP client")?;
        Ok(Self {
            api: api.trim_end_matches('/').to_owned(),
            client,
            pages: Mutex::default(),
        })
    }

    fn send(&self, access: &Access, request: RequestBuilder, step: &str) -> Result<Value, String> {
        let response = request
            .bearer_auth(&access.token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .map_err(|_| format!("GitHub did not answer the {step} request"))?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!(
                "GitHub answered the {step} request with status {}",
                status.as_u16()
            ));
        }
        response
            .json::<Value>()
            .map_err(|_| format!("GitHub answered the {step} request with invalid JSON"))
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
        let url = format!(
            "{}?state=open&per_page=1&head={}:{branch}",
            self.pulls(access),
            access.owner
        );
        let found = self.send(access, self.client.get(url), "pull request list")?;
        let pulls = found
            .as_array()
            .ok_or("GitHub answered the pull request list request with no list")?;
        Ok(pulls
            .iter()
            .find(|pull| pull["head"]["ref"] == branch)
            .and_then(|pull| pull["html_url"].as_str())
            .map(str::to_owned))
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
            .ok_or_else(|| "GitHub returned no pull request URL".into())
    }
}

impl GitHub {
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
        let mut request = self
            .client
            .get(url)
            .bearer_auth(&access.token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28");
        if let Some(known) = &known {
            request = request.header("If-None-Match", &known.etag);
        }
        let response = request
            .send()
            .map_err(|_| format!("GitHub did not answer the {step} request"))?;
        let status = response.status();
        if let (304, Some(known)) = (status.as_u16(), known) {
            return Ok(known);
        }
        if !status.is_success() {
            return Err(format!(
                "GitHub answered the {step} request with status {}",
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
            .ok_or_else(|| format!("GitHub answered the {step} request with no list"))?;
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
