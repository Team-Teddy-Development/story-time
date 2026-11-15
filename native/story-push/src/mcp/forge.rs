//! The pull requests of a hosted server (ADR 0007). A write of a registry server is a commit on
//! the branch `story-time/<ID>`; this module finds or opens the one pull request of that branch
//! through the GitHub REST API. Each call carries the installation token of one repository.

use reqwest::blocking::{Client, RequestBuilder};
use serde_json::{json, Value};
use std::time::Duration;

pub(super) struct GitHub {
    api: String,
    client: Client,
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
