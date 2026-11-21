//! verify_step over MCP, 4.5: where the copy is and how deep it goes, what a
//! run left behind, a clean checkout of a commit (`ref`), the outcome stored
//! on a plan step, and each failed test's own message.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use trammel::canon::{as_bool, as_i64, as_str, get, obj, stable};
use trammel::json::Value;

fn trammel_mcp_bin() -> PathBuf {
    let main = PathBuf::from(env!("CARGO_BIN_EXE_trammel"));
    main.parent().unwrap().join(if cfg!(windows) { "trammel-mcp.exe" } else { "trammel-mcp" })
}

struct Live { child: std::process::Child, stdin: std::process::ChildStdin, lines: std::io::Lines<BufReader<std::process::ChildStdout>>, id: usize }

impl Live {
    fn start(dir: &Path, envs: &[(&str, &str)]) -> Live {
        let mut cmd = Command::new(trammel_mcp_bin());
        cmd.current_dir(dir).env_remove("TRAMMEL_TOOLS").env_remove("TRAMMEL_DISABLE").env_remove("TRAMMEL_VERIFY_CACHE");
        for (k, v) in envs { cmd.env(k, v); }
        let mut child = cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().unwrap();
        let stdin = child.stdin.take().unwrap();
        let lines = BufReader::new(child.stdout.take().unwrap()).lines();
        Live { child, stdin, lines, id: 0 }
    }

    /// One tools/call; its structuredContent (an error has `code`).
    fn call(&mut self, name: &str, args: Value) -> Value {
        self.id += 1;
        let request = obj(vec![("jsonrpc", Value::string("2.0")), ("id", Value::Int(self.id as i64)), ("method", Value::string("tools/call")), ("params", obj(vec![("name", Value::string(name)), ("arguments", args)]))]);
        writeln!(self.stdin, "{}", stable(&request)).unwrap();
        self.stdin.flush().unwrap();
        let reply = trammel::json::parse(&self.lines.next().unwrap().unwrap()).unwrap();
        get(&reply, "result").and_then(|r| get(r, "structuredContent")).cloned().unwrap_or_else(|| panic!("{}", stable(&reply)))
    }

    fn finish(mut self) { drop(self.stdin); let _ = self.child.wait(); }
}

fn project(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("trammel-vc-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git").arg("-C").arg(dir).args(["-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "core.autocrlf=false"]).args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn cmd(windows: &str, unix: &str) -> Value {
    let argv: Vec<&str> = if cfg!(windows) { vec!["cmd", "/C", windows] } else { vec!["sh", "-c", unix] };
    Value::Array(argv.into_iter().map(Value::string).collect())
}

fn args(fields: Vec<(&str, Value)>) -> Value { obj(fields) }

fn s(value: &Value, key: &str) -> String { get(value, key).and_then(as_str).unwrap_or_else(|| panic!("no string {key} in {}", stable(value))).to_string() }

fn b(value: &Value, key: &str) -> bool { get(value, key).and_then(as_bool).unwrap_or_else(|| panic!("no bool {key} in {}", stable(value))) }

fn forget(dir: &Path) {
    trammel::workspace::forget(dir);
    let _ = std::fs::remove_dir_all(dir);
}

/// A case from a large Rust workspace: a guard passes in a working tree
/// whose file was rewritten with LF and fails in a fresh checkout that
/// makes it CRLF.
#[test]
fn ref_verifies_a_clean_checkout_and_the_working_tree_is_untouched() {
    let dir = project("ref");
    git(&dir, &["init", "-q"]);
    std::fs::write(dir.join(".gitattributes"), "guard.txt text eol=crlf\n").unwrap();
    std::fs::write(dir.join("guard.txt"), "a\nb\n").unwrap();
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", "guard"]);
    std::fs::write(dir.join("untracked.txt"), "only in the working tree").unwrap();
    let head = git(&dir, &["rev-parse", "HEAD"]);
    let index_before = std::fs::read(dir.join(".git/index")).unwrap();
    // The guard: guard.txt is the 4 bytes "a\nb\n" (LF), and an untracked
    // file is there.
    let guard = cmd(
        "for %I in (guard.txt) do @if not %~zI==4 (echo guard.txt is %~zI bytes, want 4& exit 1)",
        "n=$(wc -c < guard.txt | tr -d ' '); [ $n -eq 4 ] || { echo \"guard.txt is $n bytes, want 4\"; exit 1; }",
    );
    let mut live = Live::start(&dir, &[]);
    let tree = live.call("verify_step", args(vec![("test_cmd", guard.clone())]));
    assert!(b(&tree, "success"), "{}", stable(&tree));
    assert_eq!(s(&tree, "source"), "working_tree");
    assert!(get(&tree, "commit").is_none());
    let commit = live.call("verify_step", args(vec![("test_cmd", guard.clone()), ("ref", Value::string("HEAD"))]));
    assert!(!b(&commit, "success"), "the checkout has CRLF: {}", stable(&commit));
    assert_eq!(s(&commit, "source"), "commit");
    assert_eq!(s(&commit, "commit"), head);
    assert_eq!(s(&commit, "ref"), "HEAD");
    assert!(s(&commit, "output").contains("guard.txt is 6 bytes"), "{}", stable(&commit));
    // Tracked files only: the untracked file is not in the checkout.
    let listing = cmd("if exist untracked.txt exit 1", "[ ! -e untracked.txt ]");
    assert!(b(&live.call("verify_step", args(vec![("test_cmd", listing.clone()), ("ref", Value::string("HEAD"))])), "success"));
    assert!(!b(&live.call("verify_step", args(vec![("test_cmd", listing)])), "success"));
    // A ref that names nothing is an error that says what to pass.
    let bad = live.call("verify_step", args(vec![("test_cmd", guard), ("ref", Value::string("no-such-branch"))]));
    assert_eq!(s(&bad, "code"), "bad_ref", "{}", stable(&bad));
    assert!(s(&bad, "message").contains("no-such-branch") && s(&bad, "hint").contains("HEAD"));
    live.finish();
    assert_eq!(std::fs::read(dir.join("guard.txt")).unwrap(), b"a\nb\n");
    assert_eq!(std::fs::read(dir.join(".git/index")).unwrap(), index_before, "the project's index is untouched");
    assert_eq!(git(&dir, &["rev-parse", "HEAD"]), head);
    assert_eq!(git(&dir, &["worktree", "list"]).lines().count(), 1, "no worktree was added");
    forget(&dir);
}

/// The other case from that workspace: a check restored a file with CRLF
/// and its own "restored" line passed. The reply now names it.
#[test]
fn left_behind_names_created_deleted_and_line_ending_only_changes() {
    let dir = project("left");
    std::fs::write(dir.join("guard.txt"), "a\nb\n").unwrap();
    std::fs::write(dir.join("gone.txt"), "x").unwrap();
    let mut live = Live::start(&dir, &[]);
    let messy = cmd(
        "(echo a& echo b)> guard.txt& echo new> made.txt& del gone.txt& mkdir target& echo o> target\\out.bin",
        "printf 'a\\r\\nb\\r\\n' > guard.txt; echo new > made.txt; rm gone.txt; mkdir target; echo o > target/out.bin",
    );
    let reply = live.call("verify_step", args(vec![("test_cmd", messy)]));
    assert!(b(&reply, "success"), "{}", stable(&reply));
    let left = get(&reply, "left_behind").unwrap();
    assert_eq!(get(left, "created").and_then(as_i64), Some(1), "made.txt only, not build output under target: {}", stable(left));
    assert_eq!(get(left, "modified").and_then(as_i64), Some(1));
    assert_eq!(get(left, "deleted").and_then(as_i64), Some(1));
    assert_eq!(get(left, "line_endings_only").and_then(as_i64), Some(1));
    let files = stable(get(left, "files").unwrap());
    assert!(files.contains(r#"{"change":"line_endings","now":"CRLF","path":"guard.txt"}"#), "{files}");
    assert!(files.contains(r#"{"change":"created","path":"made.txt"}"#) && files.contains(r#"{"change":"deleted","path":"gone.txt"}"#), "{files}");
    // The next run starts from the project's bytes again.
    let clean = live.call("verify_step", args(vec![("test_cmd", cmd("exit 0", "true"))]));
    assert_eq!(stable(get(&clean, "left_behind").unwrap()), r#"{"created":0,"deleted":0,"modified":0}"#);
    live.finish();
    assert_eq!(std::fs::read(dir.join("guard.txt")).unwrap(), b"a\nb\n", "the working tree is untouched");
    forget(&dir);
}

/// A plan survives a context summary with its verification: verify_step
/// stores each outcome on the step, get_plan and export_plan show it, and
/// resume points at the first step without a passing verify.
#[test]
fn verify_outcomes_are_stored_on_plan_steps_and_resume_points_at_the_next() {
    let dir = project("plan");
    std::fs::write(dir.join("models.py"), "class Item:\n    pass\n").unwrap();
    std::fs::write(dir.join("store.py"), "from models import Item\n\nclass Store:\n    pass\n").unwrap();
    // A libtest-shaped failure, printed by a script.
    let (script, run) = if cfg!(windows) {
        // .\ because cmd may not search the current directory
        // (NoDefaultCurrentDirectoryInExePath).
        ("fail.cmd", vec!["cmd", "/C", ".\\fail.cmd"])
    } else {
        ("fail.sh", vec!["sh", "fail.sh"])
    };
    let lines = ["running 2 tests", "test tests::adds ... FAILED", "test tests::other ... ok", "", "failures:", "", "---- tests::adds stdout ----", "thread 'tests::adds' panicked at src/lib.rs:9:9:", "assertion failed: total == 4", "", "failures:", "    tests::adds", "", "test result: FAILED. 1 passed; 1 failed"];
    let body = if cfg!(windows) {
        let mut text = String::from("@echo off\r\n");
        for line in lines { text.push_str(&if line.is_empty() { "echo.\r\n".to_string() } else { format!("echo {line}\r\n") }); }
        text.push_str("exit /b 1\r\n");
        text
    } else {
        let mut text = String::new();
        for line in lines { text.push_str(&format!("echo \"{line}\"\n")); }
        text.push_str("exit 1\n");
        text
    };
    std::fs::write(dir.join(script), body).unwrap();
    let failing = Value::Array(run.into_iter().map(Value::string).collect());
    let passing = cmd("exit 0", "true");
    let mut live = Live::start(&dir, &[]);
    let plan = live.call("start_plan", args(vec![("goal", Value::string("add stock to Item in models.py and store.py"))]));
    let plan_id = get(&plan, "plan_id").and_then(as_i64).unwrap();
    let got = live.call("get_plan", args(vec![("plan_id", Value::Int(plan_id))]));
    let Some(Value::Array(steps)) = get(&got, "steps") else { panic!("{}", stable(&got)) };
    let ids: Vec<i64> = steps.iter().map(|st| get(st, "id").and_then(as_i64).unwrap()).collect();
    assert!(ids.len() >= 2, "{}", stable(&got));
    // start_plan already says each step's id, so get_plan is not needed.
    let Some(Value::Array(planned)) = get(&plan, "strategy").and_then(|st| get(st, "steps")) else { panic!("{}", stable(&plan)) };
    assert_eq!(planned.iter().map(|st| get(st, "id").and_then(as_i64).unwrap()).collect::<Vec<_>>(), ids);
    assert!(s(&plan, "next").contains(&format!("step_id {}", ids[0])), "{}", stable(&plan));
    // Half a link is refused, and so is a step of another plan.
    let half = live.call("verify_step", args(vec![("test_cmd", passing.clone()), ("plan_id", Value::Int(plan_id))]));
    assert_eq!(s(&half, "code"), "missing_argument");
    assert!(s(&half, "message").contains("step_id"));
    let stray = live.call("verify_step", args(vec![("test_cmd", passing.clone()), ("plan_id", Value::Int(plan_id)), ("step_id", Value::Int(9999))]));
    assert_eq!(s(&stray, "code"), "step_not_found");

    // A failure: each failed test's own message, and the outcome on the step.
    let failed = live.call("verify_step", args(vec![("test_cmd", failing), ("plan_id", Value::Int(plan_id)), ("step_id", Value::Int(ids[0]))]));
    assert!(!b(&failed, "success"));
    let Some(Value::Array(tests)) = get(&failed, "failed_tests") else { panic!("{}", stable(&failed)) };
    assert_eq!(tests.len(), 1, "{}", stable(&failed));
    assert_eq!(s(&tests[0], "name"), "tests::adds");
    assert_eq!(s(&tests[0], "excerpt"), "thread 'tests::adds' panicked at src/lib.rs:9:9:\nassertion failed: total == 4");
    assert_eq!(stable(get(&failed, "recorded").unwrap()), format!(r#"{{"plan_id":{plan_id},"status":"pending","step_id":{}}}"#, ids[0]));
    let resumed = live.call("resume", args(vec![("plan_id", Value::Int(plan_id))]));
    let next = get(&resumed, "next_step").unwrap();
    assert_eq!(get(next, "id").and_then(as_i64), Some(ids[0]), "a failed step is still next: {}", stable(&resumed));
    let verification = get(next, "verification").unwrap();
    assert!(!b(verification, "passed"));
    assert_eq!(s(verification, "source"), "working_tree");
    assert!(s(verification, "command").contains(script));
    assert!(get(verification, "at").is_some());
    assert_eq!(s(verification, "excerpt"), "tests::adds: thread 'tests::adds' panicked at src/lib.rs:9:9: | assertion failed: total == 4");

    // A pass marks the step passed and moves resume on; a background run
    // stores its outcome too.
    let ok = live.call("verify_step", args(vec![("test_cmd", passing.clone()), ("plan_id", Value::Int(plan_id)), ("step_id", Value::Int(ids[0]))]));
    assert!(b(&ok, "success") && get(&ok, "recorded").is_some());
    assert_eq!(s(get(&ok, "recorded").unwrap(), "status"), "passed");
    assert!(get(&ok, "cached").is_none(), "the first pass runs the tests");
    let resumed = live.call("resume", args(vec![("plan_id", Value::Int(plan_id))]));
    assert_eq!(get(get(&resumed, "next_step").unwrap(), "id").and_then(as_i64), Some(ids[1]));
    for id in &ids[1..] {
        let job = live.call("verify_step", args(vec![("test_cmd", passing.clone()), ("plan_id", Value::Int(plan_id)), ("step_id", Value::Int(*id)), ("background", Value::Bool(true))]));
        let done = live.call("job_status", args(vec![("job_id", Value::string(s(&job, "job_id"))), ("wait_s", Value::Int(60))]));
        assert_eq!(s(&done, "status"), "completed", "{}", stable(&done));
        assert!(get(get(&done, "result").unwrap(), "recorded").is_some(), "{}", stable(&done));
        // Nothing changed since the pass: the same result, not run again.
        assert!(get(get(&done, "result").unwrap(), "cached").is_some(), "{}", stable(&done));
    }
    let resumed = live.call("resume", args(vec![("plan_id", Value::Int(plan_id))]));
    assert!(matches!(get(&resumed, "next_step"), Some(Value::Null)), "{}", stable(&resumed));
    assert!(s(&resumed, "next").contains("complete_plan"));
    // get_plan and export_plan show each step's verification.
    let got = live.call("get_plan", args(vec![("plan_id", Value::Int(plan_id))]));
    let Some(Value::Array(steps)) = get(&got, "steps") else { panic!() };
    assert!(steps.iter().all(|st| get(st, "verification").map(|v| b(v, "passed")).unwrap_or(false)), "{}", stable(&got));
    assert!(steps.iter().all(|st| s(st, "status") == "passed"), "{}", stable(&got));
    assert!(get(&got, "next_step").is_none(), "get_plan is unchanged apart from verification");
    let exported = live.call("export_plan", args(vec![("plan_id", Value::Int(plan_id))]));
    let Some(Value::Array(steps)) = get(&exported, "steps") else { panic!() };
    assert_eq!(get(&steps[0], "id").and_then(as_i64), Some(ids[0]));
    assert!(b(get(&steps[0], "verification").unwrap(), "passed"), "{}", stable(&exported));
    // A change to a file runs the tests again; rerun does too.
    std::fs::write(dir.join("models.py"), "class Item:\n    stock = 0\n").unwrap();
    let fresh = live.call("verify_step", args(vec![("test_cmd", passing.clone())]));
    assert!(b(&fresh, "success") && get(&fresh, "cached").is_none(), "{}", stable(&fresh));
    let again = live.call("verify_step", args(vec![("test_cmd", passing.clone())]));
    assert!(get(&again, "cached").is_some(), "{}", stable(&again));
    let forced = live.call("verify_step", args(vec![("test_cmd", passing.clone()), ("rerun", Value::Bool(true))]));
    assert!(get(&forced, "cached").is_none(), "{}", stable(&forced));
    live.finish();
    forget(&dir);
}

/// The copy says where it is and how deep a build goes; on Windows it warns
/// near 259 characters and refuses past it, naming TRAMMEL_VERIFY_CACHE.
#[test]
fn the_copy_reports_its_depth_and_windows_refuses_a_copy_too_deep() {
    let dir = project("deep");
    std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"x\"\nversion = \"0.1.0\"\n").unwrap();
    let base_of = |len: usize| {
        let root = std::env::temp_dir().join(format!("tvd{}", std::process::id()));
        let have = trammel::pathlen::chars(&root) + 1;
        (root.clone(), root.join("d".repeat(len.saturating_sub(have).max(1))))
    };
    // cargo's deepest file is the target dir plus 118 characters for crate
    // `x`, and the target dir is the base plus 13: base 120 gives 251 (near),
    // base 140 gives 271 (past the limit).
    let (near_root, near) = base_of(120);
    let (_, over) = base_of(140);
    let noop = cmd("exit 0", "true");
    let mut live = Live::start(&dir, &[("TRAMMEL_VERIFY_CACHE", near.to_str().unwrap())]);
    let reply = live.call("verify_step", args(vec![("test_cmd", noop.clone())]));
    live.finish();
    assert!(b(&reply, "success"), "{}", stable(&reply));
    let copy = get(&reply, "copy").unwrap();
    let path = s(copy, "path");
    assert!(path.starts_with(near.to_str().unwrap()), "{path}");
    let longest = get(copy, "longest_path").and_then(as_i64).unwrap() as usize;
    assert_eq!(longest, trammel::pathlen::chars(Path::new(&s(copy, "target"))) + 118);
    if cfg!(windows) {
        assert!((240..=259).contains(&longest), "{longest}");
        assert!(s(&reply, "path_warning").contains("TRAMMEL_VERIFY_CACHE"), "{}", stable(&reply));
    } else {
        assert!(get(&reply, "path_warning").is_none(), "the limit is Windows'");
    }
    let mut live = Live::start(&dir, &[("TRAMMEL_VERIFY_CACHE", over.to_str().unwrap())]);
    let reply = live.call("verify_step", args(vec![("test_cmd", noop)]));
    live.finish();
    if cfg!(windows) {
        assert_eq!(s(&reply, "code"), "path_too_long", "{}", stable(&reply));
        assert!(s(&reply, "message").contains("characters") && s(&reply, "hint").contains("TRAMMEL_VERIFY_CACHE"), "{}", stable(&reply));
    } else {
        assert!(b(&reply, "success"), "{}", stable(&reply));
    }
    let _ = std::fs::remove_dir_all(&near_root);
    let _ = std::fs::remove_dir_all(&dir);
}

/// complete_plan with verify and note: one call tests, leaves the handoff
/// note and closes the plan; a failure leaves the plan open.
#[test]
fn complete_plan_verifies_notes_and_closes_in_one_call() {
    let dir = project("complete");
    std::fs::write(dir.join("models.py"), "class Item:\n    pass\n").unwrap();
    std::fs::write(dir.join("store.py"), "from models import Item\n").unwrap();
    let mut live = Live::start(&dir, &[]);
    let plan = live.call("start_plan", args(vec![("goal", Value::string("add stock to Item in models.py and store.py"))]));
    let plan_id = get(&plan, "plan_id").and_then(as_i64).unwrap();
    let failing = live.call("complete_plan", args(vec![("plan_id", Value::Int(plan_id)), ("verify", Value::Bool(true)), ("note", Value::string("done")), ("test_cmd", cmd("exit 1", "false"))]));
    assert!(!b(&failing, "completed"), "{}", stable(&failing));
    assert!(!b(get(&failing, "verify").unwrap(), "success"));
    let open = live.call("get_plan", args(vec![("plan_id", Value::Int(plan_id))]));
    assert_ne!(s(&open, "status"), "completed");
    assert!(matches!(get(&open, "notes"), Some(Value::Array(n)) if n.is_empty()), "{}", stable(&open));
    let done = live.call("complete_plan", args(vec![("plan_id", Value::Int(plan_id)), ("verify", Value::Bool(true)), ("note", Value::string("stock added; store reads it")), ("test_cmd", cmd("exit 0", "true"))]));
    assert!(b(&done, "completed") && b(get(&done, "verify").unwrap(), "success"), "{}", stable(&done));
    assert!(get(&done, "note_id").is_some() && get(&done, "recipe").is_some());
    let closed = live.call("get_plan", args(vec![("plan_id", Value::Int(plan_id))]));
    assert_eq!(s(&closed, "status"), "completed");
    let Some(Value::Array(notes)) = get(&closed, "notes") else { panic!() };
    assert_eq!(s(&notes[0], "text"), "stock added; store reads it");
    live.finish();
    forget(&dir);
}
