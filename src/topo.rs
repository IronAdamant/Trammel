//! Kahn topological sort. Dependencies come first. Cycles are appended sorted.
//! Queue ties follow sorted order, matching `utils.topological_sort`.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub fn topological_sort(deps: &BTreeMap<String, Vec<String>>) -> Vec<String> {
    let mut all = BTreeSet::new();
    for (node, targets) in deps {
        all.insert(node.clone());
        for target in targets {
            all.insert(target.clone());
        }
    }
    let mut indeg: BTreeMap<String, i32> = all.iter().map(|n| (n.clone(), 0)).collect();
    let mut rev: BTreeMap<String, Vec<String>> = all.iter().map(|n| (n.clone(), Vec::new())).collect();
    for (node, targets) in deps {
        *indeg.get_mut(node).unwrap() = targets.len() as i32;
        for target in targets {
            rev.get_mut(target).unwrap().push(node.clone());
        }
    }
    for dependents in rev.values_mut() {
        dependents.sort();
    }
    let mut queue: VecDeque<String> = indeg
        .iter()
        .filter(|(_, n)| **n == 0)
        .map(|(k, _)| k.clone())
        .collect();
    let mut queued: Vec<String> = queue.iter().cloned().collect();
    queued.sort();
    queue = queued.into();
    let mut result = Vec::new();
    while let Some(node) = queue.pop_front() {
        result.push(node.clone());
        let mut ready = Vec::new();
        for dependent in rev.get(&node).cloned().unwrap_or_default() {
            let slot = indeg.get_mut(&dependent).unwrap();
            *slot -= 1;
            if *slot == 0 {
                ready.push(dependent);
            }
        }
        ready.sort();
        queue.extend(ready);
    }
    for node in all {
        if !result.contains(&node) {
            result.push(node);
        }
    }
    result
}
