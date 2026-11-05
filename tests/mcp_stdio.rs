//! `trammel mcp` and `trammel-mcp` both answer the stdio handshake.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

fn handshake(bin: &str, args: &[&str]) {
    let mut child = Command::new(bin)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"2025-06-18","capabilities":{{}},"clientInfo":{{"name":"probe","version":"0"}}}}}}"#
    )
    .unwrap();
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{{}}}}"#
    )
    .unwrap();
    drop(stdin);
    let mut lines = BufReader::new(stdout).lines();
    let init = lines.next().unwrap().unwrap();
    let tools = lines.next().unwrap().unwrap();
    assert!(init.contains("\"protocolVersion\":\"2025-06-18\""), "{init}");
    assert!(init.contains("\"name\":\"trammel\""), "{init}");
    assert!(init.contains(&format!("\"version\":\"{}\"", env!("CARGO_PKG_VERSION"))), "{init}");
    assert!(tools.contains("\"name\":\"start_plan\""), "{tools}");
    assert!(tools.contains("\"name\":\"decompose\""), "{tools}");
    let status = child.wait().unwrap();
    assert!(status.success(), "{status}");
}

fn trammel_bin() -> String {
    env!("CARGO_BIN_EXE_trammel").to_string()
}

fn trammel_mcp_bin() -> String {
    let main = std::path::PathBuf::from(trammel_bin());
    let name = if cfg!(windows) { "trammel-mcp.exe" } else { "trammel-mcp" };
    main.parent().unwrap().join(name).to_string_lossy().into_owned()
}

#[test]
fn trammel_mcp_argument_answers_initialize_and_tools_list() {
    handshake(&trammel_bin(), &["mcp"]);
}

#[test]
fn trammel_mcp_binary_answers_the_same_handshake() {
    handshake(&trammel_mcp_bin(), &[]);
}

/// Sends one request per line and returns the replies in order.
fn session(requests: &[String], cwd: &std::path::Path) -> Vec<trammel::json::Value> {
    let mut child = Command::new(trammel_mcp_bin())
        .current_dir(cwd)
        .env("TRAMMEL_MCP_SURFACE", "all")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for request in requests { writeln!(stdin, "{request}").unwrap(); }
    drop(stdin);
    let replies = BufReader::new(child.stdout.take().unwrap()).lines().map(|l| trammel::json::parse(&l.unwrap()).unwrap()).collect();
    assert!(child.wait().unwrap().success());
    replies
}

fn call(id: usize, name: &str, args: &str) -> String {
    format!(r#"{{"jsonrpc":"2.0","id":{id},"method":"tools/call","params":{{"name":"{name}","arguments":{args}}}}}"#)
}

fn at<'a>(value: &'a trammel::json::Value, path: &[&str]) -> &'a trammel::json::Value {
    path.iter().fold(value, |v, key| trammel::canon::get(v, key).unwrap_or_else(|| panic!("no {key} in {}", trammel::canon::stable(v))))
}

#[test]
fn an_agent_can_fill_arguments_from_tools_list_and_act_on_errors() {
    let dir = std::env::temp_dir().join(format!("trammel-mcp-agent-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pkg")).unwrap();
    std::fs::write(dir.join("pkg/models.py"), "class Item:\n    pass\n").unwrap();
    std::fs::write(dir.join("pkg/store.py"), "from pkg.models import Item\n\nclass Store:\n    pass\n").unwrap();
    let replies = session(&[
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#.to_string(),
        call(2, "start_plan", r#"{"goal":"add a quantity field to Item in the store"}"#),
        call(3, "get_constraints", "{}"),
        call(4, "claim_step", r#"{"plan_id":1,"step_id":0,"agent_id":"agent-b"}"#),
        call(5, "record_steps", r#"{"steps":[{"step_id":0,"status":"passed"}]}"#),
        call(6, "decompose", "{}"),
        call(7, "claim_step", r#"{"plan_id":1,"step_id":1,"agent_id":"agent-a"}"#),
        call(8, "claim_step", r#"{"plan_id":1,"step_id":1,"agent_id":"agent-b"}"#),
        call(9, "claim_step", r#"{"plan_id":1,"step_id":2,"agent_id":"agent-b"}"#),
    ], &dir);
    let tools = match at(&replies[0], &["result", "tools"]) { trammel::json::Value::Array(items) => items.clone(), _ => panic!() };
    assert_eq!(tools.len(), 33);
    for tool in &tools {
        let name = trammel::canon::stable(at(tool, &["name"]));
        let description = trammel::canon::as_str(at(tool, &["description"])).unwrap();
        assert!(description.len() > 20 && !description.contains('_') || description.contains(' '), "{name}: {description}");
        assert!(trammel::canon::get(tool, "outputSchema").is_none(), "{name}");
        let schema = at(tool, &["inputSchema"]);
        assert!(matches!(at(schema, &["properties"]), trammel::json::Value::Object(p) if !p.is_empty()), "{name}");
    }
    let start = tools.iter().find(|t| trammel::canon::get(t, "name").and_then(trammel::canon::as_str) == Some("start_plan")).unwrap();
    assert_eq!(trammel::canon::stable(at(start, &["inputSchema", "required"])), r#"["goal"]"#);
    // Every structuredContent is an object; lists arrive as {"items": [...]}.
    for reply in &replies[1..] {
        assert!(matches!(at(reply, &["result", "structuredContent"]), trammel::json::Value::Object(_)), "{}", trammel::canon::stable(reply));
    }
    assert!(matches!(at(&replies[2], &["result", "structuredContent", "items"]), trammel::json::Value::Array(_)));
    // A step_index where a step id belongs says so.
    assert_eq!(trammel::canon::as_str(at(&replies[3], &["result", "structuredContent", "code"])), Some("step_not_found"));
    assert!(trammel::canon::as_str(at(&replies[3], &["result", "structuredContent", "hint"])).unwrap().contains("not step_index"));
    assert_eq!(trammel::canon::stable(at(&replies[4], &["result", "structuredContent", "not_found"])), "[0]");
    let hint = trammel::canon::as_str(at(&replies[5], &["result", "structuredContent", "hint"])).unwrap();
    assert!(hint.contains("goal"), "{hint}");
    // Agent A holds step 1, so agent B is told to pick another one.
    assert_eq!(trammel::canon::as_bool(at(&replies[6], &["result", "structuredContent", "claimed"])), Some(true));
    assert_eq!(trammel::canon::as_str(at(&replies[7], &["result", "structuredContent", "code"])), Some("claim_held"));
    // store.py (step 2) depends on models.py (step 1), which is claimed but not done.
    assert_eq!(trammel::canon::as_str(at(&replies[8], &["result", "structuredContent", "code"])), Some("step_blocked"));
    let blocked = trammel::canon::as_str(at(&replies[8], &["result", "structuredContent", "hint"])).unwrap();
    assert!(blocked.contains("[1]") && blocked.contains("available_steps"), "{blocked}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn initialize_echoes_every_published_protocol_it_supports() {
    let dir = std::env::temp_dir();
    for version in ["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"] {
        let request = format!(r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"{version}","capabilities":{{}},"clientInfo":{{"name":"old","version":"0"}}}}}}"#);
        let replies = session(&[request], &dir);
        assert_eq!(trammel::canon::as_str(at(&replies[0], &["result", "protocolVersion"])), Some(version));
    }
    let unknown = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"1999-01-01","capabilities":{},"clientInfo":{"name":"x","version":"0"}}}"#.to_string();
    assert_eq!(trammel::canon::as_str(at(&session(&[unknown], &dir)[0], &["result", "protocolVersion"])), Some("2025-11-25"));
}
