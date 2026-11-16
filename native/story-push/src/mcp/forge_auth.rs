//! GitHub App tokens for a hosted server (ADR 0007). The server signs a short JWT with the App
//! key, exchanges it for an installation token of one repository, and gives that token to a
//! `git` child only through its environment. The token is never an argument, never in a URL,
//! and never in a file.

use reqwest::blocking::Client;
use ring::{rand::SystemRandom, signature};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const STANDARD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const URL_SAFE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
/// A token is replaced when it has less than this long to live.
const REFRESH_BEFORE: i64 = 300;

fn encode(bytes: &[u8], table: &[u8; 64], pad: bool) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(table[(n >> (18 - 6 * i)) as usize & 63] as char);
            } else if pad {
                out.push('=');
            }
        }
    }
    out
}

/// Standard base64 with padding.
pub(super) fn base64(bytes: &[u8]) -> String {
    encode(bytes, STANDARD, true)
}

/// The bytes of standard base64 text. White space and padding are skipped.
fn decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let (mut buffer, mut bits) = (0u32, 0u8);
    for byte in text.bytes() {
        if byte.is_ascii_whitespace() || byte == b'=' {
            continue;
        }
        let value = STANDARD.iter().position(|c| *c == byte)? as u32;
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// The owner and name of the repository that a clone URL names, and the origin that git sends
/// the token to. `None` for a URL that takes no token: a `file` URL.
struct Target {
    origin: String,
    owner: String,
    name: String,
}

fn target(clone: &str) -> Result<Option<Target>, String> {
    let Some((scheme, rest)) = clone.split_once("://") else {
        return Err("the clone URL has no scheme".into());
    };
    if scheme == "file" {
        return Ok(None);
    }
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = authority
        .rsplit_once(':')
        .filter(|(_, port)| port.bytes().all(|b| b.is_ascii_digit()))
        .map_or(authority, |(host, _)| host);
    let loopback = matches!(host, "127.0.0.1" | "localhost" | "[::1]");
    if authority.is_empty() || !(scheme == "https" || (scheme == "http" && loopback)) {
        return Err("a token goes only to an https URL".into());
    }
    let (owner, name) = named(path)?;
    Ok(Some(Target {
        origin: format!("{scheme}://{authority}"),
        owner,
        name,
    }))
}

/// The owner and name in the last two steps of a clone URL's path.
fn named(path: &str) -> Result<(String, String), String> {
    let mut steps = path.trim_end_matches('/').rsplit('/');
    let name = steps.next().unwrap_or_default();
    let name = name.strip_suffix(".git").unwrap_or(name);
    let owner = steps.next().unwrap_or_default();
    let step_ok = |step: &str| {
        !step.is_empty()
            && step != "."
            && step != ".."
            && step
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    };
    if !step_ok(owner) || !step_ok(name) {
        return Err("the clone URL does not end in <owner>/<repository>".into());
    }
    Ok((owner.to_owned(), name.to_owned()))
}

/// The owner and name of the repository that a clone URL of any scheme names. The forge API
/// addresses the repository by them.
pub(super) fn repository(clone: &str) -> Result<(String, String), String> {
    let (_, rest) = clone
        .split_once("://")
        .ok_or("the clone URL has no scheme")?;
    named(rest.split_once('/').map_or("", |(_, path)| path))
}

struct Token {
    value: String,
    expires: i64,
}

pub(super) struct App {
    id: String,
    key: signature::RsaKeyPair,
    api: String,
    client: Client,
    /// Installation tokens by `owner/name`. The lock is held while a token is requested, so
    /// that two callers do not request two tokens.
    tokens: Mutex<HashMap<String, Token>>,
    /// The App's slug, which names its bot user. It is read once.
    slug: Mutex<Option<String>>,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

impl App {
    /// The App with the id in `id` and the private key in `pem`, which is PKCS#1 as GitHub
    /// supplies it, or PKCS#8.
    pub(super) fn new(id: &str, pem: &str, api: &str) -> Result<Self, String> {
        let id = id.trim();
        if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.') {
            return Err("the App id is not an id".into());
        }
        let (label, body) = pem
            .split_once("-----BEGIN ")
            .and_then(|(_, rest)| rest.split_once("-----"))
            .and_then(|(label, rest)| Some((label, rest.split_once("-----END ")?.0)))
            .ok_or("the App key is not a PEM file")?;
        let der = decode(body).ok_or("the App key is not valid base64")?;
        let key = match label {
            "RSA PRIVATE KEY" => signature::RsaKeyPair::from_der(&der),
            "PRIVATE KEY" => signature::RsaKeyPair::from_pkcs8(&der),
            _ => return Err("the App key must be an RSA private key".into()),
        }
        .map_err(|_| "the App key is not a usable RSA private key")?;
        let client = Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent("story-time-mcp")
            .build()
            .map_err(|_| "could not build the HTTP client")?;
        Ok(Self {
            id: id.to_owned(),
            key,
            api: api.trim_end_matches('/').to_owned(),
            client,
            tokens: Mutex::new(HashMap::new()),
            slug: Mutex::new(None),
        })
    }

    /// The JWT that authenticates the App itself at `at`: issued 60 seconds earlier, to allow
    /// for clock drift, and valid for 9 minutes more.
    fn jwt(&self, at: i64) -> Result<String, String> {
        let part = |value: Value| encode(value.to_string().as_bytes(), URL_SAFE, false);
        let message = format!(
            "{}.{}",
            part(json!({"alg": "RS256", "typ": "JWT"})),
            part(json!({"iat": at - 60, "exp": at + 540, "iss": self.id}))
        );
        let mut signed = vec![0; self.key.public().modulus_len()];
        self.key
            .sign(
                &signature::RSA_PKCS1_SHA256,
                &SystemRandom::new(),
                message.as_bytes(),
                &mut signed,
            )
            .map_err(|_| "could not sign the App JWT")?;
        Ok(format!("{message}.{}", encode(&signed, URL_SAFE, false)))
    }

    /// Sends a request that the App itself signs, and returns the JSON of a 2xx answer.
    fn send(
        &self,
        request: reqwest::blocking::RequestBuilder,
        step: &str,
    ) -> Result<Value, String> {
        let request = request
            .bearer_auth(self.jwt(now())?)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28");
        let response = super::retry::send(request)
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

    /// The name of the App's bot user, `<slug>[bot]`, which is the author of the commits that
    /// the server makes. When GitHub does not name the slug, the name is `story-time[bot]`.
    pub(super) fn bot(&self) -> String {
        let mut slug = self.slug.lock().unwrap_or_else(|e| e.into_inner());
        if slug.is_none() {
            *slug = self
                .send(self.client.get(format!("{}/app", self.api)), "app")
                .ok()
                .and_then(|app| app["slug"].as_str().map(str::to_owned))
                .filter(|slug| {
                    !slug.is_empty()
                        && slug
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
                });
        }
        format!("{}[bot]", slug.as_deref().unwrap_or("story-time"))
    }

    fn request(&self, owner: &str, name: &str) -> Result<Token, String> {
        let send = |request, step| self.send(request, step);
        let installation = send(
            self.client
                .get(format!("{}/repos/{owner}/{name}/installation", self.api)),
            "installation",
        )?;
        let installation = installation["id"]
            .as_u64()
            .ok_or("GitHub named no installation for the repository")?;
        let granted = send(
            self.client
                .post(format!(
                    "{}/app/installations/{installation}/access_tokens",
                    self.api
                ))
                .json(&json!({"repositories": [name]})),
            "token",
        )?;
        let value = granted["token"]
            .as_str()
            .filter(|token| token.len() >= 8)
            .ok_or("GitHub returned no token")?;
        let expires = granted["expires_at"]
            .as_str()
            .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
            .map_or(now() + 3600, |at| at.timestamp());
        Ok(Token {
            value: value.to_owned(),
            expires,
        })
    }

    /// What a `git` child needs to reach `clone`: the environment that carries the token, and
    /// the secret strings to keep out of every output. A `file` URL needs neither.
    pub(super) fn git_environment(&self, clone: &str) -> Result<Access, String> {
        let Some(target) = target(clone)? else {
            return Ok(Access::default());
        };
        let token = self.token(&target.owner, &target.name)?;
        Ok(access(&target.origin, &token))
    }

    /// The installation token of the repository `owner/name`. One token serves until it has
    /// less than five minutes to live.
    pub(super) fn token(&self, owner: &str, name: &str) -> Result<String, String> {
        let full = format!("{owner}/{name}");
        let mut tokens = self.tokens.lock().unwrap_or_else(|e| e.into_inner());
        let fresh = |token: &Token| token.expires - now() > REFRESH_BEFORE;
        if !tokens.get(&full).is_some_and(fresh) {
            let token = self.request(owner, name)?;
            tokens.insert(full.clone(), token);
        }
        Ok(tokens[&full].value.clone())
    }
}

/// The environment of one `git` child, and the secrets inside it.
#[derive(Default)]
pub(super) struct Access {
    pub(super) environment: Vec<(String, String)>,
    pub(super) secrets: Vec<String>,
}

/// Git reads one extra config entry from these variables: an `Authorization` header that it
/// sends only to `origin`.
fn access(origin: &str, token: &str) -> Access {
    let basic = base64(format!("x-access-token:{token}").as_bytes());
    Access {
        environment: vec![
            ("GIT_CONFIG_COUNT".into(), "1".into()),
            (
                "GIT_CONFIG_KEY_0".into(),
                format!("http.{origin}/.extraheader"),
            ),
            (
                "GIT_CONFIG_VALUE_0".into(),
                format!("AUTHORIZATION: basic {basic}"),
            ),
        ],
        secrets: vec![token.to_owned(), basic],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
    };

    /// A throwaway 2048-bit key in the PKCS#1 form that GitHub supplies. `openssl` makes it once
    /// for each test run, so that no private key is in the repository.
    fn pkcs1() -> &'static str {
        static KEY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        KEY.get_or_init(|| {
            let made = std::process::Command::new("openssl")
                .args(["genrsa", "-traditional", "2048"])
                .output()
                .expect("openssl makes the throwaway App key");
            assert!(made.status.success());
            let key = String::from_utf8(made.stdout).unwrap();
            assert!(key.starts_with("-----BEGIN RSA PRIVATE KEY-----"), "PKCS#1");
            key
        })
    }

    fn app(api: &str) -> App {
        App::new("12345\n", pkcs1(), api).unwrap()
    }

    fn url_decode(text: &str) -> Vec<u8> {
        let standard: String = text
            .chars()
            .map(|c| match c {
                '-' => '+',
                '_' => '/',
                c => c,
            })
            .collect();
        decode(&standard).unwrap()
    }

    #[test]
    fn base64_matches_the_rfc_4648_vectors() {
        for (plain, coded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(plain.as_bytes()), coded);
            assert_eq!(decode(coded).unwrap(), plain.as_bytes());
        }
        assert_eq!(encode(&[0xfb, 0xff], URL_SAFE, false), "-_8");
        assert_eq!(decode("Zm9v\nYmFy\n").unwrap(), b"foobar");
        assert!(decode("Zm9v!").is_none());
    }

    #[test]
    fn the_jwt_names_the_app_starts_60_seconds_back_and_ends_9_minutes_ahead() {
        let app = app("http://127.0.0.1:1");
        let jwt = app.jwt(1_800_000_000).unwrap();
        let parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3);
        let header: Value = serde_json::from_slice(&url_decode(parts[0])).unwrap();
        assert_eq!(header, json!({"alg": "RS256", "typ": "JWT"}));
        let claims: Value = serde_json::from_slice(&url_decode(parts[1])).unwrap();
        assert_eq!(
            claims,
            json!({"iat": 1_799_999_940, "exp": 1_800_000_540, "iss": "12345"})
        );
        let public = signature::UnparsedPublicKey::new(
            &signature::RSA_PKCS1_2048_8192_SHA256,
            app.key.public().as_ref().to_vec(),
        );
        let signed = format!("{}.{}", parts[0], parts[1]);
        public
            .verify(signed.as_bytes(), &url_decode(parts[2]))
            .expect("the signature verifies against the key");
        assert!(public
            .verify(b"another message", &url_decode(parts[2]))
            .is_err());
    }

    #[test]
    fn a_key_that_is_not_an_rsa_pem_is_refused_without_its_text() {
        for bad in [
            "not a key",
            "-----BEGIN EC PRIVATE KEY-----\nAAAA\n-----END EC PRIVATE KEY-----\n",
            "-----BEGIN RSA PRIVATE KEY-----\nAAAA\n-----END RSA PRIVATE KEY-----\n",
        ] {
            let error = App::new("1", bad, "http://127.0.0.1:1").err().unwrap();
            assert!(!error.contains("AAAA"), "{error}");
        }
        assert!(App::new("", pkcs1(), "http://127.0.0.1:1").is_err());
    }

    #[test]
    fn a_clone_url_names_its_repository_and_only_https_takes_a_token() {
        let github = target("https://github.com/Varsity-Engine/varsity-core.git")
            .unwrap()
            .unwrap();
        assert_eq!(github.origin, "https://github.com");
        assert_eq!(
            (github.owner.as_str(), github.name.as_str()),
            ("Varsity-Engine", "varsity-core")
        );
        let local = target("http://127.0.0.1:8123/git/o/core.git")
            .unwrap()
            .unwrap();
        assert_eq!(local.origin, "http://127.0.0.1:8123");
        assert_eq!((local.owner.as_str(), local.name.as_str()), ("o", "core"));
        assert!(target("file:///srv/git/core.git").unwrap().is_none());
        for refused in [
            "http://github.com/o/core.git",
            "http://127.0.0.1.example.com/o/core.git",
            "https://github.com/core.git",
            "https://github.com/o/..",
            "ssh://github.com/o/core.git",
        ] {
            assert!(target(refused).is_err(), "{refused}");
        }
    }

    #[test]
    fn the_token_is_only_in_the_value_variable_of_the_git_environment() {
        let token = "ghs_ExampleInstallationToken";
        let access = access("https://github.com", token);
        let basic = base64(format!("x-access-token:{token}").as_bytes());
        assert_eq!(
            access.environment,
            [
                ("GIT_CONFIG_COUNT".to_owned(), "1".to_owned()),
                (
                    "GIT_CONFIG_KEY_0".to_owned(),
                    "http.https://github.com/.extraheader".to_owned()
                ),
                (
                    "GIT_CONFIG_VALUE_0".to_owned(),
                    format!("AUTHORIZATION: basic {basic}")
                ),
            ]
        );
        for (name, value) in &access.environment {
            let holds = value.contains(token) || value.contains(&basic);
            assert_eq!(holds, name == "GIT_CONFIG_VALUE_0", "{name}");
        }
        assert_eq!(access.secrets, [token.to_owned(), basic]);
    }

    /// A GitHub API stub that grants a new token on each request, with the given lifetime.
    fn stub(lifetime: i64) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let granted = Arc::new(AtomicUsize::new(0));
        let count = granted.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut request = Vec::new();
                let mut buffer = [0; 4096];
                // The body of the POST is short and arrives with the head or just after it.
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    match stream.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => request.extend_from_slice(&buffer[..n]),
                    }
                }
                let head = String::from_utf8_lossy(&request).into_owned();
                let body = if head.starts_with("GET /repos/o/core/installation ") {
                    json!({"id": 7})
                } else if head.starts_with("POST /app/installations/7/access_tokens ") {
                    let n = count.fetch_add(1, Ordering::SeqCst) + 1;
                    let expires = chrono::DateTime::from_timestamp(now() + lifetime, 0).unwrap();
                    json!({"token": format!("ghs_stub_token_{n}"), "expires_at": expires.to_rfc3339()})
                } else {
                    json!({"message": "Not Found"})
                }
                .to_string();
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        (url, granted)
    }

    fn value(access: &Access) -> &str {
        &access.environment[2].1
    }

    #[test]
    fn a_token_is_reused_until_5_minutes_before_it_expires() {
        let clone = |api: &str| format!("{api}/o/core.git");
        // An hour to live: the second call asks for nothing.
        let (api, granted) = stub(3600);
        let app = app(&api);
        let first = app.git_environment(&clone(&api)).unwrap();
        let second = app.git_environment(&clone(&api)).unwrap();
        assert_eq!(granted.load(Ordering::SeqCst), 1);
        assert_eq!(value(&first), value(&second));
        assert!(first.secrets.contains(&"ghs_stub_token_1".to_owned()));

        // Four minutes to live: each call asks for a new token.
        let (api, granted) = stub(240);
        let app = App::new("12345", pkcs1(), &api).unwrap();
        let first = app.git_environment(&clone(&api)).unwrap();
        let second = app.git_environment(&clone(&api)).unwrap();
        assert_eq!(granted.load(Ordering::SeqCst), 2);
        assert_ne!(value(&first), value(&second));

        // A file URL takes no token and asks for none.
        let none = app.git_environment("file:///srv/git/core.git").unwrap();
        assert!(none.environment.is_empty() && none.secrets.is_empty());
        assert_eq!(granted.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_refusal_from_github_is_an_error_that_names_the_step() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let api = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut buffer = [0; 4096];
                let _ = stream.read(&mut buffer);
                let _ = write!(
                    stream,
                    "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                );
            }
        });
        let error = app(&api)
            .git_environment(&format!("{api}/o/core.git"))
            .err()
            .unwrap();
        assert_eq!(
            error,
            "GitHub answered the installation request with status 404"
        );
    }
}
