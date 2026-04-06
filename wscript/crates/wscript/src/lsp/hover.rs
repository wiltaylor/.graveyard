//! Hover support for the LSP server.

use tower_lsp::lsp_types::*;
use crate::query_db::QueryDb;

/// Generate hover information for the given position.
pub fn hover(db: &QueryDb, uri: &str, position: Position) -> Option<Hover> {
    let source = db.get_source(uri)?;
    let token = find_token_at_position(source, position)?;

    let contents = match token.as_str() {
        // Keywords
        "let" => "**let** — Declare a variable binding.\n\n```wscript\nlet x = 42;\nlet mut y: String = \"hello\";\n```",
        "mut" => "**mut** — Mark a binding as mutable.",
        "fn" => "**fn** — Declare a function.\n\n```wscript\nfn name(param: Type) -> ReturnType { body }\n```",
        "struct" => "**struct** — Declare a struct type.\n\n```wscript\nstruct Point { x: f64, y: f64 }\n```",
        "enum" => "**enum** — Declare an enum type.\n\n```wscript\nenum Direction { North, South, East, West }\n```",
        "trait" => "**trait** — Declare a trait.\n\n```wscript\ntrait Describable { fn describe(&self) -> String; }\n```",
        "impl" => "**impl** — Implement methods or traits for a type.",
        "match" => "**match** — Pattern matching expression.\n\n```wscript\nmatch value {\n    pattern => expr,\n    _ => fallback,\n}\n```",
        "if" => "**if** — Conditional expression.\n\n```wscript\nif condition { then_expr } else { else_expr }\n```",
        "for" => "**for** — Iterate over a collection or range.\n\n```wscript\nfor item in collection { body }\nfor i in 0..10 { body }\n```",
        "return" => "**return** — Return a value from the current function.",
        "print" => "**print**(value) — Print a value to stdout.\n\nCalls `.to_string()` on the value and prints with a newline.",
        // Types
        "i32" => "**i32** — 32-bit signed integer (-2,147,483,648 to 2,147,483,647)",
        "i64" => "**i64** — 64-bit signed integer",
        "f32" => "**f32** — 32-bit floating-point (~7 decimal digits precision)",
        "f64" => "**f64** — 64-bit floating-point (~15 decimal digits precision)",
        "bool" => "**bool** — Boolean type: `true` or `false`",
        "char" => "**char** — Unicode scalar value (U+0000 to U+10FFFF)",
        "String" => "**String** — Heap-allocated, UTF-8 encoded, growable string.\n\nRef-counted. Assignment shares the reference.",
        "Option" => "**Option<T>** — Optional value: `Some(T)` or `None`",
        "Result" => "**Result<T, E>** — Success or failure: `Ok(T)` or `Err(E)`\n\n`Result<T>` is shorthand for `Result<T, Error>`",
        "Map" => "**Map<K, V>** — Hash map from keys to values.\n\nLiteral syntax: `#{ \"key\": value }`",
        _ => return None,
    };

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: contents.to_string(),
        }),
        range: None,
    })
}

/// Find the word at the given position in the source text.
fn find_token_at_position(source: &str, position: Position) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();
    let line = lines.get(position.line as usize)?;
    let col = position.character as usize;

    if col >= line.len() {
        return None;
    }

    // Find word boundaries
    let bytes = line.as_bytes();
    let mut start = col;
    let mut end = col;

    while start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_') {
        start -= 1;
    }
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
        end += 1;
    }

    if start == end {
        return None;
    }

    Some(line[start..end].to_string())
}
