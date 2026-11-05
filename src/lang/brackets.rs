//! Bracket balance for `verify_step`'s preflight. Each language's comments,
//! strings, character literals, and (for JavaScript) regex literals are
//! skipped the way that language writes them, so only brackets in code count.
//! Languages whose literal forms are too open to scan safely (Ruby, PHP) and
//! unknown extensions are not checked: a false alarm blocks a correct edit.

#[derive(Clone, Copy, PartialEq)]
enum Lang { Python, Rust, Js, Go, C, Cpp, Jvm, CSharp, Swift, Dart, Zig }

fn lang_of(ext: &str) -> Option<Lang> {
    Some(match ext {
        "py" | "pyi" => Lang::Python,
        "rs" => Lang::Rust,
        "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "mts" | "cts" => Lang::Js,
        "go" => Lang::Go,
        "c" | "h" => Lang::C,
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" | "hh" => Lang::Cpp,
        "java" | "kt" | "kts" => Lang::Jvm,
        "cs" => Lang::CSharp,
        "swift" => Lang::Swift,
        "dart" => Lang::Dart,
        "zig" => Lang::Zig,
        _ => return None,
    })
}

const MAX_ISSUES: usize = 5;

/// Problems such as `line 12: unbalanced '}'`, at most five. Empty when the
/// brackets balance or the language is not checked.
pub fn bracket_errors(ext: &str, source: &str) -> Vec<String> {
    let Some(lang) = lang_of(ext) else { return Vec::new() };
    let s = Scan { c: source.chars().collect(), lang };
    let mut open: Vec<(char, usize)> = Vec::new();
    let mut errors = Vec::new();
    let mut line = 1usize;
    let mut i = 0usize;
    while i < s.c.len() && errors.len() < MAX_ISSUES {
        let next = s.skip(i);
        if next > i {
            line += s.c[i..next].iter().filter(|c| **c == '\n').count();
            i = next;
            continue;
        }
        let ch = s.c[i];
        match ch {
            '\n' => line += 1,
            '(' | '[' | '{' => open.push((ch, line)),
            ')' | ']' | '}' => {
                let want = match ch { ')' => '(', ']' => '[', _ => '{' };
                // Close back to the matching opener, naming the ones skipped:
                // `g(1;` then `}` is a missing `)`, not a stray `}`.
                match open.iter().rposition(|(o, _)| *o == want) {
                    Some(at) => {
                        for (skipped, l) in open.drain(at..).skip(1) {
                            errors.push(format!("line {l}: '{skipped}' is never closed"));
                        }
                    }
                    None => errors.push(format!("line {line}: unbalanced '{ch}'")),
                }
            }
            _ => {}
        }
        i += 1;
    }
    for (ch, at) in open {
        errors.push(format!("line {at}: '{ch}' is never closed"));
    }
    errors.truncate(MAX_ISSUES);
    errors
}

struct Scan { c: Vec<char>, lang: Lang }

impl Scan {
    fn at(&self, i: usize) -> char { self.c.get(i).copied().unwrap_or('\0') }

    fn starts(&self, i: usize, text: &str) -> bool {
        text.chars().enumerate().all(|(k, ch)| self.at(i + k) == ch)
    }

    fn ident(ch: char) -> bool { ch.is_alphanumeric() || ch == '_' }

    /// The index after a comment or literal starting at `i`, or `i` when
    /// the character at `i` is code.
    fn skip(&self, i: usize) -> usize {
        let ch = self.at(i);
        let lang = self.lang;
        if lang == Lang::Python {
            if ch == '#' { return self.to_eol(i); }
            if ch == '\'' || ch == '"' { return self.quoted(i, true); }
            return i;
        }
        if ch == '/' && self.at(i + 1) == '/' { return self.to_eol(i); }
        if ch == '/' && self.at(i + 1) == '*' && lang != Lang::Zig { return self.block_comment(i); }
        match lang {
            Lang::Rust => self.rust(i),
            Lang::Js => {
                if ch == '\'' || ch == '"' { return self.quoted(i, false); }
                if ch == '`' { return self.template(i); }
                if ch == '/' && self.regex_can_start(i) { return self.regex(i); }
                i
            }
            Lang::Go => match ch {
                '"' | '\'' => self.quoted(i, false),
                '`' => self.until(i + 1, "`"),
                _ => i,
            },
            Lang::Cpp if ch == '"' && self.at(i.wrapping_sub(1)) == 'R' => self.cpp_raw(i),
            Lang::CSharp if ch == '@' && self.at(i + 1) == '"' => self.verbatim(i + 1),
            Lang::CSharp if ch == '$' && self.at(i + 1) == '@' && self.at(i + 2) == '"' => self.verbatim(i + 2),
            Lang::Swift if ch == '#' && (self.at(i + 1) == '"' || self.at(i + 1) == '#') => self.swift_raw(i),
            Lang::Zig if ch == '\\' && self.at(i + 1) == '\\' => self.to_eol(i),
            _ if ch == '"' || (ch == '\'' && lang != Lang::Swift) => {
                let triple = matches!(lang, Lang::Jvm | Lang::CSharp | Lang::Swift | Lang::Dart);
                self.quoted(i, triple)
            }
            _ => i,
        }
    }

    fn to_eol(&self, i: usize) -> usize {
        let mut j = i;
        while j < self.c.len() && self.c[j] != '\n' { j += 1; }
        j
    }

    /// Index after the first `close` at or after `from` (end of input if none).
    fn until(&self, from: usize, close: &str) -> usize {
        let mut j = from;
        while j < self.c.len() {
            if self.starts(j, close) { return j + close.chars().count(); }
            j += 1;
        }
        self.c.len()
    }

    fn block_comment(&self, i: usize) -> usize {
        let nests = matches!(self.lang, Lang::Rust | Lang::Swift | Lang::Dart | Lang::Jvm);
        let mut depth = 0usize;
        let mut j = i;
        while j < self.c.len() {
            if self.starts(j, "/*") { depth += 1; j += 2; continue; }
            if self.starts(j, "*/") {
                depth -= 1;
                j += 2;
                if depth == 0 || !nests { return j; }
                continue;
            }
            j += 1;
        }
        self.c.len()
    }

    /// A quoted string or character starting at `i`. With `triple`, three
    /// quotes open a string that may span lines. Single quotes end at the
    /// line (an unterminated one is the compiler's to report, not ours).
    fn quoted(&self, i: usize, triple: bool) -> usize {
        let q = self.at(i);
        if triple && self.at(i + 1) == q && self.at(i + 2) == q {
            let close: String = [q, q, q].iter().collect();
            let mut j = i + 3;
            while j < self.c.len() {
                if self.c[j] == '\\' { j += 2; continue; }
                if self.starts(j, &close) { return j + 3; }
                j += 1;
            }
            return self.c.len();
        }
        let mut j = i + 1;
        while j < self.c.len() {
            match self.c[j] {
                '\\' => j += 2,
                '\n' => return j,
                c if c == q => return j + 1,
                _ => j += 1,
            }
        }
        self.c.len()
    }

    fn rust(&self, i: usize) -> usize {
        let ch = self.at(i);
        // Raw strings: r"..", r#".."#, br"..", cr#".."#.
        if (ch == 'r' || ch == 'b' || ch == 'c') && !Self::ident(self.at(i.wrapping_sub(1))) {
            let mut j = i + 1;
            if ch != 'r' && self.at(j) == 'r' { j += 1; } else if ch != 'r' { return i; }
            let hashes_start = j;
            while self.at(j) == '#' { j += 1; }
            if self.at(j) == '"' {
                let close: String = std::iter::once('"').chain(std::iter::repeat('#').take(j - hashes_start)).collect();
                return self.until(j + 1, &close);
            }
            return i;
        }
        if ch == '"' {
            let mut j = i + 1;
            while j < self.c.len() {
                match self.c[j] {
                    '\\' => j += 2,
                    '"' => return j + 1,
                    _ => j += 1,
                }
            }
            return self.c.len();
        }
        if ch == '\'' {
            // 'x' and '\n', '\u{1F600}' are characters; 'a in <'a> is a lifetime.
            if self.at(i + 1) == '\\' {
                let mut j = i + 2;
                while j < self.c.len() && self.c[j] != '\'' && self.c[j] != '\n' { j += 1; }
                return j + 1;
            }
            if self.at(i + 2) == '\'' { return i + 3; }
            return i + 1;
        }
        i
    }

    /// A template literal; `${ ... }` holds code with its own braces and strings.
    fn template(&self, i: usize) -> usize {
        let mut j = i + 1;
        while j < self.c.len() {
            match self.c[j] {
                '\\' => j += 2,
                '`' => return j + 1,
                '$' if self.at(j + 1) == '{' => {
                    let mut depth = 1;
                    j += 2;
                    while j < self.c.len() && depth > 0 {
                        let next = self.skip(j);
                        if next > j { j = next; continue; }
                        match self.c[j] { '{' => depth += 1, '}' => depth -= 1, _ => {} }
                        j += 1;
                    }
                }
                _ => j += 1,
            }
        }
        self.c.len()
    }

    /// A `/` starts a regex after an operator, an opening bracket, a comma,
    /// or a keyword such as `return`; after a value it is division.
    fn regex_can_start(&self, i: usize) -> bool {
        if self.at(i + 1) == '/' || self.at(i + 1) == '*' { return false; }
        let mut j = i;
        while j > 0 && (self.c[j - 1] == ' ' || self.c[j - 1] == '\t') { j -= 1; }
        if j == 0 { return true; }
        let prev = self.c[j - 1];
        if Self::ident(prev) {
            let mut k = j;
            while k > 0 && Self::ident(self.c[k - 1]) { k -= 1; }
            let word: String = self.c[k..j].iter().collect();
            return matches!(word.as_str(), "return" | "typeof" | "case" | "in" | "of" | "delete" | "void" | "throw" | "new" | "yield" | "await");
        }
        !matches!(prev, ')' | ']' | '}' | '"' | '\'' | '`' | '.')
    }

    fn regex(&self, i: usize) -> usize {
        let mut j = i + 1;
        let mut class = false;
        while j < self.c.len() {
            match self.c[j] {
                '\\' => j += 2,
                '\n' => return i + 1, // not a regex after all; count the `/` as code
                '[' => { class = true; j += 1; }
                ']' => { class = false; j += 1; }
                '/' if !class => return j + 1,
                _ => j += 1,
            }
        }
        i + 1
    }

    /// C++ `R"delim( ... )delim"`.
    fn cpp_raw(&self, i: usize) -> usize {
        let mut j = i + 1;
        let mut delim = String::new();
        while j < self.c.len() && self.c[j] != '(' && delim.len() <= 16 { delim.push(self.c[j]); j += 1; }
        if self.at(j) != '(' { return self.quoted(i, false); }
        self.until(j + 1, &format!("){delim}\""))
    }

    /// C# `@"..."`, where `""` is a quote.
    fn verbatim(&self, quote: usize) -> usize {
        let mut j = quote + 1;
        while j < self.c.len() {
            if self.c[j] == '"' {
                if self.at(j + 1) == '"' { j += 2; continue; }
                return j + 1;
            }
            j += 1;
        }
        self.c.len()
    }

    /// Swift `#"..."#`, `##"""..."""##`.
    fn swift_raw(&self, i: usize) -> usize {
        let mut j = i;
        while self.at(j) == '#' { j += 1; }
        if self.at(j) != '"' { return i; }
        let hashes: String = std::iter::repeat('#').take(j - i).collect();
        let quotes = if self.starts(j, "\"\"\"") { "\"\"\"" } else { "\"" };
        self.until(j + quotes.len(), &format!("{quotes}{hashes}"))
    }
}

#[cfg(test)]
mod tests {
    use super::bracket_errors;

    #[test]
    fn valid_code_with_tricky_literals_passes() {
        let cases = [
            ("rs", "fn f<'a>(x: &'a str) -> char { let _ = r#\"{(\"#; let c = '{'; let u = '\\u{1F600}'; /* /* ( */ ) */ x.len(); '}' }\n"),
            ("rs", "// don't (\nfn g() { let s = \"multi\nline {\"; }\n"),
            ("py", "def f():\n    \"\"\"Don't ( worry.\n    { \"\"\"\n    return {'a': [1, 2]}  # it's ( fine\n"),
            ("py", "x = f\"{y['k']}\" + r'\\'' + '(' \n"),
            ("ts", "const r = /[(]\\//g; const t = `a ${ {b: 1}.b } ${`n{`}`; const d = (a / b) / c;\n"),
            ("js", "if (x) { return /}/.test(s) }\n"),
            ("go", "s := `raw {\n(` ; r := '}' ; t := \"(\"\n"),
            ("cs", "var p = @\"C:\\dir\\{\"\"x\"\"\"; var q = $\"{a}\";\n"),
            ("cpp", "auto s = R\"x(  ) { )x\"; int a[2] = {1, 2};\n"),
            ("kt", "val s = \"\"\"\n  ( unmatched\n\"\"\"\nfun f() { }\n"),
            ("swift", "let s = #\"(\"#; let t = \"\"\"\n{\n\"\"\"\n"),
            ("zig", "const s = \\\\ raw ( line\n;\nfn f() void {}\n"),
            ("rb", "puts %w( unbalanced \n"),
            ("txt", "("),
        ];
        for (ext, source) in cases {
            assert_eq!(bracket_errors(ext, source), Vec::<String>::new(), "{ext}: {source}");
        }
    }

    #[test]
    fn real_mistakes_are_reported_with_lines() {
        assert_eq!(bracket_errors("rs", "fn f() {\n    g(1;\n}\n"), vec!["line 2: '(' is never closed"]);
        assert_eq!(bracket_errors("py", "def f(:\n    pass\n"), vec!["line 1: '(' is never closed"]);
        assert_eq!(bracket_errors("ts", "function f() {\n  return 1;\n"), vec!["line 1: '{' is never closed"]);
        assert_eq!(bracket_errors("go", "func f() {\n}}\n"), vec!["line 2: unbalanced '}'"]);
        assert_eq!(bracket_errors("rs", &"}".repeat(20)).len(), 5);
    }
}
