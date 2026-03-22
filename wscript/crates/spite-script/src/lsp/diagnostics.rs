//! Diagnostics conversion for the LSP server.

use tower_lsp::lsp_types::*;
use crate::query_db::{DiagnosticInfo, Severity};

/// Convert internal diagnostics to LSP diagnostics.
#[allow(dead_code)]
pub fn to_lsp_diagnostics(diags: &[DiagnosticInfo]) -> Vec<Diagnostic> {
    diags
        .iter()
        .map(|d| {
            let severity = match d.severity {
                Severity::Error => DiagnosticSeverity::ERROR,
                Severity::Warning => DiagnosticSeverity::WARNING,
                Severity::Info => DiagnosticSeverity::INFORMATION,
                Severity::Hint => DiagnosticSeverity::HINT,
            };
            Diagnostic {
                range: Range {
                    start: Position {
                        line: d.span.line.saturating_sub(1),
                        character: d.span.col.saturating_sub(1),
                    },
                    end: Position {
                        line: d.span.line.saturating_sub(1),
                        character: d.span.col.saturating_sub(1) + (d.span.end - d.span.start),
                    },
                },
                severity: Some(severity),
                code: d.code.as_ref().map(|c| NumberOrString::String(c.clone())),
                source: Some("spite-script".to_string()),
                message: d.message.clone(),
                related_information: None,
                tags: None,
                code_description: None,
                data: None,
            }
        })
        .collect()
}
