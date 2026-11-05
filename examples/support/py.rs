//! Text and path helpers the repo checks share: Python-compatible line
//! splitting and number formatting, so results and the bench report keep the
//! format of the scripts they replaced (python-legacy `scripts/*.py`). Each
//! example includes this file with `#[path]`.

#![allow(dead_code)]

use std::path::Path;
use std::process::ExitCode;

/// `Path.read_text(encoding="utf-8")`: universal newlines, so `\r\n` and a
/// lone `\r` both read as `\n`. A missing file ends the check with exit 1.
pub fn read_text(path: &str) -> Result<String, ExitCode> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(String::from_utf8_lossy(&bytes).replace("\r\n", "\n").replace('\r', "\n")),
        Err(_) => Err(not_found(path)),
    }
}

pub fn not_found(path: &str) -> ExitCode {
    eprintln!("{path} not found; run the check from the repo root");
    ExitCode::from(1)
}

/// `str(Path(text))`: the OS separator, no trailing separator, no `.` parts.
pub fn path_str(text: &str) -> String {
    let sep = std::path::MAIN_SEPARATOR;
    let unified: String = if cfg!(windows) { text.replace('/', "\\") } else { text.to_string() };
    let absolute = unified.starts_with(sep);
    let parts: Vec<&str> = unified.split(sep).filter(|p| !p.is_empty() && *p != ".").collect();
    let joined = parts.join(&sep.to_string());
    match (absolute, joined.is_empty()) {
        (true, _) => format!("{sep}{joined}"),
        (false, true) => ".".into(),
        (false, false) => joined,
    }
}

/// `str.splitlines()`: every line boundary Python knows, no trailing empty line.
pub fn splitlines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\r' => {
                if chars.peek() == Some(&'\n') { chars.next(); }
                lines.push(std::mem::take(&mut current));
            }
            '\n' | '\x0b' | '\x0c' | '\x1c' | '\x1d' | '\x1e' | '\u{85}' | '\u{2028}' | '\u{2029}' => {
                lines.push(std::mem::take(&mut current));
            }
            other => current.push(other),
        }
    }
    if !current.is_empty() { lines.push(current); }
    lines
}

/// `str.isspace()` for one character.
pub fn is_space(ch: char) -> bool {
    ch.is_whitespace() || ('\x1c'..='\x1f').contains(&ch)
}

/// `str.strip()` with no argument.
pub fn strip(text: &str) -> &str {
    text.trim_matches(is_space)
}

/// `str.split(None, 1)`: the first word and the rest with leading space removed.
pub fn split_once_ws(line: &str) -> Option<(&str, &str)> {
    let line = line.trim_start_matches(is_space);
    let end = line.find(is_space)?;
    let rest = line[end..].trim_start_matches(is_space);
    if rest.is_empty() { return None; }
    Some((&line[..end], rest))
}

/// `repr(float)` for finite values: shortest round-trip digits, and a
/// trailing `.0` on whole numbers.
pub fn float(value: f64) -> String {
    let text = format!("{value}");
    if text.contains('.') || text.contains('e') || text.contains("inf") || text.contains("NaN") { text } else { format!("{text}.0") }
}

/// `round(value, 2)`: the decimal rounding of the exact binary value.
pub fn round2(value: f64) -> f64 {
    format!("{value:.2}").parse().unwrap_or(value)
}

/// Directory entry names in the order `scandir` gives them on this OS: NTFS
/// sorts case-insensitively; elsewhere the order is unspecified, so use bytes.
pub fn sorted_names(dir: &Path) -> Vec<(String, bool)> {
    let mut names: Vec<(String, bool)> = std::fs::read_dir(dir)
        .map(|it| it.flatten().map(|e| (e.file_name().to_string_lossy().into_owned(), e.file_type().map(|t| t.is_dir()).unwrap_or(false))).collect())
        .unwrap_or_default();
    if cfg!(windows) {
        names.sort_by(|a, b| a.0.to_uppercase().cmp(&b.0.to_uppercase()).then(a.0.cmp(&b.0)));
    } else {
        names.sort();
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_string_behaviours() {
        assert_eq!(splitlines("a\r\nb\rc\nd\x0ce"), vec!["a", "b", "c", "d", "e"]);
        assert_eq!(splitlines("a\n\nb\n"), vec!["a", "", "b"]);
        assert_eq!(splitlines(""), Vec::<String>::new());
        assert_eq!(strip("\t x \x1f\n"), "x");
        assert_eq!(split_once_ws("abc  src/x.rs "), Some(("abc", "src/x.rs ")));
        assert_eq!(split_once_ws("onlyone"), None);
        let sep = std::path::MAIN_SEPARATOR;
        assert_eq!(path_str("stele/./src//json.rs/"), format!("stele{sep}src{sep}json.rs"));
        assert_eq!(path_str(""), ".");
        assert_eq!(float(78.1), "78.1");
        assert_eq!(float(8.0), "8.0");
        assert_eq!(round2(7.966), 7.97);
        assert_eq!(round2(2.675), 2.67); // 2.675 is 2.67499999... in binary, as in Python
    }
}
