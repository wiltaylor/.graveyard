//! Code formatting support for the LSP server.
//!
//! Provides basic formatting: consistent indentation and spacing.

use tower_lsp::lsp_types::*;

/// Format a Wscript source file.
/// Returns a list of text edits that transform the document to canonical form.
pub fn format_document(source: &str) -> Vec<TextEdit> {
    let formatted = format_source(source);
    if formatted == source {
        return vec![];
    }

    // Replace entire document
    let line_count = source.lines().count();
    vec![TextEdit {
        range: Range {
            start: Position { line: 0, character: 0 },
            end: Position {
                line: line_count as u32,
                character: 0,
            },
        },
        new_text: formatted,
    }]
}

/// Format source text to canonical form.
fn format_source(source: &str) -> String {
    let mut result = String::new();
    let mut indent = 0u32;
    let mut prev_blank = false;

    for line in source.lines() {
        let trimmed = line.trim();

        // Skip multiple blank lines
        if trimmed.is_empty() {
            if !prev_blank && !result.is_empty() {
                result.push('\n');
            }
            prev_blank = true;
            continue;
        }
        prev_blank = false;

        // Decrease indent for closing braces
        if trimmed.starts_with('}') {
            indent = indent.saturating_sub(1);
        }

        // Write indented line
        for _ in 0..indent {
            result.push_str("    ");
        }
        result.push_str(trimmed);
        result.push('\n');

        // Increase indent after opening braces
        if trimmed.ends_with('{') {
            indent += 1;
        }
    }

    // Ensure trailing newline
    if !result.ends_with('\n') {
        result.push('\n');
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_formatting() {
        let input = "fn main() {\nlet x = 42;\nreturn x;\n}\n";
        let expected = "fn main() {\n    let x = 42;\n    return x;\n}\n";
        assert_eq!(format_source(input), expected);
    }

    #[test]
    fn test_nested_indent() {
        let input = "fn foo() {\nif x {\nreturn 1;\n}\n}\n";
        let expected = "fn foo() {\n    if x {\n        return 1;\n    }\n}\n";
        assert_eq!(format_source(input), expected);
    }
}
