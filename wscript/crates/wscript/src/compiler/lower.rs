//! Lowering from AST to IR.
//!
//! Converts the high-level Wscript AST into a flat, typed IR that maps
//! closely to WebAssembly concepts (i32/i64/f32/f64, locals, linear control
//! flow).  The IR is then consumed by `codegen` to emit actual WASM bytes.

use std::collections::HashMap;

use smol_str::SmolStr;

use super::ast::{self, AssignOp, BinOp, ExprKind, Pattern, Stmt, UnaryOp};
use super::ir::*;
use super::token::Span;
use crate::bindings::{BindingRegistry, ScriptType};

/// Map a `com_*` builtin call to its `__com_*` host import name and return
/// type. Arity is validated upstream by the type checker; the `_argc` value
/// is only used to disambiguate the variadic `com_call_*` family.
fn map_com_builtin(name: &str, _argc: usize) -> Option<(SmolStr, IrType)> {
    let (hn, ret) = match name {
        "com_create" => ("__com_create", IrType::I32),
        "com_release" => ("__com_release", IrType::Unit),
        "com_has" => ("__com_has", IrType::I32),
        "com_call_i0" => ("__com_call_i0", IrType::I32),
        "com_call_i1s" => ("__com_call_i1s", IrType::I32),
        "com_call_i1i" => ("__com_call_i1i", IrType::I32),
        "com_call_i2si" => ("__com_call_i2si", IrType::I32),
        "com_call_s0" => ("__com_call_s0", IrType::Ptr),
        "com_call_s1s" => ("__com_call_s1s", IrType::Ptr),
        "com_get_i" => ("__com_get_i", IrType::I32),
        "com_get_s" => ("__com_get_s", IrType::Ptr),
        "com_set_i" => ("__com_set_i", IrType::I32),
        "com_set_s" => ("__com_set_s", IrType::I32),
        "com_last_error" => ("__com_last_error", IrType::Ptr),
        _ => return None,
    };
    Some((SmolStr::new(hn), ret))
}

// ---------------------------------------------------------------------------
// Lowerer state
// ---------------------------------------------------------------------------

struct Lowerer {
    /// The IR module we are building.
    module: IrModule,
    /// String constant intern table: string value -> index.
    string_table: HashMap<String, u32>,
    /// Debug mode flag — when true we emit `DebugProbe` stmts.
    debug_mode: bool,
    /// Map from function name to its index in `module.functions`.
    fn_name_map: HashMap<SmolStr, usize>,
    /// Map from struct name to its index in `module.struct_layouts`.
    struct_name_map: HashMap<SmolStr, u32>,
    /// Map from enum variant path ("EnumName::VariantName") to i32 tag.
    enum_variant_tags: HashMap<SmolStr, i32>,
    /// Set of known enum type names (for type lowering).
    enum_names: HashMap<SmolStr, ()>,
    /// Counter for generating unique lambda function names.
    lambda_counter: u32,
    /// Map from variable name to the generated lambda function name.
    /// e.g. `let double = |x| x * 2;` → "double" → "__lambda_0"
    lambda_aliases: HashMap<SmolStr, SmolStr>,
    /// Map from lambda function name to the list of captured variable names
    /// (in the enclosing scope).  At call sites we append these as extra args.
    lambda_captures: HashMap<SmolStr, Vec<SmolStr>>,
    /// Map from method name to mangled function name for impl methods.
    /// e.g. "sum" → "Point__sum".  Used to resolve `obj.method()` calls.
    method_map: HashMap<SmolStr, SmolStr>,
    /// Compile-time constant values for constant folding.
    /// `const MAX: i32 = 100;` → "MAX" → IrExpr::IntConst(100).
    const_values: HashMap<SmolStr, IrExpr>,
    /// Recorded trait method signatures (trait_name → vec of method sigs).
    trait_sigs: HashMap<SmolStr, Vec<IrFnSig>>,
    /// User-registered host fn signatures keyed by name. Populated from the
    /// `BindingRegistry` passed to `lower()`. Calls that resolve to one of
    /// these names are emitted as `HostCall` under module "host".
    user_host_fns: HashMap<SmolStr, (Vec<IrType>, IrType)>,
    /// Top-level `let` / `let mut` globals, keyed by name → (IR type, mutable).
    /// Used to resolve ident reads/writes in function bodies to GlobalGet/Set.
    global_names: HashMap<SmolStr, (IrType, bool)>,
    /// Init statements for globals whose initializer is not a WASM ConstExpr
    /// (struct globals, str globals, non-literal primitives). Collected and
    /// emitted as a synthesized `__wscript_init_globals` function wired as the
    /// WASM `start` section.
    deferred_global_inits: Vec<IrStmt>,
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Lower an AST `Program` into an `IrModule`.
pub fn lower(program: &ast::Program, debug_mode: bool, bindings: &BindingRegistry) -> IrModule {
    let mut user_host_fns: HashMap<SmolStr, (Vec<IrType>, IrType)> = HashMap::new();
    let mut user_host_imports: Vec<IrHostImport> = Vec::new();
    for (name, hf) in &bindings.functions {
        let params: Vec<IrType> = hf.params.iter().map(|p| script_type_to_ir(&p.ty)).collect();
        let ret = script_type_to_ir(&hf.return_type);
        let nm = SmolStr::new(name);
        user_host_fns.insert(nm.clone(), (params.clone(), ret));
        user_host_imports.push(IrHostImport {
            name: nm,
            params,
            ret,
        });
    }

    let mut lowerer = Lowerer {
        module: IrModule {
            functions: Vec::new(),
            struct_layouts: Vec::new(),
            enum_layouts: Vec::new(),
            string_constants: Vec::new(),
            globals: Vec::new(),
            user_host_imports,
        },
        string_table: HashMap::new(),
        debug_mode,
        fn_name_map: HashMap::new(),
        struct_name_map: HashMap::new(),
        enum_variant_tags: HashMap::new(),
        enum_names: HashMap::new(),
        lambda_counter: 0,
        lambda_aliases: HashMap::new(),
        lambda_captures: HashMap::new(),
        method_map: HashMap::new(),
        const_values: HashMap::new(),
        trait_sigs: HashMap::new(),
        user_host_fns,
        global_names: HashMap::new(),
        deferred_global_inits: Vec::new(),
    };

    // Register built-in Option and Result struct layouts so that
    // Some(v)/None and Ok(v)/Err(e) can be lowered to StructNew.
    lowerer.register_builtin_option_result();

    for item in &program.items {
        lowerer.lower_item(item);
    }

    // If any globals had non-constant initializers, synthesize an
    // `__wscript_init_globals` function carrying the deferred inits. Codegen
    // wires this as the WASM start section so Wasmtime runs it automatically
    // at instantiate time (after imports are linked and after `heap_ptr`'s
    // own const init has fired).
    if !lowerer.deferred_global_inits.is_empty() {
        let body = std::mem::take(&mut lowerer.deferred_global_inits);
        lowerer.module.functions.push(IrFunction {
            name: INIT_GLOBALS_FN.into(),
            params: Vec::new(),
            ret_type: IrType::Unit,
            locals: Vec::new(),
            body,
            is_export: false,
            source_span: None,
        });
    }

    lowerer.module
}

/// Name of the synthesized function carrying deferred global initializers.
/// Codegen looks for this name and sets it as `module.start`.
pub const INIT_GLOBALS_FN: &str = "__wscript_init_globals";

/// True if an IR expression can be emitted directly as a WASM `ConstExpr`
/// for use as a global's initializer. Anything else must be deferred into
/// the synthesized `__wscript_init_globals` start function.
fn is_wasm_const(e: &IrExpr) -> bool {
    matches!(
        e,
        IrExpr::IntConst(_) | IrExpr::FloatConst(_) | IrExpr::BoolConst(_)
    )
}

// ---------------------------------------------------------------------------
// Item lowering
// ---------------------------------------------------------------------------

impl Lowerer {
    /// Register built-in `__Option` (tag, value) and `__Result` (tag, value)
    /// struct layouts used by Option/Result lowering.
    fn register_builtin_option_result(&mut self) {
        // __Option struct: fields [tag: i32, value: i32]
        let option_idx = self.module.struct_layouts.len() as u32;
        self.module.struct_layouts.push(StructLayout {
            name: "__Option".into(),
            fields: vec![
                StructFieldLayout {
                    name: "tag".into(),
                    ty: IrType::I32,
                    offset: 0,
                    type_name: None,
                },
                StructFieldLayout {
                    name: "value".into(),
                    ty: IrType::I32,
                    offset: 4,
                    type_name: None,
                },
            ],
            size: 8,
            field_offsets: vec![0, 4],
        });
        self.struct_name_map.insert("__Option".into(), option_idx);

        // __Result struct: fields [tag: i32, value: i32]
        let result_idx = self.module.struct_layouts.len() as u32;
        self.module.struct_layouts.push(StructLayout {
            name: "__Result".into(),
            fields: vec![
                StructFieldLayout {
                    name: "tag".into(),
                    ty: IrType::I32,
                    offset: 0,
                    type_name: None,
                },
                StructFieldLayout {
                    name: "value".into(),
                    ty: IrType::I32,
                    offset: 4,
                    type_name: None,
                },
            ],
            size: 8,
            field_offsets: vec![0, 4],
        });
        self.struct_name_map.insert("__Result".into(), result_idx);

        // Register built-in variant tags so match lowering can resolve them.
        // Option: None=0, Some=1
        self.enum_variant_tags.insert("None".into(), 0);
        self.enum_variant_tags.insert("Some".into(), 1);
        // Result: Ok=0, Err=1
        self.enum_variant_tags.insert("Ok".into(), 0);
        self.enum_variant_tags.insert("Err".into(), 1);
    }

    fn lower_item(&mut self, item: &ast::Item) {
        match item {
            ast::Item::FnDecl(f) => self.lower_fn_decl(f),
            ast::Item::ConstDecl(c) => self.lower_const_decl(c),
            ast::Item::GlobalDecl(g) => self.lower_global_decl(g),
            ast::Item::StructDecl(s) => self.lower_struct_decl(s),
            ast::Item::EnumDecl(e) => self.lower_enum_decl(e),
            ast::Item::ImplBlock(ib) => self.lower_impl_block(ib),
            ast::Item::TraitDecl(t) => self.lower_trait_decl(t),
            ast::Item::Error(_) => {}
        }
    }

    // ── Functions ────────────────────────────────────────────────────────

    fn lower_fn_decl(&mut self, f: &ast::FnDecl) {
        let mut ctx = FnCtx::new();

        // Register parameters as locals.
        let mut params = Vec::new();
        for p in &f.params {
            match &p.kind {
                ast::ParamKind::Named {
                    name,
                    ty,
                    default: _,
                } => {
                    let ir_ty = self.lower_type_expr(ty);
                    let idx = ctx.new_local(name.clone(), ir_ty);
                    params.push(IrParam {
                        name: name.clone(),
                        ty: ir_ty,
                    });
                    let _ = idx; // local is already tracked
                }
                ast::ParamKind::SelfRef { .. } => {
                    // `self` becomes local 0 with Ptr type
                    let idx = ctx.new_local("self".into(), IrType::Ptr);
                    params.push(IrParam {
                        name: "self".into(),
                        ty: IrType::Ptr,
                    });
                    let _ = idx;
                }
            }
        }

        // Lower the body.
        let body = self.lower_block_stmts(&f.body, &mut ctx);

        // Determine whether the function should be exported.
        let is_export = f.attrs.iter().any(|a| a.name == "export") || f.name.as_str() == "main";

        let ret_type = f
            .return_type
            .as_ref()
            .map(|t| self.lower_type_expr(t))
            .unwrap_or(IrType::Unit);

        let ir_fn = IrFunction {
            name: f.name.clone(),
            params,
            ret_type,
            locals: ctx.locals.clone(),
            body,
            is_export,
            source_span: Some(f.span),
        };

        let idx = self.module.functions.len();
        self.fn_name_map.insert(f.name.clone(), idx);
        self.module.functions.push(ir_fn);
    }

    // ── Impl blocks ──────────────────────────────────────────────────────

    fn lower_impl_block(&mut self, ib: &ast::ImplBlock) {
        // Extract the type name from the self_type (e.g. "Point").
        let type_name = match &ib.self_type.kind {
            ast::TypeExprKind::Named { name, .. } => name.clone(),
            _ => return, // cannot lower impl for non-named types
        };

        for method in &ib.methods {
            let mangled: SmolStr = SmolStr::new(format!("{}__{}", type_name, method.name));

            // Register in the method map so method calls can resolve.
            self.method_map.insert(method.name.clone(), mangled.clone());

            // Lower as a regular function with the mangled name.
            let mut ctx = FnCtx::new();
            let mut params = Vec::new();
            for p in &method.params {
                match &p.kind {
                    ast::ParamKind::Named {
                        name,
                        ty,
                        default: _,
                    } => {
                        let ir_ty = self.lower_type_expr(ty);
                        let _idx = ctx.new_local(name.clone(), ir_ty);
                        params.push(IrParam {
                            name: name.clone(),
                            ty: ir_ty,
                        });
                    }
                    ast::ParamKind::SelfRef { .. } => {
                        let _idx = ctx.new_local("self".into(), IrType::Ptr);
                        params.push(IrParam {
                            name: "self".into(),
                            ty: IrType::Ptr,
                        });
                    }
                }
            }

            let body = self.lower_block_stmts(&method.body, &mut ctx);
            let ret_type = method
                .return_type
                .as_ref()
                .map(|t| self.lower_type_expr(t))
                .unwrap_or(IrType::Unit);

            let ir_fn = IrFunction {
                name: mangled.clone(),
                params,
                ret_type,
                locals: ctx.locals.clone(),
                body,
                is_export: false,
                source_span: Some(method.span),
            };

            let idx = self.module.functions.len();
            self.fn_name_map.insert(mangled, idx);
            self.module.functions.push(ir_fn);
        }
    }

    // ── Const declarations ──────────────────────────────────────────────

    fn lower_const_decl(&mut self, c: &ast::ConstDecl) {
        // Evaluate the const value.  For compile-time literals we store them
        // in the const_values map for constant folding at reference sites.
        let mut ctx = FnCtx::new();
        let init = self.lower_expr(&c.value, &mut ctx);
        // Store in the constant folding map so references substitute directly.
        self.const_values.insert(c.name.clone(), init);
    }

    fn lower_global_decl(&mut self, g: &ast::GlobalDecl) {
        let ty = match &g.ty {
            Some(te) => self.lower_type_expr(te),
            None => match &g.value.kind {
                ast::ExprKind::IntLit(_) => IrType::I32,
                ast::ExprKind::FloatLit(_) => IrType::F64,
                ast::ExprKind::BoolLit(_) => IrType::Bool,
                _ => IrType::I32,
            },
        };
        // Source type name for reflection: "str" for string globals, the
        // struct name for named-type globals, None for primitives.
        let type_name: Option<SmolStr> = g.ty.as_ref().and_then(|te| match &te.kind {
            ast::TypeExprKind::StringType => Some("str".into()),
            ast::TypeExprKind::Named { name, .. } if !self.enum_names.contains_key(name) => {
                Some(name.clone())
            }
            _ => None,
        });

        let mut ctx = FnCtx::new();
        let init = self.lower_expr(&g.value, &mut ctx);
        self.global_names.insert(g.name.clone(), (ty, g.mutable));

        if is_wasm_const(&init) {
            self.module.globals.push(IrGlobal {
                name: g.name.clone(),
                ty,
                init: Some(init),
                mutable: g.mutable,
                type_name,
            });
        } else {
            // Non-constant initializer: declare the WASM global as mutable
            // with a zero placeholder and schedule the real init for the
            // synthesized `__wscript_init_globals` function.
            self.module.globals.push(IrGlobal {
                name: g.name.clone(),
                ty,
                init: None,
                mutable: true,
                type_name,
            });
            self.deferred_global_inits.push(IrStmt::Assign {
                target: IrLValue::Global(g.name.clone()),
                value: init,
            });
        }
    }

    // ── Trait declarations ──────────────────────────────────────────────

    fn lower_trait_decl(&mut self, t: &ast::TraitDecl) {
        let mut sigs = Vec::new();
        for item in &t.items {
            match item {
                ast::TraitItem::FnSig(sig) => {
                    let params: Vec<IrType> = sig
                        .params
                        .iter()
                        .map(|p| match &p.kind {
                            ast::ParamKind::Named { ty, .. } => self.lower_type_expr(ty),
                            ast::ParamKind::SelfRef { .. } => IrType::Ptr,
                        })
                        .collect();
                    let ret = sig
                        .return_type
                        .as_ref()
                        .map(|t| self.lower_type_expr(t))
                        .unwrap_or(IrType::Unit);
                    sigs.push(IrFnSig {
                        name: sig.name.clone(),
                        params,
                        ret,
                    });
                }
                ast::TraitItem::FnDecl(f) => {
                    let params: Vec<IrType> = f
                        .params
                        .iter()
                        .map(|p| match &p.kind {
                            ast::ParamKind::Named { ty, .. } => self.lower_type_expr(ty),
                            ast::ParamKind::SelfRef { .. } => IrType::Ptr,
                        })
                        .collect();
                    let ret = f
                        .return_type
                        .as_ref()
                        .map(|t| self.lower_type_expr(t))
                        .unwrap_or(IrType::Unit);
                    sigs.push(IrFnSig {
                        name: f.name.clone(),
                        params,
                        ret,
                    });
                }
            }
        }
        self.trait_sigs.insert(t.name.clone(), sigs);
    }

    // ── Struct declarations ─────────────────────────────────────────────

    fn lower_struct_decl(&mut self, s: &ast::StructDecl) {
        let mut fields = Vec::new();
        let mut field_offsets = Vec::new();
        let mut offset: u32 = 0;

        for f in &s.fields {
            let ty = self.lower_type_expr(&f.ty);
            let size = type_size(ty);
            let align = size;
            if align > 0 {
                offset = (offset + align - 1) & !(align - 1);
            }
            field_offsets.push(offset);
            let type_name = type_expr_name(&f.ty);
            fields.push(StructFieldLayout {
                name: f.name.clone(),
                ty,
                offset,
                type_name,
            });
            offset += size;
        }

        let layout_idx = self.module.struct_layouts.len() as u32;
        self.module.struct_layouts.push(StructLayout {
            name: s.name.clone(),
            fields,
            size: offset,
            field_offsets,
        });
        self.struct_name_map.insert(s.name.clone(), layout_idx);
    }

    // ── Enum declarations ────────────────────────────────────────────────

    fn lower_enum_decl(&mut self, e: &ast::EnumDecl) {
        self.enum_names.insert(e.name.clone(), ());

        let mut variant_layouts = Vec::new();
        let mut max_payload: u32 = 0;
        let mut has_payload = false;

        for (tag, variant) in e.variants.iter().enumerate() {
            let tag = tag as u32;
            // Register the variant tag for path lookup (e.g. "Color::Red" => 0).
            let key = SmolStr::new(format!("{}::{}", e.name, variant.name));
            self.enum_variant_tags.insert(key, tag as i32);

            let (payload_types, payload_size) = match &variant.kind {
                ast::VariantKind::Tuple(types) => {
                    has_payload = true;
                    let tys: Vec<IrType> = types.iter().map(|t| self.lower_type_expr(t)).collect();
                    let size: u32 = tys.iter().map(|t| type_size(*t)).sum();
                    (tys, size)
                }
                _ => (Vec::new(), 0),
            };
            if payload_size > max_payload {
                max_payload = payload_size;
            }

            variant_layouts.push(EnumVariantLayout {
                name: variant.name.clone(),
                tag,
                payload_types,
                payload_size,
            });
        }

        self.module.enum_layouts.push(EnumLayout {
            name: e.name.clone(),
            variants: variant_layouts,
            tag_size: 4,
            max_payload_size: max_payload,
        });

        // If any variant carries data, create a struct layout for this enum
        // so it can be allocated on the shadow stack: {tag: i32, payload: i32}.
        // For v0.1 we support single-i32-payload variants.
        if has_payload {
            let layout_idx = self.module.struct_layouts.len() as u32;
            self.module.struct_layouts.push(StructLayout {
                name: SmolStr::new(format!("__enum_{}", e.name)),
                fields: vec![
                    StructFieldLayout {
                        name: "tag".into(),
                        ty: IrType::I32,
                        offset: 0,
                        type_name: None,
                    },
                    StructFieldLayout {
                        name: "value".into(),
                        ty: IrType::I32,
                        offset: 4,
                        type_name: None,
                    },
                ],
                size: 8,
                field_offsets: vec![0, 4],
            });
            self.struct_name_map
                .insert(SmolStr::new(format!("__enum_{}", e.name)), layout_idx);
        }
    }

    // ── Type lowering (simplified) ──────────────────────────────────────

    fn lower_type_expr(&self, ty: &ast::TypeExpr) -> IrType {
        match &ty.kind {
            ast::TypeExprKind::Primitive(p) => match p {
                ast::PrimitiveType::I8
                | ast::PrimitiveType::I16
                | ast::PrimitiveType::I32
                | ast::PrimitiveType::U8
                | ast::PrimitiveType::U16
                | ast::PrimitiveType::U32 => IrType::I32,
                ast::PrimitiveType::I64
                | ast::PrimitiveType::I128
                | ast::PrimitiveType::U64
                | ast::PrimitiveType::U128 => IrType::I64,
                ast::PrimitiveType::F32 => IrType::F32,
                ast::PrimitiveType::F64 => IrType::F64,
                ast::PrimitiveType::Bool => IrType::Bool,
                ast::PrimitiveType::Char => IrType::I32,
            },
            ast::TypeExprKind::StringType => IrType::Ptr,
            ast::TypeExprKind::Unit => IrType::Unit,
            ast::TypeExprKind::OptionType(_) | ast::TypeExprKind::ResultType(_, _) => IrType::Ptr, // struct-based tagged union
            ast::TypeExprKind::Array(_)
            | ast::TypeExprKind::Map(_, _)
            | ast::TypeExprKind::FnType { .. }
            | ast::TypeExprKind::RefType { .. }
            | ast::TypeExprKind::Tuple(_) => IrType::Ptr,
            ast::TypeExprKind::Named { name, .. } => {
                if self.enum_names.contains_key(name) {
                    IrType::I32
                } else {
                    IrType::Ptr
                }
            }
            ast::TypeExprKind::Error => IrType::I32,
        }
    }

    /// Estimate the byte size of a lowered IR expression's result.
    /// For primitives, uses their natural size; for pointers/structs, 4 bytes (i32 handle).
    fn ir_expr_size(&self, _expr: &IrExpr, _ctx: &FnCtx) -> u32 {
        // For now, all values stored in ref cells are pointer-sized (i32).
        // Primitives (i32, bool, ptr) = 4 bytes, f64/i64 = 8 bytes.
        // We default to 4 since most values are i32/ptr.
        // TODO: track the actual type through lowering for f64/i64 refs.
        4
    }

    /// Map a value size back to an IrType for HeapStore/HeapLoad.
    fn ir_type_for_value_size(&self, size: u32) -> IrType {
        match size {
            8 => IrType::I64,
            _ => IrType::I32,
        }
    }
}

// ---------------------------------------------------------------------------
// Function-local context
// ---------------------------------------------------------------------------

struct FnCtx {
    locals: Vec<IrLocal>,
    name_to_local: HashMap<SmolStr, u32>,
    next_local: u32,
}

impl FnCtx {
    fn new() -> Self {
        Self {
            locals: Vec::new(),
            name_to_local: HashMap::new(),
            next_local: 0,
        }
    }

    fn new_local(&mut self, name: SmolStr, ty: IrType) -> u32 {
        let idx = self.next_local;
        self.next_local += 1;
        self.locals.push(IrLocal {
            name: name.clone(),
            ty,
            index: idx,
        });
        self.name_to_local.insert(name, idx);
        idx
    }

    fn lookup(&self, name: &str) -> Option<u32> {
        self.name_to_local.get(name).copied()
    }

    /// Return the `IrType` of a local by its index.
    fn local_type(&self, idx: u32) -> IrType {
        self.locals
            .iter()
            .find(|l| l.index == idx)
            .map(|l| l.ty)
            .unwrap_or(IrType::I32)
    }
}

// ---------------------------------------------------------------------------
// Statement lowering
// ---------------------------------------------------------------------------

impl Lowerer {
    fn lower_block_stmts(&mut self, block: &ast::Block, ctx: &mut FnCtx) -> Vec<IrStmt> {
        let mut stmts = Vec::new();
        for s in &block.stmts {
            if self.debug_mode {
                let span = self.stmt_span(s);
                let locals: Vec<u32> = ctx.locals.iter().map(|l| l.index).collect();
                stmts.push(IrStmt::DebugProbe { span, locals });
            }
            self.lower_stmt(s, ctx, &mut stmts);
        }
        stmts
    }

    fn stmt_span(&self, s: &Stmt) -> Span {
        match s {
            Stmt::Let(l) => l.span,
            Stmt::Expr(e) => e.span,
            Stmt::Item(_) => Span::dummy(),
            Stmt::Error(sp) => *sp,
        }
    }

    fn lower_stmt(&mut self, s: &Stmt, ctx: &mut FnCtx, out: &mut Vec<IrStmt>) {
        match s {
            Stmt::Let(l) => self.lower_let(l, ctx, out),
            Stmt::Expr(e) => self.lower_expr_stmt(e, ctx, out),
            Stmt::Item(item) => {
                // Nested item (e.g. local fn) — lower as a top-level item for now.
                self.lower_item(item);
            }
            Stmt::Error(_) => {}
        }
    }

    fn lower_let(&mut self, l: &ast::LetStmt, ctx: &mut FnCtx, out: &mut Vec<IrStmt>) {
        // Handle tuple destructuring: let (a, b) = expr;
        if let Pattern::Tuple { elements, .. } = &l.pattern {
            if let Some(init) = &l.init {
                let tuple_expr = self.lower_expr(init, ctx);
                let tmp = ctx.new_local(SmolStr::new("__tuple_tmp"), IrType::Ptr);
                out.push(IrStmt::Let {
                    local: tmp,
                    ty: IrType::Ptr,
                    value: Some(tuple_expr),
                });
                let n = elements.len();
                let tuple_name: SmolStr = SmolStr::new(format!("__Tuple{}_i32", n));
                if !self.struct_name_map.contains_key(&tuple_name) {
                    let mut fields = Vec::new();
                    let mut field_offsets = Vec::new();
                    for i in 0..n {
                        let offset = (i as u32) * 4;
                        field_offsets.push(offset);
                        fields.push(StructFieldLayout {
                            name: SmolStr::new(format!("_{}", i)),
                            ty: IrType::I32,
                            offset,
                            type_name: None,
                        });
                    }
                    let layout_idx = self.module.struct_layouts.len() as u32;
                    self.module.struct_layouts.push(StructLayout {
                        name: tuple_name.clone(),
                        fields,
                        size: (n as u32) * 4,
                        field_offsets,
                    });
                    self.struct_name_map.insert(tuple_name.clone(), layout_idx);
                }
                let layout_idx = *self.struct_name_map.get(&tuple_name).unwrap();
                for (i, pat) in elements.iter().enumerate() {
                    if let Pattern::Ident { name, .. } = pat {
                        let local_idx = ctx.new_local(name.clone(), IrType::I32);
                        out.push(IrStmt::Let {
                            local: local_idx,
                            ty: IrType::I32,
                            value: Some(IrExpr::FieldGet {
                                object: Box::new(IrExpr::LocalGet(tmp)),
                                layout_index: layout_idx,
                                field_index: i as u32,
                            }),
                        });
                    }
                }
            }
            return;
        }

        let name = match &l.pattern {
            Pattern::Ident { name, .. } => name.clone(),
            _ => SmolStr::new("_"),
        };

        // Detect lambda assignment: `let f = |x| ...;`
        // When the initializer is a lambda, lower_expr will create an
        // IrFunction and bump lambda_counter.  We record the variable
        // name as an alias so calls like `f(5)` resolve to the lambda fn.
        let is_lambda_init = matches!(
            l.init.as_ref().map(|e| &e.kind),
            Some(ExprKind::Lambda { .. })
        );
        let lambda_counter_before = self.lambda_counter;

        let ty = if let Some(t) = &l.ty {
            self.lower_type_expr(t)
        } else if let Some(init) = &l.init {
            // Infer type from the initializer when no annotation is present.
            infer_expr_type(init, ctx)
        } else {
            IrType::I32
        };
        let local_idx = ctx.new_local(name.clone(), ty);
        let value = l.init.as_ref().map(|e| self.lower_expr(e, ctx));

        // If a lambda was created during lowering, register the alias.
        if is_lambda_init
            && self.lambda_counter > lambda_counter_before
            && let Some(lambda_fn_name) = self.last_lambda_name()
        {
            self.lambda_aliases.insert(name, lambda_fn_name);
        }

        out.push(IrStmt::Let {
            local: local_idx,
            ty,
            value,
        });
    }

    fn lower_expr_stmt(&mut self, e: &ast::ExprStmt, ctx: &mut FnCtx, out: &mut Vec<IrStmt>) {
        match &e.expr.kind {
            // Assignments are lowered as IrStmt::Assign rather than expressions.
            ExprKind::Assign { op, target, value } => {
                self.lower_assign(op, target, value, ctx, out);
            }
            ExprKind::Return(val) => {
                let v = val.as_ref().map(|v| self.lower_expr(v, ctx));
                out.push(IrStmt::Return(v));
            }
            ExprKind::Break(val) => {
                // If there is a value we emit it as a preceding expr, but
                // for v0.1 we just emit Break.
                let _ = val;
                out.push(IrStmt::Break);
            }
            ExprKind::Continue => {
                out.push(IrStmt::Continue);
            }
            ExprKind::If {
                condition,
                then_block,
                else_block,
            } => {
                let cond = self.lower_expr(condition, ctx);
                let then_body = self.lower_block_stmts(then_block, ctx);
                let else_body = match else_block {
                    Some(els) => self.lower_else_expr(els, ctx),
                    None => Vec::new(),
                };
                out.push(IrStmt::If {
                    condition: cond,
                    then_body,
                    else_body,
                });
            }
            ExprKind::While { condition, body } => {
                // Desugar: loop { if !cond { break }; <body> }
                let cond = self.lower_expr(condition, ctx);
                let break_cond = IrExpr::UnaryOp {
                    op: IrUnaryOp::EqzI32,
                    operand: Box::new(cond),
                };
                let mut loop_body = vec![IrStmt::If {
                    condition: break_cond,
                    then_body: vec![IrStmt::Break],
                    else_body: Vec::new(),
                }];
                loop_body.extend(self.lower_block_stmts(body, ctx));
                out.push(IrStmt::Loop { body: loop_body });
            }
            ExprKind::For {
                pattern,
                iterable,
                body,
            } => {
                let var_name = match pattern {
                    Pattern::Ident { name, .. } => name.clone(),
                    _ => SmolStr::new("_iter"),
                };

                match &iterable.kind {
                    ExprKind::Range {
                        start,
                        end,
                        inclusive,
                    } => {
                        // Desugar: for i in start..end { body }
                        //   =>  let i = start;
                        //       loop { if i >= end { break }; body; i = i + 1; }
                        // For inclusive (..=), use > instead of >=.
                        let start_expr = start
                            .as_ref()
                            .map(|e| self.lower_expr(e, ctx))
                            .unwrap_or(IrExpr::IntConst(0));
                        let end_expr = end
                            .as_ref()
                            .map(|e| self.lower_expr(e, ctx))
                            .unwrap_or(IrExpr::IntConst(0));

                        let local_idx = ctx.new_local(var_name, IrType::I32);
                        out.push(IrStmt::Let {
                            local: local_idx,
                            ty: IrType::I32,
                            value: Some(start_expr),
                        });

                        let cmp_op = if *inclusive {
                            IrBinOp::GtI32S
                        } else {
                            IrBinOp::GeI32S
                        };
                        let break_cond = IrExpr::BinOp {
                            op: cmp_op,
                            lhs: Box::new(IrExpr::LocalGet(local_idx)),
                            rhs: Box::new(end_expr),
                        };
                        let mut loop_body = vec![IrStmt::If {
                            condition: break_cond,
                            then_body: vec![IrStmt::Break],
                            else_body: Vec::new(),
                        }];
                        loop_body.extend(self.lower_block_stmts(body, ctx));
                        loop_body.push(IrStmt::Assign {
                            target: IrLValue::Local(local_idx),
                            value: IrExpr::BinOp {
                                op: IrBinOp::AddI32,
                                lhs: Box::new(IrExpr::LocalGet(local_idx)),
                                rhs: Box::new(IrExpr::IntConst(1)),
                            },
                        });
                        out.push(IrStmt::Loop { body: loop_body });
                    }
                    _ => {
                        // Non-range iterable: placeholder -- emit body as loop.
                        let loop_body = self.lower_block_stmts(body, ctx);
                        out.push(IrStmt::Loop { body: loop_body });
                    }
                }
            }
            ExprKind::Loop { body } => {
                let loop_body = self.lower_block_stmts(body, ctx);
                out.push(IrStmt::Loop { body: loop_body });
            }
            _ => {
                let expr = self.lower_expr(&e.expr, ctx);
                out.push(IrStmt::Expr(expr));
            }
        }
    }

    fn lower_else_expr(&mut self, expr: &ast::Expr, ctx: &mut FnCtx) -> Vec<IrStmt> {
        match &expr.kind {
            ExprKind::Block(block) => self.lower_block_stmts(block, ctx),
            ExprKind::If {
                condition,
                then_block,
                else_block,
            } => {
                let cond = self.lower_expr(condition, ctx);
                let then_body = self.lower_block_stmts(then_block, ctx);
                let else_body = match else_block {
                    Some(els) => self.lower_else_expr(els, ctx),
                    None => Vec::new(),
                };
                vec![IrStmt::If {
                    condition: cond,
                    then_body,
                    else_body,
                }]
            }
            _ => {
                let e = self.lower_expr(expr, ctx);
                vec![IrStmt::Expr(e)]
            }
        }
    }

    fn lower_assign(
        &mut self,
        op: &AssignOp,
        target: &ast::Expr,
        value: &ast::Expr,
        ctx: &mut FnCtx,
        out: &mut Vec<IrStmt>,
    ) {
        let rhs = self.lower_expr(value, ctx);

        // Handle deref assignment: *ref = value → HeapStore
        if let ExprKind::Unary {
            op: UnaryOp::Deref,
            operand,
        } = &target.kind
        {
            let addr = self.lower_expr(operand, ctx);
            // Store as I32 (default for all ref cell values).
            // TODO: track actual inner type for f64/i64 refs.
            out.push(IrStmt::Expr(IrExpr::HeapStore {
                addr: Box::new(addr),
                offset: 0,
                value: Box::new(rhs),
                ty: IrType::I32,
            }));
            return;
        }

        // Field assignment: `obj.field = value` (and compound variants).
        if let ExprKind::FieldAccess { object, field } = &target.kind {
            let mut found = None;
            for (li, layout) in self.module.struct_layouts.iter().enumerate() {
                for (fi, f) in layout.fields.iter().enumerate() {
                    if f.name == *field {
                        found = Some((li as u32, fi as u32));
                        break;
                    }
                }
                if found.is_some() {
                    break;
                }
            }
            if let Some((layout_index, field_index)) = found {
                let obj_ir = self.lower_expr(object, ctx);
                let final_rhs = match op {
                    AssignOp::Assign => rhs,
                    _ => {
                        let lhs_read = IrExpr::FieldGet {
                            object: Box::new(self.lower_expr(object, ctx)),
                            layout_index,
                            field_index,
                        };
                        let bin_op = match op {
                            AssignOp::AddAssign => IrBinOp::AddI32,
                            AssignOp::SubAssign => IrBinOp::SubI32,
                            AssignOp::MulAssign => IrBinOp::MulI32,
                            AssignOp::DivAssign => IrBinOp::DivI32S,
                            AssignOp::RemAssign => IrBinOp::RemI32S,
                            AssignOp::BitAndAssign => IrBinOp::AndI32,
                            AssignOp::BitOrAssign => IrBinOp::OrI32,
                            AssignOp::BitXorAssign => IrBinOp::XorI32,
                            AssignOp::ShlAssign => IrBinOp::ShlI32,
                            AssignOp::ShrAssign => IrBinOp::ShrI32S,
                            AssignOp::Assign => unreachable!(),
                        };
                        IrExpr::BinOp {
                            op: bin_op,
                            lhs: Box::new(lhs_read),
                            rhs: Box::new(rhs),
                        }
                    }
                };
                out.push(IrStmt::Expr(IrExpr::FieldSet {
                    object: Box::new(obj_ir),
                    layout_index,
                    field_index,
                    value: Box::new(final_rhs),
                }));
                return;
            }
        }

        // Resolve target to an IrLValue.
        let lvalue = match &target.kind {
            ExprKind::Ident(name) => {
                if let Some(idx) = ctx.lookup(name) {
                    IrLValue::Local(idx)
                } else if self.global_names.contains_key(name) {
                    IrLValue::Global(name.clone())
                } else {
                    IrLValue::Local(0)
                }
            }
            ExprKind::Path(parts) => {
                // Treat as global for now.
                let full = parts
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join("::");
                IrLValue::Global(SmolStr::new(full))
            }
            _ => {
                // Fallback — emit as Expr.
                let val = self.lower_expr(target, ctx);
                out.push(IrStmt::Expr(val));
                out.push(IrStmt::Expr(rhs));
                return;
            }
        };

        // Check whether the target local is F64.
        let target_is_f64 = match &lvalue {
            IrLValue::Local(idx) => ctx.local_type(*idx) == IrType::F64,
            _ => false,
        };

        // Compound assignment: target = target <op> value
        let final_rhs = match op {
            AssignOp::Assign => rhs,
            AssignOp::AddAssign => self.make_compound(
                &lvalue,
                if target_is_f64 {
                    IrBinOp::AddF64
                } else {
                    IrBinOp::AddI32
                },
                rhs,
            ),
            AssignOp::SubAssign => self.make_compound(
                &lvalue,
                if target_is_f64 {
                    IrBinOp::SubF64
                } else {
                    IrBinOp::SubI32
                },
                rhs,
            ),
            AssignOp::MulAssign => self.make_compound(
                &lvalue,
                if target_is_f64 {
                    IrBinOp::MulF64
                } else {
                    IrBinOp::MulI32
                },
                rhs,
            ),
            AssignOp::DivAssign => self.make_compound(
                &lvalue,
                if target_is_f64 {
                    IrBinOp::DivF64
                } else {
                    IrBinOp::DivI32S
                },
                rhs,
            ),
            AssignOp::RemAssign => self.make_compound(&lvalue, IrBinOp::RemI32S, rhs),
            AssignOp::BitAndAssign => self.make_compound(&lvalue, IrBinOp::AndI32, rhs),
            AssignOp::BitOrAssign => self.make_compound(&lvalue, IrBinOp::OrI32, rhs),
            AssignOp::BitXorAssign => self.make_compound(&lvalue, IrBinOp::XorI32, rhs),
            AssignOp::ShlAssign => self.make_compound(&lvalue, IrBinOp::ShlI32, rhs),
            AssignOp::ShrAssign => self.make_compound(&lvalue, IrBinOp::ShrI32S, rhs),
        };

        out.push(IrStmt::Assign {
            target: lvalue,
            value: final_rhs,
        });
    }

    fn make_compound(&self, lvalue: &IrLValue, op: IrBinOp, rhs: IrExpr) -> IrExpr {
        let lhs = match lvalue {
            IrLValue::Local(idx) => IrExpr::LocalGet(*idx),
            IrLValue::Global(name) => IrExpr::GlobalGet(name.clone()),
            _ => IrExpr::IntConst(0),
        };
        IrExpr::BinOp {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        }
    }
}

// ---------------------------------------------------------------------------
// Expression lowering
// ---------------------------------------------------------------------------

impl Lowerer {
    fn lower_expr(&mut self, expr: &ast::Expr, ctx: &mut FnCtx) -> IrExpr {
        match &expr.kind {
            // ── Literals ────────────────────────────────────────────────
            ExprKind::IntLit(v) => IrExpr::IntConst(*v as i64),
            ExprKind::FloatLit(v) => IrExpr::FloatConst(*v),
            ExprKind::BoolLit(v) => IrExpr::BoolConst(*v),
            ExprKind::CharLit(c) => IrExpr::IntConst(*c as i64),
            ExprKind::StringLit(s) => {
                let idx = self.intern_string(s.clone());
                IrExpr::StringConst(idx)
            }
            ExprKind::UnitLit => IrExpr::IntConst(0),

            // ── Names ───────────────────────────────────────────────────
            ExprKind::Ident(name) => {
                // Built-in: `None` → __Option { tag: 0, value: 0 }
                if name.as_str() == "None"
                    && let Some(&layout_idx) = self.struct_name_map.get("__Option")
                {
                    return IrExpr::StructNew {
                        layout_index: layout_idx,
                        fields: vec![IrExpr::IntConst(0), IrExpr::IntConst(0)],
                    };
                }
                // Constant folding: substitute compile-time const values.
                if let Some(val) = self.const_values.get(name).cloned() {
                    return val;
                }
                if let Some(idx) = ctx.lookup(name) {
                    IrExpr::LocalGet(idx)
                } else if self.global_names.contains_key(name) {
                    IrExpr::GlobalGet(name.clone())
                } else {
                    // Could be a global or a function reference; emit as a
                    // zero-arg call placeholder for now.
                    IrExpr::Call {
                        func: name.clone(),
                        args: Vec::new(),
                    }
                }
            }
            ExprKind::Path(parts) => {
                let full: SmolStr = SmolStr::new(
                    parts
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join("::"),
                );
                // Check if this path refers to an enum variant.
                if let Some(&tag) = self.enum_variant_tags.get(&full) {
                    IrExpr::IntConst(tag as i64)
                } else {
                    // Try mangled impl method name (e.g. Point::new → Point__new).
                    let func = if parts.len() == 2 {
                        let mangled = SmolStr::new(format!("{}__{}", parts[0], parts[1]));
                        if self.fn_name_map.contains_key(&mangled) {
                            mangled
                        } else {
                            full
                        }
                    } else {
                        full
                    };
                    IrExpr::Call {
                        func,
                        args: Vec::new(),
                    }
                }
            }

            // ── Binary operations ───────────────────────────────────────
            ExprKind::Binary { op, lhs, rhs } => {
                // String concatenation: if either operand is a string and op is Add.
                if *op == BinOp::Add
                    && (self.is_string_expr(lhs, ctx) || self.is_string_expr(rhs, ctx))
                {
                    let l = self.lower_expr(lhs, ctx);
                    let r = self.lower_expr(rhs, ctx);
                    return IrExpr::HostCall {
                        module: "env".into(),
                        name: "__str_concat".into(),
                        args: vec![l, r],
                        ret: IrType::Ptr,
                    };
                }
                // String equality.
                if *op == BinOp::Eq
                    && (self.is_string_expr(lhs, ctx) || self.is_string_expr(rhs, ctx))
                {
                    let l = self.lower_expr(lhs, ctx);
                    let r = self.lower_expr(rhs, ctx);
                    return IrExpr::HostCall {
                        module: "env".into(),
                        name: "__str_eq".into(),
                        args: vec![l, r],
                        ret: IrType::I32,
                    };
                }
                let use_f64 = is_f64_context(lhs, rhs, ctx);
                let l = self.lower_expr(lhs, ctx);
                let r = self.lower_expr(rhs, ctx);
                IrExpr::BinOp {
                    op: if use_f64 {
                        lower_binop_f64(*op)
                    } else {
                        lower_binop(*op)
                    },
                    lhs: Box::new(l),
                    rhs: Box::new(r),
                }
            }

            // ── Unary operations ────────────────────────────────────────
            ExprKind::Unary { op, operand } => {
                match op {
                    // Reference creation: &x or &mut x
                    // Allocate a ref cell [rc:i32 | data:N] and store the value
                    UnaryOp::Ref | UnaryOp::RefMut => {
                        let inner = self.lower_expr(operand, ctx);
                        let value_size = self.ir_expr_size(&inner, ctx);
                        let total_size = 4 + value_size; // refcount header + data
                        let alloc_local = ctx.new_local("__ref_alloc".into(), IrType::Ptr);
                        let data_local = ctx.new_local("__ref_data".into(), IrType::Ptr);
                        // Sequence: alloc, store rc=1, store value, return data ptr
                        IrExpr::Block {
                            stmts: vec![
                                // alloc_local = HeapAlloc(total_size)
                                IrStmt::Let {
                                    local: alloc_local,
                                    ty: IrType::Ptr,
                                    value: Some(IrExpr::HeapAlloc { size: total_size }),
                                },
                                // data_local = alloc_local + 4
                                IrStmt::Let {
                                    local: data_local,
                                    ty: IrType::Ptr,
                                    value: Some(IrExpr::BinOp {
                                        op: IrBinOp::AddI32,
                                        lhs: Box::new(IrExpr::LocalGet(alloc_local)),
                                        rhs: Box::new(IrExpr::IntConst(4)),
                                    }),
                                },
                                // Store refcount = 1 at alloc_local
                                IrStmt::Expr(IrExpr::HeapStore {
                                    addr: Box::new(IrExpr::LocalGet(alloc_local)),
                                    offset: 0,
                                    value: Box::new(IrExpr::IntConst(1)),
                                    ty: IrType::I32,
                                }),
                                // Store value at data_local
                                IrStmt::Expr(IrExpr::HeapStore {
                                    addr: Box::new(IrExpr::LocalGet(data_local)),
                                    offset: 0,
                                    value: Box::new(inner),
                                    ty: self.ir_type_for_value_size(value_size),
                                }),
                            ],
                            result: Box::new(IrExpr::LocalGet(data_local)),
                        }
                    }
                    // Dereference: *ref
                    UnaryOp::Deref => {
                        let inner = self.lower_expr(operand, ctx);
                        // The operand is a pointer (Ptr) to a ref cell.
                        // The dereferenced value type defaults to I32 for now
                        // (i32, bool, ptr all stored as 4 bytes).
                        // TODO: track actual inner type for f64/i64 refs.
                        IrExpr::HeapLoad {
                            addr: Box::new(inner),
                            offset: 0,
                            ty: IrType::I32,
                        }
                    }
                    _ => {
                        let use_f64 = infer_expr_type(operand, ctx) == IrType::F64;
                        let inner = self.lower_expr(operand, ctx);
                        match op {
                            // Integer negation: lower as `0 - x` since WASM has no i32.neg.
                            UnaryOp::Neg if !use_f64 => IrExpr::BinOp {
                                op: IrBinOp::SubI32,
                                lhs: Box::new(IrExpr::IntConst(0)),
                                rhs: Box::new(inner),
                            },
                            _ => {
                                let ir_op = if use_f64 && *op == UnaryOp::Neg {
                                    IrUnaryOp::NegF64
                                } else {
                                    lower_unaryop(*op)
                                };
                                IrExpr::UnaryOp {
                                    op: ir_op,
                                    operand: Box::new(inner),
                                }
                            }
                        }
                    }
                }
            }

            // ── Assignment as expression ────────────────────────────────
            ExprKind::Assign { target, value, .. } => {
                let rhs = self.lower_expr(value, ctx);
                match &target.kind {
                    ExprKind::Ident(name) => {
                        let idx = ctx.lookup(name).unwrap_or(0);
                        IrExpr::LocalSet(idx, Box::new(rhs))
                    }
                    // *ref = value → HeapStore into the ref cell
                    ExprKind::Unary {
                        op: UnaryOp::Deref,
                        operand,
                    } => {
                        let addr = self.lower_expr(operand, ctx);
                        IrExpr::HeapStore {
                            addr: Box::new(addr),
                            offset: 0,
                            value: Box::new(rhs),
                            ty: IrType::I32,
                        }
                    }
                    _ => rhs, // fallback
                }
            }

            // ── Function calls ──────────────────────────────────────────
            ExprKind::Call { callee, args } => {
                let func_name = self.callee_name(callee);

                // Built-in constructors: Some(v), Ok(v), Err(e)
                if func_name == "Some" && args.len() == 1 {
                    let val = self.lower_expr(&args[0].value, ctx);
                    if let Some(&layout_idx) = self.struct_name_map.get("__Option") {
                        return IrExpr::StructNew {
                            layout_index: layout_idx,
                            fields: vec![IrExpr::IntConst(1), val],
                        };
                    }
                }
                if func_name == "Ok" && args.len() == 1 {
                    let val = self.lower_expr(&args[0].value, ctx);
                    if let Some(&layout_idx) = self.struct_name_map.get("__Result") {
                        return IrExpr::StructNew {
                            layout_index: layout_idx,
                            fields: vec![IrExpr::IntConst(0), val],
                        };
                    }
                }
                if func_name == "Err" && args.len() == 1 {
                    let val = self.lower_expr(&args[0].value, ctx);
                    if let Some(&layout_idx) = self.struct_name_map.get("__Result") {
                        return IrExpr::StructNew {
                            layout_index: layout_idx,
                            fields: vec![IrExpr::IntConst(1), val],
                        };
                    }
                }

                // Data-carrying enum variant constructor: e.g. Shape::Circle(5)
                // The func_name will be "Shape::Circle".  Check if we have a
                // variant tag and a struct layout for it.
                if let Some(&tag) = self.enum_variant_tags.get(&func_name) {
                    // Extract enum name from "EnumName::VariantName"
                    if let Some(pos) = func_name.find("::") {
                        let enum_name = &func_name[..pos];
                        let layout_key = SmolStr::new(format!("__enum_{}", enum_name));
                        if let Some(&layout_idx) = self.struct_name_map.get(&layout_key) {
                            // For v0.1: single payload field
                            let val = if !args.is_empty() {
                                self.lower_expr(&args[0].value, ctx)
                            } else {
                                IrExpr::IntConst(0)
                            };
                            return IrExpr::StructNew {
                                layout_index: layout_idx,
                                fields: vec![IrExpr::IntConst(tag as i64), val],
                            };
                        }
                    }
                }

                let ir_args: Vec<IrExpr> = args
                    .iter()
                    .map(|a| self.lower_expr(&a.value, ctx))
                    .collect();

                // Built-in: com_* → __com_* host calls (Windows COM bridge).
                if let Some((host_name, ret)) = map_com_builtin(func_name.as_str(), ir_args.len()) {
                    return IrExpr::HostCall {
                        module: "env".into(),
                        name: host_name,
                        args: ir_args,
                        ret,
                    };
                }

                // Built-in: print(expr)
                if func_name == "print" && ir_args.len() == 1 {
                    let is_string = self.is_string_expr(&args[0].value, ctx);
                    let host_name: SmolStr = if is_string {
                        "__str_print".into()
                    } else {
                        "__print_i32".into()
                    };
                    return IrExpr::HostCall {
                        module: "env".into(),
                        name: host_name,
                        args: ir_args,
                        ret: IrType::Unit,
                    };
                }

                // Append captured locals as extra args for lambda calls.
                let mut final_args = ir_args;
                if let Some(captures) = self.lambda_captures.get(&func_name) {
                    for cap_name in captures.clone() {
                        if let Some(idx) = ctx.lookup(&cap_name) {
                            final_args.push(IrExpr::LocalGet(idx));
                        }
                    }
                }

                if let Some((_params, ret)) = self.user_host_fns.get(&func_name).cloned() {
                    return IrExpr::HostCall {
                        module: "host".into(),
                        name: func_name,
                        args: final_args,
                        ret,
                    };
                }

                IrExpr::Call {
                    func: func_name,
                    args: final_args,
                }
            }

            // ── Method calls → Call with receiver as first arg ──────────
            ExprKind::MethodCall {
                object,
                method,
                args,
            } => {
                // Pipeline operations: .filter(closure), .map(closure),
                // .for_each(closure), .collect()
                match method.as_str() {
                    "filter" if args.len() == 1 => {
                        return self.lower_pipeline_filter(object, &args[0].value, ctx);
                    }
                    "map" if args.len() == 1 => {
                        return self.lower_pipeline_map(object, &args[0].value, ctx);
                    }
                    "for_each" if args.len() == 1 => {
                        return self.lower_pipeline_for_each(object, &args[0].value, ctx);
                    }
                    "collect" if args.is_empty() => {
                        // .collect() on an array is a no-op (already an array).
                        return self.lower_expr(object, ctx);
                    }
                    _ => {}
                }
                // String method calls: .len() → __str_len host call.
                if self.is_string_expr(object, ctx) {
                    let receiver = self.lower_expr(object, ctx);
                    let mut ir_args = vec![receiver];
                    ir_args.extend(args.iter().map(|a| self.lower_expr(&a.value, ctx)));
                    let host_name: SmolStr = match method.as_str() {
                        "len" => "__str_len".into(),
                        "contains" => "__str_contains".into(),
                        "starts_with" => "__str_starts_with".into(),
                        "ends_with" => "__str_ends_with".into(),
                        "trim" => "__str_trim".into(),
                        "to_uppercase" => "__str_to_upper".into(),
                        "to_lowercase" => "__str_to_lower".into(),
                        "replace" => "__str_replace".into(),
                        "split" => "__str_split".into(),
                        "char_count" => "__str_char_count".into(),
                        "is_empty" => "__str_is_empty".into(),
                        "repeat" => "__str_repeat".into(),
                        _ => method.clone(),
                    };
                    if host_name.starts_with("__str_") {
                        return IrExpr::HostCall {
                            module: "env".into(),
                            name: host_name,
                            args: ir_args,
                            ret: IrType::I32,
                        };
                    }
                }
                // Array method calls: .sum(), .len(), .push(), .contains(), .reverse()
                // These are dispatched by method name when the object is not a string.
                // String .len() is already handled above; array-specific methods
                // (sum, contains, reverse, push) never conflict with strings.
                {
                    let host_name: SmolStr = match method.as_str() {
                        "sum" => "__arr_sum".into(),
                        "contains" => "__arr_contains".into(),
                        "reverse" => "__arr_reverse".into(),
                        "push" => "__arr_push".into(),
                        "first" => "__arr_first".into(),
                        "last" => "__arr_last".into(),
                        "min" => "__arr_min".into(),
                        "max" => "__arr_max".into(),
                        "sort" => "__arr_sort".into(),
                        "dedup" => "__arr_dedup".into(),
                        "join" => "__arr_join_str".into(),
                        // .count() is just .len()
                        "count" => "__arr_len".into(),
                        // .len() on non-string objects → array len
                        "len" if !self.is_string_expr(object, ctx) => "__arr_len".into(),
                        _ => SmolStr::default(),
                    };
                    if !host_name.is_empty() {
                        let receiver = self.lower_expr(object, ctx);
                        let mut ir_args = vec![receiver];
                        ir_args.extend(args.iter().map(|a| self.lower_expr(&a.value, ctx)));
                        let ret = if method.as_str() == "push" {
                            IrType::Unit
                        } else {
                            IrType::I32
                        };
                        return IrExpr::HostCall {
                            module: "env".into(),
                            name: host_name,
                            args: ir_args,
                            ret,
                        };
                    }
                }
                let mut ir_args = vec![self.lower_expr(object, ctx)];
                ir_args.extend(args.iter().map(|a| self.lower_expr(&a.value, ctx)));
                // Resolve the method name via the impl method map.
                let func_name = self
                    .method_map
                    .get(method)
                    .cloned()
                    .unwrap_or_else(|| method.clone());
                IrExpr::Call {
                    func: func_name,
                    args: ir_args,
                }
            }

            // ── If as expression ────────────────────────────────────────
            ExprKind::If {
                condition,
                then_block,
                else_block,
            } => {
                let cond = self.lower_expr(condition, ctx);
                let then_expr = self.lower_block_as_expr(then_block, ctx);
                let else_expr = match else_block {
                    Some(e) => self.lower_expr(e, ctx),
                    None => IrExpr::IntConst(0),
                };
                IrExpr::IfExpr {
                    condition: Box::new(cond),
                    then_expr: Box::new(then_expr),
                    else_expr: Box::new(else_expr),
                }
            }

            // ── Block expression ────────────────────────────────────────
            ExprKind::Block(block) => self.lower_block_as_expr(block, ctx),

            // ── While / For / Loop as expression → int 0 ────────────────
            ExprKind::While { .. } | ExprKind::For { .. } | ExprKind::Loop { .. } => {
                // These are typically used as statements.  When used as
                // expressions they produce unit.
                IrExpr::IntConst(0)
            }

            // ── Return ──────────────────────────────────────────────────
            ExprKind::Return(val) => {
                // Return is mostly handled at stmt level, but can appear in
                // expression position.
                let v = val.as_ref().map(|v| self.lower_expr(v, ctx));
                // We emit a Block with a Return inside.
                IrExpr::Block {
                    stmts: vec![IrStmt::Return(v)],
                    result: Box::new(IrExpr::IntConst(0)),
                }
            }

            // ── Break / Continue ────────────────────────────────────────
            ExprKind::Break(_) => IrExpr::Block {
                stmts: vec![IrStmt::Break],
                result: Box::new(IrExpr::IntConst(0)),
            },
            ExprKind::Continue => IrExpr::Block {
                stmts: vec![IrStmt::Continue],
                result: Box::new(IrExpr::IntConst(0)),
            },

            // ── Field access ──────────────────────────────────────────────
            ExprKind::FieldAccess { object, field } => {
                let obj = self.lower_expr(object, ctx);
                // Try to find the struct layout that contains this field.
                // Heuristic: scan all layouts for a matching field name.
                let mut found = None;
                for (li, layout) in self.module.struct_layouts.iter().enumerate() {
                    for (fi, f) in layout.fields.iter().enumerate() {
                        if f.name == *field {
                            found = Some((li as u32, fi as u32));
                            break;
                        }
                    }
                    if found.is_some() {
                        break;
                    }
                }
                if let Some((layout_index, field_index)) = found {
                    IrExpr::FieldGet {
                        object: Box::new(obj),
                        layout_index,
                        field_index,
                    }
                } else {
                    // Unknown field — just return the object expression.
                    obj
                }
            }

            ExprKind::Index { object, index } => {
                let obj = self.lower_expr(object, ctx);
                let idx = self.lower_expr(index, ctx);
                IrExpr::HostCall {
                    module: "env".into(),
                    name: "__arr_get".into(),
                    args: vec![obj, idx],
                    ret: IrType::I32,
                }
            }

            ExprKind::TupleIndex { object, index } => {
                let obj = self.lower_expr(object, ctx);
                // Look up the tuple struct layout by scanning for __TupleN_i32 layouts.
                // The field index corresponds directly to the tuple index.
                let field_name = SmolStr::new(format!("_{}", index));
                let mut found = None;
                for (li, layout) in self.module.struct_layouts.iter().enumerate() {
                    if layout.name.starts_with("__Tuple") {
                        for (fi, f) in layout.fields.iter().enumerate() {
                            if f.name == field_name {
                                found = Some((li as u32, fi as u32));
                                break;
                            }
                        }
                        if found.is_some() {
                            break;
                        }
                    }
                }
                if let Some((layout_index, field_index)) = found {
                    IrExpr::FieldGet {
                        object: Box::new(obj),
                        layout_index,
                        field_index,
                    }
                } else {
                    // Fallback: just return the object
                    obj
                }
            }

            // ── Cast ────────────────────────────────────────────────────
            ExprKind::Cast {
                expr: cast_expr,
                ty,
            } => {
                let from = infer_expr_type(cast_expr, ctx);
                let to = self.lower_type_expr(ty);
                let inner = self.lower_expr(cast_expr, ctx);
                if from == to {
                    // Same type — no-op.
                    inner
                } else {
                    // Determine the appropriate conversion unary op.
                    let conv_op = match (from, to) {
                        (IrType::I32, IrType::F64) => Some(IrUnaryOp::ConvertI32SToF64),
                        (IrType::F64, IrType::I32) => Some(IrUnaryOp::TruncF64ToI32S),
                        (IrType::I32, IrType::I64) => Some(IrUnaryOp::ExtendI32SToI64),
                        (IrType::I64, IrType::I32) => Some(IrUnaryOp::WrapI64ToI32),
                        (IrType::I32, IrType::F32) => Some(IrUnaryOp::ConvertI32SToF32),
                        (IrType::F32, IrType::I32) => Some(IrUnaryOp::TruncF32ToI32S),
                        (IrType::F32, IrType::F64) => Some(IrUnaryOp::PromoteF32ToF64),
                        (IrType::F64, IrType::F32) => Some(IrUnaryOp::DemoteF64ToF32),
                        _ => None,
                    };
                    if let Some(op) = conv_op {
                        IrExpr::UnaryOp {
                            op,
                            operand: Box::new(inner),
                        }
                    } else {
                        // Fallback: emit a Cast node for codegen to handle.
                        IrExpr::Cast {
                            expr: Box::new(inner),
                            from,
                            to,
                        }
                    }
                }
            }

            // ── Pipe (desugar: lhs |> rhs  →  rhs(lhs)) ────────────────
            ExprKind::Pipe { lhs, rhs } => {
                // Check if the RHS is a call to a pipeline method (filter, map, etc.)
                // `lhs |> filter(closure)` desugars like `lhs.filter(closure)`
                match &rhs.kind {
                    ExprKind::Call { callee, args } => {
                        let name = self.callee_name(callee);
                        match name.as_str() {
                            "filter" if args.len() == 1 => {
                                self.lower_pipeline_filter(lhs, &args[0].value, ctx)
                            }
                            "map" if args.len() == 1 => {
                                self.lower_pipeline_map(lhs, &args[0].value, ctx)
                            }
                            "for_each" if args.len() == 1 => {
                                self.lower_pipeline_for_each(lhs, &args[0].value, ctx)
                            }
                            "collect" if args.is_empty() => self.lower_expr(lhs, ctx),
                            "any" if args.len() == 1 => {
                                self.lower_pipeline_any(lhs, &args[0].value, ctx)
                            }
                            "all" if args.len() == 1 => {
                                self.lower_pipeline_all(lhs, &args[0].value, ctx)
                            }
                            "find" if args.len() == 1 => {
                                self.lower_pipeline_find(lhs, &args[0].value, ctx)
                            }
                            "reduce" if args.len() == 1 => {
                                self.lower_pipeline_reduce(lhs, &args[0].value, ctx)
                            }
                            "fold" if args.len() == 2 => {
                                self.lower_pipeline_fold(lhs, &args[0].value, &args[1].value, ctx)
                            }
                            "take" if args.len() == 1 => {
                                self.lower_pipeline_take(lhs, &args[0].value, ctx)
                            }
                            "skip" if args.len() == 1 => {
                                self.lower_pipeline_skip(lhs, &args[0].value, ctx)
                            }
                            _ => {
                                // Generic pipe: lhs |> f(args) → f(lhs, args...)
                                let mut ir_args = vec![self.lower_expr(lhs, ctx)];
                                ir_args.extend(args.iter().map(|a| self.lower_expr(&a.value, ctx)));
                                IrExpr::Call {
                                    func: name,
                                    args: ir_args,
                                }
                            }
                        }
                    }
                    _ => {
                        // Simple pipe: lhs |> f → f(lhs)
                        let arg = self.lower_expr(lhs, ctx);
                        let func_name = self.callee_name(rhs);
                        IrExpr::Call {
                            func: func_name,
                            args: vec![arg],
                        }
                    }
                }
            }

            // ── Lambda ─────────────────────────────────────────────────────
            ExprKind::Lambda {
                params,
                return_type,
                body,
            } => self.lower_lambda(params, return_type.as_deref(), body, ctx),

            // ── Template literal with interpolation ──────────────────────
            ExprKind::TemplateLit(segments) => {
                let mut parts: Vec<IrExpr> = Vec::new();
                for seg in segments {
                    match seg {
                        ast::TemplateSegment::Literal(s) => {
                            let idx = self.intern_string(s.clone());
                            parts.push(IrExpr::StringConst(idx));
                        }
                        ast::TemplateSegment::Expr(e) => {
                            let inner = self.lower_expr(e, ctx);
                            if self.is_string_expr(e, ctx) {
                                parts.push(inner);
                            } else {
                                parts.push(IrExpr::HostCall {
                                    module: "env".into(),
                                    name: "__i32_to_str".into(),
                                    args: vec![inner],
                                    ret: IrType::I32,
                                });
                            }
                        }
                    }
                }
                if parts.is_empty() {
                    let idx = self.intern_string(String::new());
                    IrExpr::StringConst(idx)
                } else {
                    let mut acc = parts.remove(0);
                    for part in parts {
                        acc = IrExpr::HostCall {
                            module: "env".into(),
                            name: "__str_concat".into(),
                            args: vec![acc, part],
                            ret: IrType::Ptr,
                        };
                    }
                    acc
                }
            }

            // ── Array literals ───────────────────────────────────────────
            ExprKind::ArrayLit(elements) => {
                let new_arr = IrExpr::HostCall {
                    module: "env".into(),
                    name: "__arr_new".into(),
                    args: vec![],
                    ret: IrType::I32,
                };
                if elements.is_empty() {
                    return new_arr;
                }
                let arr_local = ctx.new_local(SmolStr::new("__arr_tmp"), IrType::I32);
                let mut stmts = vec![IrStmt::Let {
                    local: arr_local,
                    ty: IrType::I32,
                    value: Some(new_arr),
                }];
                for elem in elements {
                    let val = self.lower_expr(elem, ctx);
                    stmts.push(IrStmt::Expr(IrExpr::HostCall {
                        module: "env".into(),
                        name: "__arr_push".into(),
                        args: vec![IrExpr::LocalGet(arr_local), val],
                        ret: IrType::Unit,
                    }));
                }
                IrExpr::Block {
                    stmts,
                    result: Box::new(IrExpr::LocalGet(arr_local)),
                }
            }

            // ── Map literals ───────────────────────────────────────────────
            ExprKind::MapLit(entries) => {
                let new_map = IrExpr::HostCall {
                    module: "env".into(),
                    name: "__map_new".into(),
                    args: vec![],
                    ret: IrType::I32,
                };
                if entries.is_empty() {
                    return new_map;
                }
                let map_local = ctx.new_local(SmolStr::new("__map_tmp"), IrType::I32);
                let mut stmts = vec![IrStmt::Let {
                    local: map_local,
                    ty: IrType::I32,
                    value: Some(new_map),
                }];
                for (key_expr, val_expr) in entries {
                    let key = self.lower_expr(key_expr, ctx);
                    let val = self.lower_expr(val_expr, ctx);
                    stmts.push(IrStmt::Expr(IrExpr::HostCall {
                        module: "env".into(),
                        name: "__map_set".into(),
                        args: vec![IrExpr::LocalGet(map_local), key, val],
                        ret: IrType::Unit,
                    }));
                }
                IrExpr::Block {
                    stmts,
                    result: Box::new(IrExpr::LocalGet(map_local)),
                }
            }

            // ── Tuple literals ──────────────────────────────────────────────
            ExprKind::TupleLit(elements) => {
                // Register a tuple struct layout if not already present.
                let n = elements.len();
                let tuple_name: SmolStr = SmolStr::new(format!("__Tuple{}_i32", n));
                if !self.struct_name_map.contains_key(&tuple_name) {
                    let mut fields = Vec::new();
                    let mut field_offsets = Vec::new();
                    for i in 0..n {
                        let offset = (i as u32) * 4;
                        field_offsets.push(offset);
                        fields.push(StructFieldLayout {
                            name: SmolStr::new(format!("_{}", i)),
                            ty: IrType::I32,
                            offset,
                            type_name: None,
                        });
                    }
                    let layout_idx = self.module.struct_layouts.len() as u32;
                    self.module.struct_layouts.push(StructLayout {
                        name: tuple_name.clone(),
                        fields,
                        size: (n as u32) * 4,
                        field_offsets,
                    });
                    self.struct_name_map.insert(tuple_name.clone(), layout_idx);
                }
                let layout_idx = *self.struct_name_map.get(&tuple_name).unwrap();
                let ir_fields: Vec<IrExpr> =
                    elements.iter().map(|e| self.lower_expr(e, ctx)).collect();
                IrExpr::StructNew {
                    layout_index: layout_idx,
                    fields: ir_fields,
                }
            }

            // ── Struct init ───────────────────────────────────────────────
            ExprKind::StructInit { name, fields } => {
                if let Some(&layout_idx) = self.struct_name_map.get(name) {
                    let layout = &self.module.struct_layouts[layout_idx as usize];
                    // Build field values in layout order.
                    let mut ir_fields: Vec<IrExpr> = Vec::new();
                    for layout_field in &layout.fields.clone() {
                        let val = fields
                            .iter()
                            .find(|fi| fi.name == layout_field.name)
                            .and_then(|fi| fi.value.as_ref())
                            .map(|e| self.lower_expr(e, ctx))
                            .unwrap_or_else(|| {
                                // Shorthand: `Point { x }` means `Point { x: x }`
                                if let Some(idx) = ctx.lookup(&layout_field.name) {
                                    IrExpr::LocalGet(idx)
                                } else {
                                    IrExpr::IntConst(0)
                                }
                            });
                        ir_fields.push(val);
                    }
                    IrExpr::StructNew {
                        layout_index: layout_idx,
                        fields: ir_fields,
                    }
                } else {
                    // Unknown struct — fall back to 0.
                    IrExpr::IntConst(0)
                }
            }

            // ── Match expression ───────────────────────────────────────────
            ExprKind::Match { scrutinee, arms } => self.lower_match(scrutinee, arms, ctx),

            // ── Error propagation (?  operator) ──────────────────────────
            // Lower `expr?` on a Result<T,E>:
            //   let tmp = expr;
            //   if tmp.tag == 1 { return tmp; }  // Err → propagate
            //   tmp.value                        // Ok → unwrap
            ExprKind::ErrorPropagate(inner) => {
                let result_expr = self.lower_expr(inner, ctx);
                let tmp = ctx.new_local(SmolStr::new("__err_prop"), IrType::Ptr);

                if let Some(&layout_idx) = self.struct_name_map.get("__Result") {
                    let set_tmp = IrStmt::Let {
                        local: tmp,
                        ty: IrType::Ptr,
                        value: Some(result_expr),
                    };
                    // Check tag: tmp.tag (field 0)
                    let tag_get = IrExpr::FieldGet {
                        object: Box::new(IrExpr::LocalGet(tmp)),
                        layout_index: layout_idx,
                        field_index: 0,
                    };
                    let is_err = IrExpr::BinOp {
                        op: IrBinOp::EqI32,
                        lhs: Box::new(tag_get),
                        rhs: Box::new(IrExpr::IntConst(1)),
                    };
                    // If Err, return the whole Result struct (early return).
                    let early_return = IrStmt::If {
                        condition: is_err,
                        then_body: vec![IrStmt::Return(Some(IrExpr::LocalGet(tmp)))],
                        else_body: Vec::new(),
                    };
                    // Unwrap: tmp.value (field 1)
                    let unwrap = IrExpr::FieldGet {
                        object: Box::new(IrExpr::LocalGet(tmp)),
                        layout_index: layout_idx,
                        field_index: 1,
                    };
                    IrExpr::Block {
                        stmts: vec![set_tmp, early_return],
                        result: Box::new(unwrap),
                    }
                } else {
                    // Fallback: just pass through
                    result_expr
                }
            }

            // ── Range (placeholder) ─────────────────────────────────────
            ExprKind::Range { .. } => IrExpr::IntConst(0),

            // ── Macro calls ──────────────────────────────────────────────
            ExprKind::MacroCall { name, args } => {
                match name.as_str() {
                    "assert" => {
                        // assert!(condition) → if !condition { __panic() }
                        if let Some(cond_expr) = args.first() {
                            let cond = self.lower_expr(cond_expr, ctx);
                            let not_cond = IrExpr::UnaryOp {
                                op: IrUnaryOp::EqzI32,
                                operand: Box::new(cond),
                            };
                            let panic_call = IrExpr::HostCall {
                                module: "env".into(),
                                name: "__panic".into(),
                                args: vec![IrExpr::IntConst(0)],
                                ret: IrType::Unit,
                            };
                            IrExpr::Block {
                                stmts: vec![IrStmt::If {
                                    condition: not_cond,
                                    then_body: vec![IrStmt::Expr(panic_call)],
                                    else_body: Vec::new(),
                                }],
                                result: Box::new(IrExpr::IntConst(0)),
                            }
                        } else {
                            IrExpr::IntConst(0)
                        }
                    }
                    "assert_eq" => {
                        // assert_eq!(a, b) → if a != b { __panic() }
                        if args.len() >= 2 {
                            let a = self.lower_expr(&args[0], ctx);
                            let b = self.lower_expr(&args[1], ctx);
                            let ne = IrExpr::BinOp {
                                op: IrBinOp::NeI32,
                                lhs: Box::new(a),
                                rhs: Box::new(b),
                            };
                            let panic_call = IrExpr::HostCall {
                                module: "env".into(),
                                name: "__panic".into(),
                                args: vec![IrExpr::IntConst(0)],
                                ret: IrType::Unit,
                            };
                            IrExpr::Block {
                                stmts: vec![IrStmt::If {
                                    condition: ne,
                                    then_body: vec![IrStmt::Expr(panic_call)],
                                    else_body: Vec::new(),
                                }],
                                result: Box::new(IrExpr::IntConst(0)),
                            }
                        } else {
                            IrExpr::IntConst(0)
                        }
                    }
                    "dbg" => {
                        // dbg!(expr) → evaluate, print, return value
                        if let Some(arg_expr) = args.first() {
                            let val = self.lower_expr(arg_expr, ctx);
                            let tmp = ctx.new_local(SmolStr::new("__dbg_tmp"), IrType::I32);
                            let print_call = IrExpr::HostCall {
                                module: "env".into(),
                                name: "__print_i32".into(),
                                args: vec![IrExpr::LocalGet(tmp)],
                                ret: IrType::Unit,
                            };
                            IrExpr::Block {
                                stmts: vec![
                                    IrStmt::Let {
                                        local: tmp,
                                        ty: IrType::I32,
                                        value: Some(val),
                                    },
                                    IrStmt::Expr(print_call),
                                ],
                                result: Box::new(IrExpr::LocalGet(tmp)),
                            }
                        } else {
                            IrExpr::IntConst(0)
                        }
                    }
                    "todo" => {
                        // todo!() → __panic("not yet implemented")
                        let msg_idx = self.intern_string("not yet implemented".to_string());
                        IrExpr::HostCall {
                            module: "env".into(),
                            name: "__panic".into(),
                            args: vec![IrExpr::StringConst(msg_idx)],
                            ret: IrType::Unit,
                        }
                    }
                    "unreachable" => {
                        // unreachable!() → __panic("unreachable")
                        let msg_idx = self.intern_string("unreachable".to_string());
                        IrExpr::HostCall {
                            module: "env".into(),
                            name: "__panic".into(),
                            args: vec![IrExpr::StringConst(msg_idx)],
                            ret: IrType::Unit,
                        }
                    }
                    _ => IrExpr::IntConst(0),
                }
            }

            // ── IfLet / WhileLet ────────────────────────────────────────
            ExprKind::IfLet {
                pattern,
                expr,
                then_block,
                else_block,
            } => {
                // if let Some(v) = expr { then } else { else }
                // → let tmp = expr; if tmp.tag == 1 { let v = tmp.value; then } else { else }
                let scrutinee = self.lower_expr(expr, ctx);
                let tmp = ctx.new_local(SmolStr::new("__iflet_tmp"), IrType::Ptr);
                let mut stmts = vec![IrStmt::Let {
                    local: tmp,
                    ty: IrType::Ptr,
                    value: Some(scrutinee),
                }];

                // Determine the layout (Option by default for Some/None patterns).
                let layout_idx = self.struct_name_map.get("__Option").copied().unwrap_or(0);
                let tag_get = IrExpr::FieldGet {
                    object: Box::new(IrExpr::LocalGet(tmp)),
                    layout_index: layout_idx,
                    field_index: 0,
                };
                let is_some = IrExpr::BinOp {
                    op: IrBinOp::EqI32,
                    lhs: Box::new(tag_get),
                    rhs: Box::new(IrExpr::IntConst(1)),
                };

                // Bind the payload variable in the then branch.
                let mut then_stmts = Vec::new();
                if let Pattern::EnumVariant { fields, .. } = pattern {
                    for (fi, field_pat) in fields.iter().enumerate() {
                        if let Pattern::Ident { name, .. } = field_pat {
                            let payload = IrExpr::FieldGet {
                                object: Box::new(IrExpr::LocalGet(tmp)),
                                layout_index: layout_idx,
                                field_index: 1 + fi as u32,
                            };
                            let local_idx = ctx.new_local(name.clone(), IrType::I32);
                            then_stmts.push(IrStmt::Let {
                                local: local_idx,
                                ty: IrType::I32,
                                value: Some(payload),
                            });
                        }
                    }
                }
                then_stmts.extend(self.lower_block_stmts(then_block, ctx));

                let else_stmts = match else_block {
                    Some(els) => self.lower_else_expr(els, ctx),
                    None => Vec::new(),
                };

                stmts.push(IrStmt::If {
                    condition: is_some,
                    then_body: then_stmts,
                    else_body: else_stmts,
                });
                IrExpr::Block {
                    stmts,
                    result: Box::new(IrExpr::IntConst(0)),
                }
            }
            ExprKind::WhileLet {
                pattern,
                expr,
                body,
            } => {
                // while let Some(v) = expr { body }
                // → loop { let tmp = expr; if tmp.tag != 1 { break }; let v = tmp.value; body }
                let layout_idx = self.struct_name_map.get("__Option").copied().unwrap_or(0);

                let scrutinee = self.lower_expr(expr, ctx);
                let tmp = ctx.new_local(SmolStr::new("__whilelet_tmp"), IrType::Ptr);

                let tag_get = IrExpr::FieldGet {
                    object: Box::new(IrExpr::LocalGet(tmp)),
                    layout_index: layout_idx,
                    field_index: 0,
                };
                let is_none = IrExpr::BinOp {
                    op: IrBinOp::NeI32,
                    lhs: Box::new(tag_get),
                    rhs: Box::new(IrExpr::IntConst(1)),
                };

                let mut loop_body = vec![
                    IrStmt::Let {
                        local: tmp,
                        ty: IrType::Ptr,
                        value: Some(scrutinee),
                    },
                    IrStmt::If {
                        condition: is_none,
                        then_body: vec![IrStmt::Break],
                        else_body: Vec::new(),
                    },
                ];

                // Bind payload variables.
                if let Pattern::EnumVariant { fields, .. } = pattern {
                    for (fi, field_pat) in fields.iter().enumerate() {
                        if let Pattern::Ident { name, .. } = field_pat {
                            let payload = IrExpr::FieldGet {
                                object: Box::new(IrExpr::LocalGet(tmp)),
                                layout_index: layout_idx,
                                field_index: 1 + fi as u32,
                            };
                            let local_idx = ctx.new_local(name.clone(), IrType::I32);
                            loop_body.push(IrStmt::Let {
                                local: local_idx,
                                ty: IrType::I32,
                                value: Some(payload),
                            });
                        }
                    }
                }

                loop_body.extend(self.lower_block_stmts(body, ctx));
                IrExpr::Block {
                    stmts: vec![IrStmt::Loop { body: loop_body }],
                    result: Box::new(IrExpr::IntConst(0)),
                }
            }

            // ── Error recovery node ─────────────────────────────────────
            ExprKind::Error => IrExpr::IntConst(0),
        }
    }

    // ── Match lowering ──────────────────────────────────────────────────

    fn lower_match(
        &mut self,
        scrutinee: &ast::Expr,
        arms: &[ast::MatchArm],
        ctx: &mut FnCtx,
    ) -> IrExpr {
        if arms.is_empty() {
            return IrExpr::IntConst(0);
        }

        // Lower scrutinee and store in a temporary local to avoid
        // re-evaluation.
        let scrut_ir = self.lower_expr(scrutinee, ctx);

        // Detect whether this match is over a struct-based enum (Option/Result
        // or a user-defined data-carrying enum).  If so, the scrutinee is a Ptr
        // and tag comparisons use FieldGet instead of direct i32 comparison.
        let struct_layout_idx = self.detect_struct_enum_match(arms);

        let scrut_ty = if struct_layout_idx.is_some() {
            IrType::Ptr
        } else {
            IrType::I32
        };
        let tmp_local = ctx.new_local(SmolStr::new("__match_scrut"), scrut_ty);
        let set_tmp = IrStmt::Let {
            local: tmp_local,
            ty: scrut_ty,
            value: Some(scrut_ir),
        };

        // Separate wildcard/default arm from the rest.
        let mut default_body: Option<IrExpr> = None;
        let mut cond_arms: Vec<(IrExpr, IrExpr)> = Vec::new();

        for arm in arms {
            match &arm.pattern {
                Pattern::Wildcard(_) | Pattern::Ident { .. } => {
                    default_body = Some(self.lower_expr(&arm.body, ctx));
                }
                Pattern::Literal { expr: pat_expr, .. } => {
                    let pat_val = self.lower_expr(pat_expr, ctx);
                    let cond = IrExpr::BinOp {
                        op: IrBinOp::EqI32,
                        lhs: Box::new(IrExpr::LocalGet(tmp_local)),
                        rhs: Box::new(pat_val),
                    };
                    let body = self.lower_expr(&arm.body, ctx);
                    cond_arms.push((cond, body));
                }
                Pattern::EnumVariant { path, fields, .. } => {
                    let key = SmolStr::new(
                        path.iter()
                            .map(|s| s.as_str())
                            .collect::<Vec<_>>()
                            .join("::"),
                    );
                    let tag = self.enum_variant_tags.get(&key).copied().unwrap_or(0);

                    if let Some(layout_idx) = struct_layout_idx {
                        // Struct-based enum: compare tag via FieldGet.
                        let tag_get = IrExpr::FieldGet {
                            object: Box::new(IrExpr::LocalGet(tmp_local)),
                            layout_index: layout_idx,
                            field_index: 0,
                        };
                        let cond = IrExpr::BinOp {
                            op: IrBinOp::EqI32,
                            lhs: Box::new(tag_get),
                            rhs: Box::new(IrExpr::IntConst(tag as i64)),
                        };

                        // Bind payload fields if any.
                        let mut bind_stmts = Vec::new();
                        for (fi, field_pat) in fields.iter().enumerate() {
                            if let Pattern::Ident { name, .. } = field_pat {
                                let payload = IrExpr::FieldGet {
                                    object: Box::new(IrExpr::LocalGet(tmp_local)),
                                    layout_index: layout_idx,
                                    field_index: 1 + fi as u32,
                                };
                                let local_idx = ctx.new_local(name.clone(), IrType::I32);
                                bind_stmts.push(IrStmt::Let {
                                    local: local_idx,
                                    ty: IrType::I32,
                                    value: Some(payload),
                                });
                            }
                        }

                        let body_expr = self.lower_expr(&arm.body, ctx);
                        let body = if bind_stmts.is_empty() {
                            body_expr
                        } else {
                            IrExpr::Block {
                                stmts: bind_stmts,
                                result: Box::new(body_expr),
                            }
                        };
                        cond_arms.push((cond, body));
                    } else {
                        // Simple unit enum — compare tag directly.
                        let cond = IrExpr::BinOp {
                            op: IrBinOp::EqI32,
                            lhs: Box::new(IrExpr::LocalGet(tmp_local)),
                            rhs: Box::new(IrExpr::IntConst(tag as i64)),
                        };
                        let body = self.lower_expr(&arm.body, ctx);
                        cond_arms.push((cond, body));
                    }
                }
                _ => {
                    // Unsupported pattern — treat as default.
                    default_body = Some(self.lower_expr(&arm.body, ctx));
                }
            }
        }

        let fallback = default_body.unwrap_or(IrExpr::IntConst(0));

        // Build a right-folded chain of IfExpr nodes.
        let result = cond_arms
            .into_iter()
            .rev()
            .fold(fallback, |else_expr, (cond, body)| IrExpr::IfExpr {
                condition: Box::new(cond),
                then_expr: Box::new(body),
                else_expr: Box::new(else_expr),
            });

        // Wrap in a Block that first sets the temp local, then evaluates
        // the if-chain.
        IrExpr::Block {
            stmts: vec![set_tmp],
            result: Box::new(result),
        }
    }

    /// Inspect match arms to determine if the match is over a struct-based
    /// enum.  Returns `Some(layout_idx)` if so, `None` if it is a plain i32
    /// enum or integer match.
    fn detect_struct_enum_match(&self, arms: &[ast::MatchArm]) -> Option<u32> {
        for arm in arms {
            if let Pattern::EnumVariant { path, .. } = &arm.pattern {
                let variant_name = path.last().map(|s| s.as_str()).unwrap_or("");
                // Built-in Option variants
                if variant_name == "Some" || variant_name == "None" {
                    return self.struct_name_map.get("__Option").copied();
                }
                // Built-in Result variants
                if variant_name == "Ok" || variant_name == "Err" {
                    return self.struct_name_map.get("__Result").copied();
                }
                // User-defined data-carrying enums
                if path.len() >= 2 {
                    let enum_name = &path[0];
                    let layout_key = SmolStr::new(format!("__enum_{}", enum_name));
                    if let Some(&idx) = self.struct_name_map.get(&layout_key) {
                        return Some(idx);
                    }
                }
            }
        }
        None
    }

    // ── Lambda lowering ────────────────────────────────────────────────

    /// Lower a lambda expression into a new top-level `IrFunction` and
    /// return an `IrExpr::IntConst(0)` placeholder.  The real magic happens
    /// in `lower_let`: when a `let` binds a lambda, we record the variable
    /// name as an alias for the generated function so that subsequent calls
    /// through that variable resolve to the lambda function directly.
    fn lower_lambda(
        &mut self,
        params: &[ast::LambdaParam],
        return_type: Option<&ast::TypeExpr>,
        body: &ast::Expr,
        enclosing_ctx: &FnCtx,
    ) -> IrExpr {
        let lambda_name: SmolStr = SmolStr::new(format!("__lambda_{}", self.lambda_counter));
        self.lambda_counter += 1;

        // Collect the set of lambda parameter names so we can identify
        // free variables (captures) in the body.
        let param_names: std::collections::HashSet<SmolStr> =
            params.iter().map(|p| p.name.clone()).collect();

        // Scan the body for free variable references that exist in the
        // enclosing scope but not in the lambda's own parameters.
        let mut free_vars: Vec<SmolStr> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        collect_free_vars(body, &param_names, enclosing_ctx, &mut free_vars, &mut seen);

        let mut lambda_ctx = FnCtx::new();

        // Register lambda parameters as locals.
        let mut ir_params = Vec::new();
        for p in params {
            let ir_ty =
                p.ty.as_ref()
                    .map(|t| self.lower_type_expr(t))
                    .unwrap_or(IrType::I32);
            let _idx = lambda_ctx.new_local(p.name.clone(), ir_ty);
            ir_params.push(IrParam {
                name: p.name.clone(),
                ty: ir_ty,
            });
        }

        // Add captured variables as extra hidden parameters (lambda lifting).
        // We register the local under the *original* variable name so the
        // lambda body can reference it normally (e.g. `base` not `__capture_base`).
        // The IrParam uses a `__capture_` prefix for clarity in IR dumps.
        let mut capture_names: Vec<SmolStr> = Vec::new();
        for var_name in &free_vars {
            if let Some(outer_idx) = enclosing_ctx.lookup(var_name) {
                let ty = enclosing_ctx.local_type(outer_idx);
                let capture_param_name: SmolStr = SmolStr::new(format!("__capture_{}", var_name));
                // Register under the original name so body lowering resolves it.
                let _idx = lambda_ctx.new_local(var_name.clone(), ty);
                ir_params.push(IrParam {
                    name: capture_param_name,
                    ty,
                });
                capture_names.push(var_name.clone());
            }
        }

        // Store capture info so call sites can append captured locals.
        if !capture_names.is_empty() {
            self.lambda_captures
                .insert(lambda_name.clone(), capture_names);
        }

        // Determine return type.
        let ret_type = return_type
            .map(|t| self.lower_type_expr(t))
            .unwrap_or(IrType::I32);

        // Lower the body.  If the body is a Block, lower it as block stmts.
        // Otherwise, treat the expression as an implicit return value.
        let ir_body = match &body.kind {
            ExprKind::Block(block) => self.lower_block_stmts(block, &mut lambda_ctx),
            _ => {
                // Single-expression body: wrap as `return <expr>;`
                let expr = self.lower_expr(body, &mut lambda_ctx);
                vec![IrStmt::Return(Some(expr))]
            }
        };

        let ir_fn = IrFunction {
            name: lambda_name.clone(),
            params: ir_params,
            ret_type,
            locals: lambda_ctx.locals,
            body: ir_body,
            is_export: false,
            source_span: None,
        };

        let idx = self.module.functions.len();
        self.fn_name_map.insert(lambda_name.clone(), idx);
        self.module.functions.push(ir_fn);

        // Return 0 as the expression value.  The variable-to-function
        // binding is established in `lower_let` instead.
        IrExpr::IntConst(0)
    }

    /// Generate the lambda function name that was most recently created.
    /// Called by `lower_let` to associate variable names with lambdas.
    fn last_lambda_name(&self) -> Option<SmolStr> {
        if self.lambda_counter == 0 {
            None
        } else {
            Some(SmolStr::new(format!(
                "__lambda_{}",
                self.lambda_counter - 1
            )))
        }
    }

    // ── Pipeline operation lowering ─────────────────────────────────────

    /// Resolve a lambda expression to a function name.  If the expression
    /// is a lambda literal, lower it (creating `__lambda_N`) and return the
    /// name.  If it is an identifier that aliases a lambda, return the alias.
    fn resolve_lambda_fn(&mut self, closure_expr: &ast::Expr, ctx: &mut FnCtx) -> SmolStr {
        match &closure_expr.kind {
            ExprKind::Lambda {
                params,
                return_type,
                body,
            } => {
                // Lower the lambda — this creates __lambda_N and bumps the counter.
                let _placeholder = self.lower_lambda(params, return_type.as_deref(), body, ctx);
                self.last_lambda_name()
                    .unwrap_or_else(|| SmolStr::new("__unknown"))
            }
            ExprKind::Ident(name) => {
                // Variable that might alias a lambda.
                self.lambda_aliases
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| name.clone())
            }
            _ => {
                // Not a recognized lambda form — lower as expression and
                // return a placeholder name.
                let _val = self.lower_expr(closure_expr, ctx);
                SmolStr::new("__unknown")
            }
        }
    }

    /// Build a call to `lambda_fn(elem)`, including captured variables.
    fn make_lambda_call(&self, lambda_fn: &SmolStr, elem_local: u32, ctx: &FnCtx) -> IrExpr {
        let mut call_args = vec![IrExpr::LocalGet(elem_local)];
        // Append captured locals as extra args.
        if let Some(captures) = self.lambda_captures.get(lambda_fn) {
            for cap_name in captures.clone() {
                if let Some(idx) = ctx.lookup(&cap_name) {
                    call_args.push(IrExpr::LocalGet(idx));
                }
            }
        }
        IrExpr::Call {
            func: lambda_fn.clone(),
            args: call_args,
        }
    }

    /// Lower `arr.filter(|x| predicate)` to a loop that calls the lambda
    /// on each element and pushes matching elements to a new array.
    fn lower_pipeline_filter(
        &mut self,
        array_expr: &ast::Expr,
        closure_expr: &ast::Expr,
        ctx: &mut FnCtx,
    ) -> IrExpr {
        let lambda_fn = self.resolve_lambda_fn(closure_expr, ctx);
        let arr = self.lower_expr(array_expr, ctx);

        let arr_local = ctx.new_local(SmolStr::new("__pipe_arr"), IrType::I32);
        let result_local = ctx.new_local(SmolStr::new("__pipe_result"), IrType::I32);
        let i_local = ctx.new_local(SmolStr::new("__pipe_i"), IrType::I32);
        let elem_local = ctx.new_local(SmolStr::new("__pipe_elem"), IrType::I32);

        let mut stmts = Vec::new();

        stmts.push(IrStmt::Let {
            local: arr_local,
            ty: IrType::I32,
            value: Some(arr),
        });
        stmts.push(IrStmt::Let {
            local: result_local,
            ty: IrType::I32,
            value: Some(IrExpr::HostCall {
                module: "env".into(),
                name: "__arr_new".into(),
                args: vec![],
                ret: IrType::I32,
            }),
        });
        stmts.push(IrStmt::Let {
            local: i_local,
            ty: IrType::I32,
            value: Some(IrExpr::IntConst(0)),
        });

        let break_cond = IrExpr::BinOp {
            op: IrBinOp::GeI32S,
            lhs: Box::new(IrExpr::LocalGet(i_local)),
            rhs: Box::new(IrExpr::HostCall {
                module: "env".into(),
                name: "__arr_len".into(),
                args: vec![IrExpr::LocalGet(arr_local)],
                ret: IrType::I32,
            }),
        };
        let get_elem = IrExpr::HostCall {
            module: "env".into(),
            name: "__arr_get".into(),
            args: vec![IrExpr::LocalGet(arr_local), IrExpr::LocalGet(i_local)],
            ret: IrType::I32,
        };
        let predicate_call = self.make_lambda_call(&lambda_fn, elem_local, ctx);
        let push_call = IrExpr::HostCall {
            module: "env".into(),
            name: "__arr_push".into(),
            args: vec![IrExpr::LocalGet(result_local), IrExpr::LocalGet(elem_local)],
            ret: IrType::Unit,
        };

        let loop_body = vec![
            IrStmt::If {
                condition: break_cond,
                then_body: vec![IrStmt::Break],
                else_body: Vec::new(),
            },
            IrStmt::Let {
                local: elem_local,
                ty: IrType::I32,
                value: Some(get_elem),
            },
            IrStmt::If {
                condition: predicate_call,
                then_body: vec![IrStmt::Expr(push_call)],
                else_body: Vec::new(),
            },
            IrStmt::Assign {
                target: IrLValue::Local(i_local),
                value: IrExpr::BinOp {
                    op: IrBinOp::AddI32,
                    lhs: Box::new(IrExpr::LocalGet(i_local)),
                    rhs: Box::new(IrExpr::IntConst(1)),
                },
            },
        ];
        stmts.push(IrStmt::Loop { body: loop_body });

        IrExpr::Block {
            stmts,
            result: Box::new(IrExpr::LocalGet(result_local)),
        }
    }

    /// Lower `arr.map(|x| transform)` to a loop that calls the lambda
    /// on each element and pushes the result to a new array.
    fn lower_pipeline_map(
        &mut self,
        array_expr: &ast::Expr,
        closure_expr: &ast::Expr,
        ctx: &mut FnCtx,
    ) -> IrExpr {
        let lambda_fn = self.resolve_lambda_fn(closure_expr, ctx);
        let arr = self.lower_expr(array_expr, ctx);

        let arr_local = ctx.new_local(SmolStr::new("__pipe_arr"), IrType::I32);
        let result_local = ctx.new_local(SmolStr::new("__pipe_result"), IrType::I32);
        let i_local = ctx.new_local(SmolStr::new("__pipe_i"), IrType::I32);
        let elem_local = ctx.new_local(SmolStr::new("__pipe_elem"), IrType::I32);
        let mapped_local = ctx.new_local(SmolStr::new("__pipe_mapped"), IrType::I32);

        let mut stmts = Vec::new();

        stmts.push(IrStmt::Let {
            local: arr_local,
            ty: IrType::I32,
            value: Some(arr),
        });
        stmts.push(IrStmt::Let {
            local: result_local,
            ty: IrType::I32,
            value: Some(IrExpr::HostCall {
                module: "env".into(),
                name: "__arr_new".into(),
                args: vec![],
                ret: IrType::I32,
            }),
        });
        stmts.push(IrStmt::Let {
            local: i_local,
            ty: IrType::I32,
            value: Some(IrExpr::IntConst(0)),
        });

        let break_cond = IrExpr::BinOp {
            op: IrBinOp::GeI32S,
            lhs: Box::new(IrExpr::LocalGet(i_local)),
            rhs: Box::new(IrExpr::HostCall {
                module: "env".into(),
                name: "__arr_len".into(),
                args: vec![IrExpr::LocalGet(arr_local)],
                ret: IrType::I32,
            }),
        };
        let get_elem = IrExpr::HostCall {
            module: "env".into(),
            name: "__arr_get".into(),
            args: vec![IrExpr::LocalGet(arr_local), IrExpr::LocalGet(i_local)],
            ret: IrType::I32,
        };
        let map_call = self.make_lambda_call(&lambda_fn, elem_local, ctx);
        let push_call = IrExpr::HostCall {
            module: "env".into(),
            name: "__arr_push".into(),
            args: vec![
                IrExpr::LocalGet(result_local),
                IrExpr::LocalGet(mapped_local),
            ],
            ret: IrType::Unit,
        };

        let loop_body = vec![
            IrStmt::If {
                condition: break_cond,
                then_body: vec![IrStmt::Break],
                else_body: Vec::new(),
            },
            IrStmt::Let {
                local: elem_local,
                ty: IrType::I32,
                value: Some(get_elem),
            },
            IrStmt::Let {
                local: mapped_local,
                ty: IrType::I32,
                value: Some(map_call),
            },
            IrStmt::Expr(push_call),
            IrStmt::Assign {
                target: IrLValue::Local(i_local),
                value: IrExpr::BinOp {
                    op: IrBinOp::AddI32,
                    lhs: Box::new(IrExpr::LocalGet(i_local)),
                    rhs: Box::new(IrExpr::IntConst(1)),
                },
            },
        ];
        stmts.push(IrStmt::Loop { body: loop_body });

        IrExpr::Block {
            stmts,
            result: Box::new(IrExpr::LocalGet(result_local)),
        }
    }

    /// Lower `arr.for_each(|x| action)` to a loop that calls the lambda
    /// on each element for its side effects.
    fn lower_pipeline_for_each(
        &mut self,
        array_expr: &ast::Expr,
        closure_expr: &ast::Expr,
        ctx: &mut FnCtx,
    ) -> IrExpr {
        let lambda_fn = self.resolve_lambda_fn(closure_expr, ctx);
        let arr = self.lower_expr(array_expr, ctx);

        let arr_local = ctx.new_local(SmolStr::new("__pipe_arr"), IrType::I32);
        let i_local = ctx.new_local(SmolStr::new("__pipe_i"), IrType::I32);
        let elem_local = ctx.new_local(SmolStr::new("__pipe_elem"), IrType::I32);

        let mut stmts = Vec::new();

        stmts.push(IrStmt::Let {
            local: arr_local,
            ty: IrType::I32,
            value: Some(arr),
        });
        stmts.push(IrStmt::Let {
            local: i_local,
            ty: IrType::I32,
            value: Some(IrExpr::IntConst(0)),
        });

        let break_cond = IrExpr::BinOp {
            op: IrBinOp::GeI32S,
            lhs: Box::new(IrExpr::LocalGet(i_local)),
            rhs: Box::new(IrExpr::HostCall {
                module: "env".into(),
                name: "__arr_len".into(),
                args: vec![IrExpr::LocalGet(arr_local)],
                ret: IrType::I32,
            }),
        };
        let get_elem = IrExpr::HostCall {
            module: "env".into(),
            name: "__arr_get".into(),
            args: vec![IrExpr::LocalGet(arr_local), IrExpr::LocalGet(i_local)],
            ret: IrType::I32,
        };
        let action_call = self.make_lambda_call(&lambda_fn, elem_local, ctx);

        let loop_body = vec![
            IrStmt::If {
                condition: break_cond,
                then_body: vec![IrStmt::Break],
                else_body: Vec::new(),
            },
            IrStmt::Let {
                local: elem_local,
                ty: IrType::I32,
                value: Some(get_elem),
            },
            IrStmt::Expr(action_call),
            IrStmt::Assign {
                target: IrLValue::Local(i_local),
                value: IrExpr::BinOp {
                    op: IrBinOp::AddI32,
                    lhs: Box::new(IrExpr::LocalGet(i_local)),
                    rhs: Box::new(IrExpr::IntConst(1)),
                },
            },
        ];
        stmts.push(IrStmt::Loop { body: loop_body });

        IrExpr::Block {
            stmts,
            result: Box::new(IrExpr::IntConst(0)),
        }
    }

    /// Lower `arr.take(n)` to a loop that copies first n elements.
    fn lower_pipeline_take(
        &mut self,
        array_expr: &ast::Expr,
        n_expr: &ast::Expr,
        ctx: &mut FnCtx,
    ) -> IrExpr {
        let arr = self.lower_expr(array_expr, ctx);
        let n = self.lower_expr(n_expr, ctx);

        let arr_local = ctx.new_local(SmolStr::new("__pipe_arr"), IrType::I32);
        let n_local = ctx.new_local(SmolStr::new("__pipe_n"), IrType::I32);
        let result_local = ctx.new_local(SmolStr::new("__pipe_result"), IrType::I32);
        let i_local = ctx.new_local(SmolStr::new("__pipe_i"), IrType::I32);

        let mut stmts = vec![
            IrStmt::Let {
                local: arr_local,
                ty: IrType::I32,
                value: Some(arr),
            },
            IrStmt::Let {
                local: n_local,
                ty: IrType::I32,
                value: Some(n),
            },
        ];
        stmts.push(IrStmt::Let {
            local: result_local,
            ty: IrType::I32,
            value: Some(IrExpr::HostCall {
                module: "env".into(),
                name: "__arr_new".into(),
                args: vec![],
                ret: IrType::I32,
            }),
        });
        stmts.push(IrStmt::Let {
            local: i_local,
            ty: IrType::I32,
            value: Some(IrExpr::IntConst(0)),
        });

        // Break when i >= n or i >= arr.len()
        let break_cond_n = IrExpr::BinOp {
            op: IrBinOp::GeI32S,
            lhs: Box::new(IrExpr::LocalGet(i_local)),
            rhs: Box::new(IrExpr::LocalGet(n_local)),
        };
        let break_cond_len = IrExpr::BinOp {
            op: IrBinOp::GeI32S,
            lhs: Box::new(IrExpr::LocalGet(i_local)),
            rhs: Box::new(IrExpr::HostCall {
                module: "env".into(),
                name: "__arr_len".into(),
                args: vec![IrExpr::LocalGet(arr_local)],
                ret: IrType::I32,
            }),
        };
        let break_cond = IrExpr::BinOp {
            op: IrBinOp::OrI32,
            lhs: Box::new(break_cond_n),
            rhs: Box::new(break_cond_len),
        };

        let get_elem = IrExpr::HostCall {
            module: "env".into(),
            name: "__arr_get".into(),
            args: vec![IrExpr::LocalGet(arr_local), IrExpr::LocalGet(i_local)],
            ret: IrType::I32,
        };
        let push_call = IrExpr::HostCall {
            module: "env".into(),
            name: "__arr_push".into(),
            args: vec![IrExpr::LocalGet(result_local), get_elem],
            ret: IrType::Unit,
        };

        let loop_body = vec![
            IrStmt::If {
                condition: break_cond,
                then_body: vec![IrStmt::Break],
                else_body: Vec::new(),
            },
            IrStmt::Expr(push_call),
            IrStmt::Assign {
                target: IrLValue::Local(i_local),
                value: IrExpr::BinOp {
                    op: IrBinOp::AddI32,
                    lhs: Box::new(IrExpr::LocalGet(i_local)),
                    rhs: Box::new(IrExpr::IntConst(1)),
                },
            },
        ];
        stmts.push(IrStmt::Loop { body: loop_body });

        IrExpr::Block {
            stmts,
            result: Box::new(IrExpr::LocalGet(result_local)),
        }
    }

    /// Lower `arr.skip(n)` to a loop that skips first n elements.
    fn lower_pipeline_skip(
        &mut self,
        array_expr: &ast::Expr,
        n_expr: &ast::Expr,
        ctx: &mut FnCtx,
    ) -> IrExpr {
        let arr = self.lower_expr(array_expr, ctx);
        let n = self.lower_expr(n_expr, ctx);

        let arr_local = ctx.new_local(SmolStr::new("__pipe_arr"), IrType::I32);
        let n_local = ctx.new_local(SmolStr::new("__pipe_n"), IrType::I32);
        let result_local = ctx.new_local(SmolStr::new("__pipe_result"), IrType::I32);
        let i_local = ctx.new_local(SmolStr::new("__pipe_i"), IrType::I32);

        let mut stmts = vec![
            IrStmt::Let {
                local: arr_local,
                ty: IrType::I32,
                value: Some(arr),
            },
            IrStmt::Let {
                local: n_local,
                ty: IrType::I32,
                value: Some(n),
            },
        ];
        stmts.push(IrStmt::Let {
            local: result_local,
            ty: IrType::I32,
            value: Some(IrExpr::HostCall {
                module: "env".into(),
                name: "__arr_new".into(),
                args: vec![],
                ret: IrType::I32,
            }),
        });
        // Start i at n
        stmts.push(IrStmt::Let {
            local: i_local,
            ty: IrType::I32,
            value: Some(IrExpr::LocalGet(n_local)),
        });

        let break_cond = IrExpr::BinOp {
            op: IrBinOp::GeI32S,
            lhs: Box::new(IrExpr::LocalGet(i_local)),
            rhs: Box::new(IrExpr::HostCall {
                module: "env".into(),
                name: "__arr_len".into(),
                args: vec![IrExpr::LocalGet(arr_local)],
                ret: IrType::I32,
            }),
        };

        let get_elem = IrExpr::HostCall {
            module: "env".into(),
            name: "__arr_get".into(),
            args: vec![IrExpr::LocalGet(arr_local), IrExpr::LocalGet(i_local)],
            ret: IrType::I32,
        };
        let push_call = IrExpr::HostCall {
            module: "env".into(),
            name: "__arr_push".into(),
            args: vec![IrExpr::LocalGet(result_local), get_elem],
            ret: IrType::Unit,
        };

        let loop_body = vec![
            IrStmt::If {
                condition: break_cond,
                then_body: vec![IrStmt::Break],
                else_body: Vec::new(),
            },
            IrStmt::Expr(push_call),
            IrStmt::Assign {
                target: IrLValue::Local(i_local),
                value: IrExpr::BinOp {
                    op: IrBinOp::AddI32,
                    lhs: Box::new(IrExpr::LocalGet(i_local)),
                    rhs: Box::new(IrExpr::IntConst(1)),
                },
            },
        ];
        stmts.push(IrStmt::Loop { body: loop_body });

        IrExpr::Block {
            stmts,
            result: Box::new(IrExpr::LocalGet(result_local)),
        }
    }

    /// Lower `arr.any(|x| pred)` to a loop, return 1 if any match.
    fn lower_pipeline_any(
        &mut self,
        array_expr: &ast::Expr,
        closure_expr: &ast::Expr,
        ctx: &mut FnCtx,
    ) -> IrExpr {
        let lambda_fn = self.resolve_lambda_fn(closure_expr, ctx);
        let arr = self.lower_expr(array_expr, ctx);

        let arr_local = ctx.new_local(SmolStr::new("__pipe_arr"), IrType::I32);
        let result_local = ctx.new_local(SmolStr::new("__pipe_result"), IrType::I32);
        let i_local = ctx.new_local(SmolStr::new("__pipe_i"), IrType::I32);
        let elem_local = ctx.new_local(SmolStr::new("__pipe_elem"), IrType::I32);

        let mut stmts = Vec::new();
        stmts.push(IrStmt::Let {
            local: arr_local,
            ty: IrType::I32,
            value: Some(arr),
        });
        stmts.push(IrStmt::Let {
            local: result_local,
            ty: IrType::I32,
            value: Some(IrExpr::IntConst(0)),
        });
        stmts.push(IrStmt::Let {
            local: i_local,
            ty: IrType::I32,
            value: Some(IrExpr::IntConst(0)),
        });

        let break_cond = IrExpr::BinOp {
            op: IrBinOp::GeI32S,
            lhs: Box::new(IrExpr::LocalGet(i_local)),
            rhs: Box::new(IrExpr::HostCall {
                module: "env".into(),
                name: "__arr_len".into(),
                args: vec![IrExpr::LocalGet(arr_local)],
                ret: IrType::I32,
            }),
        };
        let get_elem = IrExpr::HostCall {
            module: "env".into(),
            name: "__arr_get".into(),
            args: vec![IrExpr::LocalGet(arr_local), IrExpr::LocalGet(i_local)],
            ret: IrType::I32,
        };
        let predicate_call = self.make_lambda_call(&lambda_fn, elem_local, ctx);

        let loop_body = vec![
            IrStmt::If {
                condition: break_cond,
                then_body: vec![IrStmt::Break],
                else_body: Vec::new(),
            },
            IrStmt::Let {
                local: elem_local,
                ty: IrType::I32,
                value: Some(get_elem),
            },
            IrStmt::If {
                condition: predicate_call,
                then_body: vec![
                    IrStmt::Assign {
                        target: IrLValue::Local(result_local),
                        value: IrExpr::IntConst(1),
                    },
                    IrStmt::Break,
                ],
                else_body: Vec::new(),
            },
            IrStmt::Assign {
                target: IrLValue::Local(i_local),
                value: IrExpr::BinOp {
                    op: IrBinOp::AddI32,
                    lhs: Box::new(IrExpr::LocalGet(i_local)),
                    rhs: Box::new(IrExpr::IntConst(1)),
                },
            },
        ];
        stmts.push(IrStmt::Loop { body: loop_body });

        IrExpr::Block {
            stmts,
            result: Box::new(IrExpr::LocalGet(result_local)),
        }
    }

    /// Lower `arr.all(|x| pred)` to a loop, return 0 if any don't match.
    fn lower_pipeline_all(
        &mut self,
        array_expr: &ast::Expr,
        closure_expr: &ast::Expr,
        ctx: &mut FnCtx,
    ) -> IrExpr {
        let lambda_fn = self.resolve_lambda_fn(closure_expr, ctx);
        let arr = self.lower_expr(array_expr, ctx);

        let arr_local = ctx.new_local(SmolStr::new("__pipe_arr"), IrType::I32);
        let result_local = ctx.new_local(SmolStr::new("__pipe_result"), IrType::I32);
        let i_local = ctx.new_local(SmolStr::new("__pipe_i"), IrType::I32);
        let elem_local = ctx.new_local(SmolStr::new("__pipe_elem"), IrType::I32);

        let mut stmts = Vec::new();
        stmts.push(IrStmt::Let {
            local: arr_local,
            ty: IrType::I32,
            value: Some(arr),
        });
        stmts.push(IrStmt::Let {
            local: result_local,
            ty: IrType::I32,
            value: Some(IrExpr::IntConst(1)),
        });
        stmts.push(IrStmt::Let {
            local: i_local,
            ty: IrType::I32,
            value: Some(IrExpr::IntConst(0)),
        });

        let break_cond = IrExpr::BinOp {
            op: IrBinOp::GeI32S,
            lhs: Box::new(IrExpr::LocalGet(i_local)),
            rhs: Box::new(IrExpr::HostCall {
                module: "env".into(),
                name: "__arr_len".into(),
                args: vec![IrExpr::LocalGet(arr_local)],
                ret: IrType::I32,
            }),
        };
        let get_elem = IrExpr::HostCall {
            module: "env".into(),
            name: "__arr_get".into(),
            args: vec![IrExpr::LocalGet(arr_local), IrExpr::LocalGet(i_local)],
            ret: IrType::I32,
        };
        let predicate_call = self.make_lambda_call(&lambda_fn, elem_local, ctx);

        // If predicate is false (eqz), set result=0 and break
        let not_pred = IrExpr::UnaryOp {
            op: IrUnaryOp::EqzI32,
            operand: Box::new(predicate_call),
        };

        let loop_body = vec![
            IrStmt::If {
                condition: break_cond,
                then_body: vec![IrStmt::Break],
                else_body: Vec::new(),
            },
            IrStmt::Let {
                local: elem_local,
                ty: IrType::I32,
                value: Some(get_elem),
            },
            IrStmt::If {
                condition: not_pred,
                then_body: vec![
                    IrStmt::Assign {
                        target: IrLValue::Local(result_local),
                        value: IrExpr::IntConst(0),
                    },
                    IrStmt::Break,
                ],
                else_body: Vec::new(),
            },
            IrStmt::Assign {
                target: IrLValue::Local(i_local),
                value: IrExpr::BinOp {
                    op: IrBinOp::AddI32,
                    lhs: Box::new(IrExpr::LocalGet(i_local)),
                    rhs: Box::new(IrExpr::IntConst(1)),
                },
            },
        ];
        stmts.push(IrStmt::Loop { body: loop_body });

        IrExpr::Block {
            stmts,
            result: Box::new(IrExpr::LocalGet(result_local)),
        }
    }

    /// Lower `arr.find(|x| pred)` to a loop, return first matching element or 0.
    fn lower_pipeline_find(
        &mut self,
        array_expr: &ast::Expr,
        closure_expr: &ast::Expr,
        ctx: &mut FnCtx,
    ) -> IrExpr {
        let lambda_fn = self.resolve_lambda_fn(closure_expr, ctx);
        let arr = self.lower_expr(array_expr, ctx);

        let arr_local = ctx.new_local(SmolStr::new("__pipe_arr"), IrType::I32);
        let result_local = ctx.new_local(SmolStr::new("__pipe_result"), IrType::I32);
        let i_local = ctx.new_local(SmolStr::new("__pipe_i"), IrType::I32);
        let elem_local = ctx.new_local(SmolStr::new("__pipe_elem"), IrType::I32);

        let mut stmts = Vec::new();
        stmts.push(IrStmt::Let {
            local: arr_local,
            ty: IrType::I32,
            value: Some(arr),
        });
        stmts.push(IrStmt::Let {
            local: result_local,
            ty: IrType::I32,
            value: Some(IrExpr::IntConst(0)),
        });
        stmts.push(IrStmt::Let {
            local: i_local,
            ty: IrType::I32,
            value: Some(IrExpr::IntConst(0)),
        });

        let break_cond = IrExpr::BinOp {
            op: IrBinOp::GeI32S,
            lhs: Box::new(IrExpr::LocalGet(i_local)),
            rhs: Box::new(IrExpr::HostCall {
                module: "env".into(),
                name: "__arr_len".into(),
                args: vec![IrExpr::LocalGet(arr_local)],
                ret: IrType::I32,
            }),
        };
        let get_elem = IrExpr::HostCall {
            module: "env".into(),
            name: "__arr_get".into(),
            args: vec![IrExpr::LocalGet(arr_local), IrExpr::LocalGet(i_local)],
            ret: IrType::I32,
        };
        let predicate_call = self.make_lambda_call(&lambda_fn, elem_local, ctx);

        let loop_body = vec![
            IrStmt::If {
                condition: break_cond,
                then_body: vec![IrStmt::Break],
                else_body: Vec::new(),
            },
            IrStmt::Let {
                local: elem_local,
                ty: IrType::I32,
                value: Some(get_elem),
            },
            IrStmt::If {
                condition: predicate_call,
                then_body: vec![
                    IrStmt::Assign {
                        target: IrLValue::Local(result_local),
                        value: IrExpr::LocalGet(elem_local),
                    },
                    IrStmt::Break,
                ],
                else_body: Vec::new(),
            },
            IrStmt::Assign {
                target: IrLValue::Local(i_local),
                value: IrExpr::BinOp {
                    op: IrBinOp::AddI32,
                    lhs: Box::new(IrExpr::LocalGet(i_local)),
                    rhs: Box::new(IrExpr::IntConst(1)),
                },
            },
        ];
        stmts.push(IrStmt::Loop { body: loop_body });

        IrExpr::Block {
            stmts,
            result: Box::new(IrExpr::LocalGet(result_local)),
        }
    }

    /// Lower `arr.reduce(|a, b| expr)` to a fold left without initial value.
    /// Uses first element as initial accumulator.
    fn lower_pipeline_reduce(
        &mut self,
        array_expr: &ast::Expr,
        closure_expr: &ast::Expr,
        ctx: &mut FnCtx,
    ) -> IrExpr {
        let lambda_fn = self.resolve_lambda_fn(closure_expr, ctx);
        let arr = self.lower_expr(array_expr, ctx);

        let arr_local = ctx.new_local(SmolStr::new("__pipe_arr"), IrType::I32);
        let acc_local = ctx.new_local(SmolStr::new("__pipe_acc"), IrType::I32);
        let i_local = ctx.new_local(SmolStr::new("__pipe_i"), IrType::I32);

        let mut stmts = Vec::new();
        stmts.push(IrStmt::Let {
            local: arr_local,
            ty: IrType::I32,
            value: Some(arr),
        });
        // acc = arr[0]
        stmts.push(IrStmt::Let {
            local: acc_local,
            ty: IrType::I32,
            value: Some(IrExpr::HostCall {
                module: "env".into(),
                name: "__arr_get".into(),
                args: vec![IrExpr::LocalGet(arr_local), IrExpr::IntConst(0)],
                ret: IrType::I32,
            }),
        });
        // i = 1
        stmts.push(IrStmt::Let {
            local: i_local,
            ty: IrType::I32,
            value: Some(IrExpr::IntConst(1)),
        });

        let break_cond = IrExpr::BinOp {
            op: IrBinOp::GeI32S,
            lhs: Box::new(IrExpr::LocalGet(i_local)),
            rhs: Box::new(IrExpr::HostCall {
                module: "env".into(),
                name: "__arr_len".into(),
                args: vec![IrExpr::LocalGet(arr_local)],
                ret: IrType::I32,
            }),
        };

        // Call lambda with two args: acc and current element
        let get_elem = IrExpr::HostCall {
            module: "env".into(),
            name: "__arr_get".into(),
            args: vec![IrExpr::LocalGet(arr_local), IrExpr::LocalGet(i_local)],
            ret: IrType::I32,
        };

        // Build call: lambda_fn(acc, elem)
        let mut call_args = vec![IrExpr::LocalGet(acc_local), get_elem];
        if let Some(captures) = self.lambda_captures.get(&lambda_fn) {
            for cap_name in captures.clone() {
                if let Some(idx) = ctx.lookup(&cap_name) {
                    call_args.push(IrExpr::LocalGet(idx));
                }
            }
        }
        let fold_call = IrExpr::Call {
            func: lambda_fn,
            args: call_args,
        };

        let loop_body = vec![
            IrStmt::If {
                condition: break_cond,
                then_body: vec![IrStmt::Break],
                else_body: Vec::new(),
            },
            IrStmt::Assign {
                target: IrLValue::Local(acc_local),
                value: fold_call,
            },
            IrStmt::Assign {
                target: IrLValue::Local(i_local),
                value: IrExpr::BinOp {
                    op: IrBinOp::AddI32,
                    lhs: Box::new(IrExpr::LocalGet(i_local)),
                    rhs: Box::new(IrExpr::IntConst(1)),
                },
            },
        ];
        stmts.push(IrStmt::Loop { body: loop_body });

        IrExpr::Block {
            stmts,
            result: Box::new(IrExpr::LocalGet(acc_local)),
        }
    }

    /// Lower `arr.fold(init, |acc, x| expr)` to a loop with accumulator.
    fn lower_pipeline_fold(
        &mut self,
        array_expr: &ast::Expr,
        init_expr: &ast::Expr,
        closure_expr: &ast::Expr,
        ctx: &mut FnCtx,
    ) -> IrExpr {
        let lambda_fn = self.resolve_lambda_fn(closure_expr, ctx);
        let arr = self.lower_expr(array_expr, ctx);
        let init = self.lower_expr(init_expr, ctx);

        let arr_local = ctx.new_local(SmolStr::new("__pipe_arr"), IrType::I32);
        let acc_local = ctx.new_local(SmolStr::new("__pipe_acc"), IrType::I32);
        let i_local = ctx.new_local(SmolStr::new("__pipe_i"), IrType::I32);

        let mut stmts = Vec::new();
        stmts.push(IrStmt::Let {
            local: arr_local,
            ty: IrType::I32,
            value: Some(arr),
        });
        stmts.push(IrStmt::Let {
            local: acc_local,
            ty: IrType::I32,
            value: Some(init),
        });
        stmts.push(IrStmt::Let {
            local: i_local,
            ty: IrType::I32,
            value: Some(IrExpr::IntConst(0)),
        });

        let break_cond = IrExpr::BinOp {
            op: IrBinOp::GeI32S,
            lhs: Box::new(IrExpr::LocalGet(i_local)),
            rhs: Box::new(IrExpr::HostCall {
                module: "env".into(),
                name: "__arr_len".into(),
                args: vec![IrExpr::LocalGet(arr_local)],
                ret: IrType::I32,
            }),
        };

        let get_elem = IrExpr::HostCall {
            module: "env".into(),
            name: "__arr_get".into(),
            args: vec![IrExpr::LocalGet(arr_local), IrExpr::LocalGet(i_local)],
            ret: IrType::I32,
        };

        // Build call: lambda_fn(acc, elem)
        let mut call_args = vec![IrExpr::LocalGet(acc_local), get_elem];
        if let Some(captures) = self.lambda_captures.get(&lambda_fn) {
            for cap_name in captures.clone() {
                if let Some(idx) = ctx.lookup(&cap_name) {
                    call_args.push(IrExpr::LocalGet(idx));
                }
            }
        }
        let fold_call = IrExpr::Call {
            func: lambda_fn,
            args: call_args,
        };

        let loop_body = vec![
            IrStmt::If {
                condition: break_cond,
                then_body: vec![IrStmt::Break],
                else_body: Vec::new(),
            },
            IrStmt::Assign {
                target: IrLValue::Local(acc_local),
                value: fold_call,
            },
            IrStmt::Assign {
                target: IrLValue::Local(i_local),
                value: IrExpr::BinOp {
                    op: IrBinOp::AddI32,
                    lhs: Box::new(IrExpr::LocalGet(i_local)),
                    rhs: Box::new(IrExpr::IntConst(1)),
                },
            },
        ];
        stmts.push(IrStmt::Loop { body: loop_body });

        IrExpr::Block {
            stmts,
            result: Box::new(IrExpr::LocalGet(acc_local)),
        }
    }

    // ── Helpers ─────────────────────────────────────────────────────────

    /// Returns `true` if the expression is known to be a string type.
    fn is_string_expr(&self, expr: &ast::Expr, ctx: &FnCtx) -> bool {
        match &expr.kind {
            ExprKind::StringLit(_) | ExprKind::TemplateLit(_) => true,
            ExprKind::Ident(name) => {
                if let Some(idx) = ctx.lookup(name) {
                    ctx.local_type(idx) == IrType::Ptr
                } else {
                    false
                }
            }
            ExprKind::Binary { op, lhs, rhs } => {
                *op == BinOp::Add
                    && (self.is_string_expr(lhs, ctx) || self.is_string_expr(rhs, ctx))
            }
            _ => false,
        }
    }

    fn lower_block_as_expr(&mut self, block: &ast::Block, ctx: &mut FnCtx) -> IrExpr {
        let mut stmts = self.lower_block_stmts(block, ctx);
        if stmts.is_empty() {
            return IrExpr::IntConst(0);
        }
        // The last statement might be a bare expression (no semicolon) that
        // produces the block's value.  Extract it from the lowered stmts so
        // it becomes the Block result.
        let last = block.stmts.last();
        let result = match last {
            Some(Stmt::Expr(es)) if !es.has_semicolon => {
                // The last lowered stmt should be IrStmt::Expr(x).  Pop it
                // and use x as the result expression.
                if let Some(IrStmt::Expr(expr)) = stmts.pop() {
                    expr
                } else {
                    IrExpr::IntConst(0)
                }
            }
            _ => IrExpr::IntConst(0),
        };
        IrExpr::Block {
            stmts,
            result: Box::new(result),
        }
    }

    fn callee_name(&self, expr: &ast::Expr) -> SmolStr {
        let raw = match &expr.kind {
            ExprKind::Ident(name) => name.clone(),
            ExprKind::Path(parts) => {
                // For paths like Point::new, try the mangled name first.
                if parts.len() == 2 {
                    let mangled = SmolStr::new(format!("{}__{}", parts[0], parts[1]));
                    if self.fn_name_map.contains_key(&mangled) {
                        return mangled;
                    }
                    // Also check method_map for the method name.
                    if let Some(resolved) = self.method_map.get(&parts[1]) {
                        return resolved.clone();
                    }
                }
                SmolStr::new(
                    parts
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join("::"),
                )
            }
            _ => SmolStr::new("<indirect>"),
        };
        // If the callee is a variable that aliases a lambda, resolve it.
        if let Some(lambda_name) = self.lambda_aliases.get(&raw) {
            lambda_name.clone()
        } else {
            raw
        }
    }

    fn intern_string(&mut self, s: String) -> u32 {
        if let Some(&idx) = self.string_table.get(&s) {
            return idx;
        }
        let idx = self.module.string_constants.len() as u32;
        self.module.string_constants.push(s.clone());
        self.string_table.insert(s, idx);
        idx
    }
}

// ---------------------------------------------------------------------------
// Type size helper
// ---------------------------------------------------------------------------

/// Return the byte size of an `IrType` in linear memory.
fn type_size(ty: IrType) -> u32 {
    match ty {
        IrType::I32 | IrType::Bool | IrType::Ptr => 4,
        IrType::I64 => 8,
        IrType::F32 => 4,
        IrType::F64 => 8,
        IrType::Unit => 0,
    }
}

// ---------------------------------------------------------------------------
// Type inference helpers (heuristic, pre-type-checker)
// ---------------------------------------------------------------------------

/// Simple heuristic: if the expression is a float literal or a variable known
/// to be F64, return `IrType::F64`; otherwise fall back to `IrType::I32`.
fn infer_expr_type(expr: &ast::Expr, ctx: &FnCtx) -> IrType {
    match &expr.kind {
        ExprKind::FloatLit(_) => IrType::F64,
        ExprKind::IntLit(_) => IrType::I32,
        ExprKind::BoolLit(_) => IrType::Bool,
        ExprKind::Ident(name) => {
            if let Some(idx) = ctx.lookup(name) {
                ctx.local_type(idx)
            } else {
                IrType::I32
            }
        }
        ExprKind::Binary { lhs, rhs, .. } => {
            let lt = infer_expr_type(lhs, ctx);
            let rt = infer_expr_type(rhs, ctx);
            if lt == IrType::F64 || rt == IrType::F64 {
                IrType::F64
            } else {
                IrType::I32
            }
        }
        ExprKind::Unary { operand, .. } => infer_expr_type(operand, ctx),
        _ => IrType::I32,
    }
}

/// Returns `true` if the inferred type is `F64`.
fn is_f64_context(lhs: &ast::Expr, rhs: &ast::Expr, ctx: &FnCtx) -> bool {
    infer_expr_type(lhs, ctx) == IrType::F64 || infer_expr_type(rhs, ctx) == IrType::F64
}

// ---------------------------------------------------------------------------
// Operator mapping
// ---------------------------------------------------------------------------

fn lower_binop(op: BinOp) -> IrBinOp {
    // Default to i32 variants.  A real type-directed lowering would pick
    // the correct width based on operand types.
    match op {
        BinOp::Add => IrBinOp::AddI32,
        BinOp::Sub => IrBinOp::SubI32,
        BinOp::Mul => IrBinOp::MulI32,
        BinOp::Div => IrBinOp::DivI32S,
        BinOp::Rem => IrBinOp::RemI32S,
        BinOp::Eq => IrBinOp::EqI32,
        BinOp::Neq => IrBinOp::NeI32,
        BinOp::Lt => IrBinOp::LtI32S,
        BinOp::Gt => IrBinOp::GtI32S,
        BinOp::LtEq => IrBinOp::LeI32S,
        BinOp::GtEq => IrBinOp::GeI32S,
        BinOp::And => IrBinOp::AndI32,
        BinOp::Or => IrBinOp::OrI32,
        BinOp::BitAnd => IrBinOp::AndI32,
        BinOp::BitOr => IrBinOp::OrI32,
        BinOp::BitXor => IrBinOp::XorI32,
        BinOp::Shl => IrBinOp::ShlI32,
        BinOp::Shr => IrBinOp::ShrI32S,
        BinOp::Spaceship => IrBinOp::SubI32, // placeholder
    }
}

fn lower_binop_f64(op: BinOp) -> IrBinOp {
    match op {
        BinOp::Add => IrBinOp::AddF64,
        BinOp::Sub => IrBinOp::SubF64,
        BinOp::Mul => IrBinOp::MulF64,
        BinOp::Div => IrBinOp::DivF64,
        BinOp::Eq => IrBinOp::EqF64,
        BinOp::Neq => IrBinOp::NeF64,
        BinOp::Lt => IrBinOp::LtF64,
        BinOp::Gt => IrBinOp::GtF64,
        BinOp::LtEq => IrBinOp::LeF64,
        BinOp::GtEq => IrBinOp::GeF64,
        // Rem, bitwise, shifts: not meaningful for f64 — fall back to i32.
        _ => lower_binop(op),
    }
}

fn lower_unaryop(op: UnaryOp) -> IrUnaryOp {
    match op {
        UnaryOp::Neg => IrUnaryOp::NegI32,
        UnaryOp::Not => IrUnaryOp::EqzI32,
        UnaryOp::BitNot => IrUnaryOp::NotI32,
        // Ref/RefMut/Deref are handled specially in lower_expr, not here
        UnaryOp::Ref | UnaryOp::RefMut | UnaryOp::Deref => IrUnaryOp::EqzI32,
    }
}

// ---------------------------------------------------------------------------
// Free variable collection for closure captures
// ---------------------------------------------------------------------------

/// Recursively walk an AST expression and collect identifier names that are
/// *not* in `bound` (the lambda's own parameters) but *are* present in the
/// enclosing `FnCtx`.  Each such name is a captured variable.
fn collect_free_vars(
    expr: &ast::Expr,
    bound: &std::collections::HashSet<SmolStr>,
    enclosing: &FnCtx,
    out: &mut Vec<SmolStr>,
    seen: &mut std::collections::HashSet<SmolStr>,
) {
    match &expr.kind {
        ExprKind::Ident(name) => {
            if !bound.contains(name)
                && enclosing.lookup(name).is_some()
                && seen.insert(name.clone())
            {
                out.push(name.clone());
            }
        }
        ExprKind::Binary { lhs, rhs, .. } => {
            collect_free_vars(lhs, bound, enclosing, out, seen);
            collect_free_vars(rhs, bound, enclosing, out, seen);
        }
        ExprKind::Unary { operand, .. } => {
            collect_free_vars(operand, bound, enclosing, out, seen);
        }
        ExprKind::Call { callee, args } => {
            collect_free_vars(callee, bound, enclosing, out, seen);
            for a in args {
                collect_free_vars(&a.value, bound, enclosing, out, seen);
            }
        }
        ExprKind::Block(block) => {
            for stmt in &block.stmts {
                collect_free_vars_stmt(stmt, bound, enclosing, out, seen);
            }
        }
        ExprKind::Return(Some(inner)) => {
            collect_free_vars(inner, bound, enclosing, out, seen);
        }
        ExprKind::If {
            condition,
            then_block,
            else_block,
        } => {
            collect_free_vars(condition, bound, enclosing, out, seen);
            for stmt in &then_block.stmts {
                collect_free_vars_stmt(stmt, bound, enclosing, out, seen);
            }
            if let Some(e) = else_block {
                collect_free_vars(e, bound, enclosing, out, seen);
            }
        }
        ExprKind::Assign { target, value, .. } => {
            collect_free_vars(target, bound, enclosing, out, seen);
            collect_free_vars(value, bound, enclosing, out, seen);
        }
        ExprKind::MethodCall { object, args, .. } => {
            collect_free_vars(object, bound, enclosing, out, seen);
            for a in args {
                collect_free_vars(&a.value, bound, enclosing, out, seen);
            }
        }
        ExprKind::FieldAccess { object, .. } => {
            collect_free_vars(object, bound, enclosing, out, seen);
        }
        ExprKind::Index { object, index } => {
            collect_free_vars(object, bound, enclosing, out, seen);
            collect_free_vars(index, bound, enclosing, out, seen);
        }
        ExprKind::Cast { expr, .. } => {
            collect_free_vars(expr, bound, enclosing, out, seen);
        }
        ExprKind::Pipe { lhs, rhs } => {
            collect_free_vars(lhs, bound, enclosing, out, seen);
            collect_free_vars(rhs, bound, enclosing, out, seen);
        }
        _ => {}
    }
}

/// Walk a statement looking for free variable references.
fn collect_free_vars_stmt(
    stmt: &Stmt,
    bound: &std::collections::HashSet<SmolStr>,
    enclosing: &FnCtx,
    out: &mut Vec<SmolStr>,
    seen: &mut std::collections::HashSet<SmolStr>,
) {
    match stmt {
        Stmt::Expr(es) => collect_free_vars(&es.expr, bound, enclosing, out, seen),
        Stmt::Let(l) => {
            if let Some(init) = &l.init {
                collect_free_vars(init, bound, enclosing, out, seen);
            }
        }
        _ => {}
    }
}

fn script_type_to_ir(st: &ScriptType) -> IrType {
    match st {
        ScriptType::I32 => IrType::I32,
        ScriptType::I64 => IrType::I64,
        ScriptType::F32 => IrType::F32,
        ScriptType::F64 => IrType::F64,
        ScriptType::Bool => IrType::Bool,
        ScriptType::Str => IrType::I32,
        ScriptType::Unit => IrType::Unit,
    }
}

fn type_expr_name(t: &ast::TypeExpr) -> Option<SmolStr> {
    match &t.kind {
        ast::TypeExprKind::Named { name, .. } => Some(name.clone()),
        ast::TypeExprKind::StringType => Some(SmolStr::new("str")),
        _ => None,
    }
}
