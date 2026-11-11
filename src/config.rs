//! `.trammel.json` and a std-only reader for `[tool.trammel]` in pyproject.toml.
//! Test command precedence: explicit, then project, then the analyzer default.

use crate::json::{self, Value};
use crate::walk::read_lossy;
use std::path::Path;

#[derive(Clone, Debug, Default)]
pub struct ProjectConfig {
    pub language: Option<String>,
    pub default_scope: Option<String>,
    pub focus_keywords: Vec<String>,
    pub focus_globs: Vec<String>,
    pub max_files: Option<i64>,
    pub test_cmd: Option<Vec<String>>,
    pub telemetry: bool,
    /// Seconds verify_step may run when the call does not say (`timeout_s`).
    pub verify_timeout_s: Option<u64>,
}

pub fn load(project: &Path) -> ProjectConfig {
    if let Some(cfg) = load_json(&project.join(".trammel.json")) {
        return cfg;
    }
    load_pyproject(&project.join("pyproject.toml")).unwrap_or_default()
}

pub fn telemetry_enabled(cfg: &ProjectConfig) -> bool {
    if std::env::var("TRAMMEL_TELEMETRY").ok().as_deref() == Some("1") {
        return true;
    }
    cfg.telemetry
}

fn load_json(path: &Path) -> Option<ProjectConfig> {
    let text = read_lossy(path)?;
    let value = json::parse(&text).ok()?;
    Some(from_value(&value))
}

fn from_value(value: &Value) -> ProjectConfig {
    ProjectConfig {
        language: string_field(value, "language"),
        default_scope: string_field(value, "default_scope"),
        focus_keywords: string_list(value, "focus_keywords"),
        focus_globs: string_list(value, "focus_globs"),
        max_files: crate::canon::get(value, "max_files").and_then(crate::canon::as_i64),
        test_cmd: cmd_field(value, "test_cmd"),
        telemetry: crate::canon::get(value, "telemetry").and_then(crate::canon::as_bool).unwrap_or(false),
        verify_timeout_s: crate::canon::get(value, "verify_timeout_s").and_then(crate::canon::as_f64).filter(|s| *s >= 1.0).map(|s| s.round() as u64),
    }
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    crate::canon::get(value, key).and_then(crate::canon::as_str).map(|s| s.to_string())
}

fn string_list(value: &Value, key: &str) -> Vec<String> {
    crate::canon::get(value, key)
        .and_then(crate::canon::as_array)
        .map(|items| items.iter().filter_map(crate::canon::as_str).map(|s| s.to_string()).collect())
        .unwrap_or_default()
}

fn cmd_field(value: &Value, key: &str) -> Option<Vec<String>> {
    let item = crate::canon::get(value, key)?;
    match item {
        Value::String(text) => Some(split_cmd(text)),
        Value::Array(items) => Some(items.iter().filter_map(crate::canon::as_str).map(|s| s.to_string()).collect()),
        _ => None,
    }
}

pub fn split_cmd(text: &str) -> Vec<String> {
    text.split_whitespace().map(|s| s.to_string()).collect()
}

/// Reads only the `[tool.trammel]` table. Other TOML is ignored.
fn load_pyproject(path: &Path) -> Option<ProjectConfig> {
    let text = read_lossy(path)?;
    let body = tool_trammel_body(&text)?;
    let mut cfg = ProjectConfig::default();
    for line in body.lines() {
        let line = strip_comment(line).trim();
        if line.is_empty() || line.starts_with('[') {
            if line.starts_with('[') {
                break;
            }
            continue;
        }
        let Some((key, raw)) = line.split_once('=') else { continue };
        let key = key.trim();
        let raw = raw.trim();
        match key {
            "language" => cfg.language = parse_string(raw),
            "default_scope" => cfg.default_scope = parse_string(raw),
            "max_files" => cfg.max_files = raw.parse().ok(),
            "telemetry" => cfg.telemetry = raw == "true",
            "verify_timeout_s" => cfg.verify_timeout_s = raw.parse::<f64>().ok().filter(|s| *s >= 1.0).map(|s| s.round() as u64),
            "focus_keywords" => cfg.focus_keywords = parse_string_array(raw),
            "focus_globs" => cfg.focus_globs = parse_string_array(raw),
            "test_cmd" => {
                cfg.test_cmd = if raw.starts_with('[') {
                    Some(parse_string_array(raw))
                } else {
                    parse_string(raw).map(|s| split_cmd(&s))
                };
            }
            _ => {}
        }
    }
    Some(cfg)
}

fn tool_trammel_body(text: &str) -> Option<&str> {
    let marker = "[tool.trammel]";
    let start = text.find(marker)? + marker.len();
    let rest = &text[start..];
    let end = rest.find("\n[").map(|i| i).unwrap_or(rest.len());
    Some(&rest[..end])
}

fn strip_comment(line: &str) -> &str {
    let mut quote = None;
    for (i, ch) in line.char_indices() {
        match (quote, ch) {
            (None, '"' | '\'') => quote = Some(ch),
            (Some(q), c) if c == q => quote = None,
            (None, '#') => return &line[..i],
            _ => {}
        }
    }
    line
}

fn parse_string(raw: &str) -> Option<String> {
    let raw = raw.trim().trim_end_matches(',');
    let quote = raw.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let rest = &raw[1..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
}

fn parse_string_array(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    for ch in raw.chars() {
        match (quote, ch) {
            (None, '"' | '\'') => quote = Some(ch),
            (Some(q), c) if c == q => {
                out.push(std::mem::take(&mut current));
                quote = None;
            }
            (Some(_), c) => current.push(c),
            _ => {}
        }
    }
    out
}

pub fn merge_keywords(goal: std::collections::BTreeSet<String>, extra: &[String], cfg: &ProjectConfig) -> std::collections::BTreeSet<String> {
    let mut out = goal;
    for word in extra.iter().chain(cfg.focus_keywords.iter()) {
        if word.len() > 2 {
            out.insert(word.to_lowercase());
        }
    }
    out
}

