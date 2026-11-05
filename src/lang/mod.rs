//! Language dispatch. Python and JSON grammars are the shared stele core.
//! The other languages use the lossless lexer in `code` plus hand-written
//! extractors. Unknown extensions stay one opaque span so bytes round-trip.

pub mod analyze;
pub mod blank;
pub mod brackets;
pub(crate) mod code;
pub mod imports;
mod json;
pub mod kind;
mod python;
pub(crate) mod python_symbols;
pub mod symbols;

use crate::syntax::cst::{Builder, Tree};
use crate::syntax::query::SymbolHit;

pub fn parse(extension: &str, source: &str) -> Tree {
    parse_with_symbols(extension, source).0
}

pub fn parse_with_symbols(extension: &str, source: &str) -> (Tree, Vec<SymbolHit>) {
    match extension {
        "py" | "pyw" | "pyx" => python::parse(source),
        "json" => (json::parse(source), Vec::new()),
        "go" => code::parse(source, code::Style::C, hits("go", source)),
        "rs" => code::parse(source, code::Style::C, hits("rust", source)),
        "c" | "h" | "cpp" | "cc" | "cxx" | "hpp" | "hxx" => code::parse(source, code::Style::C, hits("cpp", source)),
        "java" | "kt" | "kts" => code::parse(source, code::Style::C, hits("java", source)),
        "cs" => code::parse(source, code::Style::C, hits("csharp", source)),
        "rb" => code::parse(source, code::Style::Hash, hits("ruby", source)),
        "php" => code::parse(source, code::Style::Php, hits("php", source)),
        "swift" => code::parse(source, code::Style::C, hits("swift", source)),
        "dart" => code::parse(source, code::Style::C, hits("dart", source)),
        "zig" => code::parse(source, code::Style::C, hits("zig", source)),
        "ts" | "tsx" | "mts" | "js" | "jsx" | "mjs" => code::parse(source, code::Style::C, hits("typescript", source)),
        _ => (opaque(source), Vec::new()),
    }
}

fn hits(language: &str, source: &str) -> Vec<SymbolHit> {
    symbols::collect(language, source)
        .into_iter()
        .map(|(name, kind_name)| SymbolHit {
            name,
            kind: kind_name,
            role: "definition",
            line: 0,
            container: None,
        })
        .collect()
}

fn opaque(source: &str) -> Tree {
    let mut builder = Builder::new();
    builder.start(kind::FILE, 0);
    builder.leaf(kind::STRING, 0, source.len() as u32);
    builder.finish(source.len() as u32);
    builder.tree()
}
