//! Each failed test's own message, read from the test output, so a failure
//! can be understood without running the tests again. Knows cargo's libtest,
//! Python's unittest and pytest, `go test` and Jest. Anything else gives no
//! entries and the caller falls back to the output itself.

/// Characters kept per test.
pub const EXCERPT_CHARS: usize = 400;
/// Tests listed per reply.
pub const MAX_TESTS: usize = 8;

/// (test name, excerpt of its message), in output order, one entry per name.
pub fn failed_tests(output: &str) -> Vec<(String, String)> {
    let clean = strip_ansi(output).replace("\r\n", "\n");
    let lines: Vec<&str> = clean.lines().collect();
    let mut found: Vec<(String, String)> = Vec::new();
    let mut push = |name: String, body: String| {
        if name.is_empty() { return; }
        match found.iter_mut().find(|(n, _)| *n == name) {
            Some(entry) => { if entry.1.is_empty() { entry.1 = body; } }
            None => found.push((name, body)),
        }
    };
    for (name, body) in libtest(&lines) { push(name, head(&body)); }
    for (name, body) in unittest(&lines) { push(name, tail(&body)); }
    for (name, body) in pytest(&lines) { push(name, body); }
    for (name, body) in go(&lines) { push(name, head(&body)); }
    for (name, body) in jest(&lines) { push(name, head(&body)); }
    found
}

/// A one-line summary for a plan step: the first failed test and its
/// message, else the first compiler error lines, else the end of the text.
pub fn summary(output: &str, max: usize) -> String {
    let tests = failed_tests(output);
    let line = if let Some((name, excerpt)) = tests.first() {
        let more = if tests.len() > 1 { format!(" (+{} more failed)", tests.len() - 1) } else { String::new() };
        format!("{name}: {}{more}", excerpt.replace('\n', " | "))
    } else {
        let clean = strip_ansi(output).replace("\r\n", "\n");
        let errors: Vec<&str> = clean.lines().map(str::trim).filter(|l| l.starts_with("error") || l.starts_with("--> ")).take(4).collect();
        if errors.is_empty() { tail(clean.trim()).replace('\n', " | ") } else { errors.join(" | ") }
    };
    cut_head(&line, max)
}

fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for d in chars.by_ref() { if d.is_ascii_alphabetic() { break; } }
            continue;
        }
        out.push(c);
    }
    out
}

/// Drop noise lines and blank runs.
fn tidy(lines: &[&str]) -> String {
    let kept: Vec<&str> = lines.iter().copied()
        .filter(|l| !l.starts_with("note: run with `RUST_BACKTRACE=1`"))
        .filter(|l| !l.trim().is_empty())
        .collect();
    kept.join("\n")
}

fn cut_head(text: &str, max: usize) -> String {
    if text.chars().count() <= max { return text.to_string(); }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn head(text: &str) -> String { cut_head(text, EXCERPT_CHARS) }

/// A traceback ends with the error, so keep its end.
fn tail(text: &str) -> String {
    let n = text.chars().count();
    if n <= EXCERPT_CHARS { return text.to_string(); }
    let mut out = String::from("…");
    out.extend(text.chars().skip(n - (EXCERPT_CHARS - 1)));
    out
}

/// `---- name stdout ----` sections, and `test name ... FAILED` lines for
/// failures that printed nothing.
fn libtest(lines: &[&str]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if let Some(name) = line.strip_prefix("test ").and_then(|l| l.strip_suffix(" ... FAILED")) {
            out.push((name.to_string(), String::new()));
        } else if let Some(name) = line.strip_prefix("---- ").and_then(|l| l.strip_suffix(" stdout ----").or_else(|| l.strip_suffix(" stderr ----"))) {
            let start = i + 1;
            let mut end = start;
            while end < lines.len() && !lines[end].starts_with("---- ") && lines[end] != "failures:" && lines[end] != "successes:" { end += 1; }
            out.push((name.to_string(), tidy(&lines[start..end])));
            i = end;
            continue;
        }
        i += 1;
    }
    // The sections come after the FAILED lines: keep one entry per name.
    let mut merged: Vec<(String, String)> = Vec::new();
    for (name, body) in out {
        match merged.iter_mut().find(|(n, _)| *n == name) {
            Some(entry) => { if entry.1.is_empty() { entry.1 = body; } }
            None => merged.push((name, body)),
        }
    }
    merged
}

/// `FAIL: test_x (module.Class.test_x)` or `ERROR: ...` between `====` rules.
fn unittest(lines: &[&str]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let rule = |l: &str, c: char| l.len() >= 20 && l.chars().all(|x| x == c);
    let mut i = 0;
    while i + 1 < lines.len() {
        if rule(lines[i], '=') {
            let header = lines[i + 1];
            if let Some(name) = header.strip_prefix("FAIL: ").or_else(|| header.strip_prefix("ERROR: ")) {
                let mut start = i + 2;
                if start < lines.len() && rule(lines[start], '-') { start += 1; }
                let mut end = start;
                while end < lines.len() && !rule(lines[end], '=') && !rule(lines[end], '-') { end += 1; }
                out.push((name.trim().to_string(), tidy(&lines[start..end])));
                i = end;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// `____ test_name ____` sections under `= FAILURES =`; the `E` lines and
/// the closing `file:line: Error` carry the message.
fn pytest(lines: &[&str]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Some(start) = lines.iter().position(|l| l.starts_with('=') && l.contains(" FAILURES ")) else { return out };
    let header = |l: &str| -> Option<String> {
        let t = l.trim();
        if t.starts_with("___") && t.ends_with("___") {
            let name = t.trim_matches('_').trim();
            if !name.is_empty() { return Some(name.to_string()); }
        }
        None
    };
    let mut i = start + 1;
    while i < lines.len() && !lines[i].starts_with('=') {
        if let Some(name) = header(lines[i]) {
            let mut end = i + 1;
            while end < lines.len() && header(lines[end]).is_none() && !lines[end].starts_with('=') { end += 1; }
            let body = &lines[i + 1..end];
            let mut picked: Vec<&str> = body.iter().copied().filter(|l| l.starts_with("E ")).collect();
            if let Some(last) = body.iter().rev().find(|l| !l.trim().is_empty()) { if !picked.contains(last) { picked.push(last); } }
            let text = if picked.is_empty() { tail(&tidy(body)) } else { head(&tidy(&picked)) };
            out.push((name, text));
            i = end;
            continue;
        }
        i += 1;
    }
    out
}

/// `--- FAIL: TestX (0.00s)` and the indented lines after it.
fn go(lines: &[&str]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let Some(rest) = line.trim_start().strip_prefix("--- FAIL: ") else { continue };
        let name = rest.split_whitespace().next().unwrap_or("").to_string();
        let indent = line.len() - line.trim_start().len();
        let body: Vec<&str> = lines[i + 1..].iter().copied()
            .take_while(|l| l.len() - l.trim_start().len() > indent && !l.trim_start().starts_with("--- "))
            .map(str::trim)
            .collect();
        let mut text = tidy(&body);
        if text.is_empty() {
            // `go test -v` prints a test's messages before its FAIL line.
            let run = format!("=== RUN   {name}");
            if let Some(from) = lines[..i].iter().rposition(|l| l.trim_start() == run) {
                let before: Vec<&str> = lines[from + 1..i].iter().copied().filter(|l| l.starts_with(' ') || l.starts_with('\t')).map(str::trim).collect();
                text = tidy(&before);
            }
        }
        out.push((name, text));
    }
    out
}

/// `● Suite › test` blocks.
fn jest(lines: &[&str]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if let Some(name) = lines[i].trim_start().strip_prefix("● ") {
            if name.starts_with("Console") { i += 1; continue; }
            let mut end = i + 1;
            while end < lines.len() && !lines[end].trim_start().starts_with("● ") && !lines[end].starts_with("Test Suites:") { end += 1; }
            let body: Vec<&str> = lines[i + 1..end].iter().map(|l| l.trim()).collect();
            out.push((name.trim().to_string(), tidy(&body)));
            i = end;
            continue;
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn libtest_failures_carry_their_panic_message() {
        let output = "running 3 tests\ntest ok_one ... ok\ntest tests::adds ... FAILED\ntest tests::silent ... FAILED\n\nfailures:\n\n---- tests::adds stdout ----\n\nthread 'tests::adds' panicked at src/lib.rs:9:9:\nassertion `left == right` failed\n  left: 3\n right: 4\nnote: run with `RUST_BACKTRACE=1` environment variable to display a backtrace\n\n\nfailures:\n    tests::adds\n    tests::silent\n\ntest result: FAILED. 1 passed; 2 failed\n";
        let got = failed_tests(output);
        assert_eq!(got.len(), 2, "{got:?}");
        assert_eq!(got[0].0, "tests::adds");
        assert_eq!(got[0].1, "thread 'tests::adds' panicked at src/lib.rs:9:9:\nassertion `left == right` failed\n  left: 3\n right: 4");
        assert_eq!(got[1], ("tests::silent".to_string(), String::new()));
        let line = summary(output, 80);
        assert!(line.starts_with("tests::adds: thread 'tests::adds' panicked at src/lib.rs:9:9: | assertion") && line.ends_with('…') && line.chars().count() == 80, "{line}");
    }

    #[test]
    fn unittest_pytest_go_and_jest_failures_are_read() {
        let unit = "F.\n======================================================================\nFAIL: test_total (test_cart.CartTest.test_total)\n----------------------------------------------------------------------\nTraceback (most recent call last):\n  File \"test_cart.py\", line 6, in test_total\n    self.assertEqual(total(), 5)\nAssertionError: 4 != 5\n\n----------------------------------------------------------------------\nRan 2 tests in 0.001s\n\nFAILED (failures=1)\n";
        let got = failed_tests(unit);
        assert_eq!(got[0].0, "test_total (test_cart.CartTest.test_total)");
        assert!(got[0].1.ends_with("AssertionError: 4 != 5"), "{:?}", got[0].1);

        let py = "=================================== FAILURES ===================================\n__________________________________ test_total __________________________________\n\n    def test_total():\n>       assert total() == 5\nE       assert 4 == 5\nE        +  where 4 = total()\n\ntest_cart.py:4: AssertionError\n=========================== short test summary info ============================\nFAILED test_cart.py::test_total - assert 4 == 5\n";
        assert_eq!(failed_tests(py), vec![("test_total".to_string(), "E       assert 4 == 5\nE        +  where 4 = total()\ntest_cart.py:4: AssertionError".to_string())]);

        let go_out = "--- FAIL: TestTotal (0.00s)\n    cart_test.go:9: total() = 4, want 5\nFAIL\nexit status 1\n";
        assert_eq!(failed_tests(go_out), vec![("TestTotal".to_string(), "cart_test.go:9: total() = 4, want 5".to_string())]);
        let go_verbose = "=== RUN   TestTotal\n    cart_test.go:9: total() = 4, want 5\n--- FAIL: TestTotal (0.00s)\n=== RUN   TestOther\n--- PASS: TestOther (0.00s)\nFAIL\n";
        assert_eq!(failed_tests(go_verbose), vec![("TestTotal".to_string(), "cart_test.go:9: total() = 4, want 5".to_string())]);

        let jest_out = "FAIL ./cart.test.js\n  \u{1b}[1m● cart › totals\u{1b}[22m\n\n    expect(received).toBe(expected)\n\n    Expected: 5\n    Received: 4\n\nTest Suites: 1 failed, 1 total\n";
        assert_eq!(failed_tests(jest_out), vec![("cart › totals".to_string(), "expect(received).toBe(expected)\nExpected: 5\nReceived: 4".to_string())]);
    }

    #[test]
    fn long_messages_are_capped_and_unknown_output_falls_back() {
        let long = format!("test big ... FAILED\n\nfailures:\n\n---- big stdout ----\n{}\n\nfailures:\n    big\n", "y".repeat(5000));
        let got = failed_tests(&long);
        assert_eq!(got[0].1.chars().count(), EXCERPT_CHARS);
        assert!(failed_tests("all good\n").is_empty());
        assert_eq!(summary("error[E0425]: cannot find value `x`\n  --> src/lib.rs:2:5\nerror: aborting\n", 200), "error[E0425]: cannot find value `x` | --> src/lib.rs:2:5 | error: aborting");
    }
}
