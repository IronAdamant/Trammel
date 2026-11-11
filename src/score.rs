//! Step relevance and step generation. Formulas match `scoring.py`.

use crate::canon::py_round;
use crate::goal::{extract_goal_keywords, matched_keywords};
use crate::json::Value;
use std::collections::{BTreeMap, BTreeSet};

const ALPHA: f64 = 0.7;

fn words_of(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let lower = text.to_lowercase();
    let mut current = String::new();
    for ch in lower.chars() {
        if ch.is_ascii_lowercase() {
            current.push(ch);
        } else if !current.is_empty() {
            out.insert(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        out.insert(current);
    }
    out
}

fn camel_parts(text: &str) -> BTreeSet<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = BTreeSet::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_uppercase() {
            let start = i;
            i += 1;
            if i < chars.len() && chars[i].is_ascii_lowercase() {
                while i < chars.len() && chars[i].is_ascii_lowercase() {
                    i += 1;
                }
            } else {
                while i < chars.len()
                    && chars[i].is_ascii_uppercase()
                    && !(i + 1 < chars.len() && chars[i + 1].is_ascii_lowercase())
                {
                    i += 1;
                }
            }
            let part: String = chars[start..i].iter().collect::<String>().to_lowercase();
            if part.len() > 2 {
                out.insert(part);
            }
        } else if chars[i].is_ascii_lowercase() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_lowercase() {
                i += 1;
            }
            let part: String = chars[start..i].iter().collect();
            if part.len() > 2 {
                out.insert(part);
            }
        } else {
            i += 1;
        }
    }
    out
}

pub fn dep_indegree(graph: &BTreeMap<String, Vec<String>>) -> (BTreeMap<String, i32>, i32) {
    let mut indeg = BTreeMap::new();
    for deps in graph.values() {
        for dep in deps {
            *indeg.entry(dep.clone()).or_insert(0) += 1;
        }
    }
    let max = indeg.values().copied().max().unwrap_or(0);
    (indeg, max)
}

/// `(keyword score, graph score, composite)` for one file.
///
/// `anchors` are the directories the goal names (a crate, package, or folder
/// whose name is a goal word). When there are any, being inside one counts
/// for 30% and import count only for 10%, so the code the goal is about
/// beats shared helpers that everything imports.
pub fn relevance_components(
    filepath: &str,
    symbols: &[String],
    keywords: &BTreeSet<String>,
    indegree: &BTreeMap<String, i32>,
    max_indegree: i32,
    anchors: &[String],
) -> (f64, f64, f64) {
    if keywords.is_empty() {
        return (1.0, 0.0, 1.0);
    }
    let mut path_words = words_of(filepath);
    path_words.extend(camel_parts(filepath));
    let mut sym_words = BTreeSet::new();
    for symbol in symbols {
        for part in words_of(symbol) {
            if part.len() > 2 {
                sym_words.insert(part);
            }
        }
        for part in camel_parts(symbol) {
            sym_words.insert(part);
        }
    }
    let mut candidates = path_words;
    candidates.extend(sym_words);
    let matched = matched_keywords(keywords, &candidates);
    let keyword_ratio = matched.len() as f64 / keywords.len().max(1) as f64;
    let mut partial: f64 = 0.0;
    for kw in keywords {
        if !matched.contains(kw) {
            let kw_lc = kw.to_lowercase();
            // A shared prefix of four letters or more (prop / props /
            // property). A substring anywhere ("per" in "upper") is noise.
            if kw_lc.len() >= 4
                && candidates.iter().any(|cw| cw.len() >= 4 && (cw.starts_with(&kw_lc) || kw_lc.starts_with(cw.as_str())))
            {
                partial += 0.5;
            }
        }
    }
    partial = partial.min(0.3);
    let keyword_score = (keyword_ratio + partial).min(1.0);
    let graph_boost = if max_indegree <= 0 {
        0.0
    } else {
        let raw = *indegree.get(filepath).unwrap_or(&0) as f64;
        (1.0 + raw).ln() / (1.0 + max_indegree as f64).ln()
    };
    let composite = if anchors.is_empty() {
        ALPHA * keyword_score + (1.0 - ALPHA) * graph_boost
    } else {
        let near = if anchors.iter().any(|a| filepath.starts_with(&format!("{a}/"))) { 1.0 } else { 0.0 };
        0.6 * keyword_score + 0.1 * graph_boost + 0.3 * near
    };
    (keyword_score, graph_boost, composite)
}

pub fn relevance_tier(normalized: f64) -> &'static str {
    if normalized >= 0.66 {
        "high"
    } else if normalized >= 0.33 {
        "medium"
    } else {
        "low"
    }
}

pub fn filter_globs(paths: &BTreeSet<String>, globs: &[String]) -> BTreeSet<String> {
    if globs.is_empty() {
        return paths.clone();
    }
    let mut out = BTreeSet::new();
    for path in paths {
        let norm = path.replace('\\', "/");
        let base = norm.rsplit('/').next().unwrap_or(&norm);
        for glob in globs {
            let g = glob.replace('\\', "/");
            if glob_match(&norm, &g) || glob_match(base, &g) {
                out.insert(path.clone());
                break;
            }
        }
    }
    out
}

fn glob_match(text: &str, pattern: &str) -> bool {
    fn rec(text: &[u8], pat: &[u8]) -> bool {
        if pat.is_empty() {
            return text.is_empty();
        }
        if pat[0] == b'*' {
            if rec(text, &pat[1..]) {
                return true;
            }
            return !text.is_empty() && rec(&text[1..], pat);
        }
        if pat[0] == b'?' {
            return !text.is_empty() && rec(&text[1..], &pat[1..]);
        }
        !text.is_empty() && text[0] == pat[0] && rec(&text[1..], &pat[1..])
    }
    rec(text.as_bytes(), pattern.as_bytes())
}

fn step_description(filepath: &str, symbols: &[String], keywords: &BTreeSet<String>) -> String {
    let mut summary = symbols.iter().take(5).cloned().collect::<Vec<_>>().join(", ");
    if symbols.len() > 5 {
        summary.push_str(&format!(" (+{} more)", symbols.len() - 5));
    }
    if !keywords.is_empty() {
        let mut candidates = words_of(filepath);
        for symbol in symbols {
            candidates.extend(words_of(symbol).into_iter().filter(|p| p.len() > 2));
        }
        let relevant = matched_keywords(keywords, &candidates);
        if !relevant.is_empty() {
            let context = relevant.into_iter().collect::<Vec<_>>().join(", ");
            return format!("Modify {filepath} for {context}: {summary}");
        }
    }
    format!("Modify {filepath}: {summary}")
}

fn step_rationale(deps: &[String], symbols: &[String], relevance: Option<f64>) -> String {
    let mut parts = Vec::new();
    if !deps.is_empty() {
        let mut summary = deps.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
        if deps.len() > 3 {
            summary.push_str(&format!(" (+{} more)", deps.len() - 3));
        }
        parts.push(format!("depends on {summary}"));
    }
    parts.push(format!("contains {} symbol(s)", symbols.len()));
    if let Some(relevance) = relevance {
        parts.push(format!("goal relevance: {:.0}%", relevance * 100.0));
    }
    parts.join("; ")
}

pub fn generate_steps(
    file_order: &[String],
    symbols: &BTreeMap<String, Vec<String>>,
    graph: &BTreeMap<String, Vec<String>>,
    goal: &str,
    keywords: &BTreeSet<String>,
    anchors: &[String],
    roles: &BTreeMap<String, &'static str>,
) -> Vec<Value> {
    let _ = extract_goal_keywords;
    let empty: Vec<String> = Vec::new();
    let (indeg, max_in) = dep_indegree(graph);
    let mut steps = Vec::new();
    let mut file_to_step = BTreeMap::new();
    for filepath in file_order {
        let role = roles.get(filepath).copied();
        // A file the goal names gets a step even when it defines nothing.
        let named = role.map(|r| r.starts_with("named")).unwrap_or(false);
        let names = match symbols.get(filepath) {
            Some(names) if !names.is_empty() => names,
            _ if named => &empty,
            _ => continue,
        };
        let comps = if keywords.is_empty() {
            None
        } else {
            Some(relevance_components(filepath, names, keywords, &indeg, max_in, anchors))
        };
        let step_idx = steps.len() as i64;
        file_to_step.insert(filepath.clone(), step_idx);
        let dep_files = graph.get(filepath).cloned().unwrap_or_default();
        let depends_on: Vec<Value> = dep_files
            .iter()
            .filter_map(|d| file_to_step.get(d).copied())
            .map(Value::Int)
            .collect();
        let mut step = crate::canon::obj(vec![
            ("step_index", Value::Int(step_idx)),
            ("file", Value::string(filepath.clone())),
            (
                "symbols",
                Value::Array(names.iter().cloned().map(Value::String).collect()),
            ),
            ("symbol_count", Value::Int(names.len() as i64)),
            ("description", Value::string(step_description(filepath, names, keywords))),
            (
                "rationale",
                Value::string(step_rationale(&dep_files, names, comps.map(|c| c.2))),
            ),
            ("depends_on", Value::Array(depends_on)),
        ]);
        if let Some(role) = role { crate::canon::insert(&mut step, "role", Value::string(role)); }
        if let Some((kw, gr, comp)) = comps {
            crate::canon::insert(&mut step, "relevance_keyword", Value::Float(py_round(kw, 3)));
            crate::canon::insert(&mut step, "relevance_graph", Value::Float(py_round(gr, 3)));
            crate::canon::insert(&mut step, "relevance", Value::Float(py_round(comp, 3)));
        }
        steps.push(step);
    }
    if !keywords.is_empty() {
        let rels: Vec<f64> = steps.iter().filter_map(|s| crate::canon::as_f64(crate::canon::get(s, "relevance")?)).collect();
        if rels.len() >= 2 {
            let lo = rels.iter().cloned().fold(f64::INFINITY, f64::min);
            let hi = rels.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            if hi > lo {
                for step in &mut steps {
                    if let Some(r) = crate::canon::get(step, "relevance").and_then(crate::canon::as_f64) {
                        crate::canon::insert(step, "relevance", Value::Float(py_round((r - lo) / (hi - lo), 3)));
                    }
                }
            }
        }
        for step in &mut steps {
            if let Some(r) = crate::canon::get(step, "relevance").and_then(crate::canon::as_f64) {
                crate::canon::insert(step, "relevance_tier", Value::string(relevance_tier(r)));
            }
        }
    }
    if steps.is_empty() {
        steps.push(crate::canon::obj(vec![
            ("step_index", Value::Int(0)),
            ("file", Value::string("__project__")),
            ("symbols", Value::Array(vec![Value::string("root")])),
            ("symbol_count", Value::Int(1)),
            ("description", Value::string(goal.chars().take(100).collect::<String>())),
            ("rationale", Value::string("No symbols found; treating as whole-project task")),
            ("depends_on", Value::Array(vec![])),
        ]));
    }
    steps
}

/// Set a step's relevance and its tier together.
pub fn set_relevance(step: &mut Value, relevance: f64) {
    crate::canon::insert(step, "relevance", Value::Float(py_round(relevance, 3)));
    crate::canon::insert(step, "relevance_tier", Value::string(relevance_tier(relevance)));
}

/// A test file by path: under a `tests`, `test`, `__tests__` or `spec`
/// directory, or named `test_*`, `*_test`, `*_tests`, `*.test.*`, `*.spec.*`,
/// `*_spec`, `tests.rs` or `conftest.py`. Inline `#[cfg(test)]` modules do
/// not make a file a test file.
pub fn is_test_file(path: &str) -> bool {
    let norm = path.replace('\\', "/");
    let mut parts: Vec<&str> = norm.split('/').collect();
    let name = parts.pop().unwrap_or("");
    if parts.iter().any(|d| matches!(*d, "tests" | "test" | "__tests__" | "spec" | "specs")) { return true; }
    let stem = name.split('.').next().unwrap_or(name);
    let dotted = name.contains(".test.") || name.contains(".spec.");
    dotted || stem.starts_with("test_") || stem.ends_with("_test") || stem.ends_with("_tests") || stem.ends_with("_spec") || stem == "tests" || name == "conftest.py"
}

/// The source file stem a test file is about: `test_models.py` and
/// `models_test.go` give `models`, `tests/sight.rs` gives `sight`.
pub fn tested_stem(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    let stem = name.split('.').next().unwrap_or(name);
    let stem = stem.strip_prefix("test_").unwrap_or(stem);
    let stem = ["_tests", "_test", "_spec"].iter().find_map(|s| stem.strip_suffix(s)).unwrap_or(stem);
    stem.to_string()
}

#[cfg(test)]
mod test_file_tests {
    use super::*;

    #[test]
    fn test_files_are_told_apart_from_code() {
        for path in ["tests/test_file_1.py", "crates/app/tests/game.rs", "src/models_test.go", "web/app.test.ts", "web/app.spec.js", "src/ui/engine_tests.rs", "pkg/conftest.py", "src/tests.rs"] {
            assert!(is_test_file(path), "{path}");
        }
        for path in ["src/models.py", "crates/specula/src/sight.rs", "src/latest.rs", "src/contest.py", "src/testing_tools.rs"] {
            assert!(!is_test_file(path), "{path}");
        }
        assert_eq!(tested_stem("tests/test_models.py"), "models");
        assert_eq!(tested_stem("src/models_test.go"), "models");
        assert_eq!(tested_stem("crates/app/tests/sight.rs"), "sight");
        assert_eq!(tested_stem("web/app.test.ts"), "app");
    }
}
