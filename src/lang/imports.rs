//! Import graphs. Resolution rules match the Python resolvers, including the
//! cases where a prefix has to exist in the project before an edge is kept.

use crate::lang::blank::{strip_c_comments, strip_hash_comments, strip_php_comments};
use crate::walk::{collect_files, is_ignored_dir, norm_slash, read_lossy};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub fn imports(language: &str, root: &Path) -> BTreeMap<String, Vec<String>> {
    match language {
        "python" => python_imports(root),
        "typescript" => ts_imports(root, &[".ts", ".tsx", ".js", ".jsx", ".mts", ".mjs"]),
        "javascript" => ts_imports(root, &[".js", ".jsx", ".mjs"]),
        "go" => go_imports(root),
        "rust" => rust_imports(root),
        "cpp" | "c" => cpp_imports(root),
        "java" | "kotlin" => java_imports(root),
        "csharp" => csharp_imports(root),
        "ruby" => ruby_imports(root),
        "php" => php_imports(root),
        "swift" => swift_imports(root),
        "dart" => dart_imports(root),
        "zig" => zig_imports(root),
        _ => BTreeMap::new(),
    }
}

pub(crate) fn python_module_index(files: &[String]) -> (BTreeMap<String, String>, BTreeSet<String>) {
    let mut modules = BTreeMap::new();
    for rel in files {
        let mut module = rel.trim_end_matches(".py").replace('/', ".");
        if let Some(stripped) = module.strip_suffix(".__init__") {
            module = stripped.to_string();
        }
        modules.insert(module, rel.clone());
    }
    let module_set = modules.keys().cloned().collect();
    (modules, module_set)
}

pub(crate) fn python_file_deps(source: &str, rel: &str, module_set: &BTreeSet<String>, modules: &BTreeMap<String, String>) -> BTreeSet<String> {
    let mut deps = BTreeSet::new();
    for name in python_import_names(source) {
        resolve_py(&name, module_set, modules, &mut deps);
    }
    deps.remove(rel);
    deps
}

fn python_imports(root: &Path) -> BTreeMap<String, Vec<String>> {
    let files = collect_files(root, &[".py"]);
    let (modules, module_set) = python_module_index(&files);
    let mut found = Vec::new();
    let workers = worker_count(files.len());
    std::thread::scope(|scope| {
        let mut joins = Vec::with_capacity(workers);
        for worker in 0..workers {
            let files = &files;
            let modules = &modules;
            let module_set = &module_set;
            joins.push(scope.spawn(move || {
                let mut local = Vec::new();
                for (index, rel) in files.iter().enumerate() {
                    if index % workers != worker { continue; }
                    let Some(source) = read_lossy(&root.join(rel)) else { continue };
                    let deps = python_file_deps(&source, rel, module_set, modules);
                    if !deps.is_empty() { local.push((rel.clone(), deps)); }
                }
                local
            }));
        }
        for join in joins {
            if let Ok(local) = join.join() { found.extend(local); }
        }
    });
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found.into_iter().map(|(rel, deps)| (rel, deps.into_iter().collect())).collect()
}

fn worker_count(files: usize) -> usize {
    let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    cpus.clamp(1, files.max(1))
}

fn python_import_names(source: &str) -> Vec<String> {
    let tokens = crate::lang::python::lex::lex(source);
    let mut names = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        skip_trivia(source, &tokens, &mut i);
        if i >= tokens.len() { break; }
        let text = token_text(source, &tokens[i]);
        if text == "from" {
            i += 1;
            skip_trivia(source, &tokens, &mut i);
            while i < tokens.len() && token_text(source, &tokens[i]) == "." {
                i += 1;
                skip_trivia(source, &tokens, &mut i);
            }
            if let Some(name) = dotted(source, &tokens, &mut i) {
                names.push(name);
            }
        } else if text == "import" {
            i += 1;
            loop {
                skip_trivia(source, &tokens, &mut i);
                let Some(name) = dotted(source, &tokens, &mut i) else { break };
                names.push(name);
                skip_trivia(source, &tokens, &mut i);
                skip_alias(source, &tokens, &mut i);
                skip_trivia(source, &tokens, &mut i);
                if i < tokens.len() && token_text(source, &tokens[i]) == "," {
                    i += 1;
                    continue;
                }
                break;
            }
        } else {
            i += 1;
        }
    }
    names
}

fn skip_trivia(source: &str, tokens: &[crate::syntax::cst::Token], i: &mut usize) {
    while *i < tokens.len() {
        let text = token_text(source, &tokens[*i]);
        if text.chars().all(|c| c.is_whitespace()) || text.starts_with('#') {
            *i += 1;
        } else {
            break;
        }
    }
}

fn token_text<'a>(source: &'a str, token: &crate::syntax::cst::Token) -> &'a str {
    source.get(token.start as usize..token.end as usize).unwrap_or("")
}

fn dotted(source: &str, tokens: &[crate::syntax::cst::Token], i: &mut usize) -> Option<String> {
    let mut parts = Vec::new();
    while *i < tokens.len() {
        skip_trivia(source, tokens, i);
        if *i >= tokens.len() { break; }
        let text = token_text(source, &tokens[*i]);
        if text.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') {
            parts.push(text.to_string());
            *i += 1;
            if *i < tokens.len() && token_text(source, &tokens[*i]) == "." {
                *i += 1;
                continue;
            }
            break;
        }
        break;
    }
    if parts.is_empty() { None } else { Some(parts.join(".")) }
}

fn skip_alias(source: &str, tokens: &[crate::syntax::cst::Token], i: &mut usize) {
    if *i < tokens.len() && token_text(source, &tokens[*i]) == "as" {
        *i += 1;
        if *i < tokens.len() { *i += 1; }
    }
}

fn resolve_py(name: &str, module_set: &BTreeSet<String>, modules: &BTreeMap<String, String>, deps: &mut BTreeSet<String>) {
    if let Some(rel) = modules.get(name) {
        if module_set.contains(name) { deps.insert(rel.clone()); return; }
    }
    let parts: Vec<&str> = name.split('.').collect();
    for n in (1..=parts.len()).rev() {
        let prefix = parts[..n].join(".");
        if let Some(rel) = modules.get(&prefix) {
            deps.insert(rel.clone());
            return;
        }
    }
}

fn go_imports(root: &Path) -> BTreeMap<String, Vec<String>> {
    let (module, mod_dir) = read_go_mod(root);
    let Some(module) = module else { return BTreeMap::new() };
    let scope = root.strip_prefix(&mod_dir).ok().map(|p| crate::walk::to_slash(p)).unwrap_or_default();
    let mut prefix = format!("{module}/");
    if !scope.is_empty() && scope != "." {
        prefix = format!("{module}/{scope}/");
    }
    let mut dir_files: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut sources = BTreeMap::new();
    for file in crate::walk::walk_sources(root, &[".go"]) {
        sources.insert(file.rel.clone(), strip_c_comments(&file.source));
        if !file.rel.ends_with("_test.go") {
            let dir = parent(&file.rel);
            dir_files.entry(dir).or_default().push(file.rel);
        }
    }
    let mut graph = BTreeMap::new();
    for (rel, src) in &sources {
        let mut deps = BTreeSet::new();
        for imp in go_import_paths(src) {
            if let Some(rest) = imp.strip_prefix(&prefix) {
                for dep in dir_files.get(rest).into_iter().flatten() {
                    if dep != rel { deps.insert(dep.clone()); }
                }
            }
        }
        if !deps.is_empty() { graph.insert(rel.clone(), deps.into_iter().collect()); }
    }
    graph
}

fn go_import_paths(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = src;
    while let Some(at) = rest.find("import") {
        let after = &rest[at + "import".len()..];
        let trimmed = after.trim_start();
        if trimmed.starts_with('(') {
            if let Some(end) = trimmed.find(')') {
                for quoted in quoted_strings(&trimmed[1..end]) { out.push(quoted); }
                rest = &trimmed[end + 1..];
                continue;
            }
        }
        if trimmed.starts_with('"') {
            if let Some(path) = one_string(trimmed) { out.push(path); }
        }
        rest = if after.is_empty() { "" } else { &after[1.min(after.len())..] };
        if rest.is_empty() { break; }
    }
    out
}

fn quoted_strings(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('"') {
        let body = &rest[start + 1..];
        if let Some(end) = body.find('"') {
            out.push(body[..end].to_string());
            rest = &body[end + 1..];
        } else { break; }
    }
    out
}

fn one_string(text: &str) -> Option<String> {
    let body = text.strip_prefix('"')?;
    Some(body[..body.find('"')?].to_string())
}

fn read_go_mod(root: &Path) -> (Option<String>, std::path::PathBuf) {
    let mut current = root.to_path_buf();
    for _ in 0..20 {
        let go_mod = current.join("go.mod");
        if go_mod.is_file() {
            if let Some(text) = read_lossy(&go_mod) {
                for line in text.lines() {
                    if let Some(rest) = line.trim().strip_prefix("module ") {
                        return (Some(rest.split_whitespace().next().unwrap_or("").to_string()), current);
                    }
                }
            }
            return (None, current);
        }
        if !current.pop() { break; }
    }
    (None, root.to_path_buf())
}

fn rust_imports(root: &Path) -> BTreeMap<String, Vec<String>> {
    let files: BTreeSet<String> = collect_files(root, &[".rs"]).into_iter().collect();
    let crates = cargo_crates(root);
    let mut graph = BTreeMap::new();
    for rel in &files {
        let Some(src) = read_lossy(&root.join(rel)).map(|s| strip_c_comments(&s)) else { continue };
        let mut deps = BTreeSet::new();
        let dir = parent(rel);
        for path in capture_all(&src, "use crate::") {
            resolve_rust_mod(path.split("::").next().unwrap_or(""), "", &files, &mut deps);
        }
        for path in capture_all(&src, "use super::") {
            resolve_rust_mod(path.split("::").next().unwrap_or(""), &parent(&dir), &files, &mut deps);
        }
        for path in capture_all(&src, "use self::") {
            resolve_rust_mod(path.split("::").next().unwrap_or(""), &dir, &files, &mut deps);
        }
        for (name, crate_dir) in &crates {
            let needle = format!("use {name}::");
            for path in capture_all(&src, &needle) {
                resolve_rust_mod(path.split("::").next().unwrap_or(""), crate_dir, &files, &mut deps);
            }
        }
        for line in src.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("mod ") {
                if let Some(name) = rest.trim_end().strip_suffix(';') {
                    let name = name.trim();
                    if is_ident(name) { resolve_rust_mod(name, &dir, &files, &mut deps); }
                }
            }
        }
        deps.remove(rel);
        if !deps.is_empty() { graph.insert(rel.clone(), deps.into_iter().collect()); }
    }
    graph
}

fn resolve_rust_mod(name: &str, base: &str, files: &BTreeSet<String>, deps: &mut BTreeSet<String>) {
    let a = if base.is_empty() { format!("{name}.rs") } else { format!("{base}/{name}.rs") };
    let b = if base.is_empty() { format!("{name}/mod.rs") } else { format!("{base}/{name}/mod.rs") };
    for candidate in [a, b] {
        if files.contains(&candidate) { deps.insert(candidate); }
    }
}

fn cargo_crates(root: &Path) -> BTreeMap<String, String> {
    let mut crates = BTreeMap::new();
    let Some(text) = read_lossy(&root.join("Cargo.toml")) else { return crates };
    let Some(start) = text.find("members") else { return crates };
    let Some(open) = text[start..].find('[') else { return crates };
    let Some(close) = text[start + open..].find(']') else { return crates };
    let body = &text[start + open + 1..start + open + close];
    for member in quoted_strings(body) {
        let dir = root.join(&member);
        if !dir.is_dir() { continue; }
        let mut name = member.rsplit('/').next().unwrap_or(&member).replace('-', "_");
        if let Some(member_toml) = read_lossy(&dir.join("Cargo.toml")) {
            if let Some(found) = toml_string(&member_toml, "name") { name = found.replace('-', "_"); }
        }
        let src = format!("{member}/src");
        let base = if root.join(&src).is_dir() { src } else { member };
        crates.insert(name, base);
    }
    crates
}

fn toml_string(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(key) {
            let rest = rest.trim().strip_prefix('=')?.trim();
            return one_string(rest);
        }
    }
    None
}

fn cpp_imports(root: &Path) -> BTreeMap<String, Vec<String>> {
    let exts = [".c", ".cpp", ".cc", ".cxx", ".h", ".hpp", ".hxx"];
    let files: BTreeSet<String> = collect_files(root, &exts).into_iter().collect();
    let mut graph = BTreeMap::new();
    for rel in &files {
        let Some(src) = read_lossy(&root.join(rel)).map(|s| strip_c_comments(&s)) else { continue };
        let mut deps = BTreeSet::new();
        let mut rest = src.as_str();
        while let Some(at) = rest.find("#include") {
            let after = rest[at + "#include".len()..].trim_start();
            if let Some(path) = after.strip_prefix('"').and_then(|s| s.split('"').next()) {
                let dir = parent(rel);
                for base in [dir.as_str(), ""] {
                    let candidate = if base.is_empty() { path.to_string() } else { norm_slash(&format!("{base}/{path}")) };
                    if files.contains(&candidate) && candidate != *rel {
                        deps.insert(candidate);
                        break;
                    }
                }
            }
            rest = &rest[at + 8..];
        }
        if !deps.is_empty() { graph.insert(rel.clone(), deps.into_iter().collect()); }
    }
    graph
}

fn java_imports(root: &Path) -> BTreeMap<String, Vec<String>> {
    namespace_imports(root, &[".java", ".kt", ".kts"], "package", true)
}

fn csharp_imports(root: &Path) -> BTreeMap<String, Vec<String>> {
    let exts = [".cs"];
    let (ns_to_files, sources) = namespaces(root, &exts, "namespace", None);
    let mut graph = BTreeMap::new();
    for (rel, src) in sources {
        let mut deps = BTreeSet::new();
        for line in src.lines() {
            if let Some(rest) = line.trim().strip_prefix("using ") {
                if let Some(name) = rest.trim().strip_suffix(';') {
                    resolve_ns(name.trim(), &ns_to_files, &rel, &mut deps);
                }
            }
        }
        if !deps.is_empty() { graph.insert(rel, deps.into_iter().collect()); }
    }
    graph
}

fn namespace_imports(root: &Path, exts: &[&str], keyword: &str, siblings: bool) -> BTreeMap<String, Vec<String>> {
    let roots = java_source_roots(root);
    let (ns_to_files, sources) = namespaces(root, exts, keyword, Some(&roots));
    let mut graph = BTreeMap::new();
    for (rel, src) in sources {
        let mut deps = BTreeSet::new();
        for line in src.lines() {
            let trimmed = line.trim();
            let rest = trimmed.strip_prefix("import").or_else(|| trimmed.strip_prefix("import static"));
            if let Some(rest) = rest {
                let rest = rest.trim().trim_end_matches(';').trim();
                let name = rest.split_whitespace().next().unwrap_or("");
                if !name.is_empty() { resolve_ns(name, &ns_to_files, &rel, &mut deps); }
            }
        }
        if deps.is_empty() && siblings {
            if let Some(pkg) = package_name(&src, keyword) {
                for sibling in ns_to_files.get(&pkg).into_iter().flatten() {
                    if sibling != &rel { deps.insert(sibling.clone()); }
                }
            }
        }
        if !deps.is_empty() { graph.insert(rel, deps.into_iter().collect()); }
    }
    graph
}

fn package_name(src: &str, keyword: &str) -> Option<String> {
    for line in src.lines() {
        if let Some(rest) = line.trim().strip_prefix(keyword) {
            if rest.starts_with(char::is_whitespace) {
                let name = rest.trim().trim_end_matches(';').trim();
                if !name.is_empty() { return Some(name.to_string()); }
            }
        }
    }
    None
}

fn namespaces(root: &Path, exts: &[&str], keyword: &str, source_roots: Option<&[std::path::PathBuf]>) -> (BTreeMap<String, Vec<String>>, BTreeMap<String, String>) {
    let mut ns = BTreeMap::new();
    let mut sources = BTreeMap::new();
    let roots: Vec<&Path> = source_roots.map(|r| r.iter().map(|p| p.as_path()).collect()).unwrap_or_else(|| vec![root]);
    for src_root in roots {
        if !src_root.exists() { continue; }
        for file in crate::walk::walk_sources(src_root, exts) {
            let rel = if src_root == root { file.rel.clone() } else {
                let full = src_root.join(&file.rel);
                crate::walk::rel_slash(&full, root)
            };
            let stripped = strip_c_comments(&file.source);
            if let Some(pkg) = package_name(&stripped, keyword) {
                ns.entry(pkg).or_insert_with(Vec::new).push(rel.clone());
            }
            sources.insert(rel, stripped);
        }
    }
    (ns, sources)
}

fn resolve_ns(import_path: &str, ns: &BTreeMap<String, Vec<String>>, rel: &str, deps: &mut BTreeSet<String>) {
    let parts: Vec<&str> = import_path.split('.').collect();
    for n in (1..=parts.len()).rev() {
        let prefix = parts[..n].join(".");
        if let Some(files) = ns.get(&prefix) {
            for dep in files { if dep != rel { deps.insert(dep.clone()); } }
            return;
        }
    }
}

fn java_source_roots(root: &Path) -> Vec<std::path::PathBuf> {
    let mut roots = Vec::new();
    if root.join("build.gradle").is_file() || root.join("build.gradle.kts").is_file() {
        for candidate in ["src/main/java", "src/main/kotlin", "src/test/java", "src/test/kotlin"] {
            let full = root.join(candidate);
            if full.is_dir() { roots.push(full); }
        }
    }
    if roots.is_empty() && root.join("pom.xml").is_file() {
        for candidate in ["src/main/java", "src/test/java"] {
            let full = root.join(candidate);
            if full.is_dir() { roots.push(full); }
        }
    }
    if roots.is_empty() { roots.push(root.to_path_buf()); }
    roots
}

fn ruby_imports(root: &Path) -> BTreeMap<String, Vec<String>> {
    let files: BTreeSet<String> = collect_files(root, &[".rb"]).into_iter().collect();
    let mut stem = BTreeMap::new();
    let mut base = BTreeMap::new();
    for rel in &files {
        let s = rel.trim_end_matches(".rb");
        stem.insert(s.to_string(), rel.clone());
        base.entry(s.rsplit('/').next().unwrap_or(s).to_string()).or_insert_with(|| rel.clone());
    }
    let mut graph = BTreeMap::new();
    for rel in &files {
        let Some(src) = read_lossy(&root.join(rel)).map(|s| strip_hash_comments(&s)) else { continue };
        let mut deps = BTreeSet::new();
        for req in ruby_requires(&src) {
            if let Some(hit) = stem.get(&req).or_else(|| base.get(&req)) {
                if hit != rel { deps.insert(hit.clone()); }
            }
        }
        if !deps.is_empty() { graph.insert(rel.clone(), deps.into_iter().collect()); }
    }
    graph
}

fn ruby_requires(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    for marker in ["require_relative", "require"] {
        let mut rest = src;
        while let Some(at) = rest.find(marker) {
            let after = rest[at + marker.len()..].trim_start();
            if let Some(path) = after.strip_prefix('"').and_then(|s| s.split('"').next()).or_else(|| after.strip_prefix('\'').and_then(|s| s.split('\'').next())) {
                out.push(path.to_string());
            }
            rest = &rest[at + marker.len()..];
        }
    }
    out
}

fn php_imports(root: &Path) -> BTreeMap<String, Vec<String>> {
    let (ns, sources) = namespaces(root, &[".php"], "namespace", None);
    let dot: BTreeMap<String, Vec<String>> = ns.into_iter().map(|(k, v)| (k.replace('\\', "."), v)).collect();
    let mut graph = BTreeMap::new();
    for (rel, src) in sources {
        let src = strip_php_comments(&src);
        let mut deps = BTreeSet::new();
        for line in src.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("use ") {
                let rest = rest.trim().trim_end_matches(';').trim();
                resolve_ns(&rest.replace('\\', "."), &dot, &rel, &mut deps);
            }
        }
        if !deps.is_empty() { graph.insert(rel, deps.into_iter().collect()); }
    }
    graph
}

fn swift_imports(root: &Path) -> BTreeMap<String, Vec<String>> {
    let files: BTreeSet<String> = collect_files(root, &[".swift"]).into_iter().collect();
    let mut modules: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for dirname in ["Sources", "Tests"] {
        let dir = root.join(dirname);
        if !dir.is_dir() { continue; }
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if is_ignored_dir(&name) { continue; }
                    let prefix = format!("{dirname}/{name}/");
                    let matched: Vec<String> = files.iter().filter(|f| f.starts_with(&prefix)).cloned().collect();
                    if !matched.is_empty() { modules.insert(name, matched); }
                }
            }
        }
    }
    if modules.is_empty() {
        for rel in &files {
            let parts: Vec<&str> = rel.split('/').collect();
            if parts.len() >= 2 { modules.entry(parts[parts.len() - 2].to_string()).or_default().push(rel.clone()); }
        }
    }
    let mut graph = BTreeMap::new();
    for rel in &files {
        let Some(src) = read_lossy(&root.join(rel)).map(|s| strip_c_comments(&s)) else { continue };
        let mut deps = BTreeSet::new();
        for line in src.lines() {
            if let Some(rest) = line.trim().strip_prefix("import ") {
                let name = rest.split_whitespace().next().unwrap_or("");
                for dep in modules.get(name).into_iter().flatten() {
                    if dep != rel { deps.insert(dep.clone()); }
                }
            }
        }
        if !deps.is_empty() { graph.insert(rel.clone(), deps.into_iter().collect()); }
    }
    graph
}

fn dart_imports(root: &Path) -> BTreeMap<String, Vec<String>> {
    let files: BTreeSet<String> = collect_files(root, &[".dart"]).into_iter().collect();
    let mut graph = BTreeMap::new();
    for rel in &files {
        let Some(src) = read_lossy(&root.join(rel)).map(|s| strip_c_comments(&s)) else { continue };
        let mut deps = BTreeSet::new();
        let mut rest = src.as_str();
        while let Some(at) = rest.find("import") {
            let after = rest[at + 6..].trim_start();
            if let Some(path) = quote_either(after) {
                let path = path.split("package:").last().unwrap_or(&path);
                let _path = path.rsplit_once('/').map(|(_, _tail)| path).unwrap_or(path);
                let _ = path;
                let cleaned = after_package(quote_either(after).unwrap_or(""));
                if files.contains(&cleaned) && cleaned != *rel {
                    deps.insert(cleaned);
                } else {
                    let base = norm_slash(&format!("{}/{}", parent(rel), cleaned));
                    if files.contains(&base) && base != *rel { deps.insert(base); }
                }
            }
            rest = &rest[at + 6..];
        }
        if !deps.is_empty() { graph.insert(rel.clone(), deps.into_iter().collect()); }
    }
    graph
}

fn after_package(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("package:") {
        rest.split_once('/').map(|(_, tail)| tail.to_string()).unwrap_or_else(|| rest.to_string())
    } else { path.to_string() }
}

fn zig_imports(root: &Path) -> BTreeMap<String, Vec<String>> {
    let files: BTreeSet<String> = collect_files(root, &[".zig"]).into_iter().collect();
    let mut graph = BTreeMap::new();
    for rel in &files {
        let Some(src) = read_lossy(&root.join(rel)).map(|s| strip_c_comments(&s)) else { continue };
        let mut deps = BTreeSet::new();
        let mut rest = src.as_str();
        while let Some(at) = rest.find("@import") {
            let after = rest[at + 7..].trim_start().trim_start_matches('(').trim_start();
            if let Some(path) = one_string(after) {
                if path.ends_with(".zig") {
                    let base = norm_slash(&format!("{}/{}", parent(rel), path));
                    if files.contains(&base) && base != *rel { deps.insert(base); }
                }
            }
            rest = &rest[at + 7..];
        }
        if !deps.is_empty() { graph.insert(rel.clone(), deps.into_iter().collect()); }
    }
    graph
}

fn ts_imports(root: &Path, exts: &[&str]) -> BTreeMap<String, Vec<String>> {
    let files: BTreeSet<String> = collect_files(root, exts).into_iter().collect();
    let mut graph = BTreeMap::new();
    for rel in &files {
        let Some(src) = read_lossy(&root.join(rel)).map(|s| strip_c_comments(&s)) else { continue };
        let mut deps = BTreeSet::new();
        for path in ts_specifiers(&src) {
            if path.starts_with('.') {
                if let Some(hit) = resolve_ts(rel, &path, &files) { if hit != *rel { deps.insert(hit); } }
            }
        }
        if !deps.is_empty() { graph.insert(rel.clone(), deps.into_iter().collect()); }
    }
    graph
}

fn ts_specifiers(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    for marker in [" from ", "import(", "require(", "import "] {
        let mut rest = src;
        while let Some(at) = rest.find(marker) {
            let after = rest[at + marker.len()..].trim_start();
            if let Some(path) = quote_either(after) {
                if path.starts_with('.') { out.push(path.to_string()); }
            }
            rest = &rest[at + marker.len()..];
        }
    }
    out
}

fn resolve_ts(importing: &str, import_path: &str, files: &BTreeSet<String>) -> Option<String> {
    let mut path = import_path.to_string();
    for ext in [".js", ".mjs", ".cjs"] {
        if let Some(stripped) = path.strip_suffix(ext) { path = stripped.to_string(); break; }
    }
    let base = norm_slash(&format!("{}/{}", parent(importing), path));
    try_ts(&base, files)
}

fn try_ts(base: &str, files: &BTreeSet<String>) -> Option<String> {
    for ext in [".ts", ".tsx", ".js", ".jsx", ".mts", ".mjs"] {
        let candidate = format!("{base}{ext}");
        if files.contains(&candidate) { return Some(candidate); }
    }
    for ext in [".ts", ".tsx", ".js", ".jsx", ".mts", ".mjs"] {
        let candidate = format!("{base}/index{ext}");
        if files.contains(&candidate) { return Some(candidate); }
    }
    if files.contains(base) { Some(base.to_string()) } else { None }
}

fn quote_either(text: &str) -> Option<&str> {
    let text = text.trim_start();
    let quote = text.chars().next()?;
    if quote != '"' && quote != '\'' { return None; }
    let body = &text[1..];
    Some(&body[..body.find(quote)?])
}

fn capture_all<'a>(src: &'a str, prefix: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut rest = src;
    while let Some(at) = rest.find(prefix) {
        let after = &rest[at + prefix.len()..];
        let end = after.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == ':')).unwrap_or(after.len());
        if end > 0 { out.push(&after[..end]); }
        rest = &rest[at + prefix.len()..];
    }
    out
}

fn parent(path: &str) -> String {
    match path.rfind('/') { Some(i) => path[..i].to_string(), None => String::new() }
}

fn is_ident(text: &str) -> bool {
    let mut chars = text.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_') && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}
