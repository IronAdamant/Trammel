//! `trammel.plan` format version 1. `exported_at` is the one timestamp the
//! format already required; parity comparisons drop it.

use crate::canon;
use crate::json::Value;

pub fn from_plan(plan: &Value) -> Value {
    let steps = canon::get(plan, "steps").and_then(canon::as_array).map(|items| items.to_vec()).unwrap_or_default();
    let strategy = canon::get(plan, "strategy").cloned().unwrap_or(Value::Null);
    canon::obj(vec![
        ("format", Value::string("trammel.plan")),
        ("format_version", Value::Int(1)),
        ("goal", canon::get(plan, "goal").cloned().unwrap_or(Value::string(""))),
        ("status", canon::get(plan, "status").cloned().unwrap_or(Value::string("pending"))),
        ("plan_id", canon::get(plan, "id").cloned().unwrap_or(Value::Null)),
        ("steps", Value::Array(steps.iter().enumerate().map(|(i, step)| normalize(step, i)).collect())),
        ("strategy", strategy),
        ("dependency_graph", Value::Object(vec![])),
        ("scaffold", Value::Null),
        ("constraints", Value::Array(vec![])),
        ("metadata", canon::obj(vec![
            ("current_step", canon::get(plan, "current_step").cloned().unwrap_or(Value::Null)),
            ("total_steps", canon::get(plan, "total_steps").cloned().unwrap_or(Value::Int(steps.len() as i64))),
        ])),
    ])
}

fn normalize(step: &Value, index: usize) -> Value {
    canon::obj(vec![
        ("step_index", canon::get(step, "step_index").cloned().unwrap_or(Value::Int(index as i64))),
        ("description", canon::get(step, "description").cloned().unwrap_or(Value::string(""))),
        ("rationale", canon::get(step, "rationale").cloned().unwrap_or(Value::string(""))),
        ("depends_on", canon::get(step, "depends_on").cloned().unwrap_or(Value::Array(vec![]))),
        ("status", canon::get(step, "status").cloned().unwrap_or(Value::string("pending"))),
        ("file", canon::get(step, "file").cloned().unwrap_or(Value::Null)),
    ])
}
