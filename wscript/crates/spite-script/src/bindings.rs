use crate::runtime::value::{DebugValue, Value};
use indexmap::IndexMap;
use std::any::{Any, TypeId};
use std::fmt;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// ScriptType
// ---------------------------------------------------------------------------

/// Type information for a script type, used in host binding registration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptType {
    I8,
    I16,
    I32,
    I64,
    I128,
    U8,
    U16,
    U32,
    U64,
    U128,
    F32,
    F64,
    Bool,
    Char,
    String,
    Array(Box<ScriptType>),
    Map(Box<ScriptType>, Box<ScriptType>),
    Tuple(Vec<ScriptType>),
    Option(Box<ScriptType>),
    Result(Box<ScriptType>, Box<ScriptType>),
    Fn {
        params: Vec<ScriptType>,
        ret: Box<ScriptType>,
    },
    Named(String),
    Unit,
}

impl fmt::Display for ScriptType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScriptType::I8 => write!(f, "i8"),
            ScriptType::I16 => write!(f, "i16"),
            ScriptType::I32 => write!(f, "i32"),
            ScriptType::I64 => write!(f, "i64"),
            ScriptType::I128 => write!(f, "i128"),
            ScriptType::U8 => write!(f, "u8"),
            ScriptType::U16 => write!(f, "u16"),
            ScriptType::U32 => write!(f, "u32"),
            ScriptType::U64 => write!(f, "u64"),
            ScriptType::U128 => write!(f, "u128"),
            ScriptType::F32 => write!(f, "f32"),
            ScriptType::F64 => write!(f, "f64"),
            ScriptType::Bool => write!(f, "bool"),
            ScriptType::Char => write!(f, "char"),
            ScriptType::String => write!(f, "String"),
            ScriptType::Array(inner) => write!(f, "[{inner}]"),
            ScriptType::Map(k, v) => write!(f, "Map<{k}, {v}>"),
            ScriptType::Tuple(items) => {
                write!(f, "(")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, ")")
            }
            ScriptType::Option(inner) => write!(f, "Option<{inner}>"),
            ScriptType::Result(ok, err) => write!(f, "Result<{ok}, {err}>"),
            ScriptType::Fn { params, ret } => {
                write!(f, "fn(")?;
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{p}")?;
                }
                write!(f, ") -> {ret}")
            }
            ScriptType::Named(name) => write!(f, "{name}"),
            ScriptType::Unit => write!(f, "()"),
        }
    }
}

// ---------------------------------------------------------------------------
// ParamInfo
// ---------------------------------------------------------------------------

/// Metadata about a single parameter of a host function.
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

/// A host function registered for use from scripts.
pub struct HostFnBinding {
    pub name: String,
    pub params: Vec<ParamInfo>,
    pub return_type: ScriptType,
    pub doc: Option<String>,
    pub param_docs: Vec<(String, String)>,
    pub return_doc: Option<String>,
    pub examples: Vec<String>,
    pub closure: Arc<dyn Fn(&[Value]) -> Result<Value, String> + Send + Sync>,
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
// HostTypeBinding
// ---------------------------------------------------------------------------

/// A host type registered for use from scripts.
pub struct HostTypeBinding {
    pub name: String,
    pub rust_type_id: TypeId,
    pub doc: Option<String>,
    pub methods: IndexMap<String, HostFnBinding>,
    pub debug_display: Option<Arc<dyn Fn(&dyn Any) -> String + Send + Sync>>,
    pub debug_children: Option<Arc<dyn Fn(&dyn Any) -> Vec<(String, DebugValue)> + Send + Sync>>,
}

impl fmt::Debug for HostTypeBinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostTypeBinding")
            .field("name", &self.name)
            .field("rust_type_id", &self.rust_type_id)
            .field("doc", &self.doc)
            .field("methods", &self.methods.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

// ---------------------------------------------------------------------------
// GlobalBinding
// ---------------------------------------------------------------------------

/// A global constant registered for use from scripts.
#[derive(Debug)]
pub struct GlobalBinding {
    pub name: String,
    pub value: Value,
    pub ty: ScriptType,
}

// ---------------------------------------------------------------------------
// BindingRegistry
// ---------------------------------------------------------------------------

/// Registry of all host-registered functions, types, and globals.
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
            .field("types", &self.types.keys().collect::<Vec<_>>())
            .field("globals", &self.globals.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl BindingRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up a host function by name.
    pub fn get_function(&self, name: &str) -> Option<&HostFnBinding> {
        self.functions.get(name)
    }

    /// Look up a host type by name.
    pub fn get_type(&self, name: &str) -> Option<&HostTypeBinding> {
        self.types.get(name)
    }

    /// Look up a global by name.
    pub fn get_global(&self, name: &str) -> Option<&GlobalBinding> {
        self.globals.get(name)
    }

    /// Register a host function.
    pub fn register_function(&mut self, binding: HostFnBinding) {
        self.functions.insert(binding.name.clone(), binding);
    }

    /// Register a host type.
    pub fn register_type(&mut self, binding: HostTypeBinding) {
        self.types.insert(binding.name.clone(), binding);
    }

    /// Register a global.
    pub fn register_global(&mut self, binding: GlobalBinding) {
        self.globals.insert(binding.name.clone(), binding);
    }

    /// Return an iterator over all registered function names.
    pub fn function_names(&self) -> impl Iterator<Item = &str> {
        self.functions.keys().map(|s| s.as_str())
    }

    /// Return an iterator over all registered type names.
    pub fn type_names(&self) -> impl Iterator<Item = &str> {
        self.types.keys().map(|s| s.as_str())
    }

    /// Return an iterator over all registered global names.
    pub fn global_names(&self) -> impl Iterator<Item = &str> {
        self.globals.keys().map(|s| s.as_str())
    }
}

// ---------------------------------------------------------------------------
// IntoScriptType
// ---------------------------------------------------------------------------

/// Trait for mapping Rust types to [`ScriptType`].
pub trait IntoScriptType {
    fn script_type() -> ScriptType;
}

macro_rules! impl_into_script_type {
    ($($ty:ty => $variant:ident),* $(,)?) => {
        $(
            impl IntoScriptType for $ty {
                fn script_type() -> ScriptType {
                    ScriptType::$variant
                }
            }
        )*
    };
}

impl_into_script_type! {
    i8   => I8,
    i16  => I16,
    i32  => I32,
    i64  => I64,
    i128 => I128,
    u8   => U8,
    u16  => U16,
    u32  => U32,
    u64  => U64,
    u128 => U128,
    f32  => F32,
    f64  => F64,
    bool => Bool,
    char => Char,
}

impl IntoScriptType for String {
    fn script_type() -> ScriptType {
        ScriptType::String
    }
}

impl IntoScriptType for &str {
    fn script_type() -> ScriptType {
        ScriptType::String
    }
}

impl IntoScriptType for () {
    fn script_type() -> ScriptType {
        ScriptType::Unit
    }
}

impl<T: IntoScriptType> IntoScriptType for Vec<T> {
    fn script_type() -> ScriptType {
        ScriptType::Array(Box::new(T::script_type()))
    }
}

impl<T: IntoScriptType> IntoScriptType for Option<T> {
    fn script_type() -> ScriptType {
        ScriptType::Option(Box::new(T::script_type()))
    }
}

impl<T: IntoScriptType, E: IntoScriptType> IntoScriptType for Result<T, E> {
    fn script_type() -> ScriptType {
        ScriptType::Result(Box::new(T::script_type()), Box::new(E::script_type()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_type_display() {
        assert_eq!(ScriptType::I32.to_string(), "i32");
        assert_eq!(
            ScriptType::Array(Box::new(ScriptType::String)).to_string(),
            "[String]"
        );
        assert_eq!(
            ScriptType::Result(Box::new(ScriptType::I32), Box::new(ScriptType::String)).to_string(),
            "Result<i32, String>"
        );
    }

    #[test]
    fn into_script_type_primitives() {
        assert_eq!(i32::script_type(), ScriptType::I32);
        assert_eq!(bool::script_type(), ScriptType::Bool);
        assert_eq!(String::script_type(), ScriptType::String);
        assert_eq!(<()>::script_type(), ScriptType::Unit);
    }

    #[test]
    fn into_script_type_generic() {
        assert_eq!(
            Vec::<i32>::script_type(),
            ScriptType::Array(Box::new(ScriptType::I32))
        );
        assert_eq!(
            Option::<String>::script_type(),
            ScriptType::Option(Box::new(ScriptType::String))
        );
    }

    #[test]
    fn registry_basics() {
        let mut reg = BindingRegistry::new();
        reg.register_function(HostFnBinding {
            name: "add".into(),
            params: vec![
                ParamInfo { name: "a".into(), ty: ScriptType::I32 },
                ParamInfo { name: "b".into(), ty: ScriptType::I32 },
            ],
            return_type: ScriptType::I32,
            doc: Some("Add two integers.".into()),
            param_docs: vec![],
            return_doc: None,
            examples: vec![],
            closure: Arc::new(|args| {
                let a: i32 = args[0].clone().try_into().map_err(|e: String| e)?;
                let b: i32 = args[1].clone().try_into().map_err(|e: String| e)?;
                Ok(Value::I32(a + b))
            }),
        });

        assert!(reg.get_function("add").is_some());
        assert!(reg.get_function("sub").is_none());
        assert_eq!(reg.function_names().collect::<Vec<_>>(), vec!["add"]);
    }
}
