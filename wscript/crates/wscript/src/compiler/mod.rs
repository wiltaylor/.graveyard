//! Wscript compiler pipeline.

pub mod token;
pub mod ast;
pub mod lexer;
pub mod parser;
pub mod tycheck;
pub mod lower;
pub mod ir;
pub mod codegen;
pub mod source_map;

use crate::bindings::BindingRegistry;
use token::Span;

/// A compiler diagnostic.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub span: Span,
    pub message: String,
    pub severity: DiagnosticSeverity,
    pub code: Option<String>,
    pub hint: Option<String>,
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let level = match self.severity {
            DiagnosticSeverity::Error => "error",
            DiagnosticSeverity::Warning => "warning",
            DiagnosticSeverity::Info => "info",
        };
        write!(f, "{}: {} (line {}, col {})", level, self.message, self.span.line, self.span.col)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
}

/// The result of a successful compilation.
#[derive(Debug)]
pub struct CompileResult {
    pub ast: ast::Program,
    pub diagnostics: Vec<Diagnostic>,
    pub wasm_bytes: Option<Vec<u8>>,
    pub type_layouts: crate::reflect::TypeLayouts,
}

impl CompileResult {
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.severity == DiagnosticSeverity::Error)
    }
}

/// Compile source text into a CompileResult.
pub fn compile(
    source: &str,
    bindings: &BindingRegistry,
    debug_mode: bool,
) -> Result<CompileResult, Vec<Diagnostic>> {
    // Phase 1: Lex
    let tokens = lexer::Lexer::new(source).tokenize();

    // Phase 2: Parse
    let (ast, parse_diags) = parser::Parser::new(&tokens).parse_program();

    let mut diagnostics: Vec<Diagnostic> = parse_diags
        .into_iter()
        .map(|d| Diagnostic {
            span: d.span,
            message: d.message,
            severity: DiagnosticSeverity::Error,
            code: d.code,
            hint: d.hint,
        })
        .collect();

    if diagnostics.iter().any(|d| d.severity == DiagnosticSeverity::Error) {
        return Err(diagnostics);
    }

    // Phase 3: Type check
    // For v0.1, type check errors are reported as warnings so compilation can
    // proceed to WASM generation for testing purposes.
    let (_type_info, type_diags) = tycheck::type_check(&ast, bindings);
    diagnostics.extend(type_diags.into_iter().map(|d| Diagnostic {
        span: d.span,
        message: d.message,
        severity: DiagnosticSeverity::Warning, // treat as warnings for now
        code: Some(d.code.to_string()),
        hint: None,
    }));

    // Phase 4: Lower to IR
    let ir_module = lower::lower(&ast, debug_mode, bindings);

    // Phase 5: Codegen to WASM
    let (wasm_bytes, _source_map, type_layouts) = codegen::codegen(&ir_module, debug_mode);

    Ok(CompileResult {
        ast,
        diagnostics,
        wasm_bytes: Some(wasm_bytes),
        type_layouts,
    })
}
