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

const STOPWORDS: &[&str] = &[
    "a", "an", "the", "to", "of", "in", "on", "for", "with", "and", "or", "by", "from", "into",
    "that", "which", "whose", "who", "is", "are", "be", "it", "its", "this", "these", "those",
    "as", "at", "so", "some", "any", "all", "each", "every", "when", "where", "than", "then",
    "our", "my", "we", "us", "can", "should", "make", "sure", "new", "use", "using", "via",
];

/// Words that say the same relation.
fn relation(word: &str) -> &str {
    match word {
        "under" | "beneath" | "less" | "fewer" | "lower" | "underneath" => "below",
        "over" | "exceed" | "exceeds" | "exceeding" | "greater" => "above",
        "listing" | "lists" => "list",
        "show" | "display" | "print" => "show",
        other => other,
    }
}

/// Light suffix stripping: listing -> list, items -> item, fixes -> fix,
/// entries -> entry. Both sides stem the same way, so `updat` meets `updat`.
fn stem(word: &str) -> String {
    let n = word.len();
    if n > 5 && word.ends_with("ing") { return word[..n - 3].to_string(); }
    if n > 4 && word.ends_with("ies") { return format!("{}y", &word[..n - 3]); }
    if n > 4 && ["ses", "xes", "ches", "shes"].iter().any(|s| word.ends_with(s)) { return word[..n - 2].to_string(); }
    if n > 4 && word.ends_with("ed") { return word[..n - 2].to_string(); }
    if n > 3 && word.ends_with('s') && !word.ends_with("ss") { return word[..n - 1].to_string(); }
    word.to_string()
}

/// The content words of a goal, file path, or symbol, normalized so that
/// paraphrases meet: `low-stock`, `lowStock` and `low_stock` all give `low`
/// and `stock`; `listing items under` gives `list`, `item`, `below`.
pub fn concept_terms(text: &str) -> BTreeSet<String> {
    let mut spaced = String::new();
    let mut prev_lower = false;
    for ch in text.chars() {
        if ch.is_uppercase() && prev_lower { spaced.push(' '); }
        prev_lower = ch.is_lowercase();
        spaced.push(if ch.is_alphanumeric() { ch } else { ' ' });
    }
    let mut out = BTreeSet::new();
    for raw in spaced.to_lowercase().split_whitespace() {
        let mapped = abbrev(raw);
        let words: Vec<&str> = if mapped.is_empty() { vec![raw] } else { mapped.split_whitespace().collect() };
        for word in words {
            if word.len() < 2 || STOPWORDS.contains(&word) { continue; }
            let term = verb(relation(&stem(verb(relation(word))))).to_string();
            if !STOPWORDS.contains(&term.as_str()) { out.insert(term); }
        }
    }
    out
}

/// Dice overlap of two term sets. A query term the recipe's goal lacks but
/// one of its files or symbols names (`context`) counts half: the same work
/// described from the code side.
pub fn concept_similarity(query: &BTreeSet<String>, recipe: &BTreeSet<String>, context: &BTreeSet<String>) -> f64 {
    if query.is_empty() || recipe.is_empty() { return 0.0; }
    let shared = query.intersection(recipe).count() as f64;
    let via_code = query.iter().filter(|t| !recipe.contains(*t) && context.contains(*t)).count() as f64;
    (2.0 * (shared + 0.5 * via_code) / (query.len() + recipe.len()) as f64).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const STORED: &str = "add a low-stock report listing items whose quantity is below a threshold";

    fn concept(a: &str, b: &str) -> f64 {
        concept_similarity(&concept_terms(a), &concept_terms(b), &BTreeSet::new())
    }

    #[test]
    fn paraphrases_of_a_goal_meet() {
        let want: BTreeSet<String> = ["below", "item", "list", "low", "stock"].iter().map(|s| s.to_string()).collect();
        assert_eq!(concept_terms("listing lowStock items_under"), want);
        // The miss seen in the handoff run: the text blend scored it under 0.55.
        let query = "report items with low stock under a threshold";
        assert!(goal_similarity(query, STORED) < 0.55);
        assert!(concept(query, STORED) >= 0.7, "{}", concept(query, STORED));
        assert!(concept("fix failing auth tests", "repair the failing authentication test") >= 0.7);
    }

    #[test]
    fn different_work_stays_below_the_recipe_threshold() {
        for other in ["fix the login timeout in the auth module", "add a report of sales by region", "remove the threshold check from the store", "rename the config loader"] {
            let score = concept(other, STORED).max(goal_similarity(other, STORED));
            assert!(score < 0.55, "{other}: {score}");
        }
    }

    #[test]
    fn code_words_count_half() {
        let recipe = concept_terms("add retries");
        let context = concept_terms("src/http_client.rs sendRequest");
        let query = concept_terms("add retries to http client");
        let plain = concept_similarity(&query, &recipe, &BTreeSet::new());
        let with_code = concept_similarity(&query, &recipe, &context);
        assert!(with_code > plain, "{plain} {with_code}");
        assert_eq!(concept_similarity(&concept_terms("delete the cache"), &recipe, &context), 0.0);
    }

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
