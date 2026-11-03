//! Four processes take exclusive claims on one store.

use std::path::PathBuf;
use std::process::Command;
use trammel::canon;
use trammel::json::Value;
use trammel::store::Store;

#[test]
fn four_processes_claim_one_store() {
    if std::env::var_os("TRAMMEL_CLAIM_WORKER").is_some() {
        worker();
        return;
    }
    let dir = std::env::temp_dir().join(format!("trammel-claims-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    {
        let mut store = Store::open(&dir).unwrap();
        let steps: Vec<Value> = (0..8)
            .map(|i| {
                canon::obj(vec![
                    ("step_index", Value::Int(i)),
                    ("file", Value::string(format!("f{i}.py"))),
                    ("description", Value::string("edit")),
                    ("symbols", Value::Array(vec![Value::string("name")])),
                    ("depends_on", Value::Array(vec![])),
                ])
            })
            .collect();
        let strategy = canon::obj(vec![
            ("goal", Value::string("claim")),
            ("steps", Value::Array(steps)),
        ]);
        assert_eq!(store.create_plan("claim", &strategy), 1);
        store.save().unwrap();
    }

    let children = spawn_wave(&dir, "claim");
    for mut child in children {
        let status = child.wait().unwrap();
        assert!(status.success(), "claim worker failed: {status}");
    }

    {
        let store = Store::open(&dir).unwrap();
        let mut owners = Vec::new();
        for id in 1..=8 {
            let row = store
                .rows("steps")
                .into_iter()
                .find(|row| canon::get(row, "id").and_then(canon::as_i64) == Some(id))
                .unwrap()
                .clone();
            let owner = canon::get(&row, "claimed_by").and_then(canon::as_str).unwrap().to_string();
            let status = canon::get(&row, "status").and_then(canon::as_str).unwrap();
            assert_eq!(status, "running", "step {id}");
            if id >= 2 {
                let worker = (id - 1) / 2;
                assert_eq!(owner, format!("agent-{worker}"), "step {id}");
            } else {
                assert!(owner.starts_with("agent-"), "{owner}");
            }
            owners.push(owner);
        }
        assert_eq!(owners.len(), 8);
    }

    let mut releaser = spawn_one(&dir, "release", 0);
    assert!(releaser.wait().unwrap().success());
    let mut taker = spawn_one(&dir, "reclaim", 1);
    assert!(taker.wait().unwrap().success());

    let store = Store::open(&dir).unwrap();
    let row = store
        .rows("steps")
        .into_iter()
        .find(|row| canon::get(row, "id").and_then(canon::as_i64) == Some(2))
        .unwrap();
    assert_eq!(canon::get(row, "claimed_by").and_then(canon::as_str), Some("agent-1"));
    let _ = std::fs::remove_dir_all(&dir);
}

fn spawn_wave(dir: &std::path::Path, op: &str) -> Vec<std::process::Child> {
    (0..4).map(|i| spawn_one(dir, op, i)).collect()
}

fn spawn_one(dir: &std::path::Path, op: &str, worker: i64) -> std::process::Child {
    Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("four_processes_claim_one_store")
        .env("TRAMMEL_CLAIM_WORKER", worker.to_string())
        .env("TRAMMEL_CLAIM_OP", op)
        .env("TRAMMEL_CLAIM_STORE", dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap()
}

fn worker() {
    let dir = PathBuf::from(std::env::var("TRAMMEL_CLAIM_STORE").unwrap());
    let worker: i64 = std::env::var("TRAMMEL_CLAIM_WORKER").unwrap().parse().unwrap();
    let op = std::env::var("TRAMMEL_CLAIM_OP").unwrap_or_else(|_| "claim".into());
    let agent = format!("agent-{worker}");
    let mut store = Store::open(&dir).expect("open store");
    match op.as_str() {
        "release" => {
            store.release(2, "agent-0").expect("release step 2");
        }
        "reclaim" => {
            store.claim(1, 2, &agent).expect("reclaim step 2");
        }
        _ => {
            let _ = store.claim(1, 1, &agent);
            let first = 2 * worker + 1;
            let second = 2 * worker + 2;
            if first != 1 {
                store.claim(1, first, &agent).expect("own first step");
            }
            store.claim(1, second, &agent).expect("own second step");
        }
    }
    store.save().expect("save");
}
