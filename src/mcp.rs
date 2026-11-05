//! JSON-RPC stdio server. Speaks 2026-07-28 `server/discover` and the
//! 2025-11-25 and 2025-06-18 initialize handshakes.

use crate::canon::stable;
use crate::contract::{self, Failure};
use crate::json::{self, Value};
use crate::tools;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

/// Every published MCP revision this server answers. `initialize` echoes the
/// client's version when it is listed here; the tool calls it uses mean the
/// same thing in all of them (fields a revision lacks are ignored by clients).
pub const PROTOCOLS: &[&str] = &["2026-07-28", "2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "Trammel plans multi-file changes in dependency order and keeps plans, constraints, failures and recipes in .trammel/ so later sessions and other agents can pick work up. Loop: get_constraints and get_recipe, start_plan, then per step verify_step (tests run on a temporary copy), then complete_plan. To hand off, the next agent calls get_plan with the plan_id. TRAMMEL_MCP_SURFACE=all adds claim_step, available_steps and record_steps for several agents on one plan.";

pub fn serve(store: Option<PathBuf>) -> Result<(), String> {
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let mut line = String::new();
    let surface = std::env::var("TRAMMEL_MCP_SURFACE").unwrap_or_else(|_| "primary".into());
    let telemetry = std::env::var("TRAMMEL_TELEMETRY").ok().as_deref() == Some("1");
    loop {
        line.clear();
        let n = reader.read_line(&mut line).map_err(|e| e.to_string())?;
        if n == 0 { break; }
        if line.trim().is_empty() { continue; }
        let message = match json::parse(line.trim()) {
            Ok(value) => value,
            Err(_) => { reply_err(None, -32700, "parse error"); continue; }
        };
        let id = crate::canon::get(&message, "id").cloned();
        let method = crate::canon::get(&message, "method").and_then(crate::canon::as_str).unwrap_or("");
        if id.is_none() { continue; }
        let params = crate::canon::get(&message, "params").cloned().unwrap_or(Value::Object(vec![]));
        match method {
            "initialize" => reply(id, initialize(&params)),
            "server/discover" => reply(id, discover(&surface)),
            "tools/list" => reply(id, crate::canon::obj(vec![("tools", Value::Array(catalogue(&surface)))])),
            "tools/call" => reply(id, tool_call(&params, store.as_deref(), telemetry)),
            "ping" => reply(id, Value::Object(vec![])),
            _ => reply_err(id, -32601, "method not found"),
        }
    }
    Ok(())
}

fn initialize(params: &Value) -> Value {
    let requested = crate::canon::get(params, "protocolVersion").and_then(crate::canon::as_str).unwrap_or("2025-06-18");
    let version = if PROTOCOLS.contains(&requested) { requested } else { "2025-11-25" };
    crate::canon::obj(vec![
        ("protocolVersion", Value::string(version)),
        ("capabilities", crate::canon::obj(vec![("tools", crate::canon::obj(vec![("listChanged", Value::Bool(false))]))])),
        ("serverInfo", server_info()),
        ("instructions", Value::string(INSTRUCTIONS)),
    ])
}

fn discover(surface: &str) -> Value {
    crate::canon::obj(vec![
        ("protocolVersion", Value::string("2026-07-28")),
        ("serverInfo", server_info()),
        ("capabilities", crate::canon::obj(vec![("tools", crate::canon::obj(vec![("listChanged", Value::Bool(false))]))])),
        ("tools", Value::Array(catalogue(surface))),
    ])
}

fn server_info() -> Value {
    crate::canon::obj(vec![("name", Value::string("trammel")), ("version", Value::string(crate::VERSION))])
}

fn catalogue(surface: &str) -> Vec<Value> {
    tools::all_names().into_iter().filter(|name| surface == "all" || surface == "full" || tools::PRIMARY.contains(name)).map(tool_schema).collect()
}

fn tool_schema(name: &str) -> Value {
    let (read_only, destructive, _, _) = tools::hints(name);
    crate::canon::obj(vec![
        ("name", Value::string(name)),
        ("description", Value::string(crate::catalogue::description(name))),
        ("inputSchema", crate::catalogue::input_schema(name)),
        ("annotations", crate::canon::obj(vec![
            ("readOnlyHint", Value::Bool(read_only)),
            ("destructiveHint", Value::Bool(destructive)),
            ("idempotentHint", Value::Bool(false)),
            ("openWorldHint", Value::Bool(false)),
        ])),
    ])
}

fn tool_call(params: &Value, store: Option<&std::path::Path>, telemetry: bool) -> Value {
    let name = crate::canon::get(params, "name").and_then(crate::canon::as_str).unwrap_or("");
    let args = crate::canon::get(params, "arguments").cloned().unwrap_or(Value::Object(vec![]));
    let cwd = std::env::current_dir().unwrap_or_else(|_| path_buf_from("."));
    match tools::call(name, &args, &cwd, store, telemetry) {
        Ok(value) => content(value, false),
        Err(err) => content(err.value(), true),
    }
}

fn path_buf_from(path: &str) -> std::path::PathBuf { std::path::PathBuf::from(path) }

/// MCP requires `structuredContent` to be an object, and SDK clients reject a
/// bare array, so list results travel as `{"items": [...]}`.
pub fn content(value: Value, is_error: bool) -> Value {
    let value = match value {
        Value::Object(_) => value,
        other => crate::canon::obj(vec![("items", other)]),
    };
    let text = stable(&value);
    crate::canon::obj(vec![
        ("content", Value::Array(vec![crate::canon::obj(vec![("type", Value::string("text")), ("text", Value::string(text))])])),
        ("structuredContent", value),
        ("isError", Value::Bool(is_error)),
    ])
}

fn reply(id: Option<Value>, result: Value) {
    let id = id.unwrap_or(Value::Null);
    let body = crate::canon::obj(vec![("jsonrpc", Value::string("2.0")), ("id", id), ("result", result)]);
    println!("{}", stable(&body));
    let _ = io::stdout().flush();
}

fn reply_err(id: Option<Value>, code: i64, message: &str) {
    let body = crate::canon::obj(vec![
        ("jsonrpc", Value::string("2.0")),
        ("id", id.unwrap_or(Value::Null)),
        ("error", crate::canon::obj(vec![("code", Value::Int(code)), ("message", Value::string(message))])),
    ]);
    println!("{}", stable(&body));
    let _ = io::stdout().flush();
}

pub fn removed_preview(name: &str) -> Failure { contract::tool_removed(name, "record_steps") }
