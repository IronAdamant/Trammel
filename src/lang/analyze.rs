//! Language detection and project analysis. Python symbols come from the
//! shared parser; the other fourteen languages use the hand-written extractors.

use crate::config::ProjectConfig;
use crate::lang::python_symbols::extract;
use crate::lang::{imports, symbols};
use crate::syntax::query::SymbolHit;
use crate::walk::{collect_files, is_ignored_dir, read_lossy};
use std::collections::BTreeMap;
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

pub fn test_cmd(language: &str, root: &Path) -> Vec<String> {
    match language {
        "python" => {
            let start = if root.join("tests").is_dir() { "tests" } else { "." };
            vec!["python3".into(), "-m".into(), "unittest".into(), "discover".into(), "-q".into(), "-s".into(), start.into(), "-p".into(), "test_*.py".into()]
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
        _ => vec!["true".into()],
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
    let mut symbols = BTreeMap::new();
    let mut typed = BTreeMap::new();
    if language == "python" {
        for rel in collect_files(root, &[".py"]) {
            let Some(source) = read_lossy(&root.join(&rel)) else { continue };
            let hits = extract(&source);
            let names = python_names(&hits);
            if !names.is_empty() {
                typed.insert(rel.clone(), names.iter().map(|(n, k)| (n.clone(), (*k).to_string())).collect());
                symbols.insert(rel, names.into_iter().map(|(n, _)| n).collect());
            }
        }
    } else {
        for file in crate::walk::walk_sources(root, extensions(language)) {
            let found = symbols::collect(language, &file.source);
            if found.is_empty() { continue; }
            symbols.insert(file.rel.clone(), found.iter().map(|(n, _)| n.clone()).collect());
            typed.insert(file.rel, found.into_iter().map(|(n, k)| (n, k.to_string())).collect());
        }
    }
    Analysis {
        language: language.to_string(),
        symbols,
        typed,
        imports: imports::imports(language, root),
    }
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
