//! Type checking for SpiteScript ASTs.
//!
//! Implements bidirectional type inference over the AST produced by the parser.
//! Types are stored in a side-table (`TypeMap`) keyed by span start offset so
//! that the original AST remains untouched.

use std::collections::HashMap;
use std::fmt;

use smol_str::SmolStr;

use crate::bindings::{BindingRegistry, ScriptType};
use super::ast::*;
use super::token::Span;

// ---------------------------------------------------------------------------
// Type representation
// ---------------------------------------------------------------------------

/// A resolved type in the SpiteScript type system.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    Primitive(PrimitiveType),
    String,
    Array(Box<Type>),
    Map(Box<Type>, Box<Type>),
    Tuple(Vec<Type>),
    Option(Box<Type>),
    Result(Box<Type>, Box<Type>),
    Fn {
        params: Vec<Type>,
        ret: Box<Type>,
    },
    Ref(Box<Type>),
    Struct(usize),
    Enum(usize),
    /// Propagated error type — poisoned, suppresses further diagnostics.
    Error,
    Unit,
    /// Not yet determined.
    Unknown,
    /// Inference variable (union-find key).
    TypeVar(u32),
}

impl Type {
    /// Returns `true` when the type is numeric (integer or float).
    pub fn is_numeric(&self) -> bool {
        matches!(self, Type::Primitive(p) if p.is_numeric())
    }

    pub fn is_integer(&self) -> bool {
        matches!(self, Type::Primitive(p) if p.is_integer())
    }

    pub fn is_float(&self) -> bool {
        matches!(self, Type::Primitive(p) if p.is_float())
    }

    /// Returns `true` for the error/unknown sentinels.
    pub fn is_error_or_unknown(&self) -> bool {
        matches!(self, Type::Error | Type::Unknown | Type::TypeVar(_))
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Primitive(p) => write!(f, "{p}"),
            Type::String => write!(f, "String"),
            Type::Array(inner) => write!(f, "[{inner}]"),
            Type::Map(k, v) => write!(f, "Map<{k}, {v}>"),
            Type::Tuple(items) => {
                write!(f, "(")?;
                for (i, t) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{t}")?;
                }
                write!(f, ")")
            }
            Type::Option(inner) => write!(f, "Option<{inner}>"),
            Type::Result(ok, err) => write!(f, "Result<{ok}, {err}>"),
            Type::Fn { params, ret } => {
                write!(f, "fn(")?;
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{p}")?;
                }
                write!(f, ") -> {ret}")
            }
            Type::Ref(inner) => write!(f, "&{inner}"),
            Type::Struct(id) => write!(f, "Struct#{id}"),
            Type::Enum(id) => write!(f, "Enum#{id}"),
            Type::Error => write!(f, "<error>"),
            Type::Unit => write!(f, "()"),
            Type::Unknown => write!(f, "<unknown>"),
            Type::TypeVar(id) => write!(f, "?T{id}"),
        }
    }
}

impl fmt::Display for PrimitiveType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            PrimitiveType::I8 => "i8",
            PrimitiveType::I16 => "i16",
            PrimitiveType::I32 => "i32",
            PrimitiveType::I64 => "i64",
            PrimitiveType::I128 => "i128",
            PrimitiveType::U8 => "u8",
            PrimitiveType::U16 => "u16",
            PrimitiveType::U32 => "u32",
            PrimitiveType::U64 => "u64",
            PrimitiveType::U128 => "u128",
            PrimitiveType::F32 => "f32",
            PrimitiveType::F64 => "f64",
            PrimitiveType::Bool => "bool",
            PrimitiveType::Char => "char",
        };
        write!(f, "{s}")
    }
}

// Helper methods on PrimitiveType that are used by the checker.
impl PrimitiveType {
    fn is_numeric(self) -> bool {
        self.is_integer() || self.is_float()
    }

    fn is_integer(self) -> bool {
        matches!(
            self,
            PrimitiveType::I8
                | PrimitiveType::I16
                | PrimitiveType::I32
                | PrimitiveType::I64
                | PrimitiveType::I128
                | PrimitiveType::U8
                | PrimitiveType::U16
                | PrimitiveType::U32
                | PrimitiveType::U64
                | PrimitiveType::U128
        )
    }

    fn is_float(self) -> bool {
        matches!(self, PrimitiveType::F32 | PrimitiveType::F64)
    }
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

/// A type-checking diagnostic.
#[derive(Debug, Clone)]
pub struct TypeDiagnostic {
    pub span: Span,
    pub code: &'static str,
    pub message: String,
}

impl fmt::Display for TypeDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {} (line {}, col {})",
            self.code, self.message, self.span.line, self.span.col
        )
    }
}

// ---------------------------------------------------------------------------
// Type map / TypeInfo
// ---------------------------------------------------------------------------

/// Maps AST node span-start offsets to their resolved types.
pub type TypeMap = HashMap<u32, Type>;

/// Result of type checking: type annotations + struct/enum definitions.
#[derive(Debug, Clone)]
pub struct TypeInfo {
    /// Mapping from span start offset to resolved type.
    pub types: TypeMap,
    /// Struct definitions collected during checking.
    pub structs: Vec<StructDef>,
    /// Enum definitions collected during checking.
    pub enums: Vec<EnumDef>,
}

#[derive(Debug, Clone)]
pub struct StructDef {
    pub name: SmolStr,
    pub fields: Vec<(SmolStr, Type)>,
}

#[derive(Debug, Clone)]
pub struct EnumDef {
    pub name: SmolStr,
    pub variants: Vec<EnumVariantDef>,
}

#[derive(Debug, Clone)]
pub struct EnumVariantDef {
    pub name: SmolStr,
    pub fields: Vec<Type>,
}

// ---------------------------------------------------------------------------
// Scope / symbol table
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Binding {
    ty: Type,
    mutable: bool,
}

#[derive(Debug, Clone, Default)]
struct Scope {
    bindings: HashMap<SmolStr, Binding>,
}

// ---------------------------------------------------------------------------
// Union-find for type variables
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct UnionFind {
    parent: Vec<u32>,
    rank: Vec<u8>,
    types: Vec<Option<Type>>,
}

impl UnionFind {
    fn new() -> Self {
        Self {
            parent: Vec::new(),
            rank: Vec::new(),
            types: Vec::new(),
        }
    }

    fn fresh(&mut self) -> u32 {
        let id = self.parent.len() as u32;
        self.parent.push(id);
        self.rank.push(0);
        self.types.push(None);
        id
    }

    fn find(&mut self, mut x: u32) -> u32 {
        while self.parent[x as usize] != x {
            let p = self.parent[x as usize];
            self.parent[x as usize] = self.parent[p as usize]; // path compression
            x = self.parent[x as usize];
        }
        x
    }

    fn union(&mut self, a: u32, b: u32) {
        let a = self.find(a);
        let b = self.find(b);
        if a == b {
            return;
        }
        if self.rank[a as usize] < self.rank[b as usize] {
            self.parent[a as usize] = b;
            // merge types
            if self.types[b as usize].is_none() {
                self.types[b as usize] = self.types[a as usize].take();
            }
        } else {
            self.parent[b as usize] = a;
            if self.types[a as usize].is_none() {
                self.types[a as usize] = self.types[b as usize].take();
            }
            if self.rank[a as usize] == self.rank[b as usize] {
                self.rank[a as usize] += 1;
            }
        }
    }

    fn set(&mut self, var: u32, ty: Type) {
        let root = self.find(var);
        self.types[root as usize] = Some(ty);
    }

    fn probe(&mut self, var: u32) -> Option<Type> {
        let root = self.find(var);
        self.types[root as usize].clone()
    }
}

// ---------------------------------------------------------------------------
// Method signatures for stdlib types
// ---------------------------------------------------------------------------

struct MethodSig {
    params: Vec<Type>,
    ret: Type,
}

fn string_methods(name: &str) -> Option<MethodSig> {
    match name {
        "len" => Some(MethodSig { params: vec![], ret: Type::Primitive(PrimitiveType::I32) }),
        "is_empty" => Some(MethodSig { params: vec![], ret: Type::Primitive(PrimitiveType::Bool) }),
        "contains" => Some(MethodSig { params: vec![Type::String], ret: Type::Primitive(PrimitiveType::Bool) }),
        "starts_with" => Some(MethodSig { params: vec![Type::String], ret: Type::Primitive(PrimitiveType::Bool) }),
        "ends_with" => Some(MethodSig { params: vec![Type::String], ret: Type::Primitive(PrimitiveType::Bool) }),
        "trim" => Some(MethodSig { params: vec![], ret: Type::String }),
        "trim_start" => Some(MethodSig { params: vec![], ret: Type::String }),
        "trim_end" => Some(MethodSig { params: vec![], ret: Type::String }),
        "to_uppercase" => Some(MethodSig { params: vec![], ret: Type::String }),
        "to_lowercase" => Some(MethodSig { params: vec![], ret: Type::String }),
        "split" => Some(MethodSig { params: vec![Type::String], ret: Type::Array(Box::new(Type::String)) }),
        "replace" => Some(MethodSig { params: vec![Type::String, Type::String], ret: Type::String }),
        "chars" => Some(MethodSig { params: vec![], ret: Type::Array(Box::new(Type::Primitive(PrimitiveType::Char))) }),
        "repeat" => Some(MethodSig { params: vec![Type::Primitive(PrimitiveType::I64)], ret: Type::String }),
        _ => None,
    }
}

fn array_methods(elem: &Type, name: &str) -> Option<MethodSig> {
    match name {
        "len" => Some(MethodSig { params: vec![], ret: Type::Primitive(PrimitiveType::I32) }),
        "is_empty" => Some(MethodSig { params: vec![], ret: Type::Primitive(PrimitiveType::Bool) }),
        "push" => Some(MethodSig { params: vec![elem.clone()], ret: Type::Unit }),
        "pop" => Some(MethodSig { params: vec![], ret: Type::Option(Box::new(elem.clone())) }),
        "first" => Some(MethodSig { params: vec![], ret: Type::Option(Box::new(elem.clone())) }),
        "last" => Some(MethodSig { params: vec![], ret: Type::Option(Box::new(elem.clone())) }),
        "contains" => Some(MethodSig { params: vec![elem.clone()], ret: Type::Primitive(PrimitiveType::Bool) }),
        "reverse" => Some(MethodSig { params: vec![], ret: Type::Array(Box::new(elem.clone())) }),
        "filter" => {
            let cb = Type::Fn {
                params: vec![elem.clone()],
                ret: Box::new(Type::Primitive(PrimitiveType::Bool)),
            };
            Some(MethodSig {
                params: vec![cb],
                ret: Type::Array(Box::new(elem.clone())),
            })
        }
        "map" => {
            // map returns Array<Unknown> because we can't determine the output element type
            // without evaluating the callback. We use Unknown to indicate this.
            let cb = Type::Fn {
                params: vec![elem.clone()],
                ret: Box::new(Type::Unknown),
            };
            Some(MethodSig {
                params: vec![cb],
                ret: Type::Array(Box::new(Type::Unknown)),
            })
        }
        "sum" => {
            // Only valid for numeric arrays — we return the element type.
            Some(MethodSig { params: vec![], ret: elem.clone() })
        }
        "collect" => Some(MethodSig { params: vec![], ret: Type::Array(Box::new(elem.clone())) }),
        "enumerate" => Some(MethodSig {
            params: vec![],
            ret: Type::Array(Box::new(Type::Tuple(vec![
                Type::Primitive(PrimitiveType::I64),
                elem.clone(),
            ]))),
        }),
        "join" => Some(MethodSig { params: vec![Type::String], ret: Type::String }),
        "sort" => Some(MethodSig { params: vec![], ret: Type::Array(Box::new(elem.clone())) }),
        "min" => Some(MethodSig { params: vec![], ret: elem.clone() }),
        "max" => Some(MethodSig { params: vec![], ret: elem.clone() }),
        "count" => Some(MethodSig { params: vec![], ret: Type::Primitive(PrimitiveType::I32) }),
        "dedup" => Some(MethodSig { params: vec![], ret: Type::Array(Box::new(elem.clone())) }),
        "take" => Some(MethodSig { params: vec![Type::Primitive(PrimitiveType::I32)], ret: Type::Array(Box::new(elem.clone())) }),
        "skip" => Some(MethodSig { params: vec![Type::Primitive(PrimitiveType::I32)], ret: Type::Array(Box::new(elem.clone())) }),
        "any" | "all" | "none" => {
            let cb = Type::Fn { params: vec![elem.clone()], ret: Box::new(Type::Primitive(PrimitiveType::Bool)) };
            Some(MethodSig { params: vec![cb], ret: Type::Primitive(PrimitiveType::Bool) })
        }
        "find" => {
            let cb = Type::Fn { params: vec![elem.clone()], ret: Box::new(Type::Primitive(PrimitiveType::Bool)) };
            Some(MethodSig { params: vec![cb], ret: Type::Option(Box::new(elem.clone())) })
        }
        "for_each" => {
            let cb = Type::Fn { params: vec![elem.clone()], ret: Box::new(Type::Unit) };
            Some(MethodSig { params: vec![cb], ret: Type::Unit })
        }
        "fold" => Some(MethodSig { params: vec![elem.clone(), Type::Unknown], ret: elem.clone() }),
        "reduce" => {
            let cb = Type::Fn { params: vec![elem.clone(), elem.clone()], ret: Box::new(elem.clone()) };
            Some(MethodSig { params: vec![cb], ret: elem.clone() })
        }
        "get" => Some(MethodSig { params: vec![Type::Primitive(PrimitiveType::I64)], ret: Type::Option(Box::new(elem.clone())) }),
        "clone" => Some(MethodSig { params: vec![], ret: Type::Array(Box::new(elem.clone())) }),
        "extend" => Some(MethodSig { params: vec![Type::Array(Box::new(elem.clone()))], ret: Type::Unit }),
        _ => None,
    }
}

fn option_methods(inner: &Type, name: &str) -> Option<MethodSig> {
    match name {
        "is_some" => Some(MethodSig { params: vec![], ret: Type::Primitive(PrimitiveType::Bool) }),
        "is_none" => Some(MethodSig { params: vec![], ret: Type::Primitive(PrimitiveType::Bool) }),
        "unwrap" => Some(MethodSig { params: vec![], ret: inner.clone() }),
        "unwrap_or" => Some(MethodSig { params: vec![inner.clone()], ret: inner.clone() }),
        "map" => {
            let cb = Type::Fn {
                params: vec![inner.clone()],
                ret: Box::new(Type::Unknown),
            };
            Some(MethodSig {
                params: vec![cb],
                ret: Type::Option(Box::new(Type::Unknown)),
            })
        }
        _ => None,
    }
}

fn result_methods(ok: &Type, err: &Type, name: &str) -> Option<MethodSig> {
    match name {
        "is_ok" => Some(MethodSig { params: vec![], ret: Type::Primitive(PrimitiveType::Bool) }),
        "is_err" => Some(MethodSig { params: vec![], ret: Type::Primitive(PrimitiveType::Bool) }),
        "unwrap" => Some(MethodSig { params: vec![], ret: ok.clone() }),
        "unwrap_err" => Some(MethodSig { params: vec![], ret: err.clone() }),
        "map" => {
            let cb = Type::Fn {
                params: vec![ok.clone()],
                ret: Box::new(Type::Unknown),
            };
            Some(MethodSig {
                params: vec![cb],
                ret: Type::Result(Box::new(Type::Unknown), Box::new(err.clone())),
            })
        }
        "map_err" => {
            let cb = Type::Fn {
                params: vec![err.clone()],
                ret: Box::new(Type::Unknown),
            };
            Some(MethodSig {
                params: vec![cb],
                ret: Type::Result(Box::new(ok.clone()), Box::new(Type::Unknown)),
            })
        }
        _ => None,
    }
}

fn map_methods(key: &Type, val: &Type, name: &str) -> Option<MethodSig> {
    match name {
        "len" => Some(MethodSig { params: vec![], ret: Type::Primitive(PrimitiveType::I32) }),
        "is_empty" => Some(MethodSig { params: vec![], ret: Type::Primitive(PrimitiveType::Bool) }),
        "contains_key" => Some(MethodSig { params: vec![key.clone()], ret: Type::Primitive(PrimitiveType::Bool) }),
        "get" => Some(MethodSig { params: vec![key.clone()], ret: Type::Option(Box::new(val.clone())) }),
        "insert" => Some(MethodSig { params: vec![key.clone(), val.clone()], ret: Type::Option(Box::new(val.clone())) }),
        "remove" => Some(MethodSig { params: vec![key.clone()], ret: Type::Option(Box::new(val.clone())) }),
        "keys" => Some(MethodSig { params: vec![], ret: Type::Array(Box::new(key.clone())) }),
        "values" => Some(MethodSig { params: vec![], ret: Type::Array(Box::new(val.clone())) }),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// TypeEnv — the main type checking environment
// ---------------------------------------------------------------------------

/// The type checking environment.
pub struct TypeEnv<'a> {
    scopes: Vec<Scope>,
    /// User-defined struct definitions.
    structs: Vec<StructDef>,
    /// Mapping from struct name to its index.
    struct_names: HashMap<SmolStr, usize>,
    /// User-defined enum definitions.
    enums: Vec<EnumDef>,
    /// Mapping from enum name to its index.
    enum_names: HashMap<SmolStr, usize>,
    /// User-defined function signatures (name -> type).
    fn_sigs: HashMap<SmolStr, Type>,
    /// Union-find table for type variables.
    uf: UnionFind,
    /// Collected diagnostics.
    diagnostics: Vec<TypeDiagnostic>,
    /// Type map keyed by span start.
    type_map: TypeMap,
    /// Host bindings.
    bindings: &'a BindingRegistry,
    /// Current function return type (for `return` checking).
    current_return_type: Option<Type>,
}

impl<'a> TypeEnv<'a> {
    fn new(bindings: &'a BindingRegistry) -> Self {
        let mut env = Self {
            scopes: vec![Scope::default()],
            structs: Vec::new(),
            struct_names: HashMap::new(),
            enums: Vec::new(),
            enum_names: HashMap::new(),
            fn_sigs: HashMap::new(),
            uf: UnionFind::new(),
            diagnostics: Vec::new(),
            type_map: HashMap::new(),
            bindings,
            current_return_type: None,
        };
        env.register_builtins();
        env
    }

    // -- Scope management ---------------------------------------------------

    fn push_scope(&mut self) {
        self.scopes.push(Scope::default());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn define(&mut self, name: SmolStr, ty: Type, mutable: bool) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.bindings.insert(name, Binding { ty, mutable });
        }
    }

    fn lookup(&self, name: &str) -> Option<&Binding> {
        for scope in self.scopes.iter().rev() {
            if let Some(b) = scope.bindings.get(name) {
                return Some(b);
            }
        }
        None
    }

    // -- Diagnostics --------------------------------------------------------

    fn error(&mut self, span: Span, code: &'static str, message: String) {
        self.diagnostics.push(TypeDiagnostic {
            span,
            code,
            message,
        });
    }

    // -- Fresh type variable ------------------------------------------------

    fn fresh_var(&mut self) -> Type {
        Type::TypeVar(self.uf.fresh())
    }

    // -- Resolve a type (chase type-vars) -----------------------------------

    fn resolve(&mut self, ty: &Type) -> Type {
        match ty {
            Type::TypeVar(id) => {
                if let Some(resolved) = self.uf.probe(*id) {
                    let r = self.resolve(&resolved);
                    r
                } else {
                    ty.clone()
                }
            }
            Type::Array(inner) => Type::Array(Box::new(self.resolve(inner))),
            Type::Map(k, v) => Type::Map(Box::new(self.resolve(k)), Box::new(self.resolve(v))),
            Type::Tuple(items) => {
                Type::Tuple(items.iter().map(|t| self.resolve(t)).collect())
            }
            Type::Option(inner) => Type::Option(Box::new(self.resolve(inner))),
            Type::Result(ok, err) => Type::Result(
                Box::new(self.resolve(ok)),
                Box::new(self.resolve(err)),
            ),
            Type::Fn { params, ret } => Type::Fn {
                params: params.iter().map(|t| self.resolve(t)).collect(),
                ret: Box::new(self.resolve(ret)),
            },
            Type::Ref(inner) => Type::Ref(Box::new(self.resolve(inner))),
            _ => ty.clone(),
        }
    }

    // -- Unification --------------------------------------------------------

    fn unify(&mut self, a: &Type, b: &Type, span: Span) -> Type {
        let a = self.resolve(a);
        let b = self.resolve(b);

        if a == b {
            return a;
        }

        // Error/Unknown are compatible with anything.
        if a.is_error_or_unknown() {
            return b;
        }
        if b.is_error_or_unknown() {
            return a;
        }

        match (&a, &b) {
            (Type::TypeVar(va), Type::TypeVar(vb)) => {
                let va = *va;
                let vb = *vb;
                self.uf.union(va, vb);
                a
            }
            (Type::TypeVar(va), _) => {
                self.uf.set(*va, b.clone());
                b
            }
            (_, Type::TypeVar(vb)) => {
                self.uf.set(*vb, a.clone());
                a
            }
            (Type::Array(ia), Type::Array(ib)) => {
                let inner = self.unify(ia, ib, span);
                Type::Array(Box::new(inner))
            }
            (Type::Map(ka, va), Type::Map(kb, vb)) => {
                let k = self.unify(ka, kb, span);
                let v = self.unify(va, vb, span);
                Type::Map(Box::new(k), Box::new(v))
            }
            (Type::Tuple(as_), Type::Tuple(bs)) if as_.len() == bs.len() => {
                let items: Vec<_> = as_
                    .iter()
                    .zip(bs.iter())
                    .map(|(x, y)| self.unify(x, y, span))
                    .collect();
                Type::Tuple(items)
            }
            (Type::Option(ia), Type::Option(ib)) => {
                let inner = self.unify(ia, ib, span);
                Type::Option(Box::new(inner))
            }
            (Type::Result(oa, ea), Type::Result(ob, eb)) => {
                let ok = self.unify(oa, ob, span);
                let err = self.unify(ea, eb, span);
                Type::Result(Box::new(ok), Box::new(err))
            }
            (
                Type::Fn { params: pa, ret: ra },
                Type::Fn { params: pb, ret: rb },
            ) if pa.len() == pb.len() => {
                let params: Vec<_> = pa
                    .iter()
                    .zip(pb.iter())
                    .map(|(x, y)| self.unify(x, y, span))
                    .collect();
                let ret = self.unify(ra, rb, span);
                Type::Fn {
                    params,
                    ret: Box::new(ret),
                }
            }
            (Type::Ref(ia), Type::Ref(ib)) => {
                let inner = self.unify(ia, ib, span);
                Type::Ref(Box::new(inner))
            }
            _ => {
                self.error(span, "E001", format!("type mismatch: expected `{a}`, found `{b}`"));
                Type::Error
            }
        }
    }

    // -- Register built-in functions -----------------------------------------

    fn register_builtins(&mut self) {
        // print(value) -> ()
        self.fn_sigs.insert(
            SmolStr::new("print"),
            Type::Fn {
                params: vec![Type::Unknown], // accepts any
                ret: Box::new(Type::Unit),
            },
        );
        self.fn_sigs.insert(
            SmolStr::new("println"),
            Type::Fn {
                params: vec![Type::Unknown],
                ret: Box::new(Type::Unit),
            },
        );
        // Math
        self.fn_sigs.insert(
            SmolStr::new("abs"),
            Type::Fn {
                params: vec![Type::Primitive(PrimitiveType::F64)],
                ret: Box::new(Type::Primitive(PrimitiveType::F64)),
            },
        );
        self.fn_sigs.insert(
            SmolStr::new("sqrt"),
            Type::Fn {
                params: vec![Type::Primitive(PrimitiveType::F64)],
                ret: Box::new(Type::Primitive(PrimitiveType::F64)),
            },
        );
        self.fn_sigs.insert(
            SmolStr::new("pow"),
            Type::Fn {
                params: vec![
                    Type::Primitive(PrimitiveType::F64),
                    Type::Primitive(PrimitiveType::F64),
                ],
                ret: Box::new(Type::Primitive(PrimitiveType::F64)),
            },
        );
        self.fn_sigs.insert(
            SmolStr::new("min"),
            Type::Fn {
                params: vec![
                    Type::Primitive(PrimitiveType::F64),
                    Type::Primitive(PrimitiveType::F64),
                ],
                ret: Box::new(Type::Primitive(PrimitiveType::F64)),
            },
        );
        self.fn_sigs.insert(
            SmolStr::new("max"),
            Type::Fn {
                params: vec![
                    Type::Primitive(PrimitiveType::F64),
                    Type::Primitive(PrimitiveType::F64),
                ],
                ret: Box::new(Type::Primitive(PrimitiveType::F64)),
            },
        );
        // Some / Ok / Err constructors
        self.fn_sigs.insert(
            SmolStr::new("Some"),
            Type::Fn {
                params: vec![Type::Unknown],
                ret: Box::new(Type::Option(Box::new(Type::Unknown))),
            },
        );
        self.fn_sigs.insert(
            SmolStr::new("Ok"),
            Type::Fn {
                params: vec![Type::Unknown],
                ret: Box::new(Type::Result(
                    Box::new(Type::Unknown),
                    Box::new(Type::Unknown),
                )),
            },
        );
        self.fn_sigs.insert(
            SmolStr::new("Err"),
            Type::Fn {
                params: vec![Type::Unknown],
                ret: Box::new(Type::Result(
                    Box::new(Type::Unknown),
                    Box::new(Type::Unknown),
                )),
            },
        );
    }

    // -- Convert TypeExpr (AST) to Type -------------------------------------

    fn resolve_type_expr(&mut self, te: &TypeExpr) -> Type {
        match &te.kind {
            TypeExprKind::Primitive(p) => Type::Primitive(*p),
            TypeExprKind::StringType => Type::String,
            TypeExprKind::Array(inner) => {
                Type::Array(Box::new(self.resolve_type_expr(inner)))
            }
            TypeExprKind::Map(k, v) => Type::Map(
                Box::new(self.resolve_type_expr(k)),
                Box::new(self.resolve_type_expr(v)),
            ),
            TypeExprKind::OptionType(inner) => {
                Type::Option(Box::new(self.resolve_type_expr(inner)))
            }
            TypeExprKind::ResultType(ok, err) => {
                let ok_ty = self.resolve_type_expr(ok);
                let err_ty = err
                    .as_ref()
                    .map(|e| self.resolve_type_expr(e))
                    .unwrap_or(Type::String); // default error type
                Type::Result(Box::new(ok_ty), Box::new(err_ty))
            }
            TypeExprKind::FnType { params, ret } => {
                let ps: Vec<_> = params.iter().map(|p| self.resolve_type_expr(p)).collect();
                let r = self.resolve_type_expr(ret);
                Type::Fn {
                    params: ps,
                    ret: Box::new(r),
                }
            }
            TypeExprKind::RefType(inner) => {
                Type::Ref(Box::new(self.resolve_type_expr(inner)))
            }
            TypeExprKind::Tuple(items) => {
                Type::Tuple(items.iter().map(|t| self.resolve_type_expr(t)).collect())
            }
            TypeExprKind::Unit => Type::Unit,
            TypeExprKind::Named { name, .. } => {
                if let Some(&idx) = self.struct_names.get(name.as_str()) {
                    Type::Struct(idx)
                } else if let Some(&idx) = self.enum_names.get(name.as_str()) {
                    Type::Enum(idx)
                } else {
                    // May be a host type.
                    if self.bindings.get_type(name.as_str()).is_some() {
                        // Treat as an opaque named type — use Unknown for now.
                        Type::Unknown
                    } else {
                        self.error(te.span, "E001", format!("unknown type `{name}`"));
                        Type::Error
                    }
                }
            }
            TypeExprKind::Error => Type::Error,
        }
    }

    // -- Convert ScriptType (bindings) to Type ------------------------------

    fn from_script_type(st: &ScriptType) -> Type {
        match st {
            ScriptType::I8 => Type::Primitive(PrimitiveType::I8),
            ScriptType::I16 => Type::Primitive(PrimitiveType::I16),
            ScriptType::I32 => Type::Primitive(PrimitiveType::I32),
            ScriptType::I64 => Type::Primitive(PrimitiveType::I64),
            ScriptType::I128 => Type::Primitive(PrimitiveType::I128),
            ScriptType::U8 => Type::Primitive(PrimitiveType::U8),
            ScriptType::U16 => Type::Primitive(PrimitiveType::U16),
            ScriptType::U32 => Type::Primitive(PrimitiveType::U32),
            ScriptType::U64 => Type::Primitive(PrimitiveType::U64),
            ScriptType::U128 => Type::Primitive(PrimitiveType::U128),
            ScriptType::F32 => Type::Primitive(PrimitiveType::F32),
            ScriptType::F64 => Type::Primitive(PrimitiveType::F64),
            ScriptType::Bool => Type::Primitive(PrimitiveType::Bool),
            ScriptType::Char => Type::Primitive(PrimitiveType::Char),
            ScriptType::String => Type::String,
            ScriptType::Array(inner) => {
                Type::Array(Box::new(Self::from_script_type(inner)))
            }
            ScriptType::Map(k, v) => Type::Map(
                Box::new(Self::from_script_type(k)),
                Box::new(Self::from_script_type(v)),
            ),
            ScriptType::Tuple(items) => {
                Type::Tuple(items.iter().map(|t| Self::from_script_type(t)).collect())
            }
            ScriptType::Option(inner) => {
                Type::Option(Box::new(Self::from_script_type(inner)))
            }
            ScriptType::Result(ok, err) => Type::Result(
                Box::new(Self::from_script_type(ok)),
                Box::new(Self::from_script_type(err)),
            ),
            ScriptType::Fn { params, ret } => Type::Fn {
                params: params.iter().map(|p| Self::from_script_type(p)).collect(),
                ret: Box::new(Self::from_script_type(ret)),
            },
            ScriptType::Named(_) => Type::Unknown,
            ScriptType::Unit => Type::Unit,
        }
    }

    // -- Record type for a span ---------------------------------------------

    fn record(&mut self, span: Span, ty: &Type) {
        self.type_map.insert(span.start, ty.clone());
    }

    // -----------------------------------------------------------------------
    // Top-level checking
    // -----------------------------------------------------------------------

    fn check_program(&mut self, program: &Program) {
        // First pass: collect struct/enum/fn declarations.
        for item in &program.items {
            self.collect_item(item);
        }
        // Second pass: check bodies.
        for item in &program.items {
            self.check_item(item);
        }
    }

    fn collect_item(&mut self, item: &Item) {
        match item {
            Item::StructDecl(sd) => self.collect_struct(sd),
            Item::EnumDecl(ed) => self.collect_enum(ed),
            Item::FnDecl(fd) => self.collect_fn(fd),
            Item::ConstDecl(cd) => self.collect_const(cd),
            Item::TraitDecl(_) | Item::ImplBlock(_) | Item::Error(_) => {}
        }
    }

    fn collect_struct(&mut self, sd: &StructDecl) {
        let idx = self.structs.len();
        let fields: Vec<(SmolStr, Type)> = sd
            .fields
            .iter()
            .map(|f| (f.name.clone(), self.resolve_type_expr(&f.ty)))
            .collect();
        self.structs.push(StructDef {
            name: sd.name.clone(),
            fields,
        });
        self.struct_names.insert(sd.name.clone(), idx);
    }

    fn collect_enum(&mut self, ed: &EnumDecl) {
        let idx = self.enums.len();
        let variants: Vec<EnumVariantDef> = ed
            .variants
            .iter()
            .map(|v| {
                let fields = match &v.kind {
                    VariantKind::Unit => vec![],
                    VariantKind::Tuple(types) => {
                        types.iter().map(|t| self.resolve_type_expr(t)).collect()
                    }
                    VariantKind::Struct(fs) => {
                        fs.iter().map(|f| self.resolve_type_expr(&f.ty)).collect()
                    }
                };
                EnumVariantDef {
                    name: v.name.clone(),
                    fields,
                }
            })
            .collect();
        self.enums.push(EnumDef {
            name: ed.name.clone(),
            variants,
        });
        self.enum_names.insert(ed.name.clone(), idx);
    }

    fn collect_fn(&mut self, fd: &FnDecl) {
        let params: Vec<Type> = fd
            .params
            .iter()
            .filter_map(|p| match &p.kind {
                ParamKind::SelfRef { .. } => None,
                ParamKind::Named { ty, .. } => Some(self.resolve_type_expr(ty)),
            })
            .collect();
        let ret = fd
            .return_type
            .as_ref()
            .map(|t| self.resolve_type_expr(t))
            .unwrap_or(Type::Unit);
        let fn_ty = Type::Fn {
            params,
            ret: Box::new(ret),
        };
        self.fn_sigs.insert(fd.name.clone(), fn_ty.clone());
        self.define(fd.name.clone(), fn_ty, false);
    }

    fn collect_const(&mut self, cd: &ConstDecl) {
        let ty = cd
            .ty
            .as_ref()
            .map(|t| self.resolve_type_expr(t))
            .unwrap_or(Type::Unknown);
        self.define(cd.name.clone(), ty, false);
    }

    // -----------------------------------------------------------------------
    // Item checking (second pass)
    // -----------------------------------------------------------------------

    fn check_item(&mut self, item: &Item) {
        match item {
            Item::FnDecl(fd) => self.check_fn_decl(fd),
            Item::ConstDecl(cd) => self.check_const_decl(cd),
            Item::ImplBlock(ib) => self.check_impl_block(ib),
            Item::StructDecl(_) | Item::EnumDecl(_) | Item::TraitDecl(_) | Item::Error(_) => {}
        }
    }

    fn check_fn_decl(&mut self, fd: &FnDecl) {
        self.push_scope();

        // Bind parameters.
        for p in &fd.params {
            match &p.kind {
                ParamKind::SelfRef { .. } => {}
                ParamKind::Named { name, ty, .. } => {
                    let resolved = self.resolve_type_expr(ty);
                    self.define(name.clone(), resolved, false);
                }
            }
        }

        let ret_ty = fd
            .return_type
            .as_ref()
            .map(|t| self.resolve_type_expr(t))
            .unwrap_or(Type::Unit);
        self.current_return_type = Some(ret_ty.clone());

        let body_ty = self.check_block(&fd.body);
        self.unify(&body_ty, &ret_ty, fd.span);

        self.current_return_type = None;
        self.pop_scope();
    }

    fn check_const_decl(&mut self, cd: &ConstDecl) {
        let init_ty = self.infer_expr(&cd.value);
        if let Some(ty_expr) = &cd.ty {
            let ann = self.resolve_type_expr(ty_expr);
            self.unify(&init_ty, &ann, cd.span);
        }
        let resolved = self.resolve(&init_ty);
        self.define(cd.name.clone(), resolved, false);
    }

    fn check_impl_block(&mut self, ib: &ImplBlock) {
        // Resolve the self type for this impl block.
        let self_ty = self.resolve_type_expr(&ib.self_type);

        // Register methods in fn_sigs with mangled names.
        let type_name = match &ib.self_type.kind {
            TypeExprKind::Named { name, .. } => name.clone(),
            _ => SmolStr::new("Unknown"),
        };

        for method in &ib.methods {
            // Register as TypeName__method_name for method resolution.
            let mangled = SmolStr::from(format!("{}__{}",  type_name, method.name));
            let params: Vec<Type> = method.params.iter().filter_map(|p| match &p.kind {
                ParamKind::SelfRef { .. } => None,
                ParamKind::Named { ty, .. } => Some(self.resolve_type_expr(ty)),
            }).collect();
            let ret = method.return_type.as_ref()
                .map(|t| self.resolve_type_expr(t))
                .unwrap_or(Type::Unit);
            let fn_ty = Type::Fn { params, ret: Box::new(ret) };
            self.fn_sigs.insert(mangled, fn_ty);
        }

        for method in &ib.methods {
            self.push_scope();

            // Bind `self` in the method scope.
            self.define(SmolStr::new("self"), self_ty.clone(), false);

            // Bind named parameters.
            for p in &method.params {
                match &p.kind {
                    ParamKind::SelfRef { .. } => {} // already bound as `self`
                    ParamKind::Named { name, ty, .. } => {
                        let resolved = self.resolve_type_expr(ty);
                        self.define(name.clone(), resolved, false);
                    }
                }
            }

            let ret_ty = method.return_type.as_ref()
                .map(|t| self.resolve_type_expr(t))
                .unwrap_or(Type::Unit);
            self.current_return_type = Some(ret_ty.clone());

            let body_ty = self.check_block(&method.body);
            self.unify(&body_ty, &ret_ty, method.span);

            self.current_return_type = None;
            self.pop_scope();
        }
    }

    // -----------------------------------------------------------------------
    // Block / statement checking
    // -----------------------------------------------------------------------

    fn check_block(&mut self, block: &Block) -> Type {
        self.push_scope();
        let mut ty = Type::Unit;
        for (i, stmt) in block.stmts.iter().enumerate() {
            let is_last = i + 1 == block.stmts.len();
            match stmt {
                Stmt::Let(ls) => {
                    self.check_let(ls);
                }
                Stmt::Expr(es) => {
                    let expr_ty = self.infer_expr(&es.expr);
                    if is_last && !es.has_semicolon {
                        ty = expr_ty;
                    } else if is_last && matches!(&es.expr.kind, ExprKind::Return(_)) {
                        // If the last statement is a `return` expression (even with
                        // a semicolon), propagate the return type so that the block
                        // type matches the function's declared return type.
                        ty = expr_ty;
                    }
                }
                Stmt::Item(item) => {
                    self.collect_item(item);
                    self.check_item(item);
                }
                Stmt::Error(_) => {}
            }
        }
        self.pop_scope();
        ty
    }

    fn check_let(&mut self, ls: &LetStmt) {
        let init_ty = ls
            .init
            .as_ref()
            .map(|e| self.infer_expr(e))
            .unwrap_or_else(|| self.fresh_var());

        let ann_ty = ls
            .ty
            .as_ref()
            .map(|t| self.resolve_type_expr(t));

        let final_ty = if let Some(ann) = ann_ty {
            self.unify(&init_ty, &ann, ls.span)
        } else {
            init_ty
        };

        let resolved = self.resolve(&final_ty);
        self.bind_pattern(&ls.pattern, &resolved, ls.mutable);
    }

    fn bind_pattern(&mut self, pat: &Pattern, ty: &Type, mutable: bool) {
        match pat {
            Pattern::Ident { name, mutable: pat_mut, .. } => {
                self.define(name.clone(), ty.clone(), mutable || *pat_mut);
            }
            Pattern::Tuple { elements, .. } => {
                if let Type::Tuple(items) = ty {
                    for (p, t) in elements.iter().zip(items.iter()) {
                        self.bind_pattern(p, t, mutable);
                    }
                } else if !ty.is_error_or_unknown() {
                    // still bind names to Unknown
                    for p in elements {
                        self.bind_pattern(p, &Type::Unknown, mutable);
                    }
                }
            }
            Pattern::Wildcard(_) => {}
            Pattern::Struct { fields, .. } => {
                for (_fname, fpat) in fields {
                    self.bind_pattern(fpat, &Type::Unknown, mutable);
                }
            }
            Pattern::EnumVariant { fields, .. } => {
                for fpat in fields {
                    self.bind_pattern(fpat, &Type::Unknown, mutable);
                }
            }
            Pattern::Binding { name, subpattern, .. } => {
                self.define(name.clone(), ty.clone(), mutable);
                self.bind_pattern(subpattern, ty, mutable);
            }
            Pattern::Literal { .. } | Pattern::Range { .. } | Pattern::Error(_) => {}
        }
    }

    // -----------------------------------------------------------------------
    // Expression inference (bottom-up)
    // -----------------------------------------------------------------------

    fn infer_expr(&mut self, expr: &Expr) -> Type {
        let ty = self.infer_expr_inner(expr);
        self.record(expr.span, &ty);
        ty
    }

    fn infer_expr_inner(&mut self, expr: &Expr) -> Type {
        match &expr.kind {
            // ── Literals ──────────────────────────────────────────────
            ExprKind::IntLit(_) => Type::Primitive(PrimitiveType::I32),
            ExprKind::FloatLit(_) => Type::Primitive(PrimitiveType::F64),
            ExprKind::BoolLit(_) => Type::Primitive(PrimitiveType::Bool),
            ExprKind::CharLit(_) => Type::Primitive(PrimitiveType::Char),
            ExprKind::StringLit(_) => Type::String,
            ExprKind::TemplateLit(_) => Type::String,
            ExprKind::UnitLit => Type::Unit,

            ExprKind::ArrayLit(elems) => {
                if elems.is_empty() {
                    Type::Array(Box::new(self.fresh_var()))
                } else {
                    let first = self.infer_expr(&elems[0]);
                    for elem in &elems[1..] {
                        let et = self.infer_expr(elem);
                        self.unify(&first, &et, elem.span);
                    }
                    Type::Array(Box::new(first))
                }
            }

            ExprKind::MapLit(pairs) => {
                if pairs.is_empty() {
                    Type::Map(Box::new(self.fresh_var()), Box::new(self.fresh_var()))
                } else {
                    let kt = self.infer_expr(&pairs[0].0);
                    let vt = self.infer_expr(&pairs[0].1);
                    for (k, v) in &pairs[1..] {
                        let kti = self.infer_expr(k);
                        let vti = self.infer_expr(v);
                        self.unify(&kt, &kti, k.span);
                        self.unify(&vt, &vti, v.span);
                    }
                    Type::Map(Box::new(kt), Box::new(vt))
                }
            }

            ExprKind::TupleLit(elems) => {
                let types: Vec<_> = elems.iter().map(|e| self.infer_expr(e)).collect();
                Type::Tuple(types)
            }

            // ── Ident / Path ──────────────────────────────────────────
            ExprKind::Ident(name) => {
                if let Some(binding) = self.lookup(name.as_str()) {
                    binding.ty.clone()
                } else if let Some(fn_ty) = self.fn_sigs.get(name.as_str()) {
                    fn_ty.clone()
                } else if let Some(hf) = self.bindings.get_function(name.as_str()) {
                    let params: Vec<Type> = hf
                        .params
                        .iter()
                        .map(|p| Self::from_script_type(&p.ty))
                        .collect();
                    let ret = Self::from_script_type(&hf.return_type);
                    Type::Fn {
                        params,
                        ret: Box::new(ret),
                    }
                } else if let Some(g) = self.bindings.get_global(name.as_str()) {
                    Self::from_script_type(&g.ty)
                } else if name.as_str() == "None" {
                    Type::Option(Box::new(self.fresh_var()))
                } else {
                    self.error(expr.span, "E010", format!("undefined variable `{name}`"));
                    Type::Error
                }
            }

            ExprKind::Path(segments) => {
                // Check for enum variant: EnumName::Variant
                if segments.len() == 2 {
                    if let Some(&idx) = self.enum_names.get(segments[0].as_str()) {
                        return Type::Enum(idx);
                    }
                }
                Type::Unknown
            }

            // ── Struct init ───────────────────────────────────────────
            ExprKind::StructInit { name, fields } => {
                if let Some(&idx) = self.struct_names.get(name.as_str()) {
                    let sdef = self.structs[idx].clone();
                    // Check each initializer field.
                    for finit in fields {
                        let init_expr = finit.value.as_ref();
                        let field_ty = sdef
                            .fields
                            .iter()
                            .find(|(n, _)| n == &finit.name)
                            .map(|(_, t)| t.clone());
                        if let Some(expected) = field_ty {
                            if let Some(val) = init_expr {
                                let vt = self.infer_expr(val);
                                self.unify(&vt, &expected, val.span);
                            }
                        } else {
                            self.error(
                                expr.span,
                                "E005",
                                format!("struct `{name}` has no field `{}`", finit.name),
                            );
                        }
                    }
                    // Check that all fields are provided.
                    let provided: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
                    for (fname, _) in &sdef.fields {
                        if !provided.contains(&fname.as_str()) {
                            self.error(
                                expr.span,
                                "E005",
                                format!("missing field `{fname}` in struct `{name}`"),
                            );
                        }
                    }
                    Type::Struct(idx)
                } else {
                    self.error(expr.span, "E010", format!("undefined struct `{name}`"));
                    Type::Error
                }
            }

            // ── Binary operators ──────────────────────────────────────
            ExprKind::Binary { op, lhs, rhs } => {
                let lt = self.infer_expr(lhs);
                let rt = self.infer_expr(rhs);
                self.check_binary_op(*op, &lt, &rt, expr.span)
            }

            // ── Unary operators ───────────────────────────────────────
            ExprKind::Unary { op, operand } => {
                let ot = self.infer_expr(operand);
                match op {
                    UnaryOp::Neg => {
                        if !ot.is_numeric() && !ot.is_error_or_unknown() {
                            self.error(expr.span, "E001", format!("cannot negate `{ot}`"));
                        }
                        ot
                    }
                    UnaryOp::Not => {
                        if ot != Type::Primitive(PrimitiveType::Bool) && !ot.is_error_or_unknown() {
                            self.error(
                                expr.span,
                                "E001",
                                format!("expected `bool` for `not`, found `{ot}`"),
                            );
                        }
                        Type::Primitive(PrimitiveType::Bool)
                    }
                    UnaryOp::BitNot => {
                        if !ot.is_integer() && !ot.is_error_or_unknown() {
                            self.error(
                                expr.span,
                                "E001",
                                format!("bitwise NOT requires integer type, found `{ot}`"),
                            );
                        }
                        ot
                    }
                    UnaryOp::Ref => Type::Ref(Box::new(ot)),
                }
            }

            // ── Assignment ────────────────────────────────────────────
            ExprKind::Assign { target, value, op } => {
                let lhs_ty = self.infer_expr(target);
                let rhs_ty = self.infer_expr(value);

                // Check mutability.
                if let ExprKind::Ident(name) = &target.kind {
                    if let Some(binding) = self.lookup(name.as_str()) {
                        if !binding.mutable {
                            self.error(
                                expr.span,
                                "E007",
                                format!("cannot assign to immutable variable `{name}`"),
                            );
                        }
                    }
                }

                match op {
                    AssignOp::Assign => {
                        self.unify(&rhs_ty, &lhs_ty, expr.span);
                    }
                    _ => {
                        // Compound assignment: both sides must be same numeric type.
                        self.unify(&lhs_ty, &rhs_ty, expr.span);
                    }
                }
                Type::Unit
            }

            // ── Field access ──────────────────────────────────────────
            ExprKind::FieldAccess { object, field } => {
                let obj_ty = self.infer_expr(object);
                let resolved = self.resolve(&obj_ty);
                match &resolved {
                    Type::Struct(idx) => {
                        let idx = *idx;
                        let sdef = self.structs[idx].clone();
                        if let Some((_, fty)) = sdef.fields.iter().find(|(n, _)| n == field) {
                            fty.clone()
                        } else {
                            self.error(
                                expr.span,
                                "E005",
                                format!(
                                    "no field `{field}` on struct `{}`",
                                    sdef.name
                                ),
                            );
                            Type::Error
                        }
                    }
                    Type::Tuple(items) => {
                        // Field access on tuple is not typical — use TupleIndex instead.
                        // But if someone writes .0 etc. we handle it.
                        if let Ok(idx) = field.parse::<usize>() {
                            if idx < items.len() {
                                items[idx].clone()
                            } else {
                                self.error(
                                    expr.span,
                                    "E005",
                                    format!("tuple index {idx} out of bounds (length {})", items.len()),
                                );
                                Type::Error
                            }
                        } else {
                            self.error(
                                expr.span,
                                "E005",
                                format!("no field `{field}` on tuple"),
                            );
                            Type::Error
                        }
                    }
                    t if t.is_error_or_unknown() => Type::Error,
                    _ => {
                        self.error(
                            expr.span,
                            "E005",
                            format!("no field `{field}` on type `{resolved}`"),
                        );
                        Type::Error
                    }
                }
            }

            ExprKind::TupleIndex { object, index } => {
                let obj_ty = self.infer_expr(object);
                let resolved = self.resolve(&obj_ty);
                if let Type::Tuple(items) = &resolved {
                    let idx = *index as usize;
                    if idx < items.len() {
                        items[idx].clone()
                    } else {
                        self.error(
                            expr.span,
                            "E005",
                            format!("tuple index {idx} out of bounds (length {})", items.len()),
                        );
                        Type::Error
                    }
                } else if resolved.is_error_or_unknown() {
                    Type::Error
                } else {
                    self.error(
                        expr.span,
                        "E001",
                        format!("tuple index on non-tuple type `{resolved}`"),
                    );
                    Type::Error
                }
            }

            // ── Method call ───────────────────────────────────────────
            ExprKind::MethodCall {
                object,
                method,
                args,
            } => {
                let obj_ty = self.infer_expr(object);
                let resolved_obj = self.resolve(&obj_ty);
                let arg_types: Vec<Type> = args.iter().map(|a| self.infer_expr(&a.value)).collect();
                self.check_method_call(&resolved_obj, method, &arg_types, expr.span)
            }

            // ── Index ─────────────────────────────────────────────────
            ExprKind::Index { object, index } => {
                let obj_ty = self.infer_expr(object);
                let idx_ty = self.infer_expr(index);
                let resolved = self.resolve(&obj_ty);
                match &resolved {
                    Type::Array(elem) => {
                        if !idx_ty.is_integer() && !idx_ty.is_error_or_unknown() {
                            self.error(
                                index.span,
                                "E001",
                                format!("array index must be integer, found `{idx_ty}`"),
                            );
                        }
                        *elem.clone()
                    }
                    Type::Map(k, v) => {
                        self.unify(&idx_ty, k, index.span);
                        *v.clone()
                    }
                    Type::String => {
                        if !idx_ty.is_integer() && !idx_ty.is_error_or_unknown() {
                            self.error(
                                index.span,
                                "E001",
                                format!("string index must be integer, found `{idx_ty}`"),
                            );
                        }
                        Type::Primitive(PrimitiveType::Char)
                    }
                    t if t.is_error_or_unknown() => Type::Error,
                    _ => {
                        self.error(
                            expr.span,
                            "E001",
                            format!("cannot index into type `{resolved}`"),
                        );
                        Type::Error
                    }
                }
            }

            // ── Error propagation (?) ─────────────────────────────────
            ExprKind::ErrorPropagate(inner) => {
                let inner_ty = self.infer_expr(inner);
                let resolved = self.resolve(&inner_ty);
                match &resolved {
                    Type::Option(inner) => *inner.clone(),
                    Type::Result(ok, _) => *ok.clone(),
                    t if t.is_error_or_unknown() => Type::Error,
                    _ => {
                        self.error(
                            expr.span,
                            "E001",
                            format!("`?` operator requires `Option` or `Result`, found `{resolved}`"),
                        );
                        Type::Error
                    }
                }
            }

            // ── Cast ──────────────────────────────────────────────────
            ExprKind::Cast { expr: inner, ty } => {
                let from_ty = self.infer_expr(inner);
                let to_ty = self.resolve_type_expr(ty);

                // Only numeric casts are allowed (and Error/Unknown pass through).
                if !from_ty.is_numeric()
                    && !from_ty.is_error_or_unknown()
                    && from_ty != Type::Primitive(PrimitiveType::Char)
                {
                    self.error(
                        inner.span,
                        "E001",
                        format!("cannot cast `{from_ty}` to `{to_ty}`"),
                    );
                }
                to_ty
            }

            // ── Call ──────────────────────────────────────────────────
            ExprKind::Call { callee, args } => {
                let callee_ty = self.infer_expr(callee);
                let arg_types: Vec<Type> = args.iter().map(|a| self.infer_expr(&a.value)).collect();
                let resolved = self.resolve(&callee_ty);
                match &resolved {
                    Type::Fn { params, ret } => {
                        // Check arity.
                        if params.len() != arg_types.len()
                            && !params.iter().any(|p| p == &Type::Unknown)
                        {
                            self.error(
                                expr.span,
                                "E006",
                                format!(
                                    "expected {} argument(s), found {}",
                                    params.len(),
                                    arg_types.len()
                                ),
                            );
                        } else {
                            for (pt, at) in params.iter().zip(arg_types.iter()) {
                                if pt != &Type::Unknown {
                                    self.unify(at, pt, expr.span);
                                }
                            }
                        }
                        *ret.clone()
                    }
                    t if t.is_error_or_unknown() => Type::Unknown,
                    _ => {
                        self.error(
                            expr.span,
                            "E011",
                            format!("not a callable type: `{resolved}`"),
                        );
                        Type::Error
                    }
                }
            }

            // ── Pipe (lhs |> rhs) ────────────────────────────────────
            ExprKind::Pipe { lhs, rhs } => {
                let lhs_ty = self.infer_expr(lhs);
                // Treat pipe as method call: lhs |> func(args) == lhs.func(args)
                // Check if rhs is a Call to a known pipeline function.
                if let ExprKind::Call { callee, args } = &rhs.kind {
                    if let ExprKind::Ident(name) = &callee.kind {
                        let pipeline_ops = ["filter", "map", "collect", "take", "skip",
                            "any", "all", "find", "for_each", "fold", "reduce",
                            "sum", "min", "max", "count", "sort", "reverse", "dedup",
                            "enumerate", "first", "last", "contains"];
                        if pipeline_ops.contains(&name.as_str()) {
                            let arg_types: Vec<Type> = args.iter().map(|a| self.infer_expr(&a.value)).collect();
                            return self.check_method_call(&lhs_ty, name, &arg_types, expr.span);
                        }
                    }
                }
                let rhs_ty = self.infer_expr(rhs);
                let resolved = self.resolve(&rhs_ty);
                match &resolved {
                    Type::Fn { params, ret } => {
                        if params.len() == 1 {
                            self.unify(&lhs_ty, &params[0], expr.span);
                        }
                        *ret.clone()
                    }
                    _ => {
                        // If rhs is a method call it was already handled; just return Unknown.
                        Type::Unknown
                    }
                }
            }

            // ── If/else ───────────────────────────────────────────────
            ExprKind::If {
                condition,
                then_block,
                else_block,
            } => {
                let cond_ty = self.infer_expr(condition);
                if cond_ty != Type::Primitive(PrimitiveType::Bool)
                    && !cond_ty.is_error_or_unknown()
                {
                    self.error(
                        condition.span,
                        "E001",
                        format!("condition must be `bool`, found `{cond_ty}`"),
                    );
                }
                let then_ty = self.check_block(then_block);
                if let Some(else_expr) = else_block {
                    let else_ty = self.infer_expr(else_expr);
                    self.unify(&then_ty, &else_ty, expr.span)
                } else {
                    Type::Unit
                }
            }

            // ── if let ────────────────────────────────────────────────
            ExprKind::IfLet {
                pattern,
                expr: scrutinee,
                then_block,
                else_block,
            } => {
                let scr_ty = self.infer_expr(scrutinee);
                self.push_scope();
                self.bind_pattern(pattern, &scr_ty, false);
                let then_ty = self.check_block(then_block);
                self.pop_scope();
                if let Some(else_expr) = else_block {
                    let else_ty = self.infer_expr(else_expr);
                    self.unify(&then_ty, &else_ty, expr.span)
                } else {
                    Type::Unit
                }
            }

            // ── Match ─────────────────────────────────────────────────
            ExprKind::Match { scrutinee, arms } => {
                let scr_ty = self.infer_expr(scrutinee);

                // Check for at least a wildcard arm (basic exhaustiveness).
                let has_wildcard = arms.iter().any(|a| matches!(&a.pattern, Pattern::Wildcard(_)));
                if !has_wildcard && !arms.is_empty() {
                    // Not necessarily wrong, but emit an info-level hint via warning code.
                    // We don't report it as an error since non-wildcard matches can be exhaustive.
                }

                let mut result_ty: Option<Type> = None;
                for arm in arms {
                    self.push_scope();
                    self.bind_pattern(&arm.pattern, &scr_ty, false);
                    if let Some(guard) = &arm.guard {
                        let gt = self.infer_expr(guard);
                        if gt != Type::Primitive(PrimitiveType::Bool)
                            && !gt.is_error_or_unknown()
                        {
                            self.error(
                                guard.span,
                                "E001",
                                "match guard must be `bool`".to_string(),
                            );
                        }
                    }
                    let arm_ty = self.infer_expr(&arm.body);
                    self.pop_scope();
                    result_ty = Some(if let Some(prev) = result_ty {
                        self.unify(&prev, &arm_ty, arm.span)
                    } else {
                        arm_ty
                    });
                }
                result_ty.unwrap_or(Type::Unit)
            }

            // ── For ───────────────────────────────────────────────────
            ExprKind::For {
                pattern,
                iterable,
                body,
            } => {
                let iter_ty = self.infer_expr(iterable);
                let elem_ty = match &self.resolve(&iter_ty) {
                    Type::Array(inner) => *inner.clone(),
                    Type::Map(k, v) => Type::Tuple(vec![*k.clone(), *v.clone()]),
                    Type::String => Type::Primitive(PrimitiveType::Char),
                    _ => Type::Unknown,
                };
                self.push_scope();
                self.bind_pattern(pattern, &elem_ty, false);
                self.check_block(body);
                self.pop_scope();
                Type::Unit
            }

            // ── While ─────────────────────────────────────────────────
            ExprKind::While { condition, body } => {
                let ct = self.infer_expr(condition);
                if ct != Type::Primitive(PrimitiveType::Bool) && !ct.is_error_or_unknown() {
                    self.error(
                        condition.span,
                        "E001",
                        format!("while condition must be `bool`, found `{ct}`"),
                    );
                }
                self.check_block(body);
                Type::Unit
            }

            ExprKind::WhileLet {
                pattern,
                expr: scrutinee,
                body,
            } => {
                let scr_ty = self.infer_expr(scrutinee);
                self.push_scope();
                self.bind_pattern(pattern, &scr_ty, false);
                self.check_block(body);
                self.pop_scope();
                Type::Unit
            }

            ExprKind::Loop { body } => {
                self.check_block(body);
                // Loop returns Never/Unit — we use Unit.
                Type::Unit
            }

            // ── Jump ──────────────────────────────────────────────────
            ExprKind::Break(maybe_val) => {
                if let Some(val) = maybe_val {
                    self.infer_expr(val);
                }
                Type::Unit
            }
            ExprKind::Continue => Type::Unit,
            ExprKind::Return(maybe_val) => {
                let val_ty = maybe_val
                    .as_ref()
                    .map(|v| self.infer_expr(v))
                    .unwrap_or(Type::Unit);
                if let Some(expected) = &self.current_return_type.clone() {
                    self.unify(&val_ty, expected, expr.span);
                    // Return the expected return type so that blocks ending with
                    // `return expr;` produce the function's declared return type
                    // rather than `()`, avoiding false "type mismatch" warnings.
                    expected.clone()
                } else {
                    Type::Unit
                }
            }

            // ── Lambda ────────────────────────────────────────────────
            ExprKind::Lambda {
                params,
                return_type,
                body,
            } => {
                self.push_scope();
                let mut param_types = Vec::new();
                for lp in params {
                    let pty = lp
                        .ty
                        .as_ref()
                        .map(|t| self.resolve_type_expr(t))
                        .unwrap_or_else(|| self.fresh_var());
                    self.define(lp.name.clone(), pty.clone(), false);
                    param_types.push(pty);
                }
                let body_ty = self.infer_expr(body);
                if let Some(ret_ann) = return_type {
                    let ann = self.resolve_type_expr(ret_ann);
                    self.unify(&body_ty, &ann, expr.span);
                }
                self.pop_scope();
                Type::Fn {
                    params: param_types,
                    ret: Box::new(body_ty),
                }
            }

            // ── Range ─────────────────────────────────────────────────
            ExprKind::Range {
                start, end, ..
            } => {
                let start_ty = start
                    .as_ref()
                    .map(|e| self.infer_expr(e))
                    .unwrap_or(Type::Primitive(PrimitiveType::I64));
                let end_ty = end
                    .as_ref()
                    .map(|e| self.infer_expr(e))
                    .unwrap_or(Type::Primitive(PrimitiveType::I64));
                let unified = self.unify(&start_ty, &end_ty, expr.span);
                if !unified.is_numeric() && !unified.is_error_or_unknown() {
                    self.error(
                        expr.span,
                        "E001",
                        format!("range bounds must be numeric, found `{unified}`"),
                    );
                }
                // Ranges produce arrays of the element type.
                Type::Array(Box::new(unified))
            }

            // ── Block ─────────────────────────────────────────────────
            ExprKind::Block(block) => self.check_block(block),

            // ── Macro call ────────────────────────────────────────────
            ExprKind::MacroCall { args, .. } => {
                for arg in args {
                    self.infer_expr(arg);
                }
                Type::Unknown
            }

            // ── Error recovery ────────────────────────────────────────
            ExprKind::Error => Type::Error,
        }
    }

    // -----------------------------------------------------------------------
    // Expression checking (top-down)
    // -----------------------------------------------------------------------

    #[allow(dead_code)]
    fn check_expr(&mut self, expr: &Expr, expected: &Type) -> Type {
        // For lambdas, propagate expected function type to infer parameter types.
        if let ExprKind::Lambda {
            params,
            return_type,
            body,
        } = &expr.kind
        {
            if let Type::Fn {
                params: expected_params,
                ret: expected_ret,
            } = expected
            {
                self.push_scope();
                let mut param_types = Vec::new();
                for (i, lp) in params.iter().enumerate() {
                    let pty = lp
                        .ty
                        .as_ref()
                        .map(|t| self.resolve_type_expr(t))
                        .unwrap_or_else(|| {
                            expected_params.get(i).cloned().unwrap_or_else(|| self.fresh_var())
                        });
                    self.define(lp.name.clone(), pty.clone(), false);
                    param_types.push(pty);
                }
                let body_ty = self.infer_expr(body);
                let ret = if let Some(ret_ann) = return_type {
                    let ann = self.resolve_type_expr(ret_ann);
                    self.unify(&body_ty, &ann, expr.span)
                } else {
                    self.unify(&body_ty, expected_ret, expr.span)
                };
                self.pop_scope();
                let ty = Type::Fn {
                    params: param_types,
                    ret: Box::new(ret),
                };
                self.record(expr.span, &ty);
                return ty;
            }
        }

        let inferred = self.infer_expr(expr);
        self.unify(&inferred, expected, expr.span)
    }

    // -----------------------------------------------------------------------
    // Binary operator checking
    // -----------------------------------------------------------------------

    fn check_binary_op(&mut self, op: BinOp, lhs: &Type, rhs: &Type, span: Span) -> Type {
        let lhs = self.resolve(lhs);
        let rhs = self.resolve(rhs);

        match op {
            // Arithmetic
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => {
                // String concatenation.
                if op == BinOp::Add && (lhs == Type::String || rhs == Type::String) {
                    return Type::String;
                }
                if lhs.is_numeric() && rhs.is_numeric() {
                    self.unify(&lhs, &rhs, span)
                } else if lhs.is_error_or_unknown() || rhs.is_error_or_unknown() {
                    if lhs.is_error_or_unknown() { rhs } else { lhs }
                } else {
                    self.error(
                        span,
                        "E001",
                        format!("cannot apply `{op:?}` to `{lhs}` and `{rhs}`"),
                    );
                    Type::Error
                }
            }

            // Comparison
            BinOp::Eq | BinOp::Neq => {
                self.unify(&lhs, &rhs, span);
                Type::Primitive(PrimitiveType::Bool)
            }

            BinOp::Lt | BinOp::Gt | BinOp::LtEq | BinOp::GtEq | BinOp::Spaceship => {
                self.unify(&lhs, &rhs, span);
                if op == BinOp::Spaceship {
                    Type::Primitive(PrimitiveType::I32)
                } else {
                    Type::Primitive(PrimitiveType::Bool)
                }
            }

            // Logical
            BinOp::And | BinOp::Or => {
                if lhs != Type::Primitive(PrimitiveType::Bool) && !lhs.is_error_or_unknown() {
                    self.error(span, "E001", format!("expected `bool`, found `{lhs}`"));
                }
                if rhs != Type::Primitive(PrimitiveType::Bool) && !rhs.is_error_or_unknown() {
                    self.error(span, "E001", format!("expected `bool`, found `{rhs}`"));
                }
                Type::Primitive(PrimitiveType::Bool)
            }

            // Bitwise
            BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr => {
                if lhs.is_integer() && rhs.is_integer() {
                    self.unify(&lhs, &rhs, span)
                } else if lhs.is_error_or_unknown() || rhs.is_error_or_unknown() {
                    if lhs.is_error_or_unknown() { rhs } else { lhs }
                } else {
                    self.error(
                        span,
                        "E001",
                        format!("bitwise op requires integer types, found `{lhs}` and `{rhs}`"),
                    );
                    Type::Error
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Method call resolution
    // -----------------------------------------------------------------------

    fn check_method_call(
        &mut self,
        receiver: &Type,
        method: &SmolStr,
        arg_types: &[Type],
        span: Span,
    ) -> Type {
        let method_sig = match receiver {
            Type::String => string_methods(method.as_str()),
            Type::Array(elem) => array_methods(elem, method.as_str()),
            Type::Option(inner) => option_methods(inner, method.as_str()),
            Type::Result(ok, err) => result_methods(ok, err, method.as_str()),
            Type::Map(k, v) => map_methods(k, v, method.as_str()),
            Type::Struct(idx) => {
                // Look up impl methods by checking fn_sigs for mangled names.
                let type_name = self.structs.get(*idx)
                    .map(|s| s.name.clone())
                    .unwrap_or_default();
                let mangled = format!("{}__{}",  type_name, method);
                if let Some(fn_ty) = self.fn_sigs.get(mangled.as_str()).cloned() {
                    if let Type::Fn { params, ret } = fn_ty {
                        Some(MethodSig { params, ret: *ret })
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ if receiver.is_error_or_unknown() => return Type::Unknown,
            _ => None,
        };

        // Also check host type methods.
        if method_sig.is_none() {
            if let Some(host_sig) = self.lookup_host_method(receiver, method.as_str()) {
                // Check arity.
                if host_sig.params.len() != arg_types.len() {
                    self.error(
                        span,
                        "E006",
                        format!(
                            "method `{method}` expects {} argument(s), found {}",
                            host_sig.params.len(),
                            arg_types.len()
                        ),
                    );
                }
                return host_sig.ret;
            }
        }

        if let Some(sig) = method_sig {
            // Check arity.
            if sig.params.len() != arg_types.len() {
                self.error(
                    span,
                    "E006",
                    format!(
                        "method `{method}` expects {} argument(s), found {}",
                        sig.params.len(),
                        arg_types.len()
                    ),
                );
            } else {
                for (expected, actual) in sig.params.iter().zip(arg_types.iter()) {
                    if expected != &Type::Unknown {
                        self.unify(actual, expected, span);
                    }
                }
            }
            sig.ret
        } else if !receiver.is_error_or_unknown() {
            self.error(
                span,
                "E005",
                format!("no method `{method}` on type `{receiver}`"),
            );
            Type::Error
        } else {
            Type::Unknown
        }
    }

    fn lookup_host_method(&self, _receiver: &Type, _method: &str) -> Option<MethodSig> {
        // Walk host type bindings. We would need a way to map our Type to a host type name.
        // For now this is a placeholder for host-type method resolution.
        None
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Run the type checker on a parsed program.
///
/// Returns the type information side-table and a list of diagnostics.
pub fn type_check(
    program: &Program,
    bindings: &BindingRegistry,
) -> (TypeInfo, Vec<TypeDiagnostic>) {
    let mut env = TypeEnv::new(bindings);

    // Register host functions and globals into the environment.
    for (name, hf) in &bindings.functions {
        let params: Vec<Type> = hf
            .params
            .iter()
            .map(|p| TypeEnv::from_script_type(&p.ty))
            .collect();
        let ret = TypeEnv::from_script_type(&hf.return_type);
        let fn_ty = Type::Fn {
            params,
            ret: Box::new(ret),
        };
        env.define(SmolStr::new(name.as_str()), fn_ty, false);
    }
    for (name, g) in &bindings.globals {
        let ty = TypeEnv::from_script_type(&g.ty);
        env.define(SmolStr::new(name.as_str()), ty, false);
    }

    env.check_program(program);

    let info = TypeInfo {
        types: env.type_map,
        structs: env.structs,
        enums: env.enums,
    };
    (info, env.diagnostics)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_bindings() -> BindingRegistry {
        BindingRegistry::new()
    }

    fn dummy_span() -> Span {
        Span::dummy()
    }

    #[test]
    fn unify_same_types() {
        let bindings = empty_bindings();
        let mut env = TypeEnv::new(&bindings);
        let result = env.unify(
            &Type::Primitive(PrimitiveType::I32),
            &Type::Primitive(PrimitiveType::I32),
            dummy_span(),
        );
        assert_eq!(result, Type::Primitive(PrimitiveType::I32));
        assert!(env.diagnostics.is_empty());
    }

    #[test]
    fn unify_mismatch() {
        let bindings = empty_bindings();
        let mut env = TypeEnv::new(&bindings);
        let result = env.unify(
            &Type::Primitive(PrimitiveType::I32),
            &Type::String,
            dummy_span(),
        );
        assert_eq!(result, Type::Error);
        assert_eq!(env.diagnostics.len(), 1);
        assert_eq!(env.diagnostics[0].code, "E001");
    }

    #[test]
    fn unify_type_vars() {
        let bindings = empty_bindings();
        let mut env = TypeEnv::new(&bindings);
        let a = env.fresh_var();
        let result = env.unify(&a, &Type::Primitive(PrimitiveType::Bool), dummy_span());
        assert_eq!(result, Type::Primitive(PrimitiveType::Bool));
        assert!(env.diagnostics.is_empty());
    }

    #[test]
    fn unify_arrays() {
        let bindings = empty_bindings();
        let mut env = TypeEnv::new(&bindings);
        let a = Type::Array(Box::new(env.fresh_var()));
        let b = Type::Array(Box::new(Type::Primitive(PrimitiveType::I64)));
        let result = env.unify(&a, &b, dummy_span());
        assert_eq!(result, Type::Array(Box::new(Type::Primitive(PrimitiveType::I64))));
    }

    #[test]
    fn type_check_empty_program() {
        let program = Program {
            items: vec![],
            span: dummy_span(),
        };
        let bindings = empty_bindings();
        let (info, diags) = type_check(&program, &bindings);
        assert!(diags.is_empty());
        assert!(info.types.is_empty());
    }

    #[test]
    fn from_script_type_round_trip() {
        let cases = vec![
            (ScriptType::I32, Type::Primitive(PrimitiveType::I32)),
            (ScriptType::String, Type::String),
            (ScriptType::Unit, Type::Unit),
            (
                ScriptType::Array(Box::new(ScriptType::Bool)),
                Type::Array(Box::new(Type::Primitive(PrimitiveType::Bool))),
            ),
        ];
        for (st, expected) in cases {
            assert_eq!(TypeEnv::from_script_type(&st), expected);
        }
    }
}
