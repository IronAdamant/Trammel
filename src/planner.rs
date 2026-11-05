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
    // Directories the goal names ("Sepulcra" -> games/sepulcra). An explicit
    // scope wins over them.
    let anchors = if scope.as_deref().map(|s| s.is_empty()).unwrap_or(true) { goal_anchors(&request.root, &keywords) } else { Vec::new() };
    if request.strict_greenfield && goal::has_creation_intent(&request.goal) && goal::extract_paths_from_goal(&request.goal).is_empty() {
        return Err("the goal asks to create files but names no file path, and strict_greenfield is set".into());
    }
    let mut max_files = request.max_files.or(cfg.max_files);
    let expand = request.expand_repo.unwrap_or(true);
    if expand && max_files.map(|n| n > 15).unwrap_or(true) {
        max_files = Some(15);
    }
    // The language of the code the goal names, when it names a directory:
    // a Rust crate inside a mostly-Python repo is planned as Rust.
    let language = request.language.clone().unwrap_or_else(|| match anchors.first() {
        Some(anchor) => analyze::detect(&request.root.join(anchor), &cfg),
        None => analyze::detect(&analysis_root, &cfg),
    });
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
            order = keep_most_relevant(order, limit as usize, &symbols, &relevant, &keywords, &anchors);
            truncated = true;
            let allowed: BTreeSet<String> = order.iter().cloned().collect();
            symbols.retain(|k, _| allowed.contains(k));
            relevant.retain(|k, _| allowed.contains(k));
            for deps in relevant.values_mut() { deps.retain(|d| allowed.contains(d)); }
            all = symbols.keys().cloned().chain(relevant.keys().cloned()).collect();
        }
    }
    let mut steps = score::generate_steps(&order, &symbols, &relevant, &request.goal, &keywords, &anchors);
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
    if !anchors.is_empty() { canon::insert(&mut meta, "anchors", Value::Array(anchors.iter().cloned().map(Value::String).collect())); }
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
        if let Some(hints) = creation_hints(&request.goal, &keywords, &all, &language) {
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

/// Cut `order` to `limit` files. With goal keywords, keep the files with the
/// highest step relevance (ties by position) and leave the rest in dependency
/// order. Files without symbols produce no step, so they go last. Without
/// keywords, keep the first `limit`, as Python's `core.py` did.
fn keep_most_relevant(order: Vec<String>, limit: usize, symbols: &BTreeMap<String, Vec<String>>, graph: &BTreeMap<String, Vec<String>>, keywords: &BTreeSet<String>, anchors: &[String]) -> Vec<String> {
    if keywords.is_empty() {
        return order.into_iter().take(limit).collect();
    }
    let (indeg, max_in) = score::dep_indegree(graph);
    let mut ranked: Vec<(f64, usize)> = order.iter().enumerate().map(|(i, file)| {
        let names = symbols.get(file).map(|v| v.as_slice()).unwrap_or(&[]);
        let rank = if names.is_empty() { -1.0 } else { score::relevance_components(file, names, keywords, &indeg, max_in, anchors).2 };
        (rank, i)
    }).collect();
    ranked.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.cmp(&b.1)));
    let mut keep: Vec<usize> = ranked.into_iter().take(limit).map(|(_, i)| i).collect();
    keep.sort_unstable();
    keep.into_iter().map(|i| order[i].clone()).collect()
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

pub fn summary(result: &Value) -> Value {
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

/// Directories (relative, `/`-separated) whose name, or whose Cargo.toml /
/// package.json package name, is one of the goal's words. Nested matches
/// under an earlier one are dropped. Ignored and hidden directories are
/// skipped, and the walk stops six levels down.
pub fn goal_anchors(root: &Path, keywords: &BTreeSet<String>) -> Vec<String> {
    let variants = goal::keyword_variants(keywords);
    let mut found: Vec<String> = Vec::new();
    let mut queue = vec![(root.to_path_buf(), String::new(), 0usize)];
    while let Some((dir, rel, depth)) = queue.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        let mut subdirs: Vec<(String, PathBuf)> = entries.flatten().filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false)).map(|e| (e.file_name().to_string_lossy().into_owned(), e.path())).collect();
        subdirs.sort();
        for (name, path) in subdirs.into_iter().rev() {
            if name.starts_with('.') || crate::walk::is_ignored_dir(&name) { continue; }
            let child = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
            let package = package_name(&path);
            let hit = variants.contains(&name.to_lowercase()) || package.map(|p| variants.contains(&p)).unwrap_or(false);
            if hit && !found.iter().any(|f| child.starts_with(&format!("{f}/"))) {
                found.push(child.clone());
            }
            if depth < 6 { queue.push((path, child, depth + 1)); }
        }
    }
    found.sort();
    found
}

/// The package name a Cargo.toml or package.json in `dir` declares, lowercased
/// (an npm scope is dropped).
fn package_name(dir: &Path) -> Option<String> {
    if let Ok(text) = std::fs::read_to_string(dir.join("Cargo.toml")) {
        let mut in_package = false;
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('[') { in_package = line == "[package]"; continue; }
            if in_package {
                if let Some(value) = line.strip_prefix("name").map(str::trim).and_then(|r| r.strip_prefix('=')) {
                    return Some(value.trim().trim_matches('"').to_lowercase());
                }
            }
        }
    }
    let text = std::fs::read_to_string(dir.join("package.json")).ok()?;
    let doc = crate::json::parse(&text).ok()?;
    let name = canon::get(&doc, "name").and_then(canon::as_str)?;
    Some(name.rsplit('/').next().unwrap_or(name).to_lowercase())
}

/// The file extension new code in `dir` should use: the most common one among
/// its files, else the language's own.
fn extension_for(siblings: &[String], language: &str) -> String {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for name in siblings {
        if let Some((_, ext)) = name.rsplit_once('.') { *counts.entry(ext.to_string()).or_default() += 1; }
    }
    if let Some((ext, _)) = counts.into_iter().max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0))) {
        return ext;
    }
    analyze::extensions(language).first().map(|e| e.trim_start_matches('.').to_string()).unwrap_or_else(|| "txt".into())
}

fn creation_hints(goal: &str, keywords: &BTreeSet<String>, files: &BTreeSet<String>, language: &str) -> Option<Value> {
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
        let ext = extension_for(&siblings, language);
        for kw in keywords {
            if dir_words.contains(kw) { continue; }
            let name = format!("{kw}.{ext}");
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

/// Decompose, fan out beams, and run the test command once per beam.
/// This is the CLI path that matches Python `plan_and_execute`.
pub fn plan_and_execute(request: &Request, store_dir: Option<&Path>, test_cmd: Option<&[String]>) -> Result<Value, String> {
    let strategy = decompose(request)?;
    let steps = canon::get(&strategy, "steps").and_then(canon::as_array).unwrap_or(&[]).to_vec();
    let graph = graph_from(Some(&graph_value_from_strategy(&strategy)));
    let beams = strategies::explore(&steps, &graph, request.num_beams.max(1), &BTreeMap::new());
    // Like 3.x: the run is a plan; each beam's outcome becomes a trajectory
    // (`history`), and the plan ends completed or failed.
    let plan_id = match store_dir {
        Some(dir) => {
            let mut store = crate::store::Store::open(dir)?;
            let id = store.create_plan(&request.goal, &strategy);
            store.update_plan_status(id, "running");
            store.save()?;
            Some(id)
        }
        None => None,
    };
    let cmd = test_cmd.map(|c| c.to_vec()).filter(|c| !c.is_empty()).unwrap_or_else(|| {
        analyze::default_test_cmd(&request.root, request.language.as_deref())
    });
    let mut outcomes = Vec::with_capacity(beams.len());
    std::thread::scope(|scope| {
        let mut joins = Vec::new();
        for _ in &beams {
            let root = request.root.clone();
            let cmd = cmd.clone();
            joins.push(scope.spawn(move || {
                crate::harness::verify(&crate::harness::VerifyRequest {
                    edits: Vec::new(),
                    root,
                    test_cmd: cmd,
                    timeout: std::time::Duration::from_secs(60),
                    full_output: false,
                    cap: crate::harness::default_cap(),
                })
            }));
        }
        for join in joins {
            outcomes.push(join.join().unwrap_or_else(|_| Value::string("beam thread failed")));
        }
    });
    let success = outcomes.iter().all(|item| canon::get(item, "success").and_then(canon::as_bool).unwrap_or(false));
    if let (Some(dir), Some(id)) = (store_dir, plan_id) {
        let mut store = crate::store::Store::open(dir)?;
        for (beam, outcome) in beams.iter().zip(&outcomes) {
            let passed = canon::get(outcome, "success").and_then(canon::as_bool).unwrap_or(false);
            let steps_done = canon::get(beam, "steps").and_then(canon::as_array).map(|s| s.len() as i64).unwrap_or(0);
            let reason = if passed { None } else { canon::get(outcome, "trace").and_then(canon::as_str).and_then(|t| t.lines().map(str::trim).find(|l| !l.is_empty() && !l.chars().all(|c| c == '-' || c == '='))) };
            let variant = canon::get(beam, "variant").and_then(canon::as_str).unwrap_or("default");
            let beam_id = canon::get(beam, "beam_id").and_then(canon::as_i64).unwrap_or(0);
            store.log_trajectory(id, beam_id, variant, if passed { steps_done } else { 0 }, passed, reason);
        }
        if success { store.save_recipe(&request.goal, &strategy, true); }
        store.update_plan_status(id, if success { "completed" } else { "failed" });
        store.save()?;
    }
    Ok(canon::obj(vec![
        ("goal", Value::string(request.goal.clone())),
        ("plan_id", plan_id.map(Value::Int).unwrap_or(Value::Null)),
        ("strategy", strategy),
        ("beams", Value::Int(beams.len() as i64)),
        ("success", Value::Bool(success)),
        ("outcomes", Value::Array(outcomes)),
    ]))
}

fn graph_value_from_strategy(strategy: &Value) -> Value {
    canon::get(strategy, "dependency_graph").cloned().unwrap_or(Value::Object(vec![]))
}

#[cfg(test)]
mod cap_tests {
    use super::*;

    #[test]
    fn the_file_cap_keeps_the_files_the_goal_is_about() {
        let dir = std::env::temp_dir().join(format!("trammel-cap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..20 {
            std::fs::write(dir.join(format!("a_{i:02}.py")), format!("def helper_{i}():\n    return {i}\n")).unwrap();
        }
        std::fs::write(dir.join("zz_billing.py"), "def charge_invoice(total):\n    return total\n").unwrap();
        let request = Request::new("fix invoice charging in billing", dir.clone());
        let result = decompose(&request).unwrap();
        let files: Vec<String> = canon::get(&result, "steps").and_then(canon::as_array).unwrap().iter().filter_map(|s| canon::get(s, "file").and_then(canon::as_str).map(String::from)).collect();
        assert_eq!(files.len(), 15);
        assert!(files.contains(&"zz_billing.py".to_string()), "{files:?}");
        assert_eq!(canon::get(&result, "analysis_meta").and_then(|m| canon::get(m, "max_files_truncated")).and_then(canon::as_bool), Some(true));
        // Without keywords the cap keeps the first files in dependency order.
        let plain = keep_most_relevant((0..20).map(|i| format!("f{i:02}")).collect::<Vec<String>>(), 3, &BTreeMap::new(), &BTreeMap::new(), &BTreeSet::new(), &[]);
        assert_eq!(plain, vec!["f00", "f01", "f02"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod run_tests {
    use super::*;

    #[test]
    fn the_goal_command_records_a_plan_history_and_closes_the_plan() {
        let dir = std::env::temp_dir().join(format!("trammel-run-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("app.py"), "def main():\n    return 1\n").unwrap();
        let store = dir.join(".trammel");
        let mut request = Request::new("update main in app", dir.clone());
        request.num_beams = 2;
        let pass = crate::harness::no_op_cmd();
        let result = plan_and_execute(&request, Some(&store), Some(&pass)).unwrap();
        let id = canon::get(&result, "plan_id").and_then(canon::as_i64).unwrap();
        let fail: Vec<String> = if cfg!(windows) { vec!["cmd".into(), "/C".into(), "echo boom 1>&2 & exit 3".into()] } else { vec!["sh".into(), "-c".into(), "echo boom >&2; exit 3".into()] };
        let failed = plan_and_execute(&request, Some(&store), Some(&fail)).unwrap();
        let failed_id = canon::get(&failed, "plan_id").and_then(canon::as_i64).unwrap();
        let store = crate::store::Store::open(&store).unwrap();
        let plan = store.get_plan(id).unwrap();
        assert_eq!(canon::get(&plan, "status").and_then(canon::as_str), Some("completed"));
        let runs = store.trajectories(id);
        assert_eq!(runs.len(), 2);
        assert!(runs.iter().all(|t| canon::get(t, "success").and_then(canon::as_bool) == Some(true)));
        assert_eq!(canon::get(&store.get_plan(failed_id).unwrap(), "status").and_then(canon::as_str), Some("failed"));
        let reason = store.trajectories(failed_id).first().and_then(|t| canon::get(t, "failure_reason").and_then(canon::as_str).map(String::from));
        assert_eq!(reason.as_deref(), Some("boom"));
        assert!(store.best_recipe("update main in app", 0.55).is_some());
        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod anchor_tests {
    use super::*;

    fn write(root: &Path, rel: &str, text: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// A workspace where a shared helper is imported everywhere and the goal
    /// names one game by its directory.
    fn workspace(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("trammel-anchor-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        write(&dir, "shared/util.py", "def decorate(x):\n    return x\n\ndef per_item(x):\n    return x\n");
        for i in 0..12 {
            write(&dir, &format!("engine/mod_{i:02}.py"), &format!("from shared.util import decorate\n\ndef upper_{i}():\n    return decorate({i})\n"));
        }
        write(&dir, "games/sepulcra/map.py", "from shared.util import decorate\n\ndef buildable(cell):\n    return cell\n");
        write(&dir, "games/sepulcra/content.py", "class PropDef:\n    pass\n");
        write(&dir, "games/other/engine.py", "def tick():\n    return 0\n");
        dir
    }

    #[test]
    fn a_goal_that_names_a_directory_plans_inside_it() {
        let dir = workspace("rank");
        let request = Request::new("add a per-prop blocks_build flag to Sepulcra props: others are decoration only", dir.clone());
        let result = decompose(&request).unwrap();
        let meta = canon::get(&result, "analysis_meta").unwrap();
        assert_eq!(canon::stable(canon::get(meta, "anchors").unwrap()), r#"["games/sepulcra"]"#);
        let mut steps: Vec<(f64, String)> = canon::get(&result, "steps").and_then(canon::as_array).unwrap().iter()
            .map(|s| (canon::get(s, "relevance").and_then(canon::as_f64).unwrap_or(0.0), canon::get(s, "file").and_then(canon::as_str).unwrap().to_string()))
            .collect();
        steps.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        let top: Vec<&str> = steps.iter().take(2).map(|(_, f)| f.as_str()).collect();
        assert!(top.contains(&"games/sepulcra/map.py") && top.contains(&"games/sepulcra/content.py"), "{steps:?}");
        // The helper everything imports no longer outranks the named code.
        let helper = steps.iter().find(|(_, f)| f == "shared/util.py").map(|(r, _)| *r).unwrap_or(0.0);
        assert!(helper < steps[1].0, "{steps:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn anchors_match_directory_and_package_names() {
        let dir = workspace("names");
        write(&dir, "crates/eng/Cargo.toml", "[package]\nname = \"pathfinder\"\nversion = \"0.1.0\"\n");
        let words = |s: &str| goal::extract_goal_keywords(s);
        assert_eq!(goal_anchors(&dir, &words("speed up the pathfinder")), vec!["crates/eng"]);
        assert_eq!(goal_anchors(&dir, &words("fix Sepulcra props")), vec!["games/sepulcra"]);
        assert!(goal_anchors(&dir, &words("rename a helper")).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn identifier_parts_stay_keywords_and_new_files_follow_their_neighbours() {
        let keywords = goal::extract_goal_keywords("add a per-prop blocks_build flag; others are marked");
        assert!(keywords.contains("build") && keywords.contains("blocks") && keywords.contains("prop"), "{keywords:?}");
        assert!(!keywords.contains("per") && !keywords.contains("others"), "{keywords:?}");
        let dir = std::env::temp_dir().join(format!("trammel-hints-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        write(&dir, "Cargo.toml", "[package]\nname = \"sepulcra\"\nversion = \"0.1.0\"\n");
        write(&dir, "src/lib.rs", "pub mod map;\n");
        write(&dir, "src/map.rs", "pub fn buildable() -> bool { true }\n");
        let request = Request::new("create a props module for the map in src", dir.clone());
        let result = decompose(&request).unwrap();
        let hints = canon::stable(canon::get(&result, "creation_hints").unwrap());
        assert!(hints.contains("src/props.rs"), "{hints}");
        assert!(!hints.contains(".py"), "{hints}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
