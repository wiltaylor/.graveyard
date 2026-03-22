pub mod compiler;
pub mod bindings;
pub mod engine;
pub mod runtime;

#[cfg(feature = "lsp")]
pub mod lsp;

#[cfg(feature = "dap")]
pub mod dap;

pub mod query_db;

// Re-exports for convenience.
pub use engine::{Engine, EngineConfig};
pub use runtime::value::Value;
pub use bindings::BindingRegistry;
