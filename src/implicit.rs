//! Implicit edges: naming conventions, shared file access, and co-occurrence.
//! Rules are the embedded `patterns.json` plus an optional override file.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const PATTERNS: &str = include_str!("../data/patterns.json");

pub struct Engine {
    rules: Vec<(String, Vec<(String, f64)>)>,
    infra: Vec<(String, f64)>,
    access: BTreeMap<String, BTreeSet<String>>,
    by_module: BTreeMap<String, BTreeSet<String>>,
    co: BTreeMap<String, BTreeMap<String, i32>>,
    direct: BTreeMap<String, BTreeSet<String>>,
}

impl Engine {
    pub fn new() -> Engine {
        Engine::from_text(PATTERNS)
    }

    pub fn from_text(text: &str) -> Engine {
        let mut rules = Vec::new();
        let mut infra = Vec::new();
        if let Ok(value) = crate::json::parse(text) {
            if let Some(obj) = crate::canon::get(&value, "naming_convention_rules").and_then(|v| match v { crate::json::Value::Object(p) => Some(p), _ => None }) {
                for (suffix, targets) in obj {
                    let mut pairs = Vec::new();
                    if let crate::json::Value::Array(items) = targets {
                        for item in items {
                            if let crate::json::Value::Array(pair) = item {
                                if let (Some(name), Some(conf)) = (pair.first().and_then(crate::canon::as_str), pair.get(1).and_then(crate::canon::as_f64)) {
                                    pairs.push((name.to_string(), conf));
                                }
                            }
                        }
                    }
                    rules.push((suffix.clone(), pairs));
                }
            }
            if let Some(crate::json::Value::Array(items)) = crate::canon::get(&value, "infrastructure_patterns") {
                for item in items {
                    if let crate::json::Value::Array(pair) = item {
                        if let (Some(name), Some(conf)) = (pair.first().and_then(crate::canon::as_str), pair.get(1).and_then(crate::canon::as_f64)) {
                            infra.push((name.to_string(), conf));
                        }
                    }
                }
            }
        }
        Engine { rules, infra, access: BTreeMap::new(), by_module: BTreeMap::new(), co: BTreeMap::new(), direct: BTreeMap::new() }
    }

    pub fn analyze(&mut self, root: &Path, files: &BTreeSet<String>, graph: &BTreeMap<String, Vec<String>>) {
        self.learn(graph);
        self.scan_access(root, files);
    }

    fn learn(&mut self, graph: &BTreeMap<String, Vec<String>>) {
        for (module, deps) in graph {
            if deps.is_empty() { continue; }
            self.direct.insert(module.clone(), deps.iter().cloned().collect());
            for (i, a) in deps.iter().enumerate() {
                for b in deps.iter().skip(i + 1) {
                    *self.co.entry(a.clone()).or_default().entry(b.clone()).or_insert(0) += 1;
                    *self.co.entry(b.clone()).or_default().entry(a.clone()).or_insert(0) += 1;
                }
            }
        }
    }

    fn scan_access(&mut self, root: &Path, files: &BTreeSet<String>) {
        for file in files {
            let Some(text) = crate::walk::read_lossy(&root.join(file)) else { continue };
            for path in file_access_paths(&text) {
                let ext = path.rsplit('.').next().unwrap_or("");
                if [".js", ".ts", ".py", ".go", ".rs", ".java", ".kt", ".rb", ".php", ".cs", ".cpp", ".c", ".h"].iter().any(|e| path.ends_with(e)) {
                    let _ = ext;
                    continue;
                }
                if path.contains("node_modules") || path.contains(".git") || path.contains("venv") { continue; }
                self.access.entry(path.clone()).or_default().insert(file.clone());
                self.by_module.entry(file.clone()).or_default().insert(path);
            }
        }
    }

    pub fn hybrid(&self, explicit: &BTreeMap<String, Vec<String>>, files: &BTreeSet<String>) -> (BTreeMap<String, Vec<String>>, usize, usize, usize) {
        let mut hybrid = explicit.clone();
        let mut naming = 0usize;
        let mut shared = 0usize;
        let mut pattern = 0usize;
        let sources: Vec<String> = hybrid.keys().cloned().collect();
        for source in sources {
            let mut seen: BTreeSet<String> = hybrid.get(&source).cloned().unwrap_or_default().into_iter().collect();
            for (target, kind) in self.implicit_of(&source, files) {
                if seen.insert(target.clone()) {
                    hybrid.entry(source.clone()).or_default().push(target);
                    match kind {
                        "naming_convention" => naming += 1,
                        "shared_state" => shared += 1,
                        _ => pattern += 1,
                    }
                }
            }
        }
        (hybrid, naming, shared, pattern)
    }

    fn implicit_of(&self, source: &str, files: &BTreeSet<String>) -> Vec<(String, &'static str)> {
        let mut ranked: Vec<(String, &'static str, f64)> = Vec::new();
        if let Some((base, suffix)) = split_name(source) {
            if let Some((_, targets)) = self.rules.iter().find(|(name, _)| name == &suffix) {
                for (target_suffix, confidence) in targets {
                    for pattern in [
                        format!("{base}{target_suffix}"),
                        format!("{}_{}", base.to_lowercase(), target_suffix.to_lowercase()),
                        format!("{}-{}", base.to_lowercase(), target_suffix.to_lowercase()),
                    ] {
                        for ext in [".js", ".ts", ".tsx", ".jsx", ".py", ".go", ".rs"] {
                            let candidate = format!("{pattern}{ext}");
                            if files.contains(&candidate) {
                                ranked.push((candidate, "naming_convention", *confidence));
                                break;
                            }
                        }
                    }
                }
            }
        }
        for (pattern, confidence) in &self.infra {
            if source.to_lowercase().contains(&pattern.to_lowercase()) {
                for ext in [".js", ".ts", ".py"] {
                    let candidate = format!("{pattern}{ext}");
                    if files.contains(&candidate) {
                        ranked.push((candidate, "naming_convention", *confidence));
                        break;
                    }
                }
            }
        }
        if let Some(paths) = self.by_module.get(source) {
            for data in paths {
                if let Some(modules) = self.access.get(data) {
                    for other in modules {
                        if other != source && files.contains(other) {
                            ranked.push((other.clone(), "shared_state", 0.75));
                        }
                    }
                }
            }
        }
        if let Some(direct) = self.direct.get(source) {
            for dep in direct {
                if let Some(co) = self.co.get(dep) {
                    for (other, count) in co {
                        if *count >= 2 && !direct.contains(other) && files.contains(other) {
                            let confidence = (0.5 + (*count as f64) * 0.1).min(0.9);
                            ranked.push((other.clone(), "pattern_based", confidence));
                        }
                    }
                }
            }
        }
        ranked.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
        ranked.into_iter().map(|(t, k, _)| (t, k)).collect()
    }
}

fn strip_ext(path: &str) -> String {
    for ext in [".js", ".ts", ".tsx", ".jsx", ".py", ".go", ".rs", ".java", ".kt", ".rb", ".php", ".cs", ".cpp", ".c", ".h"] {
        if let Some(stripped) = path.strip_suffix(ext) { return stripped.to_string(); }
    }
    path.to_string()
}

fn camel_parts(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_uppercase() {
            let start = i;
            i += 1;
            if i < chars.len() && chars[i].is_ascii_lowercase() {
                while i < chars.len() && chars[i].is_ascii_lowercase() { i += 1; }
            } else {
                while i < chars.len() && chars[i].is_ascii_uppercase() && !(i + 1 < chars.len() && chars[i + 1].is_ascii_lowercase()) {
                    i += 1;
                }
            }
            out.push(chars[start..i].iter().collect());
        } else {
            i += 1;
        }
    }
    out
}

fn split_name(path: &str) -> Option<(String, String)> {
    let stem = strip_ext(path);
    let parts = camel_parts(&stem);
    let suffix = parts.last()?.clone();
    let base = if stem.contains('_') || stem.contains('-') {
        let pieces: Vec<&str> = stem.split(['_', '-']).collect();
        pieces.iter().find(|p| p.chars().next().is_some_and(|c| c.is_ascii_uppercase())).map(|s| (*s).to_string()).unwrap_or_else(|| {
            if parts.len() > 1 { parts[0].clone() } else { stem.clone() }
        })
    } else if parts.len() > 1 {
        parts[0].clone()
    } else {
        stem.clone()
    };
    if suffix.is_empty() { None } else { Some((base, suffix)) }
}

fn quoted(text: &str) -> Option<&str> {
    let text = text.trim_start();
    let quote = text.chars().next()?;
    if quote != '"' && quote != '\'' { return None; }
    let body = &text[1..];
    Some(&body[..body.find(quote)?])
}

fn file_access_paths(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for marker in ["readFileSync", "readFile", "writeFileSync", "writeFile", "readdirSync", "readdir", "open", "Path"] {
        let mut rest = text;
        while let Some(at) = rest.find(marker) {
            let after = rest[at + marker.len()..].trim_start();
            if after.starts_with('(') {
                let inner = after[1..].trim_start();
                if let Some(path) = quoted(inner) {
                    let path = path.trim_start_matches("./");
                    if !path.starts_with('/') { out.push(path.to_string()); }
                }
            }
            rest = &rest[at + marker.len()..];
        }
    }
    out
}

pub fn load_override(root: &Path) -> Engine {
    let path = root.join(".trammel").join("patterns.json");
    if let Some(text) = crate::walk::read_lossy(&path) {
        return Engine::from_text(&text);
    }
    Engine::new()
}
