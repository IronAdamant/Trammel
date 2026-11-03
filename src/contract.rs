//! Reply envelopes. Every command and tool shares one shape.

use crate::json::Value;
use std::process::ExitCode;

pub const SCHEMA_VERSION: &str = "4.0.0";

#[derive(Clone, Debug)]
pub struct Failure {
    pub code: &'static str,
    pub message: String,
    pub hint: String,
    pub retryable: bool,
    pub exit: u8,
}

impl Failure {
    pub fn new(code: &'static str, message: impl Into<String>, hint: impl Into<String>) -> Failure {
        Failure { code, message: message.into(), hint: hint.into(), retryable: false, exit: 1 }
    }

    pub fn usage(message: impl Into<String>) -> Failure {
        Failure { code: "usage", message: message.into(), hint: "Run trammel --help".into(), retryable: false, exit: 2 }
    }

    pub fn value(&self) -> Value {
        crate::canon::obj(vec![
            ("ok", Value::Bool(false)),
            ("schema_version", Value::string(SCHEMA_VERSION)),
            ("code", Value::string(self.code)),
            ("message", Value::string(self.message.clone())),
            ("hint", Value::string(self.hint.clone())),
            ("retryable", Value::Bool(self.retryable)),
        ])
    }

    pub fn exit_code(&self) -> ExitCode {
        ExitCode::from(self.exit)
    }
}

pub fn ok(mut fields: Vec<(&str, Value)>) -> Value {
    let mut pairs = vec![
        ("ok".to_string(), Value::Bool(true)),
        ("schema_version".to_string(), Value::string(SCHEMA_VERSION)),
    ];
    for (key, value) in fields.drain(..) {
        pairs.push((key.to_string(), value));
    }
    Value::Object(pairs)
}

pub fn tool_removed(name: &str, replacement: &str) -> Failure {
    Failure::new(
        "tool_removed",
        format!("{name} was removed in 4.0"),
        format!("Call {replacement} instead"),
    )
}
