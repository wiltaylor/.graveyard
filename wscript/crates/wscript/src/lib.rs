pub mod bindings;
pub mod compiler;
pub mod engine;
pub mod reflect;
pub mod runtime;

#[cfg(feature = "lsp")]
pub mod lsp;

#[cfg(feature = "dap")]
pub mod dap;

pub mod query_db;

// Re-exports for convenience.
pub use bindings::{BindingRegistry, HostFnBinding, ParamInfo, ScriptType};
pub use engine::{Engine, EngineConfig};
pub use reflect::{
    FieldInfo, FieldType, FieldValue, GlobalInfo, GlobalKind, StructTypeInfo, StructView,
    TypeLayouts,
};
pub use runtime::value::Value;
#[cfg(feature = "runtime")]
pub use runtime::vm::{CompiledScript, ScriptEngine, Vm};
