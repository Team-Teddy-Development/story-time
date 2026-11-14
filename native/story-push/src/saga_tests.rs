use super::{execute, gateway::Remote, plans, Args, Context, Tracker};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};

struct Fake {
    calls: Vec<String>,
    fail_create: bool,
    fail_remove: bool,
    change: Option<PathBuf>,
}
impl Tracker for Fake {
    fn find(&mut self, _: &Value) -> Result<Option<Remote>, String> {
        panic!("unexpected find")
    }
    fn create(&mut self, s: &Value) -> Result<Remote, String> {
        let id = s["id"].as_str().unwrap();
        self.calls.push(format!("create {id}"));
        if id == "ST-002" {
            if self.fail_create {
                return Err("injected refusal".into());
            }
            if let Some(p) = &self.change {
                fs::write(p, b"concurrent content").unwrap();
            }
        }
        Ok(Remote {
            remote_id: if id == "ST-001" { "1" } else { "2" }.into(),
            url: "http://example/item".into(),
        })
    }
    fn update(&mut self, _: &Value) -> Result<Remote, String> {
        panic!("unexpected update")
    }
    fn remove(&mut self, id: &str) -> Result<(), String> {
        self.calls.push(format!("remove {id}"));
        if self.fail_remove {
            Err("injected rollback refusal".into())
        } else {
            Ok(())
        }
    }
}

#[test]
fn rollback_and_concurrent_write_contracts() {
    for case in 0..3 {
        let dir = std::env::temp_dir().join(format!("story-saga-{}-{case}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        let mut files = Vec::new();
        for id in ["ST-001", "ST-002"] {
            let path = dir.join(format!("{id}.md"));
            let text=crate::story::serialize(&json!({"id":id,"title":"test","status":"draft","type":"story","sections":{"Story":"test"}})).unwrap();
            fs::write(&path, &text).unwrap();
            files.push((path, text));
        }
        let mut fake = Fake {
            calls: vec![],
            fail_create: case < 2,
            fail_remove: case == 1,
            change: if case == 2 {
                Some(files[1].0.clone())
            } else {
                None
            },
        };
        let ctx = Context {
            config: json!({}),
            root: dir.clone(),
            dir: dir.clone(),
        };
        let plan = plans(
            &ctx,
            &Args {
                all: true,
                ..Args::default()
            },
        )
        .unwrap();
        let result = execute(&plan, &mut fake, "2026-11-11");
        let error = result.unwrap_err();
        assert!(error.contains("Push failed"));
        let pending = |i: usize| {
            fs::read_to_string(&files[i].0).unwrap()
                == files[i]
                    .1
                    .replace("\n---\n", "\npending_create: 2026-11-11\n---\n")
        };
        if case == 1 {
            assert!(pending(0), "an uncompensated create stays pending");
        } else {
            assert_eq!(fs::read_to_string(&files[0].0).unwrap(), files[0].1);
        }
        if case < 2 {
            assert_eq!(
                fake.calls,
                vec!["create ST-001", "create ST-002", "remove 1"]
            );
            assert!(pending(1), "a failed create stays pending");
        } else {
            assert_eq!(
                fake.calls,
                vec!["create ST-001", "create ST-002", "remove 2", "remove 1"]
            );
            assert_eq!(
                fs::read_to_string(&files[1].0).unwrap(),
                "concurrent content"
            );
        }
        if case == 1 {
            assert!(error.contains("Could not roll back"));
        } else {
            assert!(error.contains("Rolled back"));
        }
        fs::remove_dir_all(dir).unwrap();
    }
}
