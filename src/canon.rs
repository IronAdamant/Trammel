//! Python-compatible JSON helpers on top of the ordered `json::Value` tree.
//!
//! `canonical` matches `json.dumps(obj, sort_keys=True)` (space separators,
//! `ensure_ascii=True`) so recipe signatures stay SHA-256 compatible.
//! `stable` emits compact JSON with sorted keys for agent replies.

use crate::json::Value;
use std::fmt::Write as _;

pub fn obj(pairs: Vec<(&str, Value)>) -> Value {
    Value::object(pairs)
}

pub fn get<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    let Value::Object(pairs) = value else {
        return None;
    };
    pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

pub fn get_mut<'a>(value: &'a mut Value, key: &str) -> Option<&'a mut Value> {
    let Value::Object(pairs) = value else {
        return None;
    };
    pairs.iter_mut().find(|(k, _)| k == key).map(|(_, v)| v)
}

pub fn insert(value: &mut Value, key: &str, item: Value) {
    let Value::Object(pairs) = value else {
        return;
    };
    if let Some((_, slot)) = pairs.iter_mut().find(|(k, _)| k == key) {
        *slot = item;
    } else {
        pairs.push((key.to_string(), item));
    }
}

pub fn as_str(value: &Value) -> Option<&str> {
    match value {
        Value::String(s) => Some(s),
        _ => None,
    }
}

pub fn as_i64(value: &Value) -> Option<i64> {
    match value {
        Value::Int(n) => Some(*n),
        Value::Float(n) => Some(*n as i64),
        _ => None,
    }
}

pub fn as_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Int(n) => Some(*n as f64),
        Value::Float(n) => Some(*n),
        _ => None,
    }
}

pub fn as_bool(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(b) => Some(*b),
        _ => None,
    }
}

pub fn as_array(value: &Value) -> Option<&[Value]> {
    match value {
        Value::Array(items) => Some(items),
        _ => None,
    }
}

/// Half-even rounding, matching Python 3 `round`.
pub fn py_round(value: f64, digits: i32) -> f64 {
    if !value.is_finite() {
        return value;
    }
    let factor = 10f64.powi(digits);
    let scaled = value * factor;
    let negative = scaled.is_sign_negative();
    let abs = scaled.abs();
    let floor = abs.floor();
    let frac = abs - floor;
    let mut rounded = if frac > 0.5 + 1e-12 {
        floor + 1.0
    } else if frac < 0.5 - 1e-12 {
        floor
    } else if (floor as i64) % 2 == 0 {
        floor
    } else {
        floor + 1.0
    };
    if negative {
        rounded = -rounded;
    }
    rounded / factor
}

pub fn canonical(value: &Value) -> String {
    let sorted = sort_keys(value);
    let mut out = String::new();
    write_python(&mut out, &sorted, true);
    out
}

pub fn stable(value: &Value) -> String {
    let sorted = sort_keys(value);
    crate::json::to_string(&sorted)
}

pub fn stable_pretty(value: &Value) -> String {
    let sorted = sort_keys(value);
    let mut out = String::new();
    write_pretty(&mut out, &sorted, 0);
    out.push('\n');
    out
}

fn sort_keys(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(sort_keys).collect()),
        Value::Object(pairs) => {
            let mut pairs: Vec<(String, Value)> = pairs
                .iter()
                .map(|(k, v)| (k.clone(), sort_keys(v)))
                .collect();
            pairs.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(pairs)
        }
        other => other.clone(),
    }
}

fn write_python(out: &mut String, value: &Value, spaces: bool) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Int(n) => {
            let _ = write!(out, "{n}");
        }
        Value::Float(n) => write_python_float(out, *n),
        Value::String(text) => write_ascii_string(out, text),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                    if spaces {
                        out.push(' ');
                    }
                }
                write_python(out, item, spaces);
            }
            out.push(']');
        }
        Value::Object(pairs) => {
            out.push('{');
            for (i, (key, item)) in pairs.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                    if spaces {
                        out.push(' ');
                    }
                }
                write_ascii_string(out, key);
                out.push(':');
                if spaces {
                    out.push(' ');
                }
                write_python(out, item, spaces);
            }
            out.push('}');
        }
    }
}

fn write_python_float(out: &mut String, n: f64) {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        let _ = write!(out, "{n:.1}");
        return;
    }
    let text = format!("{n}");
    out.push_str(&text);
}

fn write_ascii_string(out: &mut String, text: &str) {
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || (c as u32) > 0x7e => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn write_pretty(out: &mut String, value: &Value, indent: usize) {
    match value {
        Value::Array(items) if items.is_empty() => out.push_str("[]"),
        Value::Object(pairs) if pairs.is_empty() => out.push_str("{}"),
        Value::Array(items) => {
            out.push_str("[\n");
            for (i, item) in items.iter().enumerate() {
                pad(out, indent + 1);
                write_pretty(out, item, indent + 1);
                if i + 1 != items.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            pad(out, indent);
            out.push(']');
        }
        Value::Object(pairs) => {
            out.push_str("{\n");
            for (i, (key, item)) in pairs.iter().enumerate() {
                pad(out, indent + 1);
                write_ascii_string(out, key);
                out.push_str(": ");
                write_pretty(out, item, indent + 1);
                if i + 1 != pairs.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            pad(out, indent);
            out.push('}');
        }
        other => write_python(out, other, false),
    }
}

fn pad(out: &mut String, indent: usize) {
    for _ in 0..indent {
        out.push_str("  ");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_matches_python_dumps_shape() {
        let value = obj(vec![
            ("b", Value::Int(1)),
            ("a", Value::string("x")),
        ]);
        assert_eq!(canonical(&value), r#"{"a": "x", "b": 1}"#);
    }

    #[test]
    fn half_even_round() {
        assert_eq!(py_round(0.5, 0), 0.0);
        assert_eq!(py_round(1.5, 0), 2.0);
        assert_eq!(py_round(0.125, 2), 0.12);
    }
}
