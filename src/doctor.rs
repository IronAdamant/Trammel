//! Local health report. No network.

use crate::config;
use crate::json::Value;
use crate::lang::analyze::LANGUAGES;
use crate::root::{find_root, legacy_db, store_dir};
use std::path::Path;

pub fn report(start: &Path, store: Option<&Path>) -> Value {
    let root = find_root(start);
    let cfg = config::load(&root);
    let dir = store_dir(&root, store);
    let legacy = legacy_db(&root, store);
    let mut next = Vec::new();
    if legacy.is_some() && !dir.join("store").is_file() {
        next.push("run trammel migrate".to_string());
    }
    if cfg.language.is_none() && !root.join(".trammel.json").is_file() {
        next.push("optional: add .trammel.json to pin language and test_cmd".to_string());
    }
    if !config::telemetry_enabled(&cfg) {
        next.push("usage events are off; set TRAMMEL_TELEMETRY=1 or telemetry in .trammel.json".to_string());
    }
    crate::contract::ok(vec![
        ("version", Value::string(crate::VERSION)),
        ("schema_version", Value::string(crate::SCHEMA_VERSION)),
        ("project_root", Value::string(root.display().to_string())),
        ("store", Value::string(dir.display().to_string())),
        ("store_present", Value::Bool(dir.join("store").is_file())),
        ("legacy_sqlite", legacy.map(|p| Value::string(p.display().to_string())).unwrap_or(Value::Null)),
        ("languages", Value::Array(LANGUAGES.iter().copied().map(Value::string).collect())),
        ("protocols", Value::Array(vec![Value::string("2026-07-28"), Value::string("2025-11-25"), Value::string("2025-06-18")])),
        ("config_found", Value::Bool(root.join(".trammel.json").is_file() || root.join("pyproject.toml").is_file())),
        ("telemetry", Value::Bool(config::telemetry_enabled(&cfg))),
        ("next_steps", Value::Array(next.into_iter().map(Value::String).collect())),
    ])
}
