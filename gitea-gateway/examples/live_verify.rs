use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::{fs, process::Command};
fn main() {
    let token_file = std::env::var("STORY_PROOF_TOKEN_FILE").expect("token-file path required");
    let token = fs::read_to_string(token_file).expect("read isolated token");
    let token = token.trim();
    let base = "http://127.0.0.1:31987";
    let client = Client::new();
    let repo = format!("story-proof-{}", std::process::id());
    let call = |method: reqwest::Method, path: &str, payload: Option<Value>| -> Value {
        let mut request = client
            .request(method, format!("{base}/api/v1{path}"))
            .header("Authorization", format!("token {token}"));
        if let Some(data) = payload {
            request = request.json(&data);
        }
        let response = request.send().expect("isolated API transport");
        assert!(
            response.status().is_success(),
            "isolated API status {}",
            response.status()
        );
        response.json().expect("API JSON")
    };
    call(
        reqwest::Method::POST,
        "/user/repos",
        Some(json!({"name":repo,"private":true,"auto_init":false})),
    );
    let root = tempfile::tempdir().expect("project directory");
    fs::create_dir(root.path().join(".story-time")).unwrap();
    fs::create_dir(root.path().join("backlog")).unwrap();
    fs::write(root.path().join(".story-time/settings.json"), json!({"tracker":"gitea","sync":"auto","gitea":{"baseUrl":base,"owner":"storyproof","repo":repo}}).to_string()).unwrap();
    let story_path = root.path().join("backlog/ST-001.md");
    fs::write(&story_path, "---\nid: ST-001\ntitle: Live verification\nstatus: ready\ntype: story\nsize: 3\n---\n\n## Story\nAs an operator, I can push a real story.\n\n## Acceptance Criteria\n- [ ] Given a story, when pushed, then Gitea records it.\n").unwrap();
    let binary = std::env::var("STORY_PROOF_BINARY").expect("compiled binary path required");
    let push = || {
        let output = Command::new(&binary)
            .args(["story-push", "--root"])
            .arg(root.path())
            .arg("backlog/ST-001.md")
            .env("GITEA_TOKEN", token)
            .output()
            .expect("run real binary");
        assert!(
            output.status.success(),
            "story-push failed; inspect sanitized result locally"
        );
        let value: Value = serde_json::from_slice(&output.stdout).expect("CLI JSON");
        assert_eq!(value["status"], "ok");
        assert_eq!(value["plan"][0]["completed"], true);
        value
    };
    let first = push();
    assert_eq!(first["plan"][0]["action"], "create");
    let number = first["plan"][0]["remote"].as_u64().unwrap();
    let endpoint = format!("/repos/storyproof/{repo}/issues/{number}");
    let issue = call(reqwest::Method::GET, &endpoint, None);
    assert_eq!(issue["title"], "Live verification");
    assert!(issue["body"].as_str().unwrap().contains("Given a story"));
    assert!(fs::read_to_string(&story_path)
        .unwrap()
        .contains(&format!("remote: {number}")));
    let team = call(
        reqwest::Method::POST,
        &format!("/repos/storyproof/{repo}/labels"),
        Some(json!({"name":"team-owned","color":"112233"})),
    );
    call(
        reqwest::Method::POST,
        &format!("{endpoint}/labels"),
        Some(json!({"labels":[team["id"]]})),
    );
    let edited = fs::read_to_string(&story_path)
        .unwrap()
        .replace("status: ready", "status: done")
        .replace("size: 3", "size: 5");
    fs::write(&story_path, edited).unwrap();
    assert_eq!(push()["plan"][0]["action"], "update");
    let issue = call(reqwest::Method::GET, &endpoint, None);
    assert_eq!(issue["state"], "closed");
    let labels = issue["labels"].as_array().unwrap();
    assert!(labels.iter().any(|l| l["name"] == "team-owned"));
    assert!(labels.iter().any(|l| l["name"] == "size:5"));
    assert!(!labels.iter().any(|l| l["name"] == "size:3"));
    fs::write(
        &story_path,
        fs::read_to_string(&story_path)
            .unwrap()
            .replace("status: done", "status: ready"),
    )
    .unwrap();
    push();
    assert_eq!(call(reqwest::Method::GET, &endpoint, None)["state"], "open");
    let issues = call(
        reqwest::Method::GET,
        &format!("/repos/storyproof/{repo}/issues?state=all"),
        None,
    );
    assert_eq!(issues.as_array().unwrap().len(), 1);
    println!("LIVE_GITEA_VERIFIED create, repeat update, persisted links, criteria, close, reopen, team-label preservation, stale-label removal, one issue");
}
