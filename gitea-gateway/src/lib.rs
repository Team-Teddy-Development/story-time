use reqwest::{
    blocking::Client,
    header::{HeaderMap, HeaderValue, AUTHORIZATION},
    redirect::Policy,
    Url,
};
use serde::{Deserialize, Serialize};
use std::{fmt, io::Read, net::IpAddr, time::Duration};

const MAX_RESPONSE: u64 = 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct CreateIssue {
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct UpdateIssue {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Issue {
    pub number: u64,
    pub title: String,
    pub body: Option<String>,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidConfiguration,
    Transport,
    HttpStatus(u16),
    ResponseTooLarge,
    InvalidResponse,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

pub struct Gateway {
    client: Client,
    base: Url,
}
impl fmt::Debug for Gateway {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Gateway").finish_non_exhaustive()
    }
}
impl Gateway {
    pub fn new(base: &str, token: &str) -> Result<Self, Error> {
        let mut base = Url::parse(base).map_err(|_| Error::InvalidConfiguration)?;
        if !base.username().is_empty()
            || base.password().is_some()
            || base.query().is_some()
            || base.fragment().is_some()
        {
            return Err(Error::InvalidConfiguration);
        }
        let host = base.host_str().ok_or(Error::InvalidConfiguration)?;
        let loopback = host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<IpAddr>()
            .map(|ip| ip.is_loopback())
            .unwrap_or(false);
        if base.scheme() != "https" && !(base.scheme() == "http" && loopback) {
            return Err(Error::InvalidConfiguration);
        }
        if token.is_empty() {
            return Err(Error::InvalidConfiguration);
        }
        let mut auth = HeaderValue::from_str(&format!("token {token}"))
            .map_err(|_| Error::InvalidConfiguration)?;
        auth.set_sensitive(true);
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, auth);
        let client = Client::builder()
            .default_headers(headers)
            .redirect(Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|_| Error::InvalidConfiguration)?;
        if !base.path().ends_with('/') {
            let path = format!("{}/", base.path());
            base.set_path(&path);
        }
        Ok(Self { client, base })
    }
    fn endpoint(&self, owner: &str, repo: &str, number: Option<u64>) -> Result<Url, Error> {
        if [owner, repo]
            .iter()
            .any(|s| s.is_empty() || *s == "." || *s == "..")
            || number == Some(0)
        {
            return Err(Error::InvalidConfiguration);
        }
        let mut url = self.base.clone();
        {
            let mut parts = url
                .path_segments_mut()
                .map_err(|_| Error::InvalidConfiguration)?;
            parts
                .pop_if_empty()
                .extend(["api", "v1", "repos", owner, repo, "issues"]);
            if let Some(n) = number {
                parts.push(&n.to_string());
            }
        }
        Ok(url)
    }
    fn send<T: Serialize>(
        &self,
        method: reqwest::Method,
        url: Url,
        payload: &T,
    ) -> Result<Issue, Error> {
        let response = self
            .client
            .request(method, url)
            .json(payload)
            .send()
            .map_err(|_| Error::Transport)?;
        let status = response.status();
        if !status.is_success() {
            return Err(Error::HttpStatus(status.as_u16()));
        }
        let mut bytes = Vec::new();
        response
            .take(MAX_RESPONSE + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| Error::Transport)?;
        if bytes.len() as u64 > MAX_RESPONSE {
            return Err(Error::ResponseTooLarge);
        }
        serde_json::from_slice(&bytes).map_err(|_| Error::InvalidResponse)
    }
    pub fn create(&self, owner: &str, repo: &str, issue: &CreateIssue) -> Result<Issue, Error> {
        self.send(
            reqwest::Method::POST,
            self.endpoint(owner, repo, None)?,
            issue,
        )
    }
    pub fn update(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        issue: &UpdateIssue,
    ) -> Result<Issue, Error> {
        self.send(
            reqwest::Method::PATCH,
            self.endpoint(owner, repo, Some(number))?,
            issue,
        )
    }
    pub fn close(&self, owner: &str, repo: &str, number: u64) -> Result<Issue, Error> {
        #[derive(Serialize)]
        struct Close {
            state: &'static str,
        }
        self.send(
            reqwest::Method::PATCH,
            self.endpoint(owner, repo, Some(number))?,
            &Close { state: "closed" },
        )
    }
}
