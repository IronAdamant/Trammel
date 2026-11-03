//! Comment and string blanking. Newlines are kept so line anchors still match.
//! The scan is the same lightweight lexer as Python `utils._strip_string_literals`.

pub fn strip_string_literals(src: &str) -> String {
    let chars: Vec<char> = src.chars().collect();
    let n = chars.len();
    let mut out = String::new();
    let mut i = 0;
    while i < n {
        let ch = chars[i];
        if ch == 'R' && i + 1 < n && chars[i + 1] == '"' {
            let mut j = i + 2;
            while j < n && chars[j] != '(' && chars[j] != '\n' {
                j += 1;
            }
            if j < n && chars[j] == '(' {
                let delim: String = chars[i + 2..j].iter().collect();
                let closer = format!("){delim}\"");
                let rest: String = chars[j + 1..].iter().collect();
                if let Some(found) = rest.find(&closer) {
                    let end = j + 1 + found + closer.chars().count();
                    blank_range(&chars, i, end, &mut out);
                    i = end;
                    continue;
                }
            }
        }
        if ch == 'r' && i + 1 < n && (chars[i + 1] == '#' || chars[i + 1] == '"') {
            let mut j = i + 1;
            let mut hashes = 0;
            while j < n && chars[j] == '#' {
                hashes += 1;
                j += 1;
            }
            if j < n && chars[j] == '"' {
                let closer = format!("\"{}", "#".repeat(hashes));
                let rest: String = chars[j + 1..].iter().collect();
                if let Some(found) = rest.find(&closer) {
                    let end = j + 1 + found + closer.chars().count();
                    blank_range(&chars, i, end, &mut out);
                    i = end;
                    continue;
                }
            }
        }
        if ch == '"' && i + 2 < n && chars[i + 1] == '"' && chars[i + 2] == '"' {
            let rest: String = chars[i + 3..].iter().collect();
            if let Some(found) = rest.find("\"\"\"") {
                let end = i + 3 + found + 3;
                blank_range(&chars, i, end, &mut out);
                i = end;
                continue;
            }
        }
        if ch == '"' || ch == '\'' || ch == '`' {
            let mut j = i + 1;
            while j < n {
                let c = chars[j];
                if c == '\\' && j + 1 < n {
                    j += 2;
                    continue;
                }
                if c == ch {
                    j += 1;
                    break;
                }
                j += 1;
            }
            blank_range(&chars, i, j, &mut out);
            i = j;
            continue;
        }
        out.push(ch);
        i += 1;
    }
    out
}

fn blank_range(chars: &[char], start: usize, end: usize, out: &mut String) {
    for ch in &chars[start..end.min(chars.len())] {
        out.push(if *ch == '\n' { '\n' } else { ' ' });
    }
}

pub fn strip_c_comments(src: &str) -> String {
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
            i += 2;
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if chars[i] == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                out.push(if chars[i] == '\n' { '\n' } else { ' ' });
                i += 1;
            }
            if i + 1 < chars.len() {
                i += 2;
            }
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

pub fn strip_hash_comments(src: &str) -> String {
    let mut out = String::new();
    for line in src.split_inclusive('\n') {
        if let Some(pos) = line.find('#') {
            out.push_str(&line[..pos]);
            if line.ends_with('\n') {
                out.push('\n');
            }
        } else {
            out.push_str(line);
        }
    }
    out
}

pub fn strip_c_noise(src: &str) -> String {
    strip_c_comments(&strip_string_literals(src))
}

pub fn strip_hash_noise(src: &str) -> String {
    strip_hash_comments(&strip_string_literals(src))
}

pub fn strip_php_noise(src: &str) -> String {
    strip_hash_comments(&strip_c_comments(&strip_string_literals(src)))
}

pub fn strip_php_comments(src: &str) -> String {
    strip_hash_comments(&strip_c_comments(src))
}
