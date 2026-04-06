//! Incremental compiler state for the LSP server.

use crate::bindings::BindingRegistry;
use crate::compiler::ast::Program;
use crate::compiler::token::Span;
use std::collections::HashMap;
use std::sync::Arc;

/// A diagnostic message from the compiler.
#[derive(Debug, Clone)]
pub struct DiagnosticInfo {
    pub span: Span,
    pub message: String,
    pub severity: Severity,
    pub code: Option<String>,
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

/// Incremental compiler database for the LSP.
pub struct QueryDb {
    sources: HashMap<String, String>,
    asts: HashMap<String, Program>,
    diagnostics: HashMap<String, Vec<DiagnosticInfo>>,
    #[allow(dead_code)]
    host_bindings: Arc<BindingRegistry>,
}

impl QueryDb {
    pub fn new(host_bindings: Arc<BindingRegistry>) -> Self {
        Self {
            sources: HashMap::new(),
            asts: HashMap::new(),
            diagnostics: HashMap::new(),
            host_bindings,
        }
    }

    pub fn update_source(&mut self, uri: &str, source: String) {
        self.sources.insert(uri.to_string(), source.clone());
        // Re-parse the file
        let tokens = crate::compiler::lexer::Lexer::new(&source).tokenize();
        let (ast, parse_diags) = crate::compiler::parser::Parser::new(&tokens).parse_program();
        let diags: Vec<DiagnosticInfo> = parse_diags.into_iter().map(|d| DiagnosticInfo {
            span: d.span,
            message: d.message,
            severity: Severity::Error,
            code: d.code,
            hint: d.hint,
        }).collect();
        self.asts.insert(uri.to_string(), ast);
        self.diagnostics.insert(uri.to_string(), diags);
    }

    pub fn get_diagnostics(&self, uri: &str) -> &[DiagnosticInfo] {
        self.diagnostics.get(uri).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn get_ast(&self, uri: &str) -> Option<&Program> {
        self.asts.get(uri)
    }

    pub fn get_source(&self, uri: &str) -> Option<&str> {
        self.sources.get(uri).map(|s| s.as_str())
    }

    pub fn host_bindings(&self) -> &BindingRegistry {
        &self.host_bindings
    }
}
