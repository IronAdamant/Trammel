//! Goal similarity: trigram cosine, word Jaccard, substring score.
//! Weights and synonym tables match `text_similarity.py`.

use std::collections::{BTreeMap, BTreeSet};

pub fn trigram_list(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() < 3 {
        return Vec::new();
    }
    (0..chars.len() - 2)
        .map(|i| chars[i..i + 3].iter().collect::<String>().to_lowercase())
        .collect()
}

pub fn trigram_signature(text: &str) -> Vec<f64> {
    if text.chars().count() < 3 {
        return vec![1.0];
    }
    let mut counts = BTreeMap::new();
    for tri in trigram_list(text) {
        *counts.entry(tri).or_insert(0.0) += 1.0;
    }
    let mut vec: Vec<f64> = counts.into_values().collect();
    let norm = vec.iter().map(|x| x * x).sum::<f64>().sqrt();
    let norm = if norm == 0.0 { 1.0 } else { norm };
    for x in &mut vec {
        *x /= norm;
    }
    vec
}

pub fn unique_trigrams(text: &str) -> BTreeSet<String> {
    trigram_list(text).into_iter().collect()
}

pub fn trigram_bag_cosine(a: &str, b: &str) -> f64 {
    let mut ca = BTreeMap::new();
    let mut cb = BTreeMap::new();
    for tri in trigram_list(a) {
        *ca.entry(tri).or_insert(0.0) += 1.0;
    }
    for tri in trigram_list(b) {
        *cb.entry(tri).or_insert(0.0) += 1.0;
    }
    let keys: Vec<&String> = {
        let set: BTreeSet<&String> = ca.keys().chain(cb.keys()).collect();
        set.into_iter().collect()
    };
    if keys.is_empty() {
        return 1.0;
    }
    let va: Vec<f64> = keys.iter().map(|key| *ca.get(*key).unwrap_or(&0.0)).collect();
    let vb: Vec<f64> = keys.iter().map(|key| *cb.get(*key).unwrap_or(&0.0)).collect();
    cosine(&va, &vb)
}

pub fn cosine(a: &[f64], b: &[f64]) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let na = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let nb = b.iter().map(|y| y * y).sum::<f64>().sqrt();
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    a.iter().zip(b).map(|(x, y)| x * y).sum::<f64>() / (na * nb)
}

fn verb(word: &str) -> &str {
    match word {
        "refactor" | "rewrite" | "rework" | "reorganize" | "restructure" => "restructure",
        "fix" | "repair" | "patch" | "debug" | "resolve" => "fix",
        "add" | "create" | "implement" | "introduce" | "build" => "add",
        "remove" | "delete" | "drop" | "eliminate" => "remove",
        "update" | "modify" | "change" | "adjust" => "update",
        "move" | "migrate" | "relocate" | "transfer" => "move",
        "rename" => "rename",
        "test" | "verify" | "validate" | "check" => "test",
        "optimize" | "improve" | "enhance" => "optimize",
        "extract" | "split" | "separate" | "decouple" => "extract",
        other => other,
    }
}

fn abbrev(word: &str) -> &'static str {
    match word {
        "gc" => "garbage collector",
        "db" => "database",
        "auth" => "authentication",
        "config" => "configuration",
        "impl" => "implementation",
        "init" => "initialization",
        "env" => "environment",
        "deps" => "dependencies",
        "dep" => "dependency",
        "repo" => "repository",
        "fn" | "func" => "function",
        "param" => "parameter",
        "params" => "parameters",
        "args" => "arguments",
        "err" => "error",
        "msg" => "message",
        "req" => "request",
        "resp" | "res" => "response",
        "cmd" => "command",
        "ctx" => "context",
        "util" => "utility",
        "utils" => "utilities",
        "lib" => "library",
        "pkg" => "package",
        "dir" => "directory",
        "perf" => "performance",
        "opt" => "optimize",
        "mem" => "memory",
        "alloc" => "allocation",
        "io" => "input output",
        "ui" => "user interface",
        "api" => "application programming interface",
        "cli" => "command line interface",
        "orm" => "object relational mapping",
        "sdk" => "software development kit",
        "jwt" => "json web token",
        "url" => "uniform resource locator",
        "http" => "hypertext transfer protocol",
        "src" => "source",
        "dst" => "destination",
        "ref" => "reference",
        "refs" => "references",
        "idx" => "index",
        "tmp" => "temporary",
        "async" => "asynchronous",
        "sync" => "synchronous",
        _ => "",
    }
}

fn tech(word: &str) -> &'static [&'static str] {
    match word {
        "websocket" => &["socket.io", "real-time transport", "event stream", "realtime"],
        "merge" => &["combine", "integrate", "reconcile", "unify"],
        "conflict resolution" => &["ot", "crdt", "consistency", "merge"],
        "auth" => &["authentication", "authorization", "login", "token"],
        "metric" => &["telemetry", "monitoring", "observability", "dashboard"],
        "optimize" => &["improve", "enhance", "tune", "refine"],
        "validate" => &["verify", "check", "assert", "test"],
        "deploy" => &["release", "publish", "ship", "launch"],
        "cache" => &["memoize", "store", "buffer"],
        "queue" => &["buffer", "stream", "pipeline"],
        _ => &[],
    }
}

pub fn expand_goal_terms(text: &str) -> String {
    let mut expanded = Vec::new();
    for word in text.to_lowercase().split_whitespace() {
        let mapped = abbrev(word);
        expanded.push(if mapped.is_empty() { word.to_string() } else { mapped.to_string() });
        expanded.extend(tech(word).iter().map(|s| (*s).to_string()));
    }
    expanded
        .into_iter()
        .flat_map(|chunk| {
            chunk
                .split_whitespace()
                .map(|w| verb(w).to_string())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn normalize_goal(text: &str) -> String {
    let expanded = text
        .to_lowercase()
        .split_whitespace()
        .map(|w| {
            let mapped = abbrev(w);
            if mapped.is_empty() { w.to_string() } else { mapped.to_string() }
        })
        .collect::<Vec<_>>()
        .join(" ");
    expanded
        .split_whitespace()
        .map(verb)
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn word_jaccard(a: &str, b: &str) -> f64 {
    let wa: BTreeSet<&str> = a.split_whitespace().collect();
    let wb: BTreeSet<&str> = b.split_whitespace().collect();
    if wa.is_empty() && wb.is_empty() {
        return 1.0;
    }
    let inter = wa.intersection(&wb).count() as f64;
    let union = wa.union(&wb).count() as f64;
    if union == 0.0 { 1.0 } else { inter / union }
}

pub fn word_substring_score(a: &str, b: &str) -> f64 {
    let a_l = a.to_lowercase();
    let b_l = b.to_lowercase();
    let wa: Vec<&str> = a_l.split_whitespace().collect();
    let wb: Vec<&str> = b_l.split_whitespace().collect();
    if wa.is_empty() || wb.is_empty() {
        return 0.0;
    }
    let matches = wa
        .iter()
        .filter(|w_a| wb.iter().any(|w_b| w_a.contains(w_b) || w_b.contains(*w_a)))
        .count();
    matches as f64 / wa.len() as f64
}

pub fn goal_similarity(a: &str, b: &str) -> f64 {
    let tri = trigram_bag_cosine(a, b);
    let wj = word_jaccard(&normalize_goal(a), &normalize_goal(b));
    let ws = word_substring_score(&normalize_goal(a), &normalize_goal(b));
    0.3 * tri + 0.4 * wj + 0.3 * ws
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_goals_score_one() {
        let s = goal_similarity("add a timeout", "add a timeout");
        assert!((s - 1.0).abs() < 1e-9, "{s}");
    }

    #[test]
    fn empty_trigrams_are_identical() {
        assert_eq!(trigram_bag_cosine("ab", "cd"), 1.0);
    }
}
