use crate::runtime::value::Value;
use indexmap::IndexMap;
use std::fmt;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// ScriptType — host boundary only
// ---------------------------------------------------------------------------

/// Type information for a script type, used in host binding registration.
///
/// Shrunk to match the boundary `Value`: primitives + `Str` + `Unit`. The
/// internal language type system remains full-featured; this type only
/// describes what can cross the host/script edge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptType {
    I32,
    I64,
    F32,
    F64,
    Bool,
    Str,
    Unit,
}

impl fmt::Display for ScriptType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScriptType::I32 => write!(f, "i32"),
            ScriptType::I64 => write!(f, "i64"),
            ScriptType::F32 => write!(f, "f32"),
            ScriptType::F64 => write!(f, "f64"),
            ScriptType::Bool => write!(f, "bool"),
            ScriptType::Str => write!(f, "str"),
            ScriptType::Unit => write!(f, "()"),
        }
    }
}

// ---------------------------------------------------------------------------
// ParamInfo
// ---------------------------------------------------------------------------

pub struct ParamInfo {
    pub name: String,
    pub ty: ScriptType,
}

impl fmt::Debug for ParamInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParamInfo")
            .field("name", &self.name)
            .field("ty", &self.ty)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// HostFnBinding
// ---------------------------------------------------------------------------

/// Boxed closure type for host functions.
pub type HostFnClosure = Arc<dyn Fn(&[Value]) -> Result<Option<Value>, String> + Send + Sync>;

/// A host function registered for use from scripts.
///
/// The closure returns `Ok(None)` for unit, `Ok(Some(v))` for a value, or
/// `Err(msg)` to trap the script with a panic.
pub struct HostFnBinding {
    pub name: String,
    pub params: Vec<ParamInfo>,
    pub return_type: ScriptType,
    pub doc: Option<String>,
    pub param_docs: Vec<(String, String)>,
    pub return_doc: Option<String>,
    pub examples: Vec<String>,
    pub closure: HostFnClosure,
}

impl fmt::Debug for HostFnBinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostFnBinding")
            .field("name", &self.name)
            .field("params", &self.params)
            .field("return_type", &self.return_type)
            .field("doc", &self.doc)
            .finish_non_exhaustive()
    }
}

// ---------------------------------------------------------------------------
// BindingRegistry
// ---------------------------------------------------------------------------

/// Placeholder host-type binding retained so legacy consumers (LSP
/// completions, old tycheck paths) keep compiling. The shrunk host boundary
/// no longer exposes host-owned types; this map is always empty.
pub struct HostTypeBinding {
    pub name: String,
    pub doc: Option<String>,
    pub methods: IndexMap<String, HostFnBinding>,
}

impl fmt::Debug for HostTypeBinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostTypeBinding")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// Placeholder global binding — kept so tycheck still compiles. Empty in the
/// new embedding API.
#[derive(Debug)]
pub struct GlobalBinding {
    pub name: String,
    pub ty: ScriptType,
}

/// Registry of all host-registered functions.
#[derive(Default)]
pub struct BindingRegistry {
    pub functions: IndexMap<String, HostFnBinding>,
    pub types: IndexMap<String, HostTypeBinding>,
    pub globals: IndexMap<String, GlobalBinding>,
}

impl fmt::Debug for BindingRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BindingRegistry")
            .field("functions", &self.functions.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl BindingRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_function(&self, name: &str) -> Option<&HostFnBinding> {
        self.functions.get(name)
    }

    pub fn get_type(&self, name: &str) -> Option<&HostTypeBinding> {
        self.types.get(name)
    }

    pub fn get_global(&self, name: &str) -> Option<&GlobalBinding> {
        self.globals.get(name)
    }

    pub fn register_function(&mut self, binding: HostFnBinding) {
        self.functions.insert(binding.name.clone(), binding);
    }

    pub fn function_names(&self) -> impl Iterator<Item = &str> {
        self.functions.keys().map(|s| s.as_str())
    }
}

// ---------------------------------------------------------------------------
// IntoScriptType
// ---------------------------------------------------------------------------

pub trait IntoScriptType {
    fn script_type() -> ScriptType;
}

impl IntoScriptType for i32 {
    fn script_type() -> ScriptType {
        ScriptType::I32
    }
}
impl IntoScriptType for i64 {
    fn script_type() -> ScriptType {
        ScriptType::I64
    }
}
impl IntoScriptType for f32 {
    fn script_type() -> ScriptType {
        ScriptType::F32
    }
}
impl IntoScriptType for f64 {
    fn script_type() -> ScriptType {
        ScriptType::F64
    }
}
impl IntoScriptType for bool {
    fn script_type() -> ScriptType {
        ScriptType::Bool
    }
}
impl IntoScriptType for String {
    fn script_type() -> ScriptType {
        ScriptType::Str
    }
}
impl IntoScriptType for &str {
    fn script_type() -> ScriptType {
        ScriptType::Str
    }
}
impl IntoScriptType for () {
    fn script_type() -> ScriptType {
        ScriptType::Unit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_type_display() {
        assert_eq!(ScriptType::I32.to_string(), "i32");
        assert_eq!(ScriptType::Str.to_string(), "str");
    }

    #[test]
    fn into_script_type_primitives() {
        assert_eq!(i32::script_type(), ScriptType::I32);
        assert_eq!(bool::script_type(), ScriptType::Bool);
        assert_eq!(String::script_type(), ScriptType::Str);
        assert_eq!(<()>::script_type(), ScriptType::Unit);
    }

    #[test]
    fn registry_basics() {
        let mut reg = BindingRegistry::new();
        reg.register_function(HostFnBinding {
            name: "add".into(),
            params: vec![
                ParamInfo {
                    name: "a".into(),
                    ty: ScriptType::I32,
                },
                ParamInfo {
                    name: "b".into(),
                    ty: ScriptType::I32,
                },
            ],
            return_type: ScriptType::I32,
            doc: Some("Add two integers.".into()),
            param_docs: vec![],
            return_doc: None,
            examples: vec![],
            closure: Arc::new(|args| {
                let a: i32 = args[0].clone().try_into().map_err(|e: String| e)?;
                let b: i32 = args[1].clone().try_into().map_err(|e: String| e)?;
                Ok(Some(Value::I32(a + b)))
            }),
        });

        assert!(reg.get_function("add").is_some());
        assert!(reg.get_function("sub").is_none());
        assert_eq!(reg.function_names().collect::<Vec<_>>(), vec!["add"]);
    }
}
