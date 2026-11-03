//! Nine beam orderings. Stable sorts match CPython's Timsort.

use crate::json::Value;
use crate::topo::topological_sort;
use std::collections::{BTreeMap, BTreeSet};

pub struct Strategy {
    pub name: &'static str,
    pub description: &'static str,
    pub order: fn(&[Value], &BTreeMap<String, Vec<String>>) -> Vec<Value>,
}

pub fn builtin() -> &'static [Strategy] {
    &STRATEGIES
}

const STRATEGIES: [Strategy; 9] = [
    Strategy { name: "bottom_up", description: "Modify dependencies first, then dependents (safest)", order: order_bottom_up },
    Strategy { name: "top_down", description: "Modify API surface first, then internals", order: order_top_down },
    Strategy { name: "risk_first", description: "Modify most-imported files first (highest impact)", order: order_risk_first },
    Strategy { name: "critical_path", description: "Longest dependency chain first (bottleneck feedback)", order: order_critical_path },
    Strategy { name: "cohesion", description: "Tightly coupled files grouped together", order: order_cohesion },
    Strategy { name: "minimal_change", description: "Fewest symbols first (quick wins)", order: order_minimal_change },
    Strategy { name: "leaf_first", description: "Files with zero importers first (safe isolated changes)", order: order_leaf_first },
    Strategy { name: "hub_first", description: "Network hub files first (highest connectivity risk)", order: order_hub_first },
    Strategy { name: "test_adjacent", description: "Files with matching test files first (verifiable changes)", order: order_test_adjacent },
];

fn file_of(step: &Value) -> String {
    crate::canon::get(step, "file").and_then(crate::canon::as_str).unwrap_or("").to_string()
}

fn split(steps: &[Value]) -> (Vec<Value>, Vec<Value>) {
    let mut active = Vec::new();
    let mut skipped = Vec::new();
    for step in steps {
        if crate::canon::get(step, "status").and_then(crate::canon::as_str) == Some("skipped") {
            skipped.push(step.clone());
        } else {
            active.push(step.clone());
        }
    }
    (active, skipped)
}

fn importers(graph: &BTreeMap<String, Vec<String>>) -> BTreeMap<String, i32> {
    let mut counts = BTreeMap::new();
    for deps in graph.values() {
        for dep in deps {
            *counts.entry(dep.clone()).or_insert(0) += 1;
        }
    }
    counts
}

fn order_bottom_up(steps: &[Value], graph: &BTreeMap<String, Vec<String>>) -> Vec<Value> {
    let (mut active, skipped) = split(steps);
    active.sort_by_key(|s| graph.get(&file_of(s)).map(|d| d.len()).unwrap_or(0));
    active.extend(skipped);
    active
}

fn order_top_down(steps: &[Value], graph: &BTreeMap<String, Vec<String>>) -> Vec<Value> {
    let (mut active, skipped) = split(steps);
    active.sort_by(|a, b| {
        let da = graph.get(&file_of(a)).map(|d| d.len()).unwrap_or(0);
        let db = graph.get(&file_of(b)).map(|d| d.len()).unwrap_or(0);
        db.cmp(&da)
    });
    active.extend(skipped);
    active
}

fn order_risk_first(steps: &[Value], graph: &BTreeMap<String, Vec<String>>) -> Vec<Value> {
    let counts = importers(graph);
    let (active, skipped) = split(steps);
    let mut isolated = Vec::new();
    let mut batchable = Vec::new();
    for step in active {
        if crate::canon::get(&step, "incompatible_with").is_some() {
            isolated.push(step);
        } else {
            batchable.push(step);
        }
    }
    isolated.sort_by(|a, b| {
        let ca = counts.get(&file_of(a)).copied().unwrap_or(0);
        let cb = counts.get(&file_of(b)).copied().unwrap_or(0);
        cb.cmp(&ca)
    });
    let mut groups: Vec<(String, Vec<Value>)> = Vec::new();
    for step in batchable {
        let pkg = parent(&file_of(&step));
        if let Some((_, bucket)) = groups.iter_mut().find(|(name, _)| name == &pkg) {
            bucket.push(step);
        } else {
            groups.push((pkg, vec![step]));
        }
    }
    groups.sort_by(|a, b| {
        let ma = a.1.iter().map(|s| counts.get(&file_of(s)).copied().unwrap_or(0)).max().unwrap_or(0);
        let mb = b.1.iter().map(|s| counts.get(&file_of(s)).copied().unwrap_or(0)).max().unwrap_or(0);
        mb.cmp(&ma)
    });
    let mut result = isolated;
    for (_, mut group) in groups {
        group.sort_by(|a, b| {
            let ca = counts.get(&file_of(a)).copied().unwrap_or(0);
            let cb = counts.get(&file_of(b)).copied().unwrap_or(0);
            cb.cmp(&ca)
        });
        result.extend(group);
    }
    result.extend(skipped);
    result
}

fn parent(path: &str) -> String {
    match path.rfind('/') {
        Some(i) if i > 0 => path[..i].to_string(),
        _ => "__root__".into(),
    }
}

fn order_critical_path(steps: &[Value], graph: &BTreeMap<String, Vec<String>>) -> Vec<Value> {
    let (mut active, skipped) = split(steps);
    let files: BTreeSet<String> = active.iter().map(|s| file_of(s)).collect();
    let mut depth = BTreeMap::new();
    for step in &active {
        let start = file_of(step);
        if depth.contains_key(&start) {
            continue;
        }
        let mut stack = vec![(start, false)];
        let mut in_stack = BTreeSet::new();
        while let Some((node, processed)) = stack.pop() {
            if depth.contains_key(&node) {
                continue;
            }
            if processed {
                in_stack.remove(&node);
                let children: Vec<String> = graph
                    .get(&node)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|d| files.contains(d))
                    .collect();
                let d = 1 + children.iter().map(|c| depth.get(c).copied().unwrap_or(0)).max().unwrap_or(0);
                depth.insert(node, d);
                continue;
            }
            if in_stack.contains(&node) {
                in_stack.remove(&node);
                depth.insert(node, 0);
                continue;
            }
            in_stack.insert(node.clone());
            stack.push((node.clone(), true));
            if let Some(deps) = graph.get(&node) {
                for child in deps {
                    if files.contains(child) && !depth.contains_key(child) {
                        stack.push((child.clone(), false));
                    }
                }
            }
        }
    }
    active.sort_by(|a, b| {
        let da = depth.get(&file_of(a)).copied().unwrap_or(0);
        let db = depth.get(&file_of(b)).copied().unwrap_or(0);
        db.cmp(&da)
    });
    active.extend(skipped);
    active
}

fn order_cohesion(steps: &[Value], graph: &BTreeMap<String, Vec<String>>) -> Vec<Value> {
    let (active, skipped) = split(steps);
    let files: BTreeSet<String> = active.iter().map(|s| file_of(s)).collect();
    let mut adj: BTreeMap<String, BTreeSet<String>> = files.iter().map(|f| (f.clone(), BTreeSet::new())).collect();
    for (file, deps) in graph {
        if !files.contains(file) {
            continue;
        }
        for dep in deps {
            if files.contains(dep) {
                adj.get_mut(file).unwrap().insert(dep.clone());
                adj.get_mut(dep).unwrap().insert(file.clone());
            }
        }
    }
    let mut visited = BTreeSet::new();
    let mut components = Vec::new();
    for file in &files {
        if visited.contains(file) {
            continue;
        }
        let mut component = Vec::new();
        let mut stack = vec![file.clone()];
        while let Some(node) = stack.pop() {
            if !visited.insert(node.clone()) {
                continue;
            }
            component.push(node.clone());
            let mut next: Vec<String> = adj.get(&node).cloned().unwrap_or_default().into_iter().filter(|n| !visited.contains(n)).collect();
            next.sort();
            stack.extend(next);
        }
        components.push(component);
    }
    components.sort_by(|a, b| b.len().cmp(&a.len()));
    let mut file_order = Vec::new();
    for component in components {
        if component.len() == 1 {
            file_order.extend(component);
        } else {
            let set: BTreeSet<String> = component.iter().cloned().collect();
            let sub = component
                .iter()
                .map(|f| {
                    let deps = graph
                        .get(f)
                        .cloned()
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|d| set.contains(d))
                        .collect();
                    (f.clone(), deps)
                })
                .collect();
            file_order.extend(topological_sort(&sub));
        }
    }
    let mut file_to_step = BTreeMap::new();
    for step in &active {
        file_to_step.insert(file_of(step), step.clone());
    }
    let mut ordered = Vec::new();
    let mut ordered_files = BTreeSet::new();
    for file in file_order {
        if let Some(step) = file_to_step.get(&file) {
            if ordered_files.insert(file) {
                ordered.push(step.clone());
            }
        }
    }
    for step in &active {
        if !ordered_files.contains(&file_of(step)) {
            ordered.push(step.clone());
        }
    }
    ordered.extend(skipped);
    ordered
}

fn order_minimal_change(steps: &[Value], _graph: &BTreeMap<String, Vec<String>>) -> Vec<Value> {
    let (mut active, skipped) = split(steps);
    active.sort_by_key(|s| crate::canon::get(s, "symbol_count").and_then(crate::canon::as_i64).unwrap_or(0));
    active.extend(skipped);
    active
}

fn order_leaf_first(steps: &[Value], graph: &BTreeMap<String, Vec<String>>) -> Vec<Value> {
    let counts = importers(graph);
    let (mut active, skipped) = split(steps);
    active.sort_by_key(|s| counts.get(&file_of(s)).copied().unwrap_or(0));
    active.extend(skipped);
    active
}

fn order_hub_first(steps: &[Value], graph: &BTreeMap<String, Vec<String>>) -> Vec<Value> {
    let counts = importers(graph);
    let (mut active, skipped) = split(steps);
    active.sort_by(|a, b| {
        let score = |s: &Value| {
            let f = file_of(s);
            let out = graph.get(&f).map(|d| d.len()).unwrap_or(0) as i64;
            let inn = counts.get(&f).copied().unwrap_or(0) as i64;
            out * inn
        };
        score(b).cmp(&score(a))
    });
    active.extend(skipped);
    active
}

fn order_test_adjacent(steps: &[Value], _graph: &BTreeMap<String, Vec<String>>) -> Vec<Value> {
    let (mut active, skipped) = split(steps);
    let bases: BTreeSet<String> = active.iter().map(|s| stem(&file_of(s))).collect();
    active.sort_by(|a, b| has_test(b, &bases).cmp(&has_test(a, &bases)));
    active.extend(skipped);
    active
}

fn stem(path: &str) -> String {
    let base = path.rsplit('/').next().unwrap_or(path);
    base.rsplit_once('.').map(|(s, _)| s.to_string()).unwrap_or_else(|| base.to_string())
}

fn has_test(step: &Value, bases: &BTreeSet<String>) -> i32 {
    let base = stem(&file_of(step));
    let names = [format!("test_{base}"), format!("{base}_test"), format!("{base}_spec")];
    if names.iter().any(|n| bases.contains(n)) { 1 } else { 0 }
}

pub fn explore(steps: &[Value], graph: &BTreeMap<String, Vec<String>>, num_beams: usize, stats: &BTreeMap<String, (i64, i64)>) -> Vec<Value> {
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let cap = 3.max(12.min(cores));
    let n = num_beams.min(cap).max(1);
    let mut entries: Vec<&Strategy> = builtin().iter().collect();
    let rate = |entry: &&Strategy| {
        match stats.get(entry.name) {
            None => -1.0,
            Some((succ, fail)) => *succ as f64 / (*succ + *fail + 1) as f64,
        }
    };
    entries.sort_by(|a, b| rate(b).partial_cmp(&rate(a)).unwrap_or(std::cmp::Ordering::Equal));
    let ordered: Vec<(&str, &str, Vec<Value>)> = entries
        .iter()
        .map(|e| (e.name, e.description, (e.order)(steps, graph)))
        .collect();
    if ordered.is_empty() {
        return Vec::new();
    }
    let mut beams = Vec::new();
    for i in 0..n {
        let (name, desc, ordered_steps) = &ordered[i % ordered.len()];
        let (active, skipped) = split(ordered_steps);
        let edits = active
            .iter()
            .enumerate()
            .map(|(j, step)| {
                crate::canon::obj(vec![
                    ("step_index", Value::Int(crate::canon::get(step, "step_index").and_then(crate::canon::as_i64).unwrap_or(j as i64))),
                    ("path", Value::string(file_of(step))),
                    ("task", step.clone()),
                ])
            })
            .collect();
        beams.push(crate::canon::obj(vec![
            ("beam_id", Value::Int(i as i64)),
            ("variant", Value::string(*name)),
            ("variant_description", Value::string(*desc)),
            ("steps", Value::Array(ordered_steps.clone())),
            ("edits", Value::Array(edits)),
            ("skipped", Value::Array(skipped)),
        ]));
    }
    beams
}
