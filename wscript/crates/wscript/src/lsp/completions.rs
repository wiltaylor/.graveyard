//! Completion support for the LSP server.

use crate::query_db::QueryDb;
use tower_lsp::lsp_types::*;

/// Generate completions for the given position in a file.
pub fn completions(db: &QueryDb, _uri: &str, _position: Position) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    // Keywords
    let keywords = [
        "let", "mut", "const", "fn", "return", "if", "else", "match", "for", "in", "while", "loop",
        "break", "continue", "struct", "impl", "trait", "enum", "true", "false", "as", "pub",
        "self", "Self", "None", "Some", "Ok", "Err",
    ];
    for kw in &keywords {
        items.push(CompletionItem {
            label: kw.to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            ..Default::default()
        });
    }

    // Built-in types
    let types = [
        "i8", "i16", "i32", "i64", "i128", "u8", "u16", "u32", "u64", "u128", "f32", "f64", "bool",
        "char", "String", "Map", "Option", "Result", "Ref",
    ];
    for ty in &types {
        items.push(CompletionItem {
            label: ty.to_string(),
            kind: Some(CompletionItemKind::CLASS),
            ..Default::default()
        });
    }

    // Built-in functions
    let builtins = [
        ("print", "Print a value to stdout"),
        ("print_err", "Print a value to stderr"),
        ("abs", "Absolute value"),
        ("min", "Minimum of two values"),
        ("max", "Maximum of two values"),
        ("clamp", "Clamp value to range"),
        ("sqrt", "Square root"),
        ("pow", "Power"),
        ("floor", "Floor"),
        ("ceil", "Ceiling"),
        ("round", "Round"),
    ];
    for (name, doc) in &builtins {
        items.push(CompletionItem {
            label: name.to_string(),
            kind: Some(CompletionItemKind::FUNCTION),
            detail: Some(doc.to_string()),
            ..Default::default()
        });
    }

    // Host bindings
    let bindings = db.host_bindings();
    for (name, binding) in &bindings.functions {
        items.push(CompletionItem {
            label: name.clone(),
            kind: Some(CompletionItemKind::FUNCTION),
            detail: binding.doc.clone(),
            ..Default::default()
        });
    }
    for (name, type_binding) in &bindings.types {
        items.push(CompletionItem {
            label: name.clone(),
            kind: Some(CompletionItemKind::CLASS),
            detail: type_binding.doc.clone(),
            ..Default::default()
        });
    }

    // Macros
    let macros = [
        ("assert!", "Assert a condition"),
        ("assert_eq!", "Assert two values are equal"),
        ("assert_ne!", "Assert two values are not equal"),
        ("dbg!", "Debug print and pass through"),
        ("error!", "Create an error"),
        ("bail!", "Return early with an error"),
        ("ensure!", "Assert or bail"),
        ("todo!", "Mark as not implemented"),
        ("unreachable!", "Mark as unreachable"),
    ];
    for (name, doc) in &macros {
        items.push(CompletionItem {
            label: name.to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some(doc.to_string()),
            ..Default::default()
        });
    }

    items
}
