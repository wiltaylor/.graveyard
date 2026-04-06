use std::fmt;

/// Host <-> script value at the embedding boundary.
///
/// This is intentionally minimal: primitives and owned strings only. Script-level
/// types (arrays, maps, structs, enums) remain first-class inside the language,
/// but they do not cross the host boundary directly — hosts observe them via
/// reflection (see `reflect::StructView`) or dedicated accessor functions.
#[derive(Debug, Clone)]
pub enum Value {
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    Bool(bool),
    Str(String),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::I32(a), Value::I32(b)) => a == b,
            (Value::I64(a), Value::I64(b)) => a == b,
            (Value::F32(a), Value::F32(b)) => a.to_bits() == b.to_bits(),
            (Value::F64(a), Value::F64(b)) => a.to_bits() == b.to_bits(),
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            _ => false,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::I32(v) => write!(f, "{v}"),
            Value::I64(v) => write!(f, "{v}i64"),
            Value::F32(v) => write!(f, "{v}f32"),
            Value::F64(v) => write!(f, "{v}"),
            Value::Bool(v) => write!(f, "{v}"),
            Value::Str(v) => write!(f, "\"{v}\""),
        }
    }
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::I32(_) => "i32",
            Value::I64(_) => "i64",
            Value::F32(_) => "f32",
            Value::F64(_) => "f64",
            Value::Bool(_) => "bool",
            Value::Str(_) => "str",
        }
    }

    pub fn to_debug_value(&self) -> DebugValue {
        match self {
            Value::I32(v) => DebugValue::I32(*v),
            Value::I64(v) => DebugValue::I64(*v),
            Value::F32(v) => DebugValue::F64(*v as f64),
            Value::F64(v) => DebugValue::F64(*v),
            Value::Bool(v) => DebugValue::Bool(*v),
            Value::Str(v) => DebugValue::String(v.clone()),
        }
    }
}

impl From<i32> for Value { fn from(v: i32) -> Self { Value::I32(v) } }
impl From<i64> for Value { fn from(v: i64) -> Self { Value::I64(v) } }
impl From<f32> for Value { fn from(v: f32) -> Self { Value::F32(v) } }
impl From<f64> for Value { fn from(v: f64) -> Self { Value::F64(v) } }
impl From<bool> for Value { fn from(v: bool) -> Self { Value::Bool(v) } }
impl From<String> for Value { fn from(v: String) -> Self { Value::Str(v) } }
impl From<&str> for Value { fn from(v: &str) -> Self { Value::Str(v.to_owned()) } }

impl TryFrom<Value> for i32 {
    type Error = String;
    fn try_from(v: Value) -> Result<Self, Self::Error> {
        match v { Value::I32(x) => Ok(x), other => Err(format!("expected i32, got {}", other.type_name())) }
    }
}
impl TryFrom<Value> for i64 {
    type Error = String;
    fn try_from(v: Value) -> Result<Self, Self::Error> {
        match v { Value::I64(x) => Ok(x), other => Err(format!("expected i64, got {}", other.type_name())) }
    }
}
impl TryFrom<Value> for f32 {
    type Error = String;
    fn try_from(v: Value) -> Result<Self, Self::Error> {
        match v { Value::F32(x) => Ok(x), other => Err(format!("expected f32, got {}", other.type_name())) }
    }
}
impl TryFrom<Value> for f64 {
    type Error = String;
    fn try_from(v: Value) -> Result<Self, Self::Error> {
        match v { Value::F64(x) => Ok(x), other => Err(format!("expected f64, got {}", other.type_name())) }
    }
}
impl TryFrom<Value> for bool {
    type Error = String;
    fn try_from(v: Value) -> Result<Self, Self::Error> {
        match v { Value::Bool(x) => Ok(x), other => Err(format!("expected bool, got {}", other.type_name())) }
    }
}
impl TryFrom<Value> for String {
    type Error = String;
    fn try_from(v: Value) -> Result<Self, Self::Error> {
        match v { Value::Str(x) => Ok(x), other => Err(format!("expected str, got {}", other.type_name())) }
    }
}

// ---------------------------------------------------------------------------
// DebugValue (retained for DAP / debugger use)
// ---------------------------------------------------------------------------

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
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{item}")?;
                }
                write!(f, "]")
            }
            DebugValue::Map(entries) => {
                write!(f, "{{")?;
                for (i, (k, v)) in entries.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, "}}")
            }
            DebugValue::Struct { type_name, fields } => {
                write!(f, "{type_name} {{ ")?;
                for (i, (name, val)) in fields.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{name}: {val}")?;
                }
                write!(f, " }}")
            }
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
        assert_eq!(v, Value::Str("hello".into()));
        let back: String = v.try_into().unwrap();
        assert_eq!(back, "hello");
    }

    #[test]
    fn type_name_coverage() {
        assert_eq!(Value::I32(0).type_name(), "i32");
        assert_eq!(Value::Bool(true).type_name(), "bool");
        assert_eq!(Value::Str("s".into()).type_name(), "str");
    }
}
