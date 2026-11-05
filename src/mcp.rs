//! JSON-RPC stdio server. Speaks 2026-07-28 `server/discover` and the
//! 2025-11-25 and 2025-06-18 initialize handshakes.

use crate::canon::stable;
use crate::contract::{self, Failure};
use crate::groups::{self, Switches};
use std::time::SystemTime;
use crate::json::{self, Value};
use crate::tools;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

/// Every published MCP revision this server answers. `initialize` echoes the
/// client's version when it is listed here; the tool calls it uses mean the
/// same thing in all of them (fields a revision lacks are ignored by clients).
pub const PROTOCOLS: &[&str] = &["2026-07-28", "2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "Trammel plans multi-file changes in dependency order and keeps plans, constraints, failures and recipes in .trammel/ so later sessions and other agents can pick work up. Loop: get_constraints and get_recipe, start_plan, then per step verify_step (tests run on a temporary copy), then complete_plan. To hand off, add_note what you did and what is left; the next agent calls get_plan with the plan_id and reads the notes. TRAMMEL_MCP_SURFACE=all adds claim_step, available_steps and record_steps for several agents on one plan.";

pub fn serve(store: Option<PathBuf>) -> Result<(), String> {
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let mut line = String::new();
    let surface = std::env::var("TRAMMEL_MCP_SURFACE").unwrap_or_else(|_| "primary".into());
    let telemetry = std::env::var("TRAMMEL_TELEMETRY").ok().as_deref() == Some("1");
    let surface_all = surface == "all" || surface == "full";
    // A typo in TRAMMEL_TOOLS / TRAMMEL_DISABLE stops the server, so the
    // client shows it as failed instead of silently serving the wrong tools.
    let cwd = std::env::current_dir().unwrap_or_else(|_| path_buf_from("."));
    let mut live = match LiveGroups::start(&crate::root::find_root(&cwd)) {
        Ok(live) => live,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };
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
        // An agent may edit the agent-tools block mid-session. Re-read it when
        // it changed and tell the client before answering.
        if live.refresh(surface_all) {
            notify("notifications/tools/list_changed");
        }
        let switches = &live.switches;
        match method {
            "initialize" => reply(id, initialize(&params, switches)),
            "server/discover" => reply(id, discover(&surface, switches)),
            "tools/list" => reply(id, crate::canon::obj(vec![("tools", Value::Array(catalogue(&surface, switches)))])),
            "tools/call" => reply(id, tool_call(&params, store.as_deref(), telemetry, switches)),
            "ping" => reply(id, Value::Object(vec![])),
            _ => reply_err(id, -32601, "method not found"),
        }
    }
    Ok(())
}

/// The tool groups in force: env vars (fixed for the session) plus the
/// project's AGENTS.md `agent-tools` block, re-read when it changes.
struct LiveGroups {
    env_tools: Option<String>,
    env_disable: Option<String>,
    path: PathBuf,
    stamp: Option<(SystemTime, u64)>,
    switches: Switches,
}

impl LiveGroups {
    fn start(root: &std::path::Path) -> Result<LiveGroups, String> {
        let mut live = LiveGroups {
            env_tools: std::env::var("TRAMMEL_TOOLS").ok(),
            env_disable: std::env::var("TRAMMEL_DISABLE").ok(),
            path: root.join("AGENTS.md"),
            stamp: None,
            switches: Switches::default(),
        };
        live.stamp = live.stamp_now();
        live.switches = Switches::resolve(live.env_tools.as_deref(), live.env_disable.as_deref(), &live.read())?;
        Ok(live)
    }

    /// Modification time and size: a cheap test for "the file changed".
    fn stamp_now(&self) -> Option<(SystemTime, u64)> {
        let meta = std::fs::metadata(&self.path).ok()?;
        Some((meta.modified().ok()?, meta.len()))
    }

    fn read(&self) -> groups::AgentLines {
        std::fs::read_to_string(&self.path).map(|text| groups::agent_lines(&text)).unwrap_or_default()
    }

    /// Re-read AGENTS.md if it changed. True when the listing changed.
    fn refresh(&mut self, surface_all: bool) -> bool {
        let stamp = self.stamp_now();
        if stamp == self.stamp { return false; }
        self.stamp = stamp;
        // The env vars were checked at startup, so only file lines can be
        // wrong here, and those become warnings.
        let Ok(next) = Switches::resolve(self.env_tools.as_deref(), self.env_disable.as_deref(), &self.read()) else { return false };
        let changed = next.listing(surface_all) != self.switches.listing(surface_all);
        self.switches = next;
        changed
    }
}

fn initialize(params: &Value, switches: &Switches) -> Value {
    let requested = crate::canon::get(params, "protocolVersion").and_then(crate::canon::as_str).unwrap_or("2025-06-18");
    let version = if PROTOCOLS.contains(&requested) { requested } else { "2025-11-25" };
    crate::canon::obj(vec![
        ("protocolVersion", Value::string(version)),
        ("capabilities", capabilities(switches)),
        ("serverInfo", server_info()),
        ("instructions", Value::string(match switches.instruction_lines() {
            // Group lines only when a switch or an agent-tools block is in use,
            // so the default reply stays as it was.
            Some(lines) => format!("{INSTRUCTIONS}\n{lines}"),
            None => INSTRUCTIONS.to_string(),
        })),
    ])
}

/// `tools.listChanged` is true once groups are in play (an env switch or an
/// agent-tools block), because edits to the block change the list live.
fn capabilities(switches: &Switches) -> Value {
    crate::canon::obj(vec![("tools", crate::canon::obj(vec![("listChanged", Value::Bool(switches.active()))]))])
}

fn notify(method: &str) {
    let body = crate::canon::obj(vec![("jsonrpc", Value::string("2.0")), ("method", Value::string(method))]);
    println!("{}", stable(&body));
    let _ = io::stdout().flush();
}

fn discover(surface: &str, switches: &Switches) -> Value {
    crate::canon::obj(vec![
        ("protocolVersion", Value::string("2026-07-28")),
        ("serverInfo", server_info()),
        ("capabilities", capabilities(switches)),
        ("tools", Value::Array(catalogue(surface, switches))),
    ])
}

fn server_info() -> Value {
    crate::canon::obj(vec![("name", Value::string("trammel")), ("version", Value::string(crate::VERSION))])
}

fn catalogue(surface: &str, switches: &Switches) -> Vec<Value> {
    tools::all_names()
        .into_iter()
        .filter(|name| switches.listed(name, surface == "all" || surface == "full" || tools::PRIMARY.contains(name)))
        .map(tool_schema)
        .collect()
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

fn tool_call(params: &Value, store: Option<&std::path::Path>, telemetry: bool, switches: &Switches) -> Value {
    let name = crate::canon::get(params, "name").and_then(crate::canon::as_str).unwrap_or("");
    let args = crate::canon::get(params, "arguments").cloned().unwrap_or(Value::Object(vec![]));
    if let Err(line) = switches.check_call(name) {
        return disabled(&line);
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| path_buf_from("."));
    match tools::call(name, &args, &cwd, store, telemetry) {
        Ok(mut value) => {
            if name == "status" {
                if let Some(groups) = switches.summary() { crate::canon::insert(&mut value, "tool_groups", Value::string(groups)); }
                if !switches.warnings().is_empty() {
                    crate::canon::insert(&mut value, "tool_group_warnings", Value::Array(switches.warnings().iter().cloned().map(Value::String).collect()));
                }
            }
            content(value, false)
        }
        Err(err) => content(err.value(), true),
    }
}

/// A switched-off tool: the text is the convention's one line; the
/// structured failure carries the same line.
fn disabled(line: &str) -> Value {
    let failure = Failure::new("tool_disabled", line, "");
    crate::canon::obj(vec![
        ("content", Value::Array(vec![crate::canon::obj(vec![("type", Value::string("text")), ("text", Value::string(line))])])),
        ("structuredContent", failure.value()),
        ("isError", Value::Bool(true)),
    ])
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
