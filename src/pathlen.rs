//! How long the paths in a verify copy get, and whether Windows can open them.
//!
//! Windows' classic path limit is 259 characters plus the terminating NUL.
//! Since Rust 1.100 a linker that cannot open its argument file under a deep
//! target directory fails with a message that does not say why, so the copy
//! reports its longest path and refuses to run past the limit.
//!
//! The estimate is the larger of two lengths:
//! - the deepest file copied into the tree;
//! - for a cargo project, the deepest file cargo writes under the target
//!   directory: an incremental object such as
//!   `debug/incremental/<crate>-<13>/s-<10>-<7>-<25>/<25>.pre-lto.bc`, where
//!   `<crate>` is the longest crate name in the copy (packages, `[[bin]]` and
//!   other named targets, and the files under `tests/`, `examples/`,
//!   `benches/` and `src/bin/`). Measured on cargo 1.99 these are the longest
//!   paths a build writes; build scripts can go deeper.

use std::fs;
use std::path::{Path, PathBuf};

/// The longest path Windows opens without the long-path opt-in.
pub const WINDOWS_MAX: usize = 259;
/// From here on the reply warns.
pub const WARN_AT: usize = 240;

#[derive(Clone, Debug, PartialEq)]
pub struct PathLen {
    /// Characters (UTF-16 units, as Windows counts them).
    pub longest: usize,
    /// The path that long: a copied file, or a build file with its hash
    /// parts written as `x`.
    pub example: String,
}

#[derive(Debug, PartialEq)]
pub enum Check {
    Fine,
    Near,
    Over,
}

/// Where the limit applies: Windows only.
pub fn classify(longest: usize, windows: bool) -> Check {
    if !windows { return Check::Fine; }
    if longest > WINDOWS_MAX { Check::Over } else if longest >= WARN_AT { Check::Near } else { Check::Fine }
}

pub fn chars(path: &Path) -> usize {
    path.as_os_str().to_string_lossy().encode_utf16().count()
}

/// The longest path under `tree` (holding `files`, read from `origin`) and
/// `target` (where cargo writes).
pub fn estimate(tree: &Path, target: &Path, files: &[String], origin: &Path) -> PathLen {
    let mut best = PathLen { longest: chars(tree), example: tree.display().to_string() };
    for rel in files {
        let path = tree.join(rel);
        let n = chars(&path);
        if n > best.longest { best = PathLen { longest: n, example: path.display().to_string() }; }
    }
    if let Some(name) = longest_crate(files, origin) {
        let x = |n: usize| "x".repeat(n);
        let path: PathBuf = target.join("debug").join("incremental")
            .join(format!("{name}-{}", x(13)))
            .join(format!("s-{}-{}-{}", x(10), x(7), x(25)))
            .join(format!("{}.pre-lto.bc", x(25)));
        let n = chars(&path);
        if n > best.longest { best = PathLen { longest: n, example: path.display().to_string() }; }
    }
    best
}

/// The longest crate name cargo would build from the copy, or None when it
/// has no Cargo.toml.
fn longest_crate(files: &[String], origin: &Path) -> Option<String> {
    let manifests: Vec<&String> = files.iter().filter(|f| f.as_str() == "Cargo.toml" || f.ends_with("/Cargo.toml")).collect();
    if manifests.is_empty() { return None; }
    let mut names: Vec<String> = Vec::new();
    for manifest in &manifests {
        if let Ok(text) = fs::read_to_string(origin.join(manifest.as_str())) { names.extend(target_names(&text)); }
        let dir = manifest.strip_suffix("Cargo.toml").unwrap_or("");
        for sub in ["tests/", "examples/", "benches/", "src/bin/"] {
            let prefix = format!("{dir}{sub}");
            for file in files.iter().filter_map(|f| f.strip_prefix(prefix.as_str())) {
                // tests/foo.rs and tests/foo/main.rs both build `foo`.
                let stem = match file.split_once('/') {
                    Some((folder, "main.rs")) => folder,
                    Some(_) => continue,
                    None => match file.strip_suffix(".rs") { Some(stem) => stem, None => continue },
                };
                names.push(stem.to_string());
            }
        }
    }
    names.into_iter().map(|n| n.replace('-', "_")).max_by_key(|n| n.encode_utf16().count())
}

/// `name = "..."` in `[package]`, `[lib]`, `[[bin]]`, `[[test]]`,
/// `[[example]]` and `[[bench]]` sections.
fn target_names(manifest: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut section = String::new();
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            section = line.trim_matches(|c| c == '[' || c == ']').trim().to_string();
            continue;
        }
        if !matches!(section.as_str(), "package" | "lib" | "bin" | "test" | "example" | "bench") { continue; }
        let Some((key, value)) = line.split_once('=') else { continue };
        if key.trim() != "name" { continue; }
        let value = value.trim();
        if let Some(name) = value.strip_prefix('"').and_then(|v| v.split('"').next()) {
            if !name.is_empty() { out.push(name.to_string()); }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_limit_is_windows_only_with_a_warning_band() {
        assert_eq!(classify(300, false), Check::Fine);
        assert_eq!(classify(239, true), Check::Fine);
        assert_eq!(classify(240, true), Check::Near);
        assert_eq!(classify(259, true), Check::Near);
        assert_eq!(classify(260, true), Check::Over);
    }

    #[test]
    fn a_cargo_project_counts_its_longest_crate_under_the_target_dir() {
        let origin = std::env::temp_dir().join(format!("trammel-pathlen-{}", std::process::id()));
        let _ = fs::remove_dir_all(&origin);
        fs::create_dir_all(origin.join("crates/engine")).unwrap();
        fs::write(origin.join("Cargo.toml"), "[workspace]\nmembers = [\"crates/engine\"]\n").unwrap();
        fs::write(origin.join("crates/engine/Cargo.toml"), "[package]\nname = \"engine-core\"\nversion = \"0.1.0\"\n\n[dependencies]\nname = \"not-a-target\"\n").unwrap();
        let files: Vec<String> = ["Cargo.toml", "crates/engine/Cargo.toml", "crates/engine/src/lib.rs", "crates/engine/tests/a_rather_long_guard_test.rs", "crates/engine/tests/multi/main.rs"]
            .iter().map(|s| s.to_string()).collect();
        assert_eq!(longest_crate(&files, &origin).as_deref(), Some("a_rather_long_guard_test"));
        let tree = Path::new("/c/t");
        let target = Path::new("/c/b");
        let got = estimate(tree, target, &files, &origin);
        // target + debug/incremental/<crate>-13/s-10-7-25/25.pre-lto.bc
        let want = chars(target) + "/debug/incremental/".len() + "a_rather_long_guard_test".len() + 14 + 3 + 10 + 1 + 7 + 1 + 25 + 1 + 25 + ".pre-lto.bc".len();
        assert_eq!(got.longest, want, "{}", got.example);
        assert!(got.example.contains("a_rather_long_guard_test-xxxxxxxxxxxxx"), "{}", got.example);
        // Outside cargo only the copied files count.
        let plain: Vec<String> = vec!["docs/a/very/deep/file.md".into()];
        assert_eq!(estimate(tree, target, &plain, &origin).longest, chars(&tree.join("docs/a/very/deep/file.md")));
        let _ = fs::remove_dir_all(&origin);
    }
}
