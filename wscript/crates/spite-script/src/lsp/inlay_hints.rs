//! Inlay hint support for the LSP server.

use tower_lsp::lsp_types::*;
use crate::compiler::ast::{self, Program, Item, Block, Stmt, Expr, ExprKind};


/// Generate inlay hints for a parsed program.
/// Shows inferred types on let bindings without explicit type annotations.
pub fn inlay_hints(program: &Program) -> Vec<InlayHint> {
    let mut hints = Vec::new();
    for item in &program.items {
        collect_item_hints(item, &mut hints);
    }
    hints
}

fn collect_item_hints(item: &Item, hints: &mut Vec<InlayHint>) {
    match item {
        Item::FnDecl(f) => collect_block_hints(&f.body, hints),
        Item::ImplBlock(imp) => {
            for method in &imp.methods {
                collect_block_hints(&method.body, hints);
            }
        }
        _ => {}
    }
}

fn collect_block_hints(block: &Block, hints: &mut Vec<InlayHint>) {
    for stmt in &block.stmts {
        match stmt {
            Stmt::Let(let_stmt) => {
                // Show type hint if no explicit annotation
                if let_stmt.ty.is_none() {
                    if let ast::Pattern::Ident { span, name, .. } = &let_stmt.pattern {
                        hints.push(InlayHint {
                            position: Position {
                                line: span.line.saturating_sub(1),
                                character: span.col.saturating_sub(1) + name.len() as u32,
                            },
                            label: InlayHintLabel::String(": <inferred>".to_string()),
                            kind: Some(InlayHintKind::TYPE),
                            text_edits: None,
                            tooltip: None,
                            padding_left: None,
                            padding_right: None,
                            data: None,
                        });
                    }
                }
            }
            Stmt::Expr(expr_stmt) => collect_expr_hints(&expr_stmt.expr, hints),
            _ => {}
        }
    }
}

fn collect_expr_hints(expr: &Expr, hints: &mut Vec<InlayHint>) {
    match &expr.kind {
        ExprKind::If { then_block, else_block, .. } => {
            collect_block_hints(then_block, hints);
            if let Some(else_expr) = else_block {
                collect_expr_hints(else_expr, hints);
            }
        }
        ExprKind::Block(block) => collect_block_hints(block, hints),
        ExprKind::For { body, .. } | ExprKind::While { body, .. } | ExprKind::Loop { body } => {
            collect_block_hints(body, hints);
        }
        _ => {}
    }
}
