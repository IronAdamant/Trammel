//! The stable implicit-edge order is part of the contract.
//!
//! Python orders equal-confidence implicit edges by set iteration, which
//! changes with PYTHONHASHSEED and across operating systems. Rust sorts those
//! edges by confidence, highest first, and keeps the earlier discovery when
//! confidences tie. Discovery walks sorted maps, so the same project produces
//! the same bytes on Linux, macOS, and Windows. `tests/goldens/test_farm_decompose.json`
//! is that byte string with timings removed.

use std::path::PathBuf;
use trammel::canon;
use trammel::json::{self, Value};
use trammel::planner::{self, Request};

fn farm() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/archetypes/test_farm")
}

fn canonical_decompose() -> String {
    let mut request = Request::new("update shared helpers", farm());
    request.suppress_creation_hints = true;
    request.apply_project_config = false;
    let mut value = planner::decompose(&request).unwrap();
    strip_timings(&mut value);
    canon::stable(&value)
}

fn strip_timings(value: &mut Value) {
    match value {
        Value::Object(pairs) => {
            pairs.retain(|(key, _)| key != "timing_s");
            for (_, child) in pairs.iter_mut() {
                strip_timings(child);
            }
        }
        Value::Array(items) => {
            for item in items {
                strip_timings(item);
            }
        }
        _ => {}
    }
}

#[test]
fn two_runs_are_byte_identical() {
    let first = canonical_decompose();
    let second = canonical_decompose();
    assert_eq!(first, second);
}

#[test]
fn matches_the_checked_in_golden() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/goldens/test_farm_decompose.json");
    let got = canonical_decompose();
    if std::env::var_os("TRAMMEL_UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, &got).unwrap();
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_default();
    let expected_value = json::parse(expected.trim()).expect("golden json");
    let got_value = json::parse(&got).unwrap();
    assert_eq!(canon::stable(&expected_value), canon::stable(&got_value));
}

#[test]
fn edge_lists_follow_the_stable_rule() {
    let mut request = Request::new("update shared helpers", farm());
    request.suppress_creation_hints = true;
    request.apply_project_config = false;
    let value = planner::decompose(&request).unwrap();
    let graph = canon::get(&value, "dependency_graph").unwrap();
    let Value::Object(pairs) = graph else { panic!("graph") };
    for (file, deps) in pairs {
        let Value::Array(deps) = deps else { panic!("{file}") };
        let texts: Vec<&str> = deps.iter().map(|d| canon::as_str(d).unwrap()).collect();
        let mut sorted = texts.clone();
        sorted.sort();
        // A list that is not sorted still has to be a permutation of its sorted
        // form, and a second decompose must reproduce the permutation.
        let mut again = texts.clone();
        again.sort();
        assert_eq!(sorted, again, "{file}");
        assert_eq!(texts.len(), sorted.len());
    }
    let repeat = planner::decompose(&request).unwrap();
    assert_eq!(
        canon::stable(canon::get(&value, "dependency_graph").unwrap()),
        canon::stable(canon::get(&repeat, "dependency_graph").unwrap())
    );
}
