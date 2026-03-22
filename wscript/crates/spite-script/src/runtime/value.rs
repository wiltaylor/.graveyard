use std::any::Any;
use std::fmt;
use std::sync::Arc;

/// Universal value type for host <-> script data exchange.
#[derive(Debug, Clone)]
pub enum Value {
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    I128(i128),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    F32(f32),
    F64(f64),
    Bool(bool),
    Char(char),
    String(String),
    Array(Vec<Value>),
    Map(Vec<(Value, Value)>),
    Tuple(Vec<Value>),
    Option(Option<Box<Value>>),
    Result(std::result::Result<Box<Value>, Box<Value>>),
    Struct {
        type_name: String,
        fields: Vec<(String, Value)>,
    },
    HostObject(HostObjectHandle),
    Unit,
}

/// Opaque handle to a host-side object.
#[derive(Clone)]
pub struct HostObjectHandle {
    pub type_name: String,
    pub inner: Arc<dyn Any + Send + Sync>,
}

impl fmt::Debug for HostObjectHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostObjectHandle")
            .field("type_name", &self.type_name)
            .field("inner", &format_args!("Arc<dyn Any>({:p})", Arc::as_ptr(&self.inner)))
            .finish()
    }
}

impl fmt::Display for HostObjectHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<host:{}>", self.type_name)
    }
}

impl HostObjectHandle {
    /// Create a new host object handle wrapping a value of type `T`.
    pub fn new<T: Any + Send + Sync>(type_name: impl Into<String>, value: T) -> Self {
        Self {
            type_name: type_name.into(),
            inner: Arc::new(value),
        }
    }

    /// Try to downcast the inner value to a concrete type.
    pub fn downcast_ref<T: Any + Send + Sync>(&self) -> Option<&T> {
        self.inner.downcast_ref::<T>()
    }
}

// ---------------------------------------------------------------------------
// PartialEq
// ---------------------------------------------------------------------------

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::I8(a), Value::I8(b)) => a == b,
            (Value::I16(a), Value::I16(b)) => a == b,
            (Value::I32(a), Value::I32(b)) => a == b,
            (Value::I64(a), Value::I64(b)) => a == b,
            (Value::I128(a), Value::I128(b)) => a == b,
            (Value::U8(a), Value::U8(b)) => a == b,
            (Value::U16(a), Value::U16(b)) => a == b,
            (Value::U32(a), Value::U32(b)) => a == b,
            (Value::U64(a), Value::U64(b)) => a == b,
            (Value::U128(a), Value::U128(b)) => a == b,
            (Value::F32(a), Value::F32(b)) => a.to_bits() == b.to_bits(),
            (Value::F64(a), Value::F64(b)) => a.to_bits() == b.to_bits(),
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Char(a), Value::Char(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Array(a), Value::Array(b)) => a == b,
            (Value::Map(a), Value::Map(b)) => a == b,
            (Value::Tuple(a), Value::Tuple(b)) => a == b,
            (Value::Option(a), Value::Option(b)) => a == b,
            (Value::Result(a), Value::Result(b)) => a == b,
            (
                Value::Struct { type_name: tn_a, fields: f_a },
                Value::Struct { type_name: tn_b, fields: f_b },
            ) => tn_a == tn_b && f_a == f_b,
            (Value::HostObject(a), Value::HostObject(b)) => Arc::ptr_eq(&a.inner, &b.inner),
            (Value::Unit, Value::Unit) => true,
            _ => false,
        }
    }
}

// ---------------------------------------------------------------------------
// Display
// ---------------------------------------------------------------------------

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::I8(v) => write!(f, "{v}i8"),
            Value::I16(v) => write!(f, "{v}i16"),
            Value::I32(v) => write!(f, "{v}"),
            Value::I64(v) => write!(f, "{v}i64"),
            Value::I128(v) => write!(f, "{v}i128"),
            Value::U8(v) => write!(f, "{v}u8"),
            Value::U16(v) => write!(f, "{v}u16"),
            Value::U32(v) => write!(f, "{v}u32"),
            Value::U64(v) => write!(f, "{v}u64"),
            Value::U128(v) => write!(f, "{v}u128"),
            Value::F32(v) => write!(f, "{v}f32"),
            Value::F64(v) => write!(f, "{v}"),
            Value::Bool(v) => write!(f, "{v}"),
            Value::Char(v) => write!(f, "'{v}'"),
            Value::String(v) => write!(f, "\"{v}\""),
            Value::Array(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, "]")
            }
            Value::Map(entries) => {
                write!(f, "{{")?;
                for (i, (k, v)) in entries.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, "}}")
            }
            Value::Tuple(items) => {
                write!(f, "(")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                if items.len() == 1 {
                    write!(f, ",")?;
                }
                write!(f, ")")
            }
            Value::Option(opt) => match opt {
                Some(v) => write!(f, "Some({v})"),
                None => write!(f, "None"),
            },
            Value::Result(res) => match res {
                Ok(v) => write!(f, "Ok({v})"),
                Err(e) => write!(f, "Err({e})"),
            },
            Value::Struct { type_name, fields } => {
                write!(f, "{type_name} {{ ")?;
                for (i, (name, val)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{name}: {val}")?;
                }
                write!(f, " }}")
            }
            Value::HostObject(h) => write!(f, "{h}"),
            Value::Unit => write!(f, "()"),
        }
    }
}

// ---------------------------------------------------------------------------
// Value::type_name
// ---------------------------------------------------------------------------

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::I8(_) => "i8",
            Value::I16(_) => "i16",
            Value::I32(_) => "i32",
            Value::I64(_) => "i64",
            Value::I128(_) => "i128",
            Value::U8(_) => "u8",
            Value::U16(_) => "u16",
            Value::U32(_) => "u32",
            Value::U64(_) => "u64",
            Value::U128(_) => "u128",
            Value::F32(_) => "f32",
            Value::F64(_) => "f64",
            Value::Bool(_) => "bool",
            Value::Char(_) => "char",
            Value::String(_) => "String",
            Value::Array(_) => "Array",
            Value::Map(_) => "Map",
            Value::Tuple(_) => "Tuple",
            Value::Option(_) => "Option",
            Value::Result(_) => "Result",
            Value::Struct { .. } => "Struct",
            Value::HostObject(_) => "HostObject",
            Value::Unit => "()",
        }
    }

    /// Convert a runtime `Value` into a `DebugValue` for debugger display.
    pub fn to_debug_value(&self) -> DebugValue {
        match self {
            Value::I8(v) => DebugValue::I32(*v as i32),
            Value::I16(v) => DebugValue::I32(*v as i32),
            Value::I32(v) => DebugValue::I32(*v),
            Value::I64(v) => DebugValue::I64(*v),
            Value::I128(v) => DebugValue::String(v.to_string()),
            Value::U8(v) => DebugValue::I32(*v as i32),
            Value::U16(v) => DebugValue::I32(*v as i32),
            Value::U32(v) => DebugValue::I64(*v as i64),
            Value::U64(v) => DebugValue::I64(*v as i64),
            Value::U128(v) => DebugValue::String(v.to_string()),
            Value::F32(v) => DebugValue::F64(*v as f64),
            Value::F64(v) => DebugValue::F64(*v),
            Value::Bool(v) => DebugValue::Bool(*v),
            Value::Char(v) => DebugValue::String(v.to_string()),
            Value::String(v) => DebugValue::String(v.clone()),
            Value::Array(items) => {
                DebugValue::Array(items.iter().map(|i| i.to_debug_value()).collect())
            }
            Value::Map(entries) => DebugValue::Map(
                entries
                    .iter()
                    .map(|(k, v)| (k.to_debug_value(), v.to_debug_value()))
                    .collect(),
            ),
            Value::Tuple(items) => {
                DebugValue::Array(items.iter().map(|i| i.to_debug_value()).collect())
            }
            Value::Option(opt) => match opt {
                Some(v) => v.to_debug_value(),
                None => DebugValue::Null,
            },
            Value::Result(res) => match res {
                Ok(v) => v.to_debug_value(),
                Err(e) => DebugValue::String(format!("Err({})", e)),
            },
            Value::Struct { type_name, fields } => DebugValue::Struct {
                type_name: type_name.clone(),
                fields: fields
                    .iter()
                    .map(|(n, v)| (n.clone(), v.to_debug_value()))
                    .collect(),
            },
            Value::HostObject(h) => DebugValue::HostObject {
                display: h.to_string(),
                children: Vec::new(),
            },
            Value::Unit => DebugValue::Null,
        }
    }
}

// ---------------------------------------------------------------------------
// From<T> for Value
// ---------------------------------------------------------------------------

macro_rules! impl_from_value {
    ($($variant:ident($ty:ty)),* $(,)?) => {
        $(
            impl From<$ty> for Value {
                fn from(v: $ty) -> Self {
                    Value::$variant(v)
                }
            }
        )*
    };
}

impl_from_value! {
    I8(i8), I16(i16), I32(i32), I64(i64), I128(i128),
    U8(u8), U16(u16), U32(u32), U64(u64), U128(u128),
    F32(f32), F64(f64),
    Bool(bool),
    Char(char),
    String(String),
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::String(v.to_owned())
    }
}

impl From<Vec<Value>> for Value {
    fn from(v: Vec<Value>) -> Self {
        Value::Array(v)
    }
}

impl From<()> for Value {
    fn from(_: ()) -> Self {
        Value::Unit
    }
}

impl From<Option<Value>> for Value {
    fn from(v: Option<Value>) -> Self {
        Value::Option(v.map(Box::new))
    }
}

impl From<std::result::Result<Value, Value>> for Value {
    fn from(v: std::result::Result<Value, Value>) -> Self {
        Value::Result(v.map(Box::new).map_err(Box::new))
    }
}

// ---------------------------------------------------------------------------
// TryFrom<Value> for T
// ---------------------------------------------------------------------------

macro_rules! impl_try_from_value {
    ($($variant:ident => $ty:ty),* $(,)?) => {
        $(
            impl TryFrom<Value> for $ty {
                type Error = String;

                fn try_from(v: Value) -> std::result::Result<Self, Self::Error> {
                    match v {
                        Value::$variant(inner) => Ok(inner),
                        other => Err(format!(
                            "expected {}, got {}",
                            stringify!($ty),
                            other.type_name()
                        )),
                    }
                }
            }
        )*
    };
}

impl_try_from_value! {
    I32 => i32,
    I64 => i64,
    F64 => f64,
    Bool => bool,
    String => String,
}

impl TryFrom<Value> for Vec<Value> {
    type Error = String;

    fn try_from(v: Value) -> std::result::Result<Self, Self::Error> {
        match v {
            Value::Array(items) => Ok(items),
            other => Err(format!("expected Array, got {}", other.type_name())),
        }
    }
}

impl TryFrom<Value> for () {
    type Error = String;

    fn try_from(v: Value) -> std::result::Result<Self, Self::Error> {
        match v {
            Value::Unit => Ok(()),
            other => Err(format!("expected (), got {}", other.type_name())),
        }
    }
}

// ---------------------------------------------------------------------------
// DebugValue
// ---------------------------------------------------------------------------

/// Debug value representation for the debugger / DAP.
#[derive(Debug, Clone)]
pub enum DebugValue {
    I32(i32),
    I64(i64),
    F64(f64),
    Bool(bool),
    String(String),
    Array(Vec<DebugValue>),
    Map(Vec<(DebugValue, DebugValue)>),
    Struct {
        type_name: String,
        fields: Vec<(String, DebugValue)>,
    },
    HostObject {
        display: String,
        children: Vec<(String, DebugValue)>,
    },
    Null,
}

impl fmt::Display for DebugValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DebugValue::I32(v) => write!(f, "{v}"),
            DebugValue::I64(v) => write!(f, "{v}"),
            DebugValue::F64(v) => write!(f, "{v}"),
            DebugValue::Bool(v) => write!(f, "{v}"),
            DebugValue::String(v) => write!(f, "\"{v}\""),
            DebugValue::Array(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, "]")
            }
            DebugValue::Map(entries) => {
                write!(f, "{{")?;
                for (i, (k, v)) in entries.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, "}}")
            }
            DebugValue::Struct { type_name, fields } => {
                write!(f, "{type_name} {{ ")?;
                for (i, (name, val)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{name}: {val}")?;
                }
                write!(f, " }}")
            }
            DebugValue::HostObject { display, .. } => write!(f, "{display}"),
            DebugValue::Null => write!(f, "null"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_and_try_from_i32() {
        let v = Value::from(42i32);
        assert_eq!(v, Value::I32(42));
        let back: i32 = v.try_into().unwrap();
        assert_eq!(back, 42);
    }

    #[test]
    fn from_and_try_from_string() {
        let v = Value::from("hello");
        assert_eq!(v, Value::String("hello".into()));
        let back: String = v.try_into().unwrap();
        assert_eq!(back, "hello");
    }

    #[test]
    fn from_unit() {
        let v = Value::from(());
        assert_eq!(v, Value::Unit);
        let _: () = v.try_into().unwrap();
    }

    #[test]
    fn display_array() {
        let v = Value::Array(vec![Value::I32(1), Value::I32(2), Value::I32(3)]);
        assert_eq!(v.to_string(), "[1, 2, 3]");
    }

    #[test]
    fn host_object_equality_by_ptr() {
        let h1 = HostObjectHandle::new("Foo", 42u32);
        let h2 = h1.clone();
        assert_eq!(Value::HostObject(h1.clone()), Value::HostObject(h2));

        let h3 = HostObjectHandle::new("Foo", 42u32);
        assert_ne!(Value::HostObject(h1), Value::HostObject(h3));
    }

    #[test]
    fn type_name_coverage() {
        assert_eq!(Value::I32(0).type_name(), "i32");
        assert_eq!(Value::Bool(true).type_name(), "bool");
        assert_eq!(Value::Unit.type_name(), "()");
    }

    #[test]
    fn to_debug_value_basic() {
        let v = Value::I32(42);
        match v.to_debug_value() {
            DebugValue::I32(n) => assert_eq!(n, 42),
            other => panic!("expected DebugValue::I32, got {:?}", other),
        }
    }
}
