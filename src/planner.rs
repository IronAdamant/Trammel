//! Decomposition and beam exploration.

use crate::canon::{self, py_round};
use crate::config::{self, ProjectConfig};
use crate::goal::{self, Ambiguity};
use crate::hash::sha256_hex;
use crate::implicit::Engine;
use crate::json::Value;
use crate::lang::analyze;
use crate::score::{self, filter_globs};
use crate::strategies;
use crate::topo::topological_sort;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct Request {
    pub goal: String,
    pub root: PathBuf,
    pub scope: Option<String>,
    pub language: Option<String>,
    pub relevant_only: bool,
    pub min_relevance: f64,
    pub max_steps: Option<usize>,
    pub max_files: Option<i64>,
    pub summary_only: bool,
    pub focus_keywords: Vec<String>,
    pub focus_globs: Vec<String>,
    pub strict_greenfield: bool,
    pub suppress_creation_hints: bool,
    pub expand_repo: Option<bool>,
    pub apply_project_config: bool,
    pub num_beams: usize,
}

impl Request {
    pub fn new(goal: impl Into<String>, root: impl Into<PathBuf>) -> Request {
        Request {
            goal: goal.into(),
            root: root.into(),
            scope: None,
            language: None,
            relevant_only: false,
            min_relevance: 0.0,
            max_steps: None,
            max_files: None,
            summary_only: false,
            focus_keywords: Vec::new(),
            focus_globs: Vec::new(),
            strict_greenfield: false,
            suppress_creation_hints: false,
            expand_repo: None,
            apply_project_config: true,
            num_beams: 3,
        }
    }
}

pub fn decompose(request: &Request) -> Result<Value, String> {
    let started = Instant::now();
    let cfg = if request.apply_project_config { config::load(&request.root) } else { ProjectConfig::default() };
    let scope = request.scope.clone().or_else(|| cfg.default_scope.clone());
    let keywords = config::merge_keywords(goal::extract_goal_keywords(&request.goal), &request.focus_keywords, &cfg);
    let mut globs = request.focus_globs.clone();
    globs.extend(cfg.focus_globs.clone());
    let analysis_root = match &scope {
        Some(scope) if !scope.is_empty() => request.root.join(scope),
        _ => request.root.clone(),
    };
    if request.strict_greenfield && goal::has_creation_intent(&request.goal) && goal::extract_paths_from_goal(&request.goal).is_empty() {
        return Err("strict_greenfield requires explicit file paths in the goal, a non-empty scaffold, or a matching recipe with scaffold steps; otherwise set strict_greenfield=false".into());
    }
    let mut max_files = request.max_files.or(cfg.max_files);
    let expand = request.expand_repo.unwrap_or(true);
    if expand && max_files.map(|n| n > 15).unwrap_or(true) {
        max_files = Some(15);
    }
    let language = request.language.clone().unwrap_or_else(|| analyze::detect(&analysis_root, &cfg));
    let t_symbols = Instant::now();
    let analysis = analyze::analyze(&analysis_root, &language);
    let symbol_ms = t_symbols.elapsed();
    let mut symbols = analysis.symbols;
    let mut graph = analysis.imports;
    let mut all: BTreeSet<String> = symbols.keys().cloned().chain(graph.keys().cloned()).collect();
    if !globs.is_empty() {
        let kept = filter_globs(&all, &globs);
        symbols.retain(|k, _| kept.contains(k));
        graph.retain(|k, _| kept.contains(k));
        for deps in graph.values_mut() { deps.retain(|d| kept.contains(d)); }
        all = kept;
    }
    let mut engine = Engine::new();
    engine.analyze(&analysis_root, &all, &graph);
    let (hybrid, naming, shared, patterned) = engine.hybrid(&graph, &all);
    let blind = naming + shared + patterned;
    let mut relevant = BTreeMap::new();
    for (file, deps) in &hybrid {
        if all.contains(file) {
            relevant.insert(file.clone(), deps.iter().filter(|d| all.contains(*d)).cloned().collect());
        }
    }
    let mut order = topological_sort(&relevant);
    order.retain(|f| symbols.contains_key(f));
    for file in symbols.keys() {
        if !order.contains(file) { order.push(file.clone()); }
    }
    let mut truncated = false;
    if let Some(limit) = max_files {
        if limit >= 0 && order.len() > limit as usize {
            order.truncate(limit as usize);
            truncated = true;
            let allowed: BTreeSet<String> = order.iter().cloned().collect();
            symbols.retain(|k, _| allowed.contains(k));
            relevant.retain(|k, _| allowed.contains(k));
            for deps in relevant.values_mut() { deps.retain(|d| allowed.contains(d)); }
            all = symbols.keys().cloned().chain(relevant.keys().cloned()).collect();
        }
    }
    let mut steps = score::generate_steps(&order, &symbols, &relevant, &request.goal, &keywords);
    if request.relevant_only && !keywords.is_empty() {
        steps.sort_by(|a, b| {
            let ra = canon::get(a, "relevance").and_then(canon::as_f64).unwrap_or(0.0);
            let rb = canon::get(b, "relevance").and_then(canon::as_f64).unwrap_or(0.0);
            rb.partial_cmp(&ra).unwrap_or(std::cmp::Ordering::Equal)
        });
        for (i, step) in steps.iter_mut().enumerate() {
            canon::insert(step, "step_index", Value::Int(i as i64));
        }
    }
    let _ = request.min_relevance;
    let ambiguity = goal::ambiguity_score(&request.goal);
    let mut meta = canon::obj(vec![
        ("language", Value::string(language.clone())),
        ("scope", scope.clone().map(Value::String).unwrap_or(Value::Null)),
        ("files_analyzed", Value::Int(symbols.len() as i64)),
        ("dep_files", Value::Int(relevant.len() as i64)),
        ("dep_edges", Value::Int(relevant.values().map(|v| v.len() as i64).sum())),
        ("timing_s", canon::obj(vec![
            ("symbols", Value::Float(py_round(symbol_ms.as_secs_f64(), 3))),
            ("imports", Value::Float(0.0)),
            ("total", Value::Float(py_round(started.elapsed().as_secs_f64(), 3))),
        ])),
        ("ambiguity", ambiguity_value(&ambiguity)),
        ("implicit_dependency_analysis", canon::obj(vec![
            ("inferred_edges", Value::Int(blind as i64)),
            ("trammel_blind_spots", Value::Int(blind as i64)),
            ("naming_convention_inferences", Value::Int(naming as i64)),
            ("shared_state_inferences", Value::Int(shared as i64)),
            ("pattern_based_inferences", Value::Int(patterned as i64)),
        ])),
    ]);
    if truncated { canon::insert(&mut meta, "max_files_truncated", Value::Bool(true)); }
    let mut result = canon::obj(vec![
        ("goal", Value::string(request.goal.clone())),
        ("steps", Value::Array(steps)),
        ("dependency_graph", graph_value(&relevant)),
        ("constraints", Value::Array(vec![])),
        ("constraints_applied", Value::Array(vec![])),
        ("goal_fingerprint", Value::string(fingerprint(&request.goal))),
        ("analysis_meta", meta),
        ("expand_repo", Value::Bool(expand)),
        ("plan_fidelity", canon::obj(vec![
            ("strict_greenfield", Value::Bool(request.strict_greenfield)),
            ("project_config_applied", Value::Bool(request.apply_project_config && cfg.language.is_some() || cfg.default_scope.is_some() || !cfg.focus_keywords.is_empty() || !cfg.focus_globs.is_empty() || cfg.max_files.is_some() || cfg.test_cmd.is_some())),
            ("focus_globs", Value::Array(globs.into_iter().map(Value::String).collect())),
            ("max_files_cap", max_files.map(Value::Int).unwrap_or(Value::Null)),
            ("suppress_creation_hints", Value::Bool(request.suppress_creation_hints)),
            ("fallback_file_cap", Value::Int(max_files.unwrap_or(15))),
        ])),
        ("trammel_blind_spots", Value::Int(blind as i64)),
    ]);
    if !request.suppress_creation_hints {
        if let Some(hints) = creation_hints(&request.goal, &keywords, &all) {
            canon::insert(&mut result, "creation_hints", hints);
        }
    }
    if request.summary_only {
        return Ok(summary(&result));
    }
    if let Some(limit) = request.max_steps {
        if let Some(Value::Array(steps)) = canon::get_mut(&mut result, "steps") {
            let total = steps.len();
            if limit < total { steps.truncate(limit); }
            canon::insert(&mut result, "total_steps", Value::Int(total as i64));
        }
    }
    let _ = analysis_root;
    Ok(result)
}

pub fn explore(request: &Request) -> Result<Value, String> {
    let strategy = decompose(request)?;
    let steps = canon::get(&strategy, "steps").and_then(canon::as_array).unwrap_or(&[]).to_vec();
    let graph = graph_from(canon::get(&strategy, "dependency_graph"));
    let beams = strategies::explore(&steps, &graph, request.num_beams, &BTreeMap::new());
    Ok(canon::obj(vec![
        ("goal", Value::string(request.goal.clone())),
        ("strategy", strategy),
        ("beams", Value::Array(beams)),
    ]))
}

fn summary(result: &Value) -> Value {
    let steps = canon::get(result, "steps").and_then(canon::as_array).unwrap_or(&[]);
    canon::obj(vec![
        ("goal", canon::get(result, "goal").cloned().unwrap_or(Value::Null)),
        ("step_count", Value::Int(steps.len() as i64)),
        ("files", Value::Array(steps.iter().filter_map(|s| canon::get(s, "file").cloned()).collect())),
        ("goal_fingerprint", canon::get(result, "goal_fingerprint").cloned().unwrap_or(Value::Null)),
        ("analysis_meta", canon::get(result, "analysis_meta").cloned().unwrap_or(Value::Null)),
        ("constraints", canon::get(result, "constraints").cloned().unwrap_or(Value::Array(vec![]))),
        ("constraints_applied", canon::get(result, "constraints_applied").cloned().unwrap_or(Value::Array(vec![]))),
    ])
}

fn graph_value(graph: &BTreeMap<String, Vec<String>>) -> Value {
    Value::Object(graph.iter().map(|(k, v)| (k.clone(), Value::Array(v.iter().cloned().map(Value::String).collect()))).collect())
}

fn graph_from(value: Option<&Value>) -> BTreeMap<String, Vec<String>> {
    let mut out = BTreeMap::new();
    let Some(Value::Object(pairs)) = value else { return out };
    for (key, item) in pairs {
        let deps = canon::as_array(item).map(|items| items.iter().filter_map(canon::as_str).map(|s| s.to_string()).collect()).unwrap_or_default();
        out.insert(key.clone(), deps);
    }
    out
}

pub fn fingerprint(goal: &str) -> String {
    let text = canon::canonical(&Value::String(goal.to_string()));
    sha256_hex(text.as_bytes())[..16].to_string()
}

fn ambiguity_value(score: &Ambiguity) -> Value {
    canon::obj(vec![
        ("score", Value::Float(score.score)),
        ("flag", Value::string(score.flag)),
        ("signals", Value::Array(score.signals.iter().cloned().map(Value::String).collect())),
    ])
}

fn creation_hints(goal: &str, keywords: &BTreeSet<String>, files: &BTreeSet<String>) -> Option<Value> {
    if !goal::has_creation_intent(goal) { return None; }
    let mut dirs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for file in files {
        if let Some((dir, base)) = file.rsplit_once('/') {
            dirs.entry(dir.to_string()).or_default().push(base.to_string());
        }
    }
    let variants = goal::keyword_variants(keywords);
    let mut relevant = Vec::new();
    for dir in dirs.keys() {
        let words = dir.to_lowercase().split(|c: char| !c.is_ascii_lowercase()).filter(|w| !w.is_empty()).map(|s| s.to_string()).collect::<BTreeSet<_>>();
        if words.intersection(&variants).next().is_some() { relevant.push(dir.clone()); }
    }
    let mut suggested = Vec::new();
    for dir in &relevant {
        let siblings = dirs.get(dir).cloned().unwrap_or_default();
        let dir_words = dir.to_lowercase().split(|c: char| !c.is_ascii_lowercase()).filter(|w| !w.is_empty()).map(|s| s.to_string()).collect::<BTreeSet<_>>();
        for kw in keywords {
            if dir_words.contains(kw) { continue; }
            let name = format!("{kw}.py");
            let path = format!("{dir}/{name}");
            if !files.contains(&path) {
                suggested.push(canon::obj(vec![
                    ("path", Value::string(path)),
                    ("keyword", Value::string(kw.clone())),
                    ("directory", Value::string(dir.clone())),
                    ("inferred_from", Value::Array(siblings.iter().take(3).cloned().map(Value::String).collect())),
                ]));
            }
        }
    }
    suggested.truncate(8);
    Some(canon::obj(vec![
        ("creation_intent", Value::Bool(true)),
        ("goal_entities", Value::Array(keywords.iter().cloned().map(Value::String).collect())),
        ("relevant_directories", Value::Array(relevant.into_iter().map(Value::String).collect())),
        ("suggested_files", Value::Array(suggested)),
    ]))
}

pub fn graph_map(value: &Value) -> BTreeMap<String, Vec<String>> {
    graph_from(Some(value))
}

pub fn analysis_language(root: &Path) -> String {
    analyze::detect(root, &config::load(root))
}
