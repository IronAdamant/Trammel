//! Rust import graph for single crates and Cargo workspaces.
//!
//! Each `.rs` file belongs to the package whose Cargo.toml is nearest above
//! it. A path is resolved the way rustc finds module files:
//!
//! - `crate::a::b` from the crate's root directory (`src/`, or the directory
//!   of a crate root such as `tests/x.rs`, `examples/x.rs`, `src/bin/x.rs`);
//! - `self::` from the file's module directory (`src/a/` for `src/a.rs` or
//!   `src/a/mod.rs`), `super::` from its parent;
//! - `other_crate::a` from that package's `src/`, when `other_crate` is a
//!   package of the project (`-` read as `_`);
//! - a bare `a::b` as a child module of the file, when such a file exists;
//! - `mod a;` (any visibility, `#[path = "..."]` honoured) in the module
//!   directory.
//!
//! The deepest existing file wins: `crate::sight::Camera` gives
//! `src/sight.rs`, not `src/lib.rs`. Grouped imports (`use crate::{a, b::c}`)
//! and paths written inline in code count too.

use crate::lang::blank::strip_c_comments;
use crate::walk::{collect_files, read_lossy};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub fn rust_imports(root: &Path) -> BTreeMap<String, Vec<String>> {
    let files: BTreeSet<String> = collect_files(root, &[".rs"]).into_iter().collect();
    let packages = Packages::find(root, &files);
    let mut graph = BTreeMap::new();
    for rel in &files {
        let Some(src) = read_lossy(&root.join(rel)).map(|s| strip_c_comments(&s)) else { continue };
        let file = FileCtx::new(rel, &packages);
        let mut deps = BTreeSet::new();
        for path in paths_in(&src) {
            if let Some(dep) = file.resolve(&path, &packages, &files) { deps.insert(dep); }
        }
        for (name, attr) in mod_decls(&src) {
            if let Some(custom) = attr {
                let target = join(&parent(rel), &custom);
                if files.contains(&target) { deps.insert(target); }
                continue;
            }
            if let Some(dep) = module_file(&file.module_dir, &[name.as_str()], &files) { deps.insert(dep); }
        }
        deps.remove(rel);
        if !deps.is_empty() { graph.insert(rel.clone(), deps.into_iter().collect()); }
    }
    graph
}

/// Package directories (relative, `""` for the root) and the `src/` of each
/// package name.
pub struct Packages {
    dirs: BTreeSet<String>,
    by_name: BTreeMap<String, String>,
}

impl Packages {
    pub fn find(root: &Path, files: &BTreeSet<String>) -> Packages {
        let mut dirs = BTreeSet::new();
        let mut checked = BTreeSet::new();
        for rel in files {
            let mut dir = parent(rel);
            loop {
                if !checked.insert(dir.clone()) { break; }
                if root.join(&dir).join("Cargo.toml").is_file() { dirs.insert(dir.clone()); }
                if dir.is_empty() { break; }
                dir = parent(&dir);
            }
        }
        let mut by_name = BTreeMap::new();
        for dir in &dirs {
            if let Some(name) = read_lossy(&root.join(dir).join("Cargo.toml")).and_then(|t| package_name(&t)) {
                by_name.insert(name.replace('-', "_"), join(dir, "src"));
            }
        }
        Packages { dirs, by_name }
    }

    /// The package directory that owns `rel`: the deepest one above it.
    fn owner(&self, rel: &str) -> Option<&str> {
        let mut dir = parent(rel);
        loop {
            if self.dirs.contains(&dir) { return self.dirs.get(&dir).map(|s| s.as_str()); }
            if dir.is_empty() { return None; }
            dir = parent(&dir);
        }
    }

    /// The `src/` directory of package `name` (`-` read as `_`).
    pub fn src_of(&self, name: &str) -> Option<&str> {
        self.by_name.get(name).map(|s| s.as_str())
    }
}

/// `name` from the `[package]` table of a Cargo.toml.
fn package_name(text: &str) -> Option<String> {
    let mut in_package = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') { in_package = line == "[package]"; continue; }
        if !in_package { continue; }
        if let Some(rest) = line.strip_prefix("name") {
            let value = rest.trim().strip_prefix('=')?.trim();
            return Some(value.trim_matches('"').to_string());
        }
    }
    None
}

struct FileCtx {
    /// Where `crate::` starts.
    crate_dir: String,
    /// Where this file's child modules live (`self::`).
    module_dir: String,
}

impl FileCtx {
    fn new(rel: &str, packages: &Packages) -> FileCtx {
        let dir = parent(rel);
        let name = rel.rsplit('/').next().unwrap_or(rel);
        let stem = name.strip_suffix(".rs").unwrap_or(name);
        let pkg = packages.owner(rel).unwrap_or("");
        let inside = rel.strip_prefix(pkg).map(|r| r.trim_start_matches('/')).unwrap_or(rel);
        // Files that start a crate of their own: their module directory is
        // their own directory, and so is `crate::`.
        let top = |prefix: &str| inside.strip_prefix(prefix).map(|r| !r.contains('/')).unwrap_or(false);
        let is_root = matches!(inside, "src/lib.rs" | "src/main.rs" | "build.rs")
            || top("src/bin/") || top("tests/") || top("examples/") || top("benches/");
        let crate_dir = if inside.starts_with("src/bin/") {
            join(pkg, "src/bin")
        } else if inside.starts_with("src/") {
            join(pkg, "src")
        } else {
            match ["tests/", "examples/", "benches/"].iter().find(|p| inside.starts_with(*p)) {
                Some(p) => join(pkg, p.trim_end_matches('/')),
                None => pkg.to_string(),
            }
        };
        let module_dir = if is_root || matches!(stem, "mod" | "lib" | "main") { dir } else { join(&dir, stem) };
        FileCtx { crate_dir, module_dir }
    }

    fn resolve(&self, path: &[String], packages: &Packages, files: &BTreeSet<String>) -> Option<String> {
        let first = path.first()?.as_str();
        let rest: Vec<&str> = path.iter().skip(1).map(|s| s.as_str()).collect();
        match first {
            "crate" => module_file(&self.crate_dir, &rest, files),
            "self" => module_file(&self.module_dir, &rest, files),
            "super" => {
                let mut base = parent(&self.module_dir);
                let mut rest = rest.as_slice();
                while rest.first() == Some(&"super") {
                    base = parent(&base);
                    rest = &rest[1..];
                }
                module_file(&base, rest, files)
            }
            name => {
                // A child module named in code wins over a crate of the same name.
                let names: Vec<&str> = path.iter().map(|s| s.as_str()).collect();
                module_file(&self.module_dir, &names, files)
                    .or_else(|| packages.src_of(name).and_then(|src| module_file(src, &rest, files)))
            }
        }
    }
}

/// The deepest file for the module path `segments` under `base`:
/// `base/a/b.rs` or `base/a/b/mod.rs`, else `base/a.rs`, ... For an empty
/// path, `base/lib.rs` or `base/main.rs` (a whole crate).
pub(crate) fn module_file(base: &str, segments: &[&str], files: &BTreeSet<String>) -> Option<String> {
    for depth in (1..=segments.len()).rev() {
        let joined = segments[..depth].join("/");
        for candidate in [join(base, &format!("{joined}.rs")), join(base, &format!("{joined}/mod.rs"))] {
            if files.contains(&candidate) { return Some(candidate); }
        }
    }
    if segments.is_empty() {
        for root in ["lib.rs", "main.rs"] {
            let candidate = join(base, root);
            if files.contains(&candidate) { return Some(candidate); }
        }
    }
    None
}

/// Every `a::b::c` path in `src`, with `{...}` groups expanded. Leading `::`
/// is dropped, ` as x` renames and `*` / `self` leaves are cut.
pub fn paths_in(src: &str) -> Vec<Vec<String>> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let starts = (c.is_ascii_alphabetic() || c == '_') && (i == 0 || !(is_ident_char(chars[i - 1]) || chars[i - 1] == ':'));
        if !starts { i += 1; continue; }
        let mut segments = Vec::new();
        let mut j = i;
        loop {
            let start = j;
            while j < chars.len() && is_ident_char(chars[j]) { j += 1; }
            if start == j { break; }
            segments.push(chars[start..j].iter().collect::<String>());
            if j + 1 < chars.len() && chars[j] == ':' && chars[j + 1] == ':' {
                j += 2;
                while j < chars.len() && chars[j].is_whitespace() { j += 1; }
                if j < chars.len() && chars[j] == '{' {
                    if let Some(end) = matching_brace(&chars, j) {
                        let inner: String = chars[j + 1..end].iter().collect();
                        expand_group(&segments, &inner, &mut out);
                        j = end + 1;
                    }
                    segments.clear();
                    break;
                }
                continue;
            }
            break;
        }
        if segments.len() >= 2 { out.push(segments); }
        i = j.max(i + 1);
    }
    out
}

fn expand_group(prefix: &[String], inner: &str, out: &mut Vec<Vec<String>>) {
    for item in split_top(inner) {
        let item = item.split(" as ").next().unwrap_or("").trim();
        if item.is_empty() || item == "*" { continue; }
        if item == "self" {
            out.push(prefix.to_vec());
            continue;
        }
        for mut tail in paths_in(item).into_iter().chain(single(item)) {
            let mut full = prefix.to_vec();
            full.append(&mut tail);
            out.push(full);
        }
    }
}

/// A lone identifier leaf (`a` in `{a, b::c}`) that `paths_in` skips.
fn single(item: &str) -> Option<Vec<String>> {
    (!item.contains("::") && item.chars().all(is_ident_char) && !item.is_empty()).then(|| vec![item.to_string()])
}

fn split_top(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut current = String::new();
    for c in text.chars() {
        match c {
            '{' => { depth += 1; current.push(c); }
            '}' => { depth -= 1; current.push(c); }
            ',' if depth == 0 => out.push(std::mem::take(&mut current)),
            _ => current.push(c),
        }
    }
    out.push(current);
    out
}

fn matching_brace(chars: &[char], open: usize) -> Option<usize> {
    let mut depth = 0;
    for (k, c) in chars.iter().enumerate().skip(open) {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 { return Some(k); }
            }
            _ => {}
        }
    }
    None
}

/// `mod name;` declarations (any visibility and attributes on the same line),
/// with the `#[path = "..."]` value when one is given.
pub fn mod_decls(src: &str) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    let mut pending_path: Option<String> = None;
    for line in src.lines() {
        let mut rest = line.trim();
        while let Some(attr) = rest.strip_prefix("#[") {
            let Some(end) = attr.find(']') else { break };
            let body = attr[..end].trim();
            if let Some(value) = body.strip_prefix("path").map(str::trim).and_then(|v| v.strip_prefix('=')) {
                pending_path = Some(value.trim().trim_matches('"').replace('\\', "/"));
            }
            rest = attr[end + 1..].trim_start();
        }
        if rest.is_empty() { continue; }
        if let Some(after) = rest.strip_prefix("pub") {
            rest = after.trim_start();
            if rest.starts_with('(') {
                rest = rest.find(')').map(|e| rest[e + 1..].trim_start()).unwrap_or(rest);
            }
        }
        let path = pending_path.take();
        let Some(decl) = rest.strip_prefix("mod ") else { continue };
        let Some(name) = decl.trim_end().strip_suffix(';') else { continue };
        let name = name.trim();
        if !name.is_empty() && name.chars().all(is_ident_char) {
            out.push((name.to_string(), path));
        }
    }
    out
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn parent(path: &str) -> String {
    match path.rfind('/') { Some(i) => path[..i].to_string(), None => String::new() }
}

fn join(base: &str, rest: &str) -> String {
    let joined = if base.is_empty() { rest.to_string() } else { format!("{base}/{rest}") };
    crate::walk::norm_slash(&joined)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, rel: &str, text: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn paths_expand_groups_and_skip_globs() {
        let got = paths_in("use crate::{sight::Camera, game::{self, Ctx}, *};\nlet x = specula_core::Json::new();\nfoo(a::b);");
        let text: Vec<String> = got.iter().map(|p| p.join("::")).collect();
        assert!(text.contains(&"crate::sight::Camera".to_string()), "{text:?}");
        assert!(text.contains(&"crate::game".to_string()) && text.contains(&"crate::game::Ctx".to_string()), "{text:?}");
        assert!(text.contains(&"specula_core::Json::new".to_string()) && text.contains(&"a::b".to_string()), "{text:?}");
        assert!(!text.iter().any(|t| t.contains('*')), "{text:?}");
    }

    #[test]
    fn mod_declarations_with_visibility_attributes_and_path() {
        let decls = mod_decls("pub mod game;\nmod sight_tools;\npub(crate) mod a;\n#[cfg(test)] mod tests;\n#[path = \"x/y.rs\"]\nmod z;\nmod inline { }\n");
        assert_eq!(decls, vec![
            ("game".to_string(), None), ("sight_tools".to_string(), None), ("a".to_string(), None),
            ("tests".to_string(), None), ("z".to_string(), Some("x/y.rs".to_string())),
        ]);
    }

    /// A two-crate workspace: the edges a rustc user would draw.
    #[test]
    fn workspace_edges_follow_module_files() {
        let dir = std::env::temp_dir().join(format!("trammel-rustmods-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        write(&dir, "Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]\n");
        write(&dir, "crates/core/Cargo.toml", "[package]\nname = \"my-core\"\n");
        write(&dir, "crates/core/src/lib.rs", "pub mod math;\n");
        write(&dir, "crates/core/src/math.rs", "pub struct Vec3;\n");
        write(&dir, "crates/app/Cargo.toml", "[package]\nname = \"app\"\n");
        write(&dir, "crates/app/src/lib.rs", "pub mod game;\npub mod sight;\nmod ui;\n");
        write(&dir, "crates/app/src/sight.rs", "use my_core::math::Vec3;\npub struct Camera;\n");
        write(&dir, "crates/app/src/game.rs", "use crate::sight::Camera;\nuse super::ui::{self, panel};\n");
        write(&dir, "crates/app/src/ui/mod.rs", "pub mod panel;\n");
        write(&dir, "crates/app/src/ui/panel.rs", "use super::super::sight::Camera;\nuse self::inner::X;\nmod inner;\n");
        write(&dir, "crates/app/src/ui/panel/inner.rs", "pub struct X;\n");
        write(&dir, "crates/app/tests/game.rs", "mod common;\nuse app::game;\n");
        write(&dir, "crates/app/tests/common/mod.rs", "pub fn setup() {}\n");
        let graph = rust_imports(&dir);
        let deps = |f: &str| graph.get(f).cloned().unwrap_or_default();
        assert_eq!(deps("crates/app/src/lib.rs"), vec!["crates/app/src/game.rs", "crates/app/src/sight.rs", "crates/app/src/ui/mod.rs"]);
        assert_eq!(deps("crates/app/src/sight.rs"), vec!["crates/core/src/math.rs"]);
        assert_eq!(deps("crates/app/src/game.rs"), vec!["crates/app/src/sight.rs", "crates/app/src/ui/mod.rs", "crates/app/src/ui/panel.rs"]);
        assert_eq!(deps("crates/app/src/ui/panel.rs"), vec!["crates/app/src/sight.rs", "crates/app/src/ui/panel/inner.rs"]);
        assert_eq!(deps("crates/app/tests/game.rs"), vec!["crates/app/src/game.rs", "crates/app/tests/common/mod.rs"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
