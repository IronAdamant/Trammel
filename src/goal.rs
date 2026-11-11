//! Goal text analysis. Keyword, path, intent, and ambiguity rules match `goal_nlp.py`.

use std::collections::BTreeSet;

const STOP: &[&str] = &[
    "a", "an", "the", "and", "or", "but", "in", "on", "at", "to", "for", "of", "with", "by",
    "from", "as", "is", "was", "are", "were", "be", "been", "being", "have", "has", "had", "do",
    "does", "did", "will", "would", "could", "should", "may", "might", "must", "shall", "can",
    "that", "this", "these", "those", "it", "its", "not", "no", "all", "any", "each", "every",
    "some", "into", "about", "up", "out", "then", "than", "so", "very", "just", "also", "too",
    "only", "new", "their", "them", "they", "there", "others", "other", "per", "when", "where",
    "which", "while", "what", "whose", "both", "either", "such", "same", "own",
];

const CREATION: &[&str] = &[
    "add", "create", "build", "implement", "introduce", "write", "make", "generate", "develop",
    "establish", "initialize", "scaffold", "set", "setup",
];

const REFACTOR: &[&str] = &[
    "refactor", "consolidate", "deduplicate", "dry", "simplify", "extract", "merge", "reorganize",
    "cleanup", "clean", "update", "modify", "edit", "fix", "rename", "migrate", "rewrite", "split",
    "reduce", "improve",
];

const EXTS: &[&str] = &[
    "py", "js", "mjs", "cjs", "ts", "tsx", "jsx", "go", "rs", "java", "kt", "kts", "c", "h", "hpp",
    "cpp", "cc", "cxx", "cs", "rb", "php", "sql", "md", "yaml", "yml", "json", "toml",
];

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn ident_words(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_alphabetic() || chars[i] == '_' {
            let start = i;
            i += 1;
            while i < chars.len() && is_word_char(chars[i]) {
                i += 1;
            }
            out.push(chars[start..i].iter().collect::<String>().to_lowercase());
        } else {
            i += 1;
        }
    }
    out
}

fn alpha_parts(word: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for ch in word.chars() {
        if ch.is_ascii_lowercase() {
            current.push(ch);
        } else if !current.is_empty() {
            out.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

pub fn extract_goal_keywords(goal: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for word in ident_words(&goal.to_lowercase()) {
        let parts = alpha_parts(&word);
        // In an identifier such as blocks_build, "build" names the thing,
        // not the action, so creation verbs stay.
        let compound = parts.len() > 1;
        for part in parts {
            if part.len() > 2 && !STOP.contains(&part.as_str()) && (compound || !CREATION.contains(&part.as_str()))
            {
                out.insert(part);
            }
        }
    }
    out
}

fn has_code_ext(path: &str) -> bool {
    let base = path.rsplit('/').next().unwrap_or(path);
    let Some((_, ext)) = base.rsplit_once('.') else {
        return false;
    };
    EXTS.iter().any(|known| known.eq_ignore_ascii_case(ext))
}

fn looks_like_path(path: &str) -> bool {
    let p = path.trim().replace('\\', "/");
    if p.is_empty() || p.contains("..") || p.starts_with('/') {
        return false;
    }
    p.contains('/') || (p.len() > 2 && p.rsplit('/').next().unwrap_or(&p).contains('.'))
}

pub fn extract_paths_from_goal(goal: &str) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut ordered = Vec::new();
    let mut push = |raw: &str| {
        let p = raw.trim().replace('\\', "/");
        if looks_like_path(&p) && has_code_ext(&p) && seen.insert(p.clone()) {
            ordered.push(p);
        }
    };
    for raw in quoted(goal, '`') {
        if has_code_ext(&raw) {
            push(&raw);
        }
    }
    for raw in quoted(goal, '"').into_iter().chain(quoted(goal, '\'')) {
        if has_code_ext(&raw) {
            push(&raw);
        }
    }
    for token in slash_paths(goal) {
        push(&token);
    }
    ordered
}

/// Directory names that say where code lives, not what it is about.
pub const GENERIC_DIRS: &[&str] = &[
    "src", "lib", "crates", "crate", "packages", "pkg", "apps", "app", "internal", "cmd", "tests",
    "test", "examples", "benches", "source", "sources", "modules", "main",
];

/// What a goal names as files. `paths` are file paths and bare file names
/// with a code extension (`crates/specula/src/sight.rs`, `game.rs`).
/// `modules` are Rust paths (`specula::sight`, `crate::game::Ctx`). `stems`
/// are bare names in backticks or next to "module" / "file" (`sight`, "the
/// timeline module").
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NamedRefs {
    pub paths: Vec<String>,
    pub modules: Vec<Vec<String>>,
    pub stems: Vec<String>,
}

impl NamedRefs {
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty() && self.modules.is_empty() && self.stems.is_empty()
    }
}

pub fn named_refs(goal: &str) -> NamedRefs {
    let mut paths = extract_paths_from_goal(goal);
    for token in bare_file_names(goal) {
        if !paths.iter().any(|p| p == &token || p.ends_with(&format!("/{token}"))) { paths.push(token); }
    }
    let mut modules: Vec<Vec<String>> = Vec::new();
    for path in crate::lang::rust_mods::paths_in(goal) {
        if !modules.contains(&path) { modules.push(path); }
    }
    let mut stems: Vec<String> = Vec::new();
    let mut push_stem = |word: &str| {
        let w = word.trim().to_string();
        let ok = w.len() > 1 && w.chars().all(is_word_char) && w.chars().next().map(|c| c.is_ascii_alphabetic() || c == '_').unwrap_or(false);
        let lower = w.to_lowercase();
        if ok && !STOP.contains(&lower.as_str()) && !ARTICLES.contains(&lower.as_str()) && !CREATION.contains(&lower.as_str()) && !FILE_NOUNS.contains(&lower.as_str()) && !stems.contains(&w) {
            stems.push(w);
        }
    };
    for raw in quoted(goal, '`') {
        if !raw.contains("::") && !raw.contains('/') && !raw.contains('.') { push_stem(&raw); }
    }
    let words: Vec<&str> = goal.split(|c: char| !(is_word_char(c) || c == '-')).filter(|w| !w.is_empty()).collect();
    for (i, word) in words.iter().enumerate() {
        if matches!(word.to_lowercase().as_str(), "module" | "file") {
            if let Some(prev) = i.checked_sub(1).and_then(|j| words.get(j)) { push_stem(prev); }
        }
    }
    NamedRefs { paths, modules, stems }
}

/// `game.rs`, `models.py`: file names with a code extension and no directory.
fn bare_file_names(goal: &str) -> Vec<String> {
    let mut out = Vec::new();
    for token in goal.split(|c: char| !(is_path_char(c) || c == '/' || c == '\\')) {
        let token = token.trim_end_matches('.');
        if token.contains('/') || token.contains('\\') || token.starts_with('.') { continue; }
        let Some((stem, _)) = token.rsplit_once('.') else { continue };
        if stem.is_empty() || !stem.chars().next().map(|c| c.is_ascii_alphabetic() || c == '_').unwrap_or(false) { continue; }
        if has_code_ext(token) && !out.iter().any(|t: &String| t == token) { out.push(token.to_string()); }
    }
    out
}

/// The goal with every named path cut to the words it is about: a path's
/// file stem and its directories other than `GENERIC_DIRS`. Keyword scoring
/// and anchors use this text, so `crates/specula/src/sight.rs` counts as
/// "specula sight", not as "crates src".
pub fn keyword_text(goal: &str) -> String {
    path_words(goal, true)
}

/// Like `keyword_text` without file stems: the words that may name a
/// directory to plan inside. "game.rs" says nothing about a games/ folder.
pub fn anchor_text(goal: &str) -> String {
    path_words(goal, false)
}

fn path_words(goal: &str, stems: bool) -> String {
    let mut text = goal.replace('\\', "/");
    let mut paths = extract_paths_from_goal(goal);
    paths.extend(bare_file_names(goal));
    paths.sort_by_key(|p| std::cmp::Reverse(p.len()));
    for path in paths {
        let parts: Vec<&str> = path.split('/').collect();
        let Some((last, dirs)) = parts.split_last() else { continue };
        let mut words: Vec<String> = dirs.iter().filter(|part| !GENERIC_DIRS.contains(&part.to_lowercase().as_str())).map(|s| s.to_string()).collect();
        if stems { words.push(last.rsplit_once('.').map(|(s, _)| s).unwrap_or(last).to_string()); }
        text = text.replace(&path, &format!(" {} ", words.join(" ")));
    }
    text
}

fn quoted(text: &str, quote: char) -> Vec<String> {
    let mut out = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == quote {
            if let Some(end) = chars[i + 1..].iter().position(|c| *c == quote) {
                out.push(chars[i + 1..i + 1 + end].iter().collect());
                i += end + 2;
                continue;
            }
        }
        i += 1;
    }
    out
}

fn slash_paths(goal: &str) -> Vec<String> {
    let chars: Vec<char> = goal.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if is_path_char(chars[i]) {
            let start = i;
            while i < chars.len() && (is_path_char(chars[i]) || chars[i] == '/') {
                i += 1;
            }
            let token: String = chars[start..i].iter().collect();
            // A sentence may end right after a path: "... in src/app.rs."
            let token = token.trim_end_matches('.').to_string();
            if token.contains('/') && has_code_ext(&token) {
                let bounded = start == 0
                    || !is_path_char(chars[start - 1])
                    || chars.get(start.wrapping_sub(1)) == Some(&'/');
                let end_ok = i == chars.len() || !is_path_char(chars[i]);
                if bounded && end_ok {
                    out.push(token);
                }
            }
        } else {
            i += 1;
        }
    }
    out
}

fn is_path_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-'
}

pub fn has_refactor_intent(goal: &str) -> bool {
    ident_alpha(goal).iter().any(|w| REFACTOR.contains(&w.as_str()))
}

/// Nouns that, after a creation verb, mean "a new file".
const FILE_NOUNS: &[&str] = &[
    "file", "module", "class", "component", "page", "screen", "view", "endpoint", "route",
    "script", "service", "handler", "controller", "plugin", "command", "crate", "package",
    "binary", "example", "migration",
];

const ARTICLES: &[&str] = &["a", "an", "the", "new", "one", "another"];

/// Names of the new files a goal clearly asks for: "create a props module"
/// gives `props`, "add a new settings page" gives `settings`, "add a module
/// called retry_policy" gives `retry_policy`. A goal that only changes
/// existing code ("add a blocks_build flag to props") gives none.
pub fn new_file_names(goal: &str) -> Vec<String> {
    let lower = goal.to_lowercase();
    let words: Vec<&str> = lower.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-')).filter(|w| !w.is_empty()).collect();
    if !words.iter().any(|w| CREATION.contains(w) || *w == "new") { return Vec::new(); }
    let mut names = Vec::new();
    for (i, word) in words.iter().enumerate() {
        if !FILE_NOUNS.contains(word) { continue; }
        // "a module called X" / "a module named X"
        if matches!(words.get(i + 1), Some(&"called") | Some(&"named")) {
            if let Some(name) = words.get(i + 2) { names.push(name.replace('-', "_")); }
            continue;
        }
        // "a props module", "a new settings page"
        if let Some(prev) = i.checked_sub(1).and_then(|j| words.get(j)) {
            if !ARTICLES.contains(prev) && !STOP.contains(prev) && !CREATION.contains(prev) && !FILE_NOUNS.contains(prev) {
                names.push(prev.replace('-', "_"));
            }
        }
    }
    names.dedup();
    names
}

pub fn has_creation_intent(goal: &str) -> bool {
    let lower = goal.to_lowercase();
    let split: BTreeSet<&str> = lower.split_whitespace().collect();
    if CREATION.iter().any(|v| split.contains(v)) {
        return true;
    }
    if has_refactor_intent(goal) {
        return false;
    }
    const ROLES: &[&str] = &[
        "service", "services", "route", "routes", "controller", "controllers", "handler",
        "handlers", "engine", "engines", "plugin", "plugins", "registry", "registries",
        "collector", "aggregator", "middleware", "model", "models", "repository", "repositories",
        "algorithm", "algorithms", "generator", "generators", "workflow", "marketplace",
        "pipeline",
    ];
    ident_alpha(goal).iter().any(|w| ROLES.contains(&w.as_str()))
}

fn ident_alpha(goal: &str) -> Vec<String> {
    let mut out = Vec::new();
    for word in ident_words(&goal.to_lowercase()) {
        out.extend(alpha_parts(&word));
    }
    out
}

pub fn keyword_variants(keywords: &BTreeSet<String>) -> BTreeSet<String> {
    let mut variants = keywords.clone();
    for kw in keywords {
        variants.insert(format!("{kw}s"));
        variants.insert(format!("{kw}es"));
        if let Some(stripped) = kw.strip_suffix('s') {
            variants.insert(stripped.to_string());
        }
        if kw.ends_with("es") && kw.len() > 2 {
            variants.insert(kw[..kw.len() - 2].to_string());
        }
    }
    variants
}

pub fn matched_keywords(goal_keywords: &BTreeSet<String>, candidate_words: &BTreeSet<String>) -> BTreeSet<String> {
    let variants = keyword_variants(goal_keywords);
    let mut matched = BTreeSet::new();
    if variants.intersection(candidate_words).next().is_some() {
        for kw in goal_keywords {
            let one = BTreeSet::from([kw.clone()]);
            if keyword_variants(&one).intersection(candidate_words).next().is_some() {
                matched.insert(kw.clone());
            }
        }
    }
    for kw in goal_keywords {
        let kw_lc = kw.to_lowercase();
        for cw in candidate_words {
            let cw_lc = cw.to_lowercase();
            if (kw_lc.len() >= 4 && cw_lc.contains(&kw_lc)) || (cw_lc.len() >= 4 && kw_lc.contains(&cw_lc))
            {
                matched.insert(kw.clone());
            }
        }
    }
    matched
}

const PHRASES: &[&str] = &[
    "conflict resolution", "real-time", "real time", "ai-powered", "ai powered", "collaborative",
    "fallback", "merge suggestions", "smart", "intelligent", "automated", "dynamic", "adaptive",
    "context-aware", "context aware", "best practice", "industry standard", "enterprise grade",
    "scalable", "robust", "seamless", "end-to-end", "end to end", "holistic", "comprehensive",
    "integrated", "unified", "next-gen", "next gen", "cutting edge", "state of the art",
    "modernized", "optimized",
];

const SCOPE: &[&str] = &[
    "system", "framework", "platform", "infrastructure", "architecture", "ecosystem", "solution",
    "stack", "suite", "toolkit",
];

pub struct Ambiguity {
    pub score: f64,
    pub flag: &'static str,
    pub signals: Vec<String>,
}

pub fn ambiguity_score(goal: &str) -> Ambiguity {
    let goal_lower = goal.to_lowercase();
    let mut signals = Vec::new();
    let mut score: f64 = 0.0;
    for phrase in PHRASES {
        if goal_lower.contains(phrase) {
            signals.push(format!("vague_phrase: '{phrase}'"));
            score += 0.15;
        }
    }
    let words = ident_alpha(goal);
    let mut seen = BTreeSet::new();
    for word in &words {
        if SCOPE.contains(&word.as_str()) && seen.insert(word.clone()) {
            signals.push(format!("scope_word: '{word}'"));
            score += 0.10;
        }
    }
    let conjunctions = goal_lower
        .split_whitespace()
        .filter(|w| matches!(*w, "and" | "with" | "or" | "plus"))
        .count();
    if conjunctions >= 4 {
        signals.push("high_conjunction_count".into());
        score += 0.15;
    } else if conjunctions >= 2 {
        signals.push("medium_conjunction_count".into());
        score += 0.05;
    }
    if goal_lower.split_whitespace().count() > 20 {
        score += 0.05;
    }
    score = score.min(1.0);
    let flag = if score >= 0.5 {
        "high"
    } else if score >= 0.25 {
        "medium"
    } else {
        "low"
    };
    Ambiguity {
        score: crate::canon::py_round(score, 2),
        flag,
        signals,
    }
}
