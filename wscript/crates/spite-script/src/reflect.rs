//! Reflection over script-defined struct types.
//!
//! Populated at the end of codegen from `StructLayout` entries and hung off
//! `CompiledScript`. Hosts walk `StructTypeInfo` to read or write script
//! struct instances in linear memory through the `Vm::read_struct_at` /
//! `Vm::write_struct_at` APIs.

use crate::bindings::ScriptType;
use crate::runtime::value::Value;

/// Description of a script struct type's memory layout.
#[derive(Debug, Clone)]
pub struct StructTypeInfo {
    pub name: String,
    pub size: u32,
    pub fields: Vec<FieldInfo>,
}

/// A single field of a struct type.
#[derive(Debug, Clone)]
pub struct FieldInfo {
    pub name: String,
    pub ty: FieldType,
    pub offset: u32,
}

/// A field's type at the reflection boundary. Mirrors the shrunk host
/// `ScriptType` plus `Struct(type_name)` for nested structs.
#[derive(Debug, Clone)]
pub enum FieldType {
    Primitive(ScriptType),
    Struct(String),
}

/// An owned view of a struct instance loaded from script linear memory.
#[derive(Debug, Clone)]
pub struct StructView {
    pub type_name: String,
    pub fields: Vec<(String, FieldValue)>,
}

#[derive(Debug, Clone)]
pub enum FieldValue {
    Primitive(Value),
    Nested(StructView),
}

impl StructView {
    pub fn get(&self, name: &str) -> Option<&FieldValue> {
        self.fields.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }
}

/// Collection of all struct layouts known to a compiled script.
#[derive(Debug, Default)]
pub struct TypeLayouts {
    pub structs: Vec<StructTypeInfo>,
}

impl TypeLayouts {
    pub fn get(&self, name: &str) -> Option<&StructTypeInfo> {
        self.structs.iter().find(|s| s.name == name)
    }
}
