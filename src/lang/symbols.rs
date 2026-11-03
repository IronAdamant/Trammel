//! Hand-written declaration extractors. Each language walks line anchors the
//! way the Python specs did, and the first pattern to claim a name wins.

use crate::lang::blank::{strip_c_noise, strip_hash_noise, strip_php_noise};

pub fn collect(language: &str, source: &str) -> Vec<(String, &'static str)> {
    let prepared = match language {
        "ruby" => strip_hash_noise(source),
        "php" => strip_php_noise(source),
        "python" => source.to_string(),
        _ => strip_c_noise(source),
    };
    match language {
        "go" => go(&prepared),
        "rust" => rust(&prepared),
        "cpp" | "c" => cpp(&prepared),
        "java" | "kotlin" => java(&prepared),
        "csharp" => csharp(&prepared),
        "ruby" => ruby(&prepared),
        "php" => php(&prepared),
        "swift" => swift(&prepared),
        "dart" => dart(&prepared),
        "zig" => zig(&prepared),
        "typescript" | "javascript" => typescript(&prepared),
        _ => Vec::new(),
    }
}

fn go(src: &str) -> Vec<(String, &'static str)> {
    let mut out = Vec::new();
    for line in logical_lines(src) {
        if let Some(name) = func_name(&line, "func") {
            push(&mut out, name, "function");
        }
    }
    for (label, kind) in [("struct", "struct"), ("interface", "interface")] {
        for line in logical_lines(src) {
            if let Some(name) = type_kind(&line, label) {
                push(&mut out, name, kind);
            }
        }
    }
    for line in logical_lines(src) {
        if let Some(name) = type_other(&line) {
            push(&mut out, name, "type");
        }
        if let Some(name) = keyword_ident(&line, "const") {
            push(&mut out, name, "constant");
        }
        if let Some(name) = keyword_ident(&line, "var") {
            push(&mut out, name, "variable");
        }
    }
    out
}

fn rust(src: &str) -> Vec<(String, &'static str)> {
    let mut out = Vec::new();
    for line in logical_lines(src) {
        let rest = strip_vis(&line);
        let rest = rest.strip_prefix("async").map(|s| s.trim_start()).unwrap_or(rest);
        if let Some(name) = rest.strip_prefix("fn").and_then(|s| ident_after_ws(s)) {
            push(&mut out, name, "function");
        }
        for (kw, kind) in [("struct", "struct"), ("enum", "enum"), ("trait", "trait"), ("type", "type_alias")] {
            if let Some(name) = rest.strip_prefix(kw).and_then(|s| ident_after_ws(s)) {
                push(&mut out, name, kind);
            }
        }
        if let Some(name) = impl_name(&line) {
            push(&mut out, name, "impl");
        }
    }
    out
}

fn cpp(src: &str) -> Vec<(String, &'static str)> {
    let mut out = Vec::new();
    for line in logical_lines(src) {
        if let Some(name) = after_optional(&line, &["template"], "class") {
            push(&mut out, name, "class");
        }
        if let Some(name) = keyword_ident(line.trim_start().strip_prefix("typedef ").unwrap_or(line.trim_start()), "struct")
            .or_else(|| keyword_ident(&line, "struct"))
        {
            push(&mut out, name, "struct");
        }
        if let Some(name) = keyword_ident(&line, "namespace") {
            push(&mut out, name, "namespace");
        }
        if let Some(name) = enum_name(&line) {
            push(&mut out, name, "enum");
        }
        if let Some(name) = typedef_alias(&line) {
            push(&mut out, name, "type_alias");
        }
        let trimmed = line.trim_start();
        if !trimmed.starts_with("class") && !trimmed.starts_with("struct") && !trimmed.starts_with("namespace") && !trimmed.starts_with("enum") && !trimmed.starts_with("typedef") {
            if let Some(name) = cpp_function(&line) {
                push(&mut out, name, "function");
            }
        }
    }
    out
}

fn java(src: &str) -> Vec<(String, &'static str)> {
    let mods = ["public", "protected", "private", "abstract", "final", "static", "open", "internal", "data", "sealed", "override", "suspend", "inline"];
    let mut out = Vec::new();
    for line in logical_lines(src) {
        let rest = strip_words(line.trim_start(), &mods);
        if let Some(name) = keyword_ident(rest, "class") { push(&mut out, name, "class"); }
        if let Some(name) = keyword_ident(rest, "interface") { push(&mut out, name, "interface"); }
        if let Some(name) = rest.strip_prefix("enum").and_then(|s| {
            let s = s.trim_start();
            let s = s.strip_prefix("class").map(|t| t.trim_start()).unwrap_or(s);
            ident_at(s)
        }) { push(&mut out, name, "enum"); }
        if let Some(name) = keyword_ident(rest, "record") { push(&mut out, name, "record"); }
        if let Some(name) = fun_name(rest) { push(&mut out, name, "function"); }
        if let Some(name) = object_name(rest) { push(&mut out, name, "object"); }
        if let Some(name) = keyword_ident(rest, "@interface") { push(&mut out, name, "annotation"); }
    }
    out
}

fn csharp(src: &str) -> Vec<(String, &'static str)> {
    let mods = ["public", "private", "protected", "internal", "static", "abstract", "sealed", "partial", "async", "virtual", "override"];
    let mut out = Vec::new();
    for line in logical_lines(src) {
        let rest = strip_words(line.trim_start(), &mods);
        for (kw, kind) in [("class", "class"), ("interface", "interface"), ("struct", "struct"), ("enum", "enum"), ("record", "record")] {
            if let Some(name) = keyword_ident(rest, kw) {
                push(&mut out, name, kind);
            }
        }
        if !rest.starts_with("class") && !rest.starts_with("interface") && !rest.starts_with("struct") && !rest.starts_with("enum") && !rest.starts_with("record") {
            if let Some(name) = csharp_method(rest) {
                push(&mut out, name, "function");
            }
        }
    }
    out
}

fn ruby(src: &str) -> Vec<(String, &'static str)> {
    let mut out = Vec::new();
    for line in logical_lines(src) {
        if let Some(name) = keyword_ident(&line, "class") { push(&mut out, name, "class"); }
        if let Some(name) = keyword_ident(&line, "module") { push(&mut out, name, "module"); }
        if let Some(name) = line.trim_start().strip_prefix("def").and_then(|s| {
            let s = s.trim_start().strip_prefix("self.").unwrap_or(s.trim_start());
            ident_at(s)
        }) { push(&mut out, name, "function"); }
    }
    out
}

fn php(src: &str) -> Vec<(String, &'static str)> {
    let mut out = Vec::new();
    for line in logical_lines(src) {
        let rest = strip_words(line.trim_start(), &["abstract", "final"]);
        if let Some(name) = keyword_ident(rest, "class") { push(&mut out, name, "class"); }
        if let Some(name) = keyword_ident(&line, "interface") { push(&mut out, name, "interface"); }
        if let Some(name) = keyword_ident(&line, "trait") { push(&mut out, name, "trait"); }
        if let Some(name) = keyword_ident(&line, "enum") { push(&mut out, name, "enum"); }
        if let Some(name) = keyword_ident(&line, "function") { push(&mut out, name, "function"); }
        let method = strip_words(line.trim_start(), &["public", "protected", "private", "static", "abstract", "final"]);
        if method != line.trim_start() {
            if let Some(name) = keyword_ident(method, "function") {
                push(&mut out, name, "method");
            }
        }
    }
    out
}

fn swift(src: &str) -> Vec<(String, &'static str)> {
    let access = ["public", "private", "internal", "fileprivate", "open", "override", "static"];
    let mut out = Vec::new();
    for line in logical_lines(src) {
        let rest = strip_words(line.trim_start(), &access);
        for (kw, kind) in [("class", "class"), ("struct", "struct"), ("enum", "enum"), ("protocol", "protocol"), ("func", "function"), ("extension", "extension"), ("typealias", "type_alias"), ("actor", "actor")] {
            if let Some(name) = keyword_ident(if kw == "extension" || kw == "actor" { line.trim_start() } else { rest }, kw) {
                push(&mut out, name, kind);
            }
        }
    }
    out
}

fn dart(src: &str) -> Vec<(String, &'static str)> {
    let mut out = Vec::new();
    for line in logical_lines(src) {
        let rest = strip_words(line.trim_start(), &["abstract", "base", "interface", "final", "sealed", "mixin"]);
        if let Some(name) = keyword_ident(rest, "class") { push(&mut out, name, "class"); }
        if let Some(name) = line.trim_start().strip_prefix("mixin").and_then(|s| {
            let s = s.trim_start().strip_prefix("class").map(|t| t.trim_start()).unwrap_or(s.trim_start());
            ident_at(s)
        }) { push(&mut out, name, "mixin"); }
        if let Some(name) = line.trim_start().strip_prefix("extension").and_then(|s| {
            let s = s.trim_start().strip_prefix("type").map(|t| t.trim_start()).unwrap_or(s.trim_start());
            ident_at(s)
        }) { push(&mut out, name, "extension"); }
        if let Some(name) = keyword_ident(&line, "enum") { push(&mut out, name, "enum"); }
        if let Some(name) = keyword_ident(&line, "typedef") { push(&mut out, name, "type_alias"); }
        if !line.trim_start().starts_with("class") && !line.trim_start().starts_with("enum") && !line.trim_start().starts_with("mixin") {
            if let Some(name) = dart_function(&line) { push(&mut out, name, "function"); }
            if let Some(name) = dart_constructor(&line) { push(&mut out, name, "constructor"); }
        }
    }
    out
}

fn zig(src: &str) -> Vec<(String, &'static str)> {
    let mut out = Vec::new();
    for line in logical_lines(src) {
        let rest = line.trim_start().strip_prefix("pub").map(|s| s.trim_start()).unwrap_or(line.trim_start());
        if let Some(name) = keyword_ident(rest, "fn") { push(&mut out, name, "function"); }
        if let Some(name) = const_eq(rest, "struct") { push(&mut out, name, "struct"); }
        if let Some(name) = const_eq(rest, "enum") { push(&mut out, name, "enum"); }
        if let Some(name) = const_eq(rest, "union") { push(&mut out, name, "union"); }
        if let Some(name) = const_eq(rest, "@import") { push(&mut out, name, "import_const"); }
        if let Some(name) = zig_type(rest) { push(&mut out, name, "type_alias"); }
    }
    out
}

fn typescript(src: &str) -> Vec<(String, &'static str)> {
    let mut out = Vec::new();
    let lines = logical_lines(src);
    for line in &lines {
        let rest = strip_export(line.trim_start());
        let rest = rest.strip_prefix("default").map(|s| s.trim_start()).unwrap_or(rest);
        let rest = rest.strip_prefix("async").map(|s| s.trim_start()).unwrap_or(rest);
        if let Some(name) = rest.strip_prefix("function").and_then(|s| {
            let s = s.trim_start().trim_start_matches('*').trim_start();
            ident_at(s)
        }) { push(&mut out, name, "function"); }
        if let Some(name) = rest.strip_prefix("declare").and_then(|s| keyword_ident(s.trim_start(), "function")) {
            push(&mut out, name, "function");
        }
        if line.trim_start().starts_with("declare") {
            if let Some(name) = classish(line) { push(&mut out, name, "class"); }
        }
        if let Some(name) = classish(line) { push(&mut out, name, "class"); }
        if let Some(name) = keyword_ident(strip_export(line.trim_start()), "interface") { push(&mut out, name, "interface"); }
        if let Some(name) = enum_ts(line) { push(&mut out, name, "enum"); }
        if let Some(name) = type_alias_ts(line) { push(&mut out, name, "type_alias"); }
        if let Some(name) = const_fn(line) { push(&mut out, name, "function"); }
        if let Some(name) = keyword_ident(strip_export(line.trim_start()), "namespace") { push(&mut out, name, "namespace"); }
    }
    out
}

fn logical_lines(src: &str) -> Vec<String> {
    src.split('\n').map(|l| l.to_string()).collect()
}

fn push(out: &mut Vec<(String, &'static str)>, name: String, kind: &'static str) {
    if name.is_empty() || out.iter().any(|(n, _)| n == &name) {
        return;
    }
    out.push((name, kind));
}

fn ident_at(text: &str) -> Option<String> {
    let text = text.trim_start();
    let mut chars = text.chars();
    let first = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }
    let mut name = String::from(first);
    for ch in chars {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            name.push(ch);
        } else {
            break;
        }
    }
    Some(name)
}

fn ident_after_ws(text: &str) -> Option<String> {
    if text.starts_with(char::is_whitespace) {
        ident_at(text)
    } else {
        None
    }
}

fn keyword_ident(line: &str, keyword: &str) -> Option<String> {
    let rest = line.trim_start().strip_prefix(keyword)?;
    ident_after_ws(rest)
}

fn func_name(line: &str, keyword: &str) -> Option<String> {
    let rest = line.trim_start().strip_prefix(keyword)?.trim_start();
    let rest = if let Some(end) = receiver_end(rest) { rest[end..].trim_start() } else { rest };
    ident_at(rest)
}

fn receiver_end(text: &str) -> Option<usize> {
    if !text.starts_with('(') { return None; }
    text.find(')').map(|i| i + 1)
}

fn type_kind(line: &str, kind: &str) -> Option<String> {
    let rest = line.trim_start().strip_prefix("type")?.trim_start();
    let name = ident_at(rest)?;
    let after = rest.trim_start()[name.len()..].trim_start();
    let after = after.strip_prefix('[').and_then(|s| s.find(']').map(|i| s[i + 1..].trim_start())).unwrap_or(after);
    if after.starts_with(kind) { Some(name) } else { None }
}

fn type_other(line: &str) -> Option<String> {
    let rest = line.trim_start().strip_prefix("type")?.trim_start();
    let name = ident_at(rest)?;
    let after = rest.trim_start()[name.len()..].trim_start();
    let after = after.strip_prefix('[').and_then(|s| s.find(']').map(|i| s[i + 1..].trim_start())).unwrap_or(after);
    if after.starts_with("struct") || after.starts_with("interface") { None } else { Some(name) }
}

fn strip_vis(line: &str) -> &str {
    let mut rest = line.trim_start();
    if let Some(after) = rest.strip_prefix("pub") {
        rest = after.trim_start();
        if let Some(end) = rest.strip_prefix('(').and_then(|s| s.find(')')) {
            rest = rest[end + 2..].trim_start();
        }
    }
    rest
}

fn impl_name(line: &str) -> Option<String> {
    let rest = line.trim_start().strip_prefix("impl")?;
    let mut rest = rest.trim_start();
    if rest.starts_with('<') {
        let mut depth = 0i32;
        let mut end = None;
        for (i, ch) in rest.char_indices() {
            if ch == '<' { depth += 1; }
            if ch == '>' {
                depth -= 1;
                if depth == 0 { end = Some(i + 1); break; }
            }
        }
        rest = rest[end?..].trim_start();
    }
    ident_at(rest)
}

fn strip_words<'a>(mut text: &'a str, words: &[&str]) -> &'a str {
    loop {
        let trimmed = text.trim_start();
        let mut advanced = false;
        for word in words {
            if let Some(after) = trimmed.strip_prefix(word) {
                if after.starts_with(char::is_whitespace) || after.is_empty() {
                    text = after.trim_start();
                    advanced = true;
                    break;
                }
            }
        }
        if !advanced { return trimmed; }
    }
}

fn after_optional(line: &str, optional: &[&str], keyword: &str) -> Option<String> {
    let rest = strip_words(line.trim_start(), optional);
    let rest = if rest.starts_with("template") {
        let after = rest.trim_start().strip_prefix("template")?.trim_start();
        let end = after.find('>')?;
        after[end + 1..].trim_start()
    } else { rest };
    keyword_ident(rest, keyword)
}

fn enum_name(line: &str) -> Option<String> {
    let rest = line.trim_start().strip_prefix("enum")?.trim_start();
    let rest = rest.strip_prefix("class").map(|s| s.trim_start()).unwrap_or(rest);
    ident_at(rest)
}

fn typedef_alias(line: &str) -> Option<String> {
    let rest = line.trim_start().strip_prefix("typedef")?.trim_start();
    if rest.starts_with("struct") { return None; }
    let end = rest.find(';')?;
    let body = rest[..end].trim_end();
    let name_start = body.rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))?;
    ident_at(&body[name_start + 1..])
}

fn cpp_function(line: &str) -> Option<String> {
    let rest = strip_words(line.trim_start(), &["static", "inline", "constexpr", "virtual", "explicit", "extern"]);
    let paren = rest.rfind('(')?;
    if rest[paren..].contains(';') && !rest[paren..].contains(')') { return None; }
    let before = rest[..paren].trim_end();
    let name_at = before.rfind(|c: char| c.is_whitespace() || "*&<>:".contains(c))?;
    let name = ident_at(&before[name_at + 1..])?;
    if ["if", "while", "for", "switch", "return", "catch"].contains(&name.as_str()) { return None; }
    Some(name)
}

fn fun_name(rest: &str) -> Option<String> {
    let rest = rest.strip_prefix("fun")?.trim_start();
    let rest = if rest.starts_with('<') {
        let end = rest.find('>')?;
        rest[end + 1..].trim_start()
    } else { rest };
    ident_at(rest)
}

fn object_name(rest: &str) -> Option<String> {
    let rest = rest.strip_prefix("companion").map(|s| s.trim_start()).unwrap_or(rest);
    keyword_ident(rest, "object")
}

fn csharp_method(rest: &str) -> Option<String> {
    let paren = rest.find('(')?;
    let before = rest[..paren].trim_end();
    if !before.contains(' ') && !before.contains('<') { return None; }
    let name_at = before.rfind(|c: char| c.is_whitespace() || c == '>')?;
    let name = ident_at(&before[name_at + 1..])?;
    if ["if", "while", "for", "foreach", "switch", "catch", "lock", "using"].contains(&name.as_str()) { return None; }
    Some(name)
}

fn dart_function(line: &str) -> Option<String> {
    let rest = line.trim_start();
    let paren = rest.find('(')?;
    let before = rest[..paren].trim_end();
    let name = ident_at(before.rsplit_once(|c: char| c.is_whitespace() || c == '>').map(|(_, n)| n).unwrap_or(before))?;
    if ["if", "else", "for", "while", "do", "switch", "catch"].contains(&name.as_str()) { return None; }
    let after = rest[paren..].trim_start();
    if after.contains('{') || after.contains('=') { Some(name) } else { None }
}

fn dart_constructor(line: &str) -> Option<String> {
    let rest = line.trim_start().strip_prefix("factory").map(|s| s.trim_start()).unwrap_or(line.trim_start());
    let paren = rest.find('(')?;
    let name = rest[..paren].trim();
    if name.contains('.') && ident_at(name).is_some() { Some(name.to_string()) } else { None }
}

fn const_eq(rest: &str, rhs: &str) -> Option<String> {
    let rest = rest.strip_prefix("const")?.trim_start();
    let name = ident_at(rest)?;
    let after = rest[name.len()..].trim_start();
    if after.starts_with('=') && after.trim_start_matches('=').trim_start().starts_with(rhs) { Some(name) } else { None }
}

fn zig_type(rest: &str) -> Option<String> {
    let rest = rest.strip_prefix("const")?.trim_start();
    let name = ident_at(rest)?;
    let after = rest[name.len()..].trim_start();
    if after.starts_with(':') && after[1..].trim_start().starts_with("type") { Some(name) } else { None }
}

fn strip_export(line: &str) -> &str {
    line.strip_prefix("export").map(|s| s.trim_start()).unwrap_or(line)
}

fn classish(line: &str) -> Option<String> {
    let mut rest = strip_export(line.trim_start());
    rest = rest.strip_prefix("default").map(|s| s.trim_start()).unwrap_or(rest);
    rest = rest.strip_prefix("declare").map(|s| s.trim_start()).unwrap_or(rest);
    rest = rest.strip_prefix("export").map(|s| s.trim_start()).unwrap_or(rest);
    rest = rest.strip_prefix("abstract").map(|s| s.trim_start()).unwrap_or(rest);
    keyword_ident(rest, "class")
}

fn enum_ts(line: &str) -> Option<String> {
    let rest = strip_export(line.trim_start());
    let rest = rest.strip_prefix("default").map(|s| s.trim_start()).unwrap_or(rest);
    let rest = rest.strip_prefix("const").map(|s| s.trim_start()).unwrap_or(rest);
    keyword_ident(rest, "enum")
}

fn type_alias_ts(line: &str) -> Option<String> {
    let rest = strip_export(line.trim_start());
    let rest = rest.strip_prefix("type")?.trim_start();
    let name = ident_at(rest)?;
    let after = rest[name.len()..].trim_start();
    if after.starts_with('=') || after.starts_with('<') { Some(name) } else { None }
}

fn const_fn(line: &str) -> Option<String> {
    let rest = strip_export(line.trim_start());
    for kw in ["const", "let", "var"] {
        if let Some(after) = rest.strip_prefix(kw) {
            let name = ident_at(after.trim_start())?;
            let tail = after.trim_start()[name.len()..].trim_start();
            if tail.starts_with('=') {
                let rhs = tail[1..].trim_start();
                if rhs.starts_with('(') || rhs.starts_with("async") || rhs.starts_with("function") {
                    return Some(name);
                }
            }
        }
    }
    None
}
