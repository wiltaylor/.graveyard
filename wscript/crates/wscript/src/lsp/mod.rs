#[cfg(feature = "lsp")]
mod server;
#[cfg(feature = "lsp")]
mod completions;
#[cfg(feature = "lsp")]
mod hover;
#[cfg(feature = "lsp")]
mod diagnostics;
#[cfg(feature = "lsp")]
mod inlay_hints;
#[cfg(feature = "lsp")]
mod semantic_tokens;
#[cfg(feature = "lsp")]
mod formatting;

#[cfg(feature = "lsp")]
pub use server::WscriptLspServer;
