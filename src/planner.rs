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
    /// Files the caller names (relative to the project root). The goal's own
    /// paths and module names are added to these.
    pub files: Vec<String>,
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
            files: Vec::new(),
        }
    }
}

pub fn decompose(request: &Request) -> Result<Value, String> {
    let started = Instant::now();
    let cfg = if request.apply_project_config { config::load(&request.root) } else { ProjectConfig::default() };
    let scope = request.scope.clone().or_else(|| cfg.default_scope.clone());
    let keywords = config::merge_keywords(goal::extract_goal_keywords(&goal::keyword_text(&request.goal)), &request.focus_keywords, &cfg);
    let mut globs = request.focus_globs.clone();
    globs.extend(cfg.focus_globs.clone());
    let analysis_root = match &scope {
        Some(scope) if !scope.is_empty() => request.root.join(scope),
        _ => request.root.clone(),
    };
    // Directories the goal names ("Sepulcra" -> games/sepulcra). An explicit
    // scope wins over them. A named file's own stem does not anchor: "game.rs"
    // is not about a games/ directory.
    let anchor_words = config::merge_keywords(goal::extract_goal_keywords(&goal::anchor_text(&request.goal)), &request.focus_keywords, &cfg);
    let anchors = if scope.as_deref().map(|s| s.is_empty()).unwrap_or(true) { goal_anchors(&request.root, &anchor_words) } else { Vec::new() };
    if request.strict_greenfield && goal::has_creation_intent(&request.goal) && goal::extract_paths_from_goal(&request.goal).is_empty() {
        return Err("the goal asks to create files but names no file path, and strict_greenfield is set".into());
    }
    let refs = goal::named_refs(&request.goal);
    let asked_cap = request.max_files.or(cfg.max_files);
    let mut max_files = asked_cap;
    let expand = request.expand_repo.unwrap_or(true);
    if expand && max_files.map(|n| n > 15).unwrap_or(true) {
        max_files = Some(15);
    }
    // The language of the code the goal names: a named file's extension,
    // else a named directory's contents. A Rust crate inside a mostly-Python
    // repo is planned as Rust.
    let named_language = refs.paths.iter().chain(request.files.iter()).find_map(|p| language_of(p));
    let language = request.language.clone().or(named_language).unwrap_or_else(|| match anchors.first() {
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
    let about_tests = goal_is_about_tests(&keywords);
    let named = resolve_named(&refs, &request.files, &all, scope.as_deref(), &analysis_root, &language, &anchors, &anchor_words);
    let known = all.clone();
    let mut roles: BTreeMap<String, &'static str> = BTreeMap::new();
    let mut truncated = false;
    let order: Vec<String> = if !named.seeds.is_empty() {
        // The goal names its files: plan those, plus a few direct
        // dependencies, dependents and tests ranked below them.
        // Neighbours come from real imports only: inferred edges link a
        // hub file to hundreds of others.
        let picked = seeded_plan(&named.seeds, &graph, &symbols, &keywords, &anchors, asked_cap);
        let (tests, core): (Vec<String>, Vec<String>) = picked.iter().map(|(f, _)| f.clone()).partition(|f| score::is_test_file(f));
        roles = picked.into_iter().collect();
        let mut order = dependency_order(&core, &graph);
        order.extend(dependency_order(&tests, &graph));
        let allowed: BTreeSet<String> = order.iter().cloned().collect();
        symbols.retain(|k, _| allowed.contains(k));
        relevant = graph.clone();
        relevant.retain(|k, _| allowed.contains(k));
        for deps in relevant.values_mut() { deps.retain(|d| allowed.contains(d)); }
        order
    } else {
        let mut order = topological_sort(&relevant);
        order.retain(|f| symbols.contains_key(f));
        for file in symbols.keys() {
            if !order.contains(file) { order.push(file.clone()); }
        }
        if let Some(limit) = max_files {
            if limit >= 0 && order.len() > limit as usize {
                order = keep_most_relevant(order, limit as usize, &symbols, &relevant, &keywords, &anchors, about_tests);
                truncated = true;
                let allowed: BTreeSet<String> = order.iter().cloned().collect();
                symbols.retain(|k, _| allowed.contains(k));
                relevant.retain(|k, _| allowed.contains(k));
                for deps in relevant.values_mut() { deps.retain(|d| allowed.contains(d)); }
                all = symbols.keys().cloned().chain(relevant.keys().cloned()).collect();
            }
        }
        // Code first in dependency order, then the tests.
        let (tests, core): (Vec<String>, Vec<String>) = order.into_iter().partition(|f| score::is_test_file(f));
        for file in &core { roles.insert(file.clone(), "core"); }
        for file in &tests { roles.insert(file.clone(), "test"); }
        core.into_iter().chain(tests).collect()
    };
    let mut steps = score::generate_steps(&order, &symbols, &relevant, &request.goal, &keywords, &anchors, &roles);
    rank_by_role(&mut steps, !named.seeds.is_empty(), about_tests);
    if request.relevant_only && (!keywords.is_empty() || !named.seeds.is_empty()) {
        // Most relevant first, still code before tests.
        steps.sort_by(|a, b| {
            let ta = step_is_test(a);
            let tb = step_is_test(b);
            let ra = canon::get(a, "relevance").and_then(canon::as_f64).unwrap_or(0.0);
            let rb = canon::get(b, "relevance").and_then(canon::as_f64).unwrap_or(0.0);
            ta.cmp(&tb).then(rb.partial_cmp(&ra).unwrap_or(std::cmp::Ordering::Equal))
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
    named.annotate(&mut meta);
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
        let ranked = canon::get(&result, "steps").and_then(canon::as_array).map(|s| s.to_vec()).unwrap_or_default();
        let files = if named.seeds.is_empty() { &all } else { &known };
        if let Some(hints) = creation_hints(&request.goal, &keywords, files, &language, &ranked) {
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
fn keep_most_relevant(order: Vec<String>, limit: usize, symbols: &BTreeMap<String, Vec<String>>, graph: &BTreeMap<String, Vec<String>>, keywords: &BTreeSet<String>, anchors: &[String], about_tests: bool) -> Vec<String> {
    if keywords.is_empty() {
        return order.into_iter().take(limit).collect();
    }
    let (indeg, max_in) = score::dep_indegree(graph);
    let mut ranked: Vec<(f64, usize)> = order.iter().enumerate().map(|(i, file)| {
        let names = symbols.get(file).map(|v| v.as_slice()).unwrap_or(&[]);
        let mut rank = if names.is_empty() { -1.0 } else { score::relevance_components(file, names, keywords, &indeg, max_in, anchors).2 };
        // Test files fill the cap last unless the goal is about tests.
        if rank > 0.0 && !about_tests && score::is_test_file(file) { rank *= 0.5; }
        (rank, i)
    }).collect();
    ranked.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.cmp(&b.1)));
    let mut keep: Vec<usize> = ranked.into_iter().take(limit).map(|(_, i)| i).collect();
    keep.sort_unstable();
    keep.into_iter().map(|i| order[i].clone()).collect()
}

/// The language a file extension belongs to, for a file the goal names.
fn language_of(path: &str) -> Option<String> {
    let ext = path.rsplit_once('.')?.1.to_ascii_lowercase();
    let lang = match ext.as_str() {
        "rs" => "rust",
        "py" => "python",
        "go" => "go",
        "ts" | "tsx" | "mts" => "typescript",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" => "cpp",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "cs" => "csharp",
        "rb" => "ruby",
        "php" => "php",
        "swift" => "swift",
        "dart" => "dart",
        "zig" => "zig",
        _ => return None,
    };
    Some(lang.to_string())
}

/// True when the goal is about tests themselves ("add tests for ...").
fn goal_is_about_tests(keywords: &BTreeSet<String>) -> bool {
    keywords.iter().any(|k| matches!(k.as_str(), "test" | "tests" | "testing" | "spec" | "specs" | "coverage"))
}

fn step_is_test(step: &Value) -> bool {
    matches!(canon::get(step, "role").and_then(canon::as_str), Some("test") | Some("named_test"))
}

/// The files a goal names, resolved against the analyzed files.
#[derive(Default)]
struct Named {
    seeds: Vec<String>,
    unresolved: Vec<String>,
    ambiguous: Vec<(String, Vec<String>)>,
}

impl Named {
    fn annotate(&self, meta: &mut Value) {
        if !self.seeds.is_empty() {
            canon::insert(meta, "plan_source", Value::string("named_files"));
            canon::insert(meta, "named_files", Value::Array(self.seeds.iter().cloned().map(Value::String).collect()));
        }
        if !self.unresolved.is_empty() {
            canon::insert(meta, "unresolved_files", Value::Array(self.unresolved.iter().cloned().map(Value::String).collect()));
        }
        if !self.ambiguous.is_empty() {
            canon::insert(meta, "ambiguous_files", Value::Object(self.ambiguous.iter().map(|(k, v)| (k.clone(), Value::Array(v.iter().cloned().map(Value::String).collect()))).collect()));
        }
        if !self.unresolved.is_empty() || !self.ambiguous.is_empty() {
            canon::insert(meta, "named_files_hint", Value::string("Pass files as paths relative to project_root (or to scope) to plan from them exactly"));
        }
    }
}

/// One thing the goal or the caller names, and the files it may mean.
struct Reference {
    label: String,
    candidates: Vec<String>,
    /// Say so in the reply when it matches no file or several.
    report: bool,
}

/// Number of leading directory names two paths share.
fn shared_dirs(a: &str, b: &str) -> usize {
    let da: Vec<&str> = a.split('/').collect();
    let db: Vec<&str> = b.split('/').collect();
    da[..da.len().saturating_sub(1)].iter().zip(&db[..db.len().saturating_sub(1)]).take_while(|(x, y)| x == y).count()
}

/// Match the goal's paths, file names, Rust module paths and module names
/// (plus `files`) to analyzed files. A name that fits several files is
/// settled by the files already matched (same crate or directory), then by
/// the goal's anchors and words; otherwise it is reported as ambiguous.
#[allow(clippy::too_many_arguments)]
fn resolve_named(refs: &goal::NamedRefs, explicit: &[String], files: &BTreeSet<String>, scope: Option<&str>, analysis_root: &Path, language: &str, anchors: &[String], keywords: &BTreeSet<String>) -> Named {
    let scope_prefix = scope.filter(|s| !s.is_empty()).map(|s| format!("{}/", crate::walk::norm_slash(s)));
    let by_path = |raw: &str| -> Vec<String> {
        let mut q = crate::walk::norm_slash(raw);
        if let Some(prefix) = &scope_prefix {
            if let Some(rest) = q.strip_prefix(prefix.as_str()) { q = rest.to_string(); }
        }
        if q.is_empty() { return Vec::new(); }
        if files.contains(&q) { return vec![q]; }
        let suffix = format!("/{q}");
        let hits: Vec<String> = files.iter().filter(|f| f.ends_with(&suffix)).cloned().collect();
        // "sight.rs" means src/sight.rs more than tests/sight.rs, unless it
        // names a test file itself.
        let code: Vec<String> = hits.iter().filter(|f| !score::is_test_file(f)).cloned().collect();
        if !score::is_test_file(&q) && !code.is_empty() { code } else { hits }
    };
    let mut found: Vec<Reference> = Vec::new();
    for raw in explicit.iter().chain(refs.paths.iter()) {
        let label = crate::walk::norm_slash(raw);
        if found.iter().any(|r| r.label == label) { continue; }
        found.push(Reference { candidates: by_path(raw), label, report: true });
    }
    if language == "rust" && !refs.modules.is_empty() {
        let packages = crate::lang::rust_mods::Packages::find(analysis_root, files);
        for path in &refs.modules {
            let rooted = matches!(path[0].as_str(), "crate" | "self" | "super");
            let segs: Vec<&str> = path.iter().map(|s| s.as_str()).filter(|s| !matches!(*s, "crate" | "self" | "super")).collect();
            let candidates: Vec<String> = match packages.src_of(&path[0]) {
                Some(src) if segs.len() > 1 => crate::lang::rust_mods::module_file(src, &segs[1..], files).into_iter().collect(),
                _ if rooted && !segs.is_empty() => {
                    let mut hits = Vec::new();
                    for depth in (1..=segs.len()).rev() {
                        let joined = segs[..depth].join("/");
                        let (file, dir) = (format!("{joined}.rs"), format!("{joined}/mod.rs"));
                        hits = files.iter().filter(|f| **f == file || f.ends_with(&format!("/{file}")) || **f == dir || f.ends_with(&format!("/{dir}"))).cloned().collect();
                        if !hits.is_empty() { break; }
                    }
                    hits
                }
                _ => Vec::new(),
            };
            found.push(Reference { label: path.join("::"), candidates, report: false });
        }
    }
    for stem in &refs.stems {
        let lower = stem.to_lowercase();
        let candidates: Vec<String> = files.iter().filter(|f| {
            let name = f.rsplit('/').next().unwrap_or(f);
            let file_stem = name.split('.').next().unwrap_or(name).to_lowercase();
            let parent_dir = f.rsplit('/').nth(1).unwrap_or("").to_lowercase();
            file_stem == lower || (parent_dir == lower && matches!(name, "mod.rs" | "__init__.py" | "index.ts" | "index.js"))
        }).cloned().collect();
        let code: Vec<String> = candidates.iter().filter(|f| !score::is_test_file(f)).cloned().collect();
        let candidates = if code.is_empty() || lower.contains("test") { candidates } else { code };
        found.push(Reference { label: stem.clone(), candidates, report: false });
    }
    let mut named = Named::default();
    let mut open: Vec<Reference> = Vec::new();
    for reference in found {
        match reference.candidates.len() {
            1 => {
                let file = reference.candidates[0].clone();
                if !named.seeds.contains(&file) { named.seeds.push(file); }
            }
            0 => if reference.report { named.unresolved.push(reference.label) },
            _ => open.push(reference),
        }
    }
    // Settle names that fit several files, nearest to what is matched first.
    loop {
        let mut progress = false;
        let mut still = Vec::new();
        for reference in open {
            match pick_candidate(&reference.candidates, &named.seeds, anchors, keywords) {
                Some(file) => {
                    if !named.seeds.contains(&file) { named.seeds.push(file); }
                    progress = true;
                }
                None => still.push(reference),
            }
        }
        open = still;
        if !progress || open.is_empty() { break; }
    }
    for reference in open {
        if reference.report { named.ambiguous.push((reference.label, reference.candidates.into_iter().take(5).collect())); }
    }
    named
}

/// The one candidate nearest the matched files, else the one inside an
/// anchor, else the one whose directories name the most goal words.
fn pick_candidate(candidates: &[String], seeds: &[String], anchors: &[String], keywords: &BTreeSet<String>) -> Option<String> {
    let unique_best = |scores: Vec<usize>| -> Option<String> {
        let best = *scores.iter().max()?;
        if best == 0 || scores.iter().filter(|s| **s == best).count() != 1 { return None; }
        scores.iter().position(|s| *s == best).map(|i| candidates[i].clone())
    };
    if !seeds.is_empty() {
        let near: Vec<usize> = candidates.iter().map(|c| seeds.iter().map(|s| shared_dirs(c, s)).max().unwrap_or(0)).collect();
        if let Some(file) = unique_best(near) { return Some(file); }
    }
    let inside: Vec<usize> = candidates.iter().map(|c| anchors.iter().any(|a| c.starts_with(&format!("{a}/"))) as usize).collect();
    if let Some(file) = unique_best(inside) { return Some(file); }
    let variants = goal::keyword_variants(keywords);
    let words: Vec<usize> = candidates.iter().map(|c| {
        let dirs: Vec<&str> = c.split('/').collect();
        dirs[..dirs.len().saturating_sub(1)].iter().filter(|d| variants.contains(&d.to_lowercase())).count()
    }).collect();
    unique_best(words)
}

/// The named files plus their direct neighbours: what they import, what
/// imports them, and tests named after them. Neighbours are ranked by goal
/// relevance, links to named files and nearness, then capped: two per named
/// file (3 to 8) plus 2 to 4 tests. `max_files`, when given, caps the total.
fn seeded_plan(seeds: &[String], graph: &BTreeMap<String, Vec<String>>, symbols: &BTreeMap<String, Vec<String>>, keywords: &BTreeSet<String>, anchors: &[String], cap: Option<i64>) -> Vec<(String, &'static str)> {
    let seed_set: BTreeSet<&str> = seeds.iter().map(|s| s.as_str()).collect();
    // file -> (links to named files, is a dependency of one)
    let mut candidates: BTreeMap<String, (usize, bool)> = BTreeMap::new();
    for seed in seeds {
        for dep in graph.get(seed).into_iter().flatten() {
            if !seed_set.contains(dep.as_str()) {
                candidates.entry(dep.clone()).or_insert((0, true)).0 += 1;
            }
        }
    }
    for (file, deps) in graph {
        if seed_set.contains(file.as_str()) { continue; }
        let links = deps.iter().filter(|d| seed_set.contains(d.as_str())).count();
        if links > 0 { candidates.entry(file.clone()).or_insert((0, false)).0 += links; }
    }
    let seed_stems: BTreeSet<String> = seeds.iter().map(|s| score::tested_stem(s)).collect();
    for file in symbols.keys() {
        if !seed_set.contains(file.as_str()) && score::is_test_file(file) && seed_stems.contains(&score::tested_stem(file)) {
            candidates.entry(file.clone()).or_insert((1, false));
        }
    }
    let (indeg, max_in) = score::dep_indegree(graph);
    let mut core: Vec<(f64, String, bool)> = Vec::new();
    let mut tests: Vec<(f64, String)> = Vec::new();
    for (file, (links, is_dep)) in candidates {
        let Some(names) = symbols.get(&file).filter(|n| !n.is_empty()) else { continue };
        let relevance = if keywords.is_empty() { 0.0 } else { score::relevance_components(&file, names, keywords, &indeg, max_in, anchors).2 };
        let depth = file.split('/').count().max(2) - 1;
        let near = seeds.iter().map(|s| shared_dirs(&file, s)).max().unwrap_or(0) as f64 / depth as f64;
        let rank = relevance + 0.15 * (links.min(4) as f64 - 1.0) + 0.3 * near;
        if score::is_test_file(&file) { tests.push((rank, file)); } else { core.push((rank, file, is_dep)); }
    }
    let by_rank = |a: f64, b: f64| b.partial_cmp(&a).unwrap_or(std::cmp::Ordering::Equal);
    core.sort_by(|a, b| by_rank(a.0, b.0).then(a.1.cmp(&b.1)));
    tests.sort_by(|a, b| by_rank(a.0, b.0).then(a.1.cmp(&b.1)));
    let mut core_cap = (2 * seeds.len()).clamp(3, 8);
    let mut test_cap = seeds.len().clamp(2, 4);
    if let Some(cap) = cap.filter(|c| *c >= 0) {
        let room = (cap as usize).saturating_sub(seeds.len());
        core_cap = core_cap.min(room);
        test_cap = test_cap.min(room - core_cap.min(core.len()));
    }
    let mut out: Vec<(String, &'static str)> = seeds.iter().map(|s| (s.clone(), if score::is_test_file(s) { "named_test" } else { "named" })).collect();
    // Alternate between what the named files use and what uses them, so a
    // widely imported change still lists its callers.
    let (deps, users): (Vec<_>, Vec<_>) = core.into_iter().partition(|c| c.2);
    let (mut deps, mut users) = (deps.into_iter(), users.into_iter());
    let mut taken = 0;
    while taken < core_cap {
        let mut moved = false;
        for (_, file, is_dep) in [users.next(), deps.next()].into_iter().flatten() {
            if taken < core_cap {
                out.push((file, if is_dep { "dependency" } else { "dependent" }));
                taken += 1;
                moved = true;
            }
        }
        if !moved { break; }
    }
    out.extend(tests.into_iter().take(test_cap).map(|(_, f)| (f, "test")));
    out
}

/// `files` in dependency order (what a file imports comes before it), using
/// only the edges among them.
fn dependency_order(files: &[String], graph: &BTreeMap<String, Vec<String>>) -> Vec<String> {
    let set: BTreeSet<&String> = files.iter().collect();
    let sub: BTreeMap<String, Vec<String>> = files.iter().map(|f| {
        let deps = graph.get(f).map(|d| d.iter().filter(|x| set.contains(x) && *x != f).cloned().collect()).unwrap_or_default();
        (f.clone(), deps)
    }).collect();
    topological_sort(&sub)
}

/// Relevance that says what each step is for. Named files are 1.0. Their
/// neighbours keep their goal relevance scaled into 0.2-0.6, and their tests
/// into 0.1-0.4, so a neighbour never looks as important as a named file.
/// Without named files, tests are halved unless the goal is about tests.
fn rank_by_role(steps: &mut [Value], seeded: bool, about_tests: bool) {
    for step in steps.iter_mut() {
        let role = canon::get(step, "role").and_then(canon::as_str).unwrap_or("").to_string();
        let r = canon::get(step, "relevance").and_then(canon::as_f64);
        let next = match role.as_str() {
            "named" | "named_test" => Some(1.0),
            "dependency" | "dependent" => Some(0.2 + 0.4 * r.unwrap_or(0.0)),
            "test" if seeded => Some(0.1 + 0.3 * r.unwrap_or(0.0)),
            "test" if !about_tests => r.map(|r| r * 0.5),
            _ => None,
        };
        if let Some(value) = next { score::set_relevance(step, value); }
    }
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
    // "src", "tests", "crates" say where code lives, not what the goal is about.
    let variants: BTreeSet<String> = goal::keyword_variants(keywords).into_iter().filter(|v| !goal::GENERIC_DIRS.contains(&v.as_str())).collect();
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
            let by_package = package.as_ref().map(|p| variants.contains(p)).unwrap_or(false);
            let hit = by_package || (variants.contains(&name.to_lowercase()) && (package.is_some() || has_code(&path, 3)));
            if hit && !found.iter().any(|f| child.starts_with(&format!("{f}/"))) {
                found.push(child.clone());
            }
            if depth < 6 { queue.push((path, child, depth + 1)); }
        }
    }
    found.sort();
    found
}

/// True when `dir`, or a directory up to `depth` levels below it, holds a
/// source file. A folder of assets named like a goal word is no anchor.
fn has_code(dir: &Path, depth: usize) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else { return false };
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            if !name.starts_with('.') && !crate::walk::is_ignored_dir(&name) { subdirs.push(entry.path()); }
        } else if language_of(&name).is_some() {
            return true;
        }
    }
    depth > 0 && subdirs.iter().any(|d| has_code(d, depth - 1))
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

/// What a goal that adds something should touch. New files are suggested
/// only when the goal clearly asks for one (`goal::new_file_names`, or a
/// path in the goal that does not exist yet), in the directory of the most
/// relevant step, at most three. Otherwise `suggested_files` is empty and
/// `suggested_edits` names the existing files to change.
fn creation_hints(goal: &str, keywords: &BTreeSet<String>, files: &BTreeSet<String>, language: &str, steps: &[Value]) -> Option<Value> {
    if !goal::has_creation_intent(goal) { return None; }
    let mut by_relevance: Vec<(f64, String)> = steps.iter()
        .filter_map(|s| Some((canon::get(s, "relevance").and_then(canon::as_f64).unwrap_or(0.0), canon::get(s, "file").and_then(canon::as_str)?.to_string())))
        .collect();
    by_relevance.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
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
    // Paths written in the goal that do not exist yet.
    for path in goal::extract_paths_from_goal(goal) {
        if !files.contains(&path) && !suggested.iter().any(|s: &Value| canon::get(s, "path").and_then(canon::as_str) == Some(path.as_str())) {
            suggested.push(canon::obj(vec![("path", Value::string(path)), ("from", Value::string("goal"))]));
        }
    }
    // Named new modules, pages, scripts... next to the code they belong with.
    let names = goal::new_file_names(goal);
    let home = by_relevance.first().and_then(|(_, f)| f.rsplit_once('/').map(|(d, _)| d.to_string()))
        .or_else(|| relevant.first().cloned());
    if let Some(dir) = home.filter(|_| !names.is_empty()) {
        let siblings = dirs.get(&dir).cloned().unwrap_or_default();
        let ext = extension_for(&siblings, language);
        for name in names {
            let path = format!("{dir}/{name}.{ext}");
            if !files.contains(&path) {
                suggested.push(canon::obj(vec![
                    ("path", Value::string(path)),
                    ("name", Value::string(name)),
                    ("directory", Value::string(dir.clone())),
                    ("inferred_from", Value::Array(siblings.iter().take(3).cloned().map(Value::String).collect())),
                ]));
            }
        }
    }
    suggested.truncate(3);
    let mut out = canon::obj(vec![
        ("creation_intent", Value::Bool(true)),
        ("goal_entities", Value::Array(keywords.iter().cloned().map(Value::String).collect())),
        ("relevant_directories", Value::Array(relevant.into_iter().map(Value::String).collect())),
        ("suggested_files", Value::Array(suggested.clone())),
    ]);
    if suggested.is_empty() {
        let edits: Vec<Value> = by_relevance.iter().take(3).map(|(_, f)| Value::string(f.clone())).collect();
        canon::insert(&mut out, "suggested_edits", Value::Array(edits));
    }
    Some(out)
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
    let timeout = std::time::Duration::from_secs(config::load(&request.root).verify_timeout_s.unwrap_or(crate::harness::DEFAULT_TIMEOUT_S).min(crate::harness::MAX_JOB_TIMEOUT_S));
    let mut outcomes = Vec::with_capacity(beams.len());
    std::thread::scope(|scope| {
        let mut joins = Vec::new();
        for _ in &beams {
            let root = request.root.clone();
            let cmd = cmd.clone();
            joins.push(scope.spawn(move || {
                crate::harness::verify(&crate::harness::VerifyRequest::new(root, cmd, timeout))
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
        let plain = keep_most_relevant((0..20).map(|i| format!("f{i:02}")).collect::<Vec<String>>(), 3, &BTreeMap::new(), &BTreeMap::new(), &BTreeSet::new(), &[], false);
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
    fn new_files_only_when_the_goal_asks_for_one() {
        assert_eq!(goal::new_file_names("create a props module for the map"), vec!["props"]);
        assert_eq!(goal::new_file_names("add a new settings page"), vec!["settings"]);
        assert_eq!(goal::new_file_names("add a module called retry-policy"), vec!["retry_policy"]);
        assert!(goal::new_file_names("add a per-prop blocks_build flag to Sepulcra props: others are decoration only").is_empty());
        assert!(goal::new_file_names("update the props module").is_empty(), "no creation verb");
        let dir = workspace("noise");
        let request = Request::new("add a per-prop blocks_build flag to Sepulcra props: props marked blocks_build stop building on their tiles, others are decoration only", dir.clone());
        let result = decompose(&request).unwrap();
        let hints = canon::get(&result, "creation_hints").unwrap();
        assert_eq!(canon::stable(canon::get(hints, "suggested_files").unwrap()), "[]");
        let edits = canon::stable(canon::get(hints, "suggested_edits").unwrap());
        assert!(edits.contains("games/sepulcra/"), "{edits}");
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

#[cfg(test)]
mod named_tests {
    use super::*;

    fn write(root: &Path, rel: &str, text: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// A Rust workspace shaped like Specula's timeline lane: the goal names
    /// sight.rs and game.rs, and many unrelated tool modules exist.
    fn workspace(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("trammel-named-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        write(&dir, "Cargo.toml", "[workspace]\nmembers = [\"crates/app\", \"games/arena\"]\n");
        write(&dir, "crates/app/Cargo.toml", "[package]\nname = \"app\"\n");
        let mut lib = String::from("pub mod sight;\npub mod game;\npub mod timeline;\n");
        for name in ["anim_tools", "gi_tools", "perf", "studio", "camera_tools", "playback_tools"] {
            lib.push_str(&format!("pub mod {name};\n"));
            write(&dir, &format!("crates/app/src/{name}.rs"), &format!("use crate::game::Game;\npub fn {name}_timeline_camera(g: &Game) {{ let _ = g; }}\n"));
        }
        write(&dir, "crates/app/src/lib.rs", &lib);
        write(&dir, "crates/app/src/sight.rs", "pub struct Camera { pub fov: f32 }\n");
        write(&dir, "crates/app/src/game.rs", "use crate::sight::Camera;\npub struct Game { pub camera: Camera }\n");
        write(&dir, "crates/app/src/timeline.rs", "use crate::game::Game;\npub fn play_timeline(g: &mut Game) { let _ = g; }\n");
        write(&dir, "crates/app/tests/sight.rs", "use app::sight::Camera;\n#[test]\nfn camera_fov() { let _ = Camera { fov: 1.0 }; }\n");
        write(&dir, "games/arena/Cargo.toml", "[package]\nname = \"arena\"\n");
        write(&dir, "games/arena/src/lib.rs", "pub mod game;\n");
        write(&dir, "games/arena/src/game.rs", "pub fn arena_game() {}\n");
        dir
    }

    fn steps(result: &Value) -> Vec<(String, String, f64)> {
        canon::get(result, "steps").and_then(canon::as_array).unwrap().iter().map(|s| (
            canon::get(s, "file").and_then(canon::as_str).unwrap().to_string(),
            canon::get(s, "role").and_then(canon::as_str).unwrap_or("").to_string(),
            canon::get(s, "relevance").and_then(canon::as_f64).unwrap_or(-1.0),
        )).collect()
    }

    #[test]
    fn a_goal_that_names_files_plans_from_them() {
        let dir = workspace("paths");
        let request = Request::new("Add a camera timeline: extend Camera in crates/app/src/sight.rs and play it back from crates/app/src/game.rs.", dir.clone());
        let result = decompose(&request).unwrap();
        let got = steps(&result);
        let files: Vec<&str> = got.iter().map(|s| s.0.as_str()).collect();
        // Named files first-class, in dependency order: game.rs uses sight.rs.
        let sight = files.iter().position(|f| *f == "crates/app/src/sight.rs").expect("sight.rs is planned");
        let game = files.iter().position(|f| *f == "crates/app/src/game.rs").expect("game.rs is planned");
        assert!(sight < game, "{got:?}");
        assert!(got.iter().filter(|s| s.1 == "named").all(|s| s.2 == 1.0), "{got:?}");
        // Neighbours rank below the named files, and the cap holds.
        assert!(got.iter().filter(|s| s.1 != "named").all(|s| s.2 <= 0.6), "{got:?}");
        assert!(got.len() <= 2 + 4 + 2, "{got:?}");
        // The test named after sight.rs comes last.
        assert_eq!(got.last().map(|s| (s.0.as_str(), s.1.as_str())), Some(("crates/app/tests/sight.rs", "test")), "{got:?}");
        assert!(!files.contains(&"games/arena/src/game.rs"), "{got:?}");
        let meta = canon::get(&result, "analysis_meta").unwrap();
        assert_eq!(canon::get(meta, "plan_source").and_then(canon::as_str), Some("named_files"));
        assert_eq!(canon::get(meta, "language").and_then(canon::as_str), Some("rust"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bare_file_names_settle_near_the_other_named_files() {
        let dir = workspace("bare");
        // game.rs exists twice; sight.rs only once, in crates/app.
        let result = decompose(&Request::new("record keyframes in sight.rs (Camera) and play them from game.rs", dir.clone())).unwrap();
        let named: Vec<String> = steps(&result).into_iter().filter(|s| s.1 == "named").map(|s| s.0).collect();
        assert_eq!(named, vec!["crates/app/src/sight.rs", "crates/app/src/game.rs"]);
        // On its own, game.rs is ambiguous and says so.
        let alone = decompose(&Request::new("speed up game.rs", dir.clone())).unwrap();
        let meta = canon::get(&alone, "analysis_meta").unwrap();
        let ambiguous = canon::stable(canon::get(meta, "ambiguous_files").unwrap());
        assert_eq!(ambiguous, r#"{"game.rs":["crates/app/src/game.rs","games/arena/src/game.rs"]}"#);
        assert!(canon::get(meta, "named_files_hint").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn files_argument_modules_and_unresolved_paths() {
        let dir = workspace("files");
        let mut request = Request::new("rework playback", dir.clone());
        request.files = vec!["crates/app/src/timeline.rs".into(), "crates/app/src/missing.rs".into()];
        let result = decompose(&request).unwrap();
        let got = steps(&result);
        assert_eq!(got[got.iter().position(|s| s.1 == "named").unwrap()].0, "crates/app/src/timeline.rs");
        let meta = canon::get(&result, "analysis_meta").unwrap();
        assert_eq!(canon::stable(canon::get(meta, "unresolved_files").unwrap()), r#"["crates/app/src/missing.rs"]"#);
        // A Rust module path names a file too.
        let module = decompose(&Request::new("change how app::sight::Camera stores its fov", dir.clone())).unwrap();
        let named: Vec<String> = steps(&module).into_iter().filter(|s| s.1 == "named").map(|s| s.0).collect();
        assert_eq!(named, vec!["crates/app/src/sight.rs"]);
        // So does a module name in backticks.
        let stem = decompose(&Request::new("tidy the `timeline` module", dir.clone())).unwrap();
        let named: Vec<String> = steps(&stem).into_iter().filter(|s| s.1 == "named").map(|s| s.0).collect();
        assert_eq!(named, vec!["crates/app/src/timeline.rs"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// map1's report: test steps ranked alongside the core files.
    #[test]
    fn without_named_files_code_comes_first_then_tests_ranked_lower() {
        let farm = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/archetypes/test_farm");
        let mut request = Request::new("update shared helpers in module code", farm);
        request.apply_project_config = false;
        let got = steps(&decompose(&request).unwrap());
        let first_test = got.iter().position(|s| s.1 == "test").unwrap();
        assert!(got[..first_test].iter().all(|s| s.1 == "core"), "{got:?}");
        assert!(got[first_test..].iter().all(|s| s.1 == "test"), "{got:?}");
        let best_test = got[first_test..].iter().map(|s| s.2).fold(0.0, f64::max);
        let best_core = got[..first_test].iter().map(|s| s.2).fold(0.0, f64::max);
        assert!(best_test <= best_core * 0.5 + 1e-9, "{got:?}");
    }

    #[test]
    fn path_words_feed_keywords_but_not_anchors() {
        assert_eq!(goal::keyword_text("fix crates/specula/src/sight.rs."), "fix  specula sight .");
        assert_eq!(goal::anchor_text("fix crates/specula/src/sight.rs and game.rs").split_whitespace().collect::<Vec<_>>(), vec!["fix", "specula", "and"]);
        let refs = goal::named_refs("edit `sight` and the timeline module (not the test file), specula::game::Ctx, game.rs, and src/a/b.py.");
        assert_eq!(refs.paths, vec!["src/a/b.py", "game.rs"]);
        assert_eq!(refs.stems, vec!["sight", "timeline"]);
        assert!(refs.modules.contains(&vec!["specula".to_string(), "game".to_string(), "Ctx".to_string()]));
    }
}
