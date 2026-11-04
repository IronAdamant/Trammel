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
    assert!(init.contains("\"version\":\"4.0.1\""), "{init}");
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
