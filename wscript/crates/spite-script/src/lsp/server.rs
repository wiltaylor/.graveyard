use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};
use crate::query_db::QueryDb;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::lsp_types::Url;

pub struct SpiteLspServer {
    client: Client,
    db: Arc<RwLock<QueryDb>>,
}

impl SpiteLspServer {
    pub fn new(client: Client, db: Arc<RwLock<QueryDb>>) -> Self {
        Self { client, db }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for SpiteLspServer {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
                completion_provider: Some(CompletionOptions::default()),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                inlay_hint_provider: Some(OneOf::Left(true)),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(SemanticTokensOptions {
                        legend: SemanticTokensLegend {
                            token_types: super::semantic_tokens::TOKEN_TYPES.to_vec(),
                            token_modifiers: super::semantic_tokens::TOKEN_MODIFIERS.to_vec(),
                        },
                        full: Some(SemanticTokensFullOptions::Bool(true)),
                        range: None,
                        ..Default::default()
                    })
                ),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn shutdown(&self) -> Result<()> { Ok(()) }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        let text = params.text_document.text;
        let mut db: tokio::sync::RwLockWriteGuard<'_, QueryDb> = self.db.write().await;
        db.update_source(&uri, text);
        drop(db);
        self.publish_diagnostics(&uri).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        if let Some(change) = params.content_changes.into_iter().last() {
            let mut db: tokio::sync::RwLockWriteGuard<'_, QueryDb> = self.db.write().await;
            db.update_source(&uri, change.text);
            drop(db);
            self.publish_diagnostics(&uri).await;
        }
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri.to_string();
        let position = params.text_document_position.position;
        let db = self.db.read().await;
        let items = super::completions::completions(&db, &uri, position);
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri.to_string();
        let position = params.text_document_position_params.position;
        let db = self.db.read().await;
        Ok(super::hover::hover(&db, &uri, position))
    }

    async fn document_symbol(&self, params: DocumentSymbolParams) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri.to_string();
        let db = self.db.read().await;
        if let Some(ast) = db.get_ast(&uri) {
            let mut symbols = Vec::new();
            for item in &ast.items {
                if let Some(sym) = item_to_symbol(item) {
                    symbols.push(sym);
                }
            }
            Ok(Some(DocumentSymbolResponse::Flat(symbols)))
        } else {
            Ok(None)
        }
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri.to_string();
        let db = self.db.read().await;
        if let Some(source) = db.get_source(&uri) {
            let edits = super::formatting::format_document(source);
            Ok(Some(edits))
        } else {
            Ok(None)
        }
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let uri = params.text_document.uri.to_string();
        let db = self.db.read().await;
        if let Some(ast) = db.get_ast(&uri) {
            let hints = super::inlay_hints::inlay_hints(ast);
            Ok(Some(hints))
        } else {
            Ok(None)
        }
    }

    async fn semantic_tokens_full(&self, params: SemanticTokensParams) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri.to_string();
        let db = self.db.read().await;
        if let Some(source) = db.get_source(&uri) {
            let tokens = super::semantic_tokens::semantic_tokens(source);
            Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: tokens,
            })))
        } else {
            Ok(None)
        }
    }
}

fn span_to_range(span: &crate::compiler::token::Span) -> Range {
    Range {
        start: Position { line: span.line.saturating_sub(1), character: span.col.saturating_sub(1) },
        end: Position { line: span.line.saturating_sub(1), character: span.col.saturating_sub(1) + (span.end - span.start) },
    }
}

#[allow(deprecated)]
fn item_to_symbol(item: &crate::compiler::ast::Item) -> Option<SymbolInformation> {
    use crate::compiler::ast::Item;
    match item {
        Item::FnDecl(f) => Some(SymbolInformation {
            name: f.name.to_string(),
            kind: SymbolKind::FUNCTION,
            tags: None,
            deprecated: None,
            location: Location { uri: Url::parse("file:///").unwrap(), range: span_to_range(&f.span) },
            container_name: None,
        }),
        Item::StructDecl(s) => Some(SymbolInformation {
            name: s.name.to_string(),
            kind: SymbolKind::STRUCT,
            tags: None,
            deprecated: None,
            location: Location { uri: Url::parse("file:///").unwrap(), range: span_to_range(&s.span) },
            container_name: None,
        }),
        Item::EnumDecl(e) => Some(SymbolInformation {
            name: e.name.to_string(),
            kind: SymbolKind::ENUM,
            tags: None,
            deprecated: None,
            location: Location { uri: Url::parse("file:///").unwrap(), range: span_to_range(&e.span) },
            container_name: None,
        }),
        Item::TraitDecl(t) => Some(SymbolInformation {
            name: t.name.to_string(),
            kind: SymbolKind::INTERFACE,
            tags: None,
            deprecated: None,
            location: Location { uri: Url::parse("file:///").unwrap(), range: span_to_range(&t.span) },
            container_name: None,
        }),
        Item::ConstDecl(c) => Some(SymbolInformation {
            name: c.name.to_string(),
            kind: SymbolKind::CONSTANT,
            tags: None,
            deprecated: None,
            location: Location { uri: Url::parse("file:///").unwrap(), range: span_to_range(&c.span) },
            container_name: None,
        }),
        _ => None,
    }
}

impl SpiteLspServer {
    async fn publish_diagnostics(&self, uri: &str) {
        let db: tokio::sync::RwLockReadGuard<'_, QueryDb> = self.db.read().await;
        let diags = db.get_diagnostics(uri);
        let lsp_diags: Vec<Diagnostic> = diags.iter().map(|d| {
            let severity = match d.severity {
                crate::query_db::Severity::Error => DiagnosticSeverity::ERROR,
                crate::query_db::Severity::Warning => DiagnosticSeverity::WARNING,
                crate::query_db::Severity::Info => DiagnosticSeverity::INFORMATION,
                crate::query_db::Severity::Hint => DiagnosticSeverity::HINT,
            };
            let code: Option<NumberOrString> = d.code.as_ref().map(|c: &String| NumberOrString::String(c.clone()));
            Diagnostic {
                range: Range {
                    start: Position { line: d.span.line.saturating_sub(1), character: d.span.col.saturating_sub(1) },
                    end: Position { line: d.span.line.saturating_sub(1), character: d.span.col.saturating_sub(1) + (d.span.end - d.span.start) },
                },
                severity: Some(severity),
                code,
                message: d.message.clone(),
                ..Default::default()
            }
        }).collect();
        // parse the URI back
        if let Ok(parsed_uri) = uri.parse() {
            self.client.publish_diagnostics(parsed_uri, lsp_diags, None).await;
        }
    }
}
