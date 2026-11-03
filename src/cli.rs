//! Hand-written argv parser. JSON is the default; `--human` prints text.

use crate::canon::stable;
use crate::contract::{self, Failure};
use crate::doctor;
use crate::json::{self, Value};
use crate::migrate;
use crate::root::{find_root, store_dir};
use crate::store::Store;
use crate::tools;
use std::env;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process::ExitCode;

pub fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => code,
        Err(err) => {
            if human_flag(&args) {
                eprintln!("{}", err.message);
            } else {
                eprintln!("{}", stable(&err.value()));
            }
            err.exit_code()
        }
    }
}

fn run(args: &[String]) -> Result<ExitCode, Failure> {
    if args.iter().any(|a| a == "--version" || a == "-V") {
        emit(&contract::ok(vec![("name", Value::string("trammel")), ("version", Value::string(crate::VERSION))]), human_flag(args));
        return Ok(ExitCode::SUCCESS);
    }
    if args.iter().any(|a| a == "--help" || a == "-h") || (args.is_empty() && io::IsTerminal::is_terminal(&io::stdin())) {
        emit(&help_value(), true);
        return if args.is_empty() { Err(Failure::usage("a goal or a subcommand is required")) } else { Ok(ExitCode::SUCCESS) };
    }
    let human = human_flag(args);
    let store = flag_value(args, "--store").or_else(|| env::var("TRAMMEL_DB_PATH").ok()).map(PathBuf::from);
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let telemetry = env::var("TRAMMEL_TELEMETRY").ok().as_deref() == Some("1");
    let command = args.iter().find(|a| !a.starts_with('-')).cloned();
    match command.as_deref() {
        Some("doctor") => { emit(&doctor::report(&cwd, store.as_deref()), human); Ok(ExitCode::SUCCESS) }
        Some("migrate") => migrate_cmd(args, &cwd, store.as_deref(), human),
        Some("dump") => dump_cmd(&cwd, store.as_deref(), human),
        Some("mcp") => { crate::mcp::serve(store).map_err(|e| Failure::new("mcp", e, ""))?; Ok(ExitCode::SUCCESS) }
        Some("run") => {
            let goal = positional_goal(args).unwrap_or_default();
            run_goal(&goal, args, &cwd, store.as_deref(), human)
        }
        Some(tool) if tools::all_names().contains(&tool) || tool == "record_step" || tool == "create_plan" => {
            let value = object_args(args);
            let result = tools::call(tool, &value, &cwd, store.as_deref(), telemetry)?;
            emit(&contract::ok(vec![("tool", Value::string(tool)), ("result", result)]), human);
            Ok(ExitCode::SUCCESS)
        }
        Some(goal) => run_goal(goal, args, &cwd, store.as_deref(), human),
        None => {
            let goal = read_stdin_goal()?;
            run_goal(&goal, args, &cwd, store.as_deref(), human)
        }
    }
}

fn run_goal(goal: &str, args: &[String], cwd: &std::path::Path, store: Option<&std::path::Path>, human: bool) -> Result<ExitCode, Failure> {
    if goal.is_empty() { return Err(Failure::usage("a goal is required")); }
    let root = flag_value(args, "--root").map(PathBuf::from).unwrap_or_else(|| cwd.to_path_buf());
    let mut fields = vec![
        ("goal".to_string(), Value::string(goal)),
        ("project_root".to_string(), Value::string(root.display().to_string())),
    ];
    if let Some(scope) = flag_value(args, "--scope") { fields.push(("scope".into(), Value::string(scope))); }
    if let Some(language) = flag_value(args, "--language") { fields.push(("language".into(), Value::string(language))); }
    if let Some(beams) = flag_value(args, "--beams") { fields.push(("num_beams".into(), Value::Int(beams.parse().unwrap_or(3)))); }
    if let Some(test_cmd) = flag_words(args, "--test-cmd") {
        fields.push(("test_cmd".into(), Value::Array(test_cmd.into_iter().map(Value::String).collect())));
    }
    let dry = args.iter().any(|a| a == "--dry-run");
    let tool = if dry { "explore" } else { "run" };
    let result = if dry {
        tools::call(tool, &Value::Object(fields), cwd, store, false)?
    } else {
        crate::planner::plan_and_execute(&fields_request(&fields), store, flag_words(args, "--test-cmd").as_deref()).map_err(|e| Failure::new("run", e, "Check the test command"))?
    };
    if let Some(path) = flag_value(args, "--export") {
        std::fs::write(path, crate::canon::stable_pretty(&result)).map_err(|e| Failure::new("export", e.to_string(), ""))?;
    }
    emit(&contract::ok(vec![("result", result)]), human);
    Ok(ExitCode::SUCCESS)
}

fn migrate_cmd(args: &[String], cwd: &std::path::Path, store: Option<&std::path::Path>, human: bool) -> Result<ExitCode, Failure> {
    let from = flag_value(args, "--from").map(PathBuf::from).unwrap_or_else(|| find_root(cwd).join("trammel.db"));
    let doc = migrate::import_database(&from).map_err(|e| Failure::new("migrate", e, "The file must be a SQLite database, WAL included"))?;
    let root = find_root(cwd);
    let dir = store_dir(&root, store);
    let mut opened = Store::open(&dir).map_err(|e| Failure::new("store", e, ""))?;
    let recipes = migrate::row_count(&doc, "recipes");
    let plans = migrate::row_count(&doc, "plans");
    opened.replace_document(doc);
    opened.save().map_err(|e| Failure::new("store", e, ""))?;
    emit(&contract::ok(vec![
        ("migrated_from", Value::string(from.display().to_string())),
        ("store", Value::string(dir.display().to_string())),
        ("recipes", Value::Int(recipes as i64)),
        ("plans", Value::Int(plans as i64)),
        ("source_untouched", Value::Bool(true)),
    ]), human);
    Ok(ExitCode::SUCCESS)
}

fn dump_cmd(cwd: &std::path::Path, store: Option<&std::path::Path>, human: bool) -> Result<ExitCode, Failure> {
    let dir = store_dir(&find_root(cwd), store);
    let opened = Store::open_shared(&dir).map_err(|e| Failure::new("store", e, ""))?;
    let doc = opened.document().clone();
    if human {
        println!("{}", crate::canon::stable_pretty(&doc));
    } else if let Value::Object(pairs) = doc {
        for (table, rows) in pairs {
            if let Value::Array(rows) = rows {
                for row in rows {
                    println!("{}", stable(&crate::canon::obj(vec![("table", Value::string(table.clone())), ("row", row)])));
                }
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn help_value() -> Value {
    contract::ok(vec![
        ("usage", Value::string("trammel GOAL | trammel <tool> | trammel doctor | trammel migrate | trammel mcp")),
        ("tools", Value::Array(tools::all_names().into_iter().map(Value::string).collect())),
    ])
}

fn emit(value: &Value, human: bool) {
    if human {
        println!("{}", crate::canon::stable_pretty(value));
    } else {
        println!("{}", stable(value));
    }
}

fn human_flag(args: &[String]) -> bool { args.iter().any(|a| a == "--human") }

fn flag_value(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn flag_words(args: &[String], name: &str) -> Option<Vec<String>> {
    let start = args.iter().position(|a| a == name)? + 1;
    let mut words = Vec::new();
    for arg in &args[start..] {
        if arg.starts_with('-') { break; }
        words.push(arg.clone());
    }
    if words.is_empty() { None } else { Some(words) }
}

fn fields_request(fields: &[(String, Value)]) -> crate::planner::Request {
    let mut goal = String::new();
    let mut root = std::path::PathBuf::from(".");
    let mut request = crate::planner::Request::new("", "");
    for (key, value) in fields {
        match key.as_str() {
            "goal" => goal = crate::canon::as_str(value).unwrap_or("").to_string(),
            "project_root" => root = std::path::PathBuf::from(crate::canon::as_str(value).unwrap_or(".")),
            "scope" => request.scope = crate::canon::as_str(value).map(|s| s.to_string()),
            "language" => request.language = crate::canon::as_str(value).map(|s| s.to_string()),
            "num_beams" => request.num_beams = crate::canon::as_i64(value).unwrap_or(3) as usize,
            _ => {}
        }
    }
    request.goal = goal;
    request.root = root;
    request
}

fn positional_goal(args: &[String]) -> Option<String> {
    args.iter().filter(|a| !a.starts_with('-')).nth(1).cloned()
}

fn object_args(args: &[String]) -> Value {
    let mut pairs = Vec::new();
    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        if let Some(key) = arg.strip_prefix("--") {
            let key = key.replace('-', "_");
            if i + 1 < args.len() && !args[i + 1].starts_with('-') {
                let raw = &args[i + 1];
                let value = if let Ok(n) = raw.parse::<i64>() { Value::Int(n) } else if raw == "true" { Value::Bool(true) } else if raw == "false" { Value::Bool(false) } else if let Ok(parsed) = json::parse(raw) { parsed } else { Value::string(raw.clone()) };
                pairs.push((key, value));
                i += 2;
                continue;
            }
            pairs.push((key, Value::Bool(true)));
        }
        i += 1;
    }
    Value::Object(pairs)
}

fn read_stdin_goal() -> Result<String, Failure> {
    let mut buf = String::new();
    io::stdin().read_to_string(&mut buf).map_err(|e| Failure::new("stdin", e.to_string(), ""))?;
    if let Ok(value) = json::parse(buf.trim()) {
        if let Some(goal) = crate::canon::get(&value, "goal").and_then(crate::canon::as_str) { return Ok(goal.to_string()); }
    }
    Ok(buf.trim().to_string())
}
