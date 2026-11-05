//! Fail if Cargo.lock names any package other than trammel, or if
//! `[dependencies]` in Cargo.toml is not empty. Run from the repo root:
//! `cargo run --example check_zero_deps`. Same checks, results and exit
//! codes as the `scripts/check_zero_deps.py` it replaced.

#[path = "support/py.rs"]
mod py;

use std::collections::BTreeSet;
use std::process::ExitCode;

/// The names `re.findall(r'^name = "([^"]+)"', text, re.M)` finds.
fn lock_names(lock: &str) -> Vec<String> {
    lock.split('\n')
        .filter_map(|line| {
            let rest = line.strip_prefix("name = \"")?;
            let end = rest.find('"')?;
            (end > 0).then(|| rest[..end].to_string())
        })
        .collect()
}

/// `toml.split("[dependencies]", 1)[-1].split("[", 1)[0]`, stripped.
fn dependencies_section(toml: &str) -> &str {
    let after = toml.split_once("[dependencies]").map(|(_, rest)| rest).unwrap_or(toml);
    py::strip(after.split('[').next().unwrap_or(""))
}

fn check(lock: &str, toml: &str) -> Result<&'static str, String> {
    let extra: BTreeSet<String> = lock_names(lock).into_iter().filter(|n| n != "trammel").collect();
    if !extra.is_empty() {
        return Err(format!("Cargo.lock lists packages other than trammel: {}", extra.into_iter().collect::<Vec<_>>().join(", ")));
    }
    if !dependencies_section(toml).is_empty() {
        return Err("[dependencies] is not empty".into());
    }
    Ok("zero dependencies")
}

fn main() -> ExitCode {
    let lock = match py::read_text("Cargo.lock") { Ok(t) => t, Err(code) => return code };
    let toml = match py::read_text("Cargo.toml") { Ok(t) => t, Err(code) => return code };
    match check(&lock, &toml) {
        Ok(message) => { println!("{message}"); ExitCode::SUCCESS }
        Err(message) => { println!("{message}"); ExitCode::from(1) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCK: &str = "version = 4\n\n[[package]]\nname = \"trammel\"\nversion = \"0.0.0\"\n";
    const TOML: &str = "[package]\nname = \"trammel\"\n\n[dependencies]\n\n[[bin]]\nname = \"trammel\"\n";

    #[test]
    fn this_repo_shape_passes() {
        assert_eq!(check(LOCK, TOML), Ok("zero dependencies"));
    }

    #[test]
    fn an_extra_lock_package_fails_with_sorted_names() {
        let lock = format!("{LOCK}\n[[package]]\nname = \"serde\"\n\n[[package]]\nname = \"anyhow\"\n\n[[package]]\nname = \"serde\"\n");
        assert_eq!(check(&lock, TOML), Err("Cargo.lock lists packages other than trammel: anyhow, serde".into()));
    }

    #[test]
    fn a_dependency_line_fails() {
        let toml = TOML.replace("[dependencies]\n", "[dependencies]\nserde = \"1\"\n");
        assert_eq!(check(LOCK, &toml), Err("[dependencies] is not empty".into()));
    }

    #[test]
    fn lock_name_lines_follow_the_python_regex() {
        assert_eq!(lock_names("name = \"a\"\n  name = \"b\"\nname = \"\"\nname = \"c\" # x\nname = \"d"), vec!["a", "c"]);
    }
}
