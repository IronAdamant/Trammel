//! Lossless lexer shared by the languages that do not yet have a statement
//! grammar in the stele core. Every byte is a token, so the tree round-trips.
//! Broken input is tokenised, never panicked on.

use crate::lang::kind;
use crate::syntax::cst::{Builder, Tree};
use crate::syntax::query::SymbolHit;

#[derive(Clone, Copy)]
pub enum Style {
    C,
    Hash,
    Php,
}

pub fn parse(source: &str, style: Style, symbols: Vec<SymbolHit>) -> (Tree, Vec<SymbolHit>) {
    (lex(source, style), symbols)
}

pub fn lex(source: &str, style: Style) -> Tree {
    let chars: Vec<char> = source.chars().collect();
    // Byte offset of every char, so each token's offsets are O(1).
    let mut offsets = Vec::with_capacity(chars.len() + 1);
    let mut total = 0u32;
    for ch in &chars {
        offsets.push(total);
        total += ch.len_utf8() as u32;
    }
    offsets.push(total);
    let byte_at = |_: &[char], index: usize| offsets[index];
    let mut builder = Builder::new();
    builder.start(kind::FILE, 0);
    let mut i = 0usize;
    while i < chars.len() {
        let start = byte_at(&chars, i);
        if chars[i].is_whitespace() && chars[i] != '\n' {
            while i < chars.len() && chars[i].is_whitespace() && chars[i] != '\n' {
                i += 1;
            }
            builder.leaf(kind::WHITESPACE, start, byte_at(&chars, i));
            continue;
        }
        if chars[i] == '\n' {
            i += 1;
            builder.leaf(kind::NEWLINE, start, byte_at(&chars, i));
            continue;
        }
        if is_comment(&chars, i, style) {
            i = comment_end(&chars, i, style);
            builder.leaf(kind::COMMENT, start, byte_at(&chars, i));
            continue;
        }
        if let Some(end) = string_end(&chars, i) {
            builder.leaf(kind::STRING, start, byte_at(&chars, end));
            i = end;
            continue;
        }
        if is_ident_start(chars[i]) {
            i += 1;
            while i < chars.len() && is_ident_continue(chars[i]) {
                i += 1;
            }
            builder.leaf(kind::IDENT, start, byte_at(&chars, i));
            continue;
        }
        if chars[i].is_ascii_digit() {
            i += 1;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.' || chars[i] == '_') {
                i += 1;
            }
            builder.leaf(kind::NUMBER, start, byte_at(&chars, i));
            continue;
        }
        i += 1;
        builder.leaf(kind::OP, start, byte_at(&chars, i));
    }
    builder.finish(source.len() as u32);
    builder.tree()
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn is_comment(chars: &[char], i: usize, style: Style) -> bool {
    match style {
        Style::C => chars[i] == '/' && i + 1 < chars.len() && (chars[i + 1] == '/' || chars[i + 1] == '*'),
        Style::Hash => chars[i] == '#',
        Style::Php => chars[i] == '#' || (chars[i] == '/' && i + 1 < chars.len() && (chars[i + 1] == '/' || chars[i + 1] == '*')),
    }
}

fn comment_end(chars: &[char], i: usize, style: Style) -> usize {
    if matches!(style, Style::C | Style::Php) && chars[i] == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
        let mut j = i + 2;
        while j + 1 < chars.len() && !(chars[j] == '*' && chars[j + 1] == '/') {
            j += 1;
        }
        return if j + 1 < chars.len() { j + 2 } else { chars.len() };
    }
    if matches!(style, Style::C | Style::Php) && chars[i] == '/' {
        let mut j = i + 2;
        while j < chars.len() && chars[j] != '\n' {
            j += 1;
        }
        return j;
    }
    let mut j = i + 1;
    while j < chars.len() && chars[j] != '\n' {
        j += 1;
    }
    j
}

fn string_end(chars: &[char], i: usize) -> Option<usize> {
    let ch = chars[i];
    if ch != '"' && ch != '\'' && ch != '`' {
        return None;
    }
    let mut j = i + 1;
    while j < chars.len() {
        if chars[j] == '\\' && j + 1 < chars.len() && ch != '`' {
            j += 2;
            continue;
        }
        if chars[j] == ch {
            return Some(j + 1);
        }
        if chars[j] == '\n' && ch != '`' {
            return Some(j);
        }
        j += 1;
    }
    Some(chars.len())
}

