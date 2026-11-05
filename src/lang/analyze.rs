//! Language detection and project analysis. Python symbols come from the
//! shared parser; the other fourteen languages use the hand-written extractors.

use crate::config::ProjectConfig;
use crate::lang::python_symbols::extract;
use crate::lang::{imports, symbols};
use crate::syntax::query::SymbolHit;
use crate::walk::{collect_files, is_ignored_dir, read_lossy};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

pub const LANGUAGES: &[&str] = &[
    "python", "typescript", "javascript", "go", "rust", "cpp", "c", "java", "kotlin", "csharp",
    "ruby", "php", "swift", "dart", "zig",
];

pub fn extensions(language: &str) -> &'static [&'static str] {
    match language {
        "python" => &[".py"],
        "typescript" => &[".ts", ".tsx", ".js", ".jsx", ".mts", ".mjs"],
        "javascript" => &[".js", ".jsx", ".mjs"],
        "go" => &[".go"],
        "rust" => &[".rs"],
        "cpp" | "c" => &[".c", ".cpp", ".cc", ".cxx", ".h", ".hpp", ".hxx"],
        "java" | "kotlin" => &[".java", ".kt", ".kts"],
        "csharp" => &[".cs"],
        "ruby" => &[".rb"],
        "php" => &[".php"],
        "swift" => &[".swift"],
        "dart" => &[".dart"],
        "zig" => &[".zig"],
        _ => &[".py"],
    }
}

/// The test command when the caller gives none: `test_cmd` from
/// `.trammel.json` / `[tool.trammel]`, else the default for `language`, else
/// the default for the detected language. Detection only runs when needed.
pub fn default_test_cmd(root: &Path, language: Option<&str>) -> Vec<String> {
    let cfg = crate::config::load(root);
    if let Some(cmd) = cfg.test_cmd.clone().filter(|c| !c.is_empty()) {
        return cmd;
    }
    let language = language.map(str::to_string).unwrap_or_else(|| detect(root, &cfg));
    test_cmd(&language, root)
}

pub fn test_cmd(language: &str, root: &Path) -> Vec<String> {
    match language {
        "python" => {
            let start = if root.join("tests").is_dir() { "tests" } else { "." };
            let mut cmd = crate::harness::python_cmd();
            cmd.extend(["-m", "unittest", "discover", "-q", "-s", start, "-p", "test_*.py"].map(String::from));
            cmd
        }
        "go" => vec!["go".into(), "test".into(), "./...".into()],
        "rust" => vec!["cargo".into(), "test".into()],
        "javascript" | "typescript" => {
            if let Some(text) = read_lossy(&root.join("package.json")) {
                if text.contains("\"test\"") { return vec!["npm".into(), "test".into()]; }
            }
            vec!["npx".into(), "jest".into(), "--passWithNoTests".into()]
        }
        "ruby" => vec!["ruby".into(), "-Itest".into(), "-e".into(), "Dir['test/**/test_*.rb'].each {|f| require f}".into()],
        "php" => vec!["phpunit".into()],
        "java" | "kotlin" => vec!["mvn".into(), "-q".into(), "test".into()],
        "csharp" => vec!["dotnet".into(), "test".into()],
        "swift" => vec!["swift".into(), "test".into()],
        "dart" => vec!["dart".into(), "test".into()],
        "zig" => vec!["zig".into(), "build".into(), "test".into()],
        _ => crate::harness::no_op_cmd(),
    }
}

pub fn detect(root: &Path, cfg: &ProjectConfig) -> String {
    if let Some(lang) = &cfg.language {
        if LANGUAGES.contains(&lang.as_str()) { return lang.clone(); }
    }
    if let Some(lang) = detect_config(root) { return lang; }
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for lang in ["python", "typescript", "javascript", "go", "rust", "cpp", "java", "csharp", "ruby", "php", "swift", "dart", "zig"] {
        counts.insert(lang, collect_files(root, extensions(lang)).len());
    }
    counts.into_iter().max_by_key(|(_, n)| *n).filter(|(_, n)| *n > 0).map(|(k, _)| k.to_string()).unwrap_or_else(|| "python".into())
}

fn detect_config(root: &Path) -> Option<String> {
    let has = |name: &str| root.join(name).is_file();
    if has("Cargo.toml") { return Some("rust".into()); }
    if has("go.mod") { return Some("go".into()); }
    if has("Package.swift") { return Some("swift".into()); }
    if has("build.zig") { return Some("zig".into()); }
    if has("pubspec.yaml") { return Some("dart".into()); }
    if has("tsconfig.json") { return Some("typescript".into()); }
    if has("CMakeLists.txt") || has("SConstruct") { return Some("cpp".into()); }
    if let Ok(entries) = fs::read_dir(root) {
        if entries.flatten().any(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.ends_with(".csproj") || name.ends_with(".sln")
        }) { return Some("csharp".into()); }
    }
    if has("setup.py") || has("setup.cfg") { return Some("python".into()); }
    if has("pyproject.toml") {
        if read_lossy(&root.join("pyproject.toml")).unwrap_or_default().contains("[project]") {
            return Some("python".into());
        }
    }
    if has("Gemfile") { return Some("ruby".into()); }
    if has("composer.json") { return Some("php".into()); }
    if has("package.json") { return Some("javascript".into()); }
    if has("build.gradle") || has("build.gradle.kts") || has("pom.xml") { return Some("java".into()); }
    None
}

pub struct Analysis {
    pub language: String,
    pub symbols: BTreeMap<String, Vec<String>>,
    pub typed: BTreeMap<String, Vec<(String, String)>>,
    pub imports: BTreeMap<String, Vec<String>>,
}

pub fn analyze(root: &Path, language: &str) -> Analysis {
    if language == "python" {
        let (symbols, typed, imports) = python_project(root);
        return Analysis { language: language.to_string(), symbols, typed, imports };
    }
    let mut symbols = BTreeMap::new();
    let mut typed = BTreeMap::new();
    for file in crate::walk::walk_sources(root, extensions(language)) {
        let found = symbols::collect(language, &file.source);
        if found.is_empty() { continue; }
        symbols.insert(file.rel.clone(), found.iter().map(|(n, _)| n.clone()).collect());
        typed.insert(file.rel, found.into_iter().map(|(n, k)| (n, k.to_string())).collect());
    }
    Analysis {
        language: language.to_string(),
        symbols,
        typed,
        imports: imports::imports(language, root),
    }
}

/// One read per file, then symbols and imports on a worker pool. Results are
/// merged by path, so the maps match a serial walk.
fn python_project(root: &Path) -> (BTreeMap<String, Vec<String>>, BTreeMap<String, Vec<(String, String)>>, BTreeMap<String, Vec<String>>) {
    let files = collect_files(root, &[".py"]);
    let (modules, module_set) = imports::python_module_index(&files);
    // Largest sources first so one heavy module cannot sit behind another
    // on the same worker. Merge is by path, so this does not change output.
    let mut loaded: Vec<(usize, String, String)> = files.iter().filter_map(|rel| {
        let source = read_lossy(&root.join(rel))?;
        Some((source.len(), rel.clone(), source))
    }).collect();
    loaded.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).clamp(1, loaded.len().max(1));
    let next = std::sync::atomic::AtomicUsize::new(0);
    let mut parts: Vec<Vec<(String, Vec<(String, &'static str)>, BTreeSet<String>)>> = Vec::new();
    std::thread::scope(|scope| {
        let mut joins = Vec::with_capacity(workers);
        for _ in 0..workers {
            let loaded = &loaded;
            let modules = &modules;
            let module_set = &module_set;
            let next = &next;
            joins.push(scope.spawn(move || {
                let mut local = Vec::new();
                loop {
                    let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if index >= loaded.len() { break; }
                    let (_, rel, source) = &loaded[index];
                    let names = python_names(&extract(source));
                    let deps = imports::python_file_deps(source, rel, module_set, modules);
                    if names.is_empty() && deps.is_empty() { continue; }
                    local.push((rel.clone(), names, deps));
                }
                local
            }));
        }
        for join in joins {
            if let Ok(local) = join.join() { parts.push(local); }
        }
    });
    let mut symbols = BTreeMap::new();
    let mut typed = BTreeMap::new();
    let mut graph = BTreeMap::new();
    for local in parts {
        for (rel, names, deps) in local {
            if !names.is_empty() {
                typed.insert(rel.clone(), names.iter().map(|(n, k)| (n.clone(), (*k).to_string())).collect());
                symbols.insert(rel.clone(), names.into_iter().map(|(n, _)| n).collect());
            }
            if !deps.is_empty() {
                graph.insert(rel, deps.into_iter().collect());
            }
        }
    }
    (symbols, typed, graph)
}

fn python_names(hits: &[SymbolHit]) -> Vec<(String, &'static str)> {
    hits.iter()
        .filter(|hit| hit.role == "definition" && (hit.kind == "function" || hit.kind == "class"))
        .map(|hit| (hit.name.clone(), hit.kind))
        .collect()
}

pub fn estimate(root: &Path, language: &str) -> usize {
    let mut count = 0usize;
    let mut stack = vec![root.to_path_buf()];
    let exts = extensions(language);
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if entry.path().is_dir() {
                if !is_ignored_dir(&name) { stack.push(entry.path()); }
            } else if exts.iter().any(|ext| name.ends_with(ext)) {
                count += 1;
            }
        }
    }
    count
}

#[cfg(test)]
mod test_cmd_tests {
    use super::*;

    #[test]
    fn the_default_test_command_follows_config_then_language() {
        let dir = std::env::temp_dir().join(format!("trammel-testcmd-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("go.mod"), "module example.com/x\n").unwrap();
        fs::write(dir.join("main.go"), "package main\n\nfunc main() {}\n").unwrap();
        assert_eq!(default_test_cmd(&dir, None), vec!["go", "test", "./..."], "detected, not Python");
        assert_eq!(default_test_cmd(&dir, Some("rust")), vec!["cargo", "test"]);
        fs::write(dir.join(".trammel.json"), r#"{"test_cmd": ["make", "check"]}"#).unwrap();
        assert_eq!(default_test_cmd(&dir, Some("rust")), vec!["make", "check"], "project config wins");
        let _ = fs::remove_dir_all(&dir);
    }
}
