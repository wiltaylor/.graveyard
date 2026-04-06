//! WASM code generation from IR via the walrus crate.
//!
//! Uses `FunctionBuilder` to emit instructions, which is the supported
//! walrus API (rather than manually constructing `Instr` values).

use std::collections::HashMap;

use smol_str::SmolStr;
use walrus::{
    ConstExpr, DataKind, FunctionBuilder, FunctionId, GlobalId, InstrSeqBuilder, LocalId,
    MemoryId, Module, ValType,
};

use super::ir::*;
use super::source_map::{SourceMap, SourceMapEntry};
use crate::bindings::ScriptType;
use crate::reflect::{FieldInfo, FieldType, GlobalInfo, GlobalKind, StructTypeInfo, TypeLayouts};

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Generate WASM bytes (and a source map) from an IR module.
pub fn codegen(ir: &IrModule, debug_mode: bool) -> (Vec<u8>, SourceMap, TypeLayouts) {
    let mut cg = WasmCodegen::new(debug_mode);
    cg.emit_module(ir);
    let wasm_bytes = cg.module.emit_wasm();
    let layouts = build_type_layouts(ir);
    (wasm_bytes, cg.source_map, layouts)
}

/// Build a `TypeLayouts` from the IR struct layouts. User-facing struct
/// types only — skips internal `__` prefixed layouts.
fn build_type_layouts(ir: &IrModule) -> TypeLayouts {
    let mut out = TypeLayouts::default();
    for layout in &ir.struct_layouts {
        if layout.name.starts_with("__") {
            continue;
        }
        let fields = layout
            .fields
            .iter()
            .map(|f| {
                let ty = match f.type_name.as_deref() {
                    Some("str") => FieldType::Primitive(ScriptType::Str),
                    Some(name)
                        if ir
                            .struct_layouts
                            .iter()
                            .any(|s| s.name.as_str() == name && !name.starts_with("__")) =>
                    {
                        FieldType::Struct(name.to_string())
                    }
                    _ => FieldType::Primitive(ir_to_script_type(f.ty)),
                };
                FieldInfo { name: f.name.to_string(), ty, offset: f.offset }
            })
            .collect();
        out.structs.push(StructTypeInfo {
            name: layout.name.to_string(),
            size: layout.size,
            fields,
        });
    }

    // Populate GlobalInfo from IR globals. Struct/str classification comes
    // from the `type_name` field recorded by the lowerer.
    for g in &ir.globals {
        let kind = match g.type_name.as_deref() {
            Some("str") => GlobalKind::Primitive(ScriptType::Str),
            Some(name)
                if ir
                    .struct_layouts
                    .iter()
                    .any(|s| s.name.as_str() == name && !name.starts_with("__")) =>
            {
                GlobalKind::Struct(name.to_string())
            }
            _ => GlobalKind::Primitive(ir_to_script_type(g.ty)),
        };
        out.globals.push(GlobalInfo {
            name: g.name.to_string(),
            mutable: g.mutable,
            kind,
        });
    }
    out
}

fn ir_to_script_type(ty: IrType) -> ScriptType {
    match ty {
        IrType::I32 => ScriptType::I32,
        IrType::I64 => ScriptType::I64,
        IrType::F32 => ScriptType::F32,
        IrType::F64 => ScriptType::F64,
        IrType::Bool => ScriptType::Bool,
        IrType::Ptr => ScriptType::I32,
        IrType::Unit => ScriptType::Unit,
    }
}

// ---------------------------------------------------------------------------
// WasmCodegen state
// ---------------------------------------------------------------------------

struct WasmCodegen {
    module: Module,
    fn_ids: HashMap<SmolStr, FunctionId>,
    host_fn_ids: HashMap<SmolStr, FunctionId>,
    /// User-declared top-level globals (from script `let` items), keyed by
    /// script name. Exported as `g_<name>` so the host `Vm` can read/write.
    user_globals: HashMap<SmolStr, GlobalId>,
    /// (offset, length) for each string constant in linear memory.
    string_offsets: Vec<(u32, u32)>,
    #[allow(dead_code)]
    memory: MemoryId,
    #[allow(dead_code)]
    stack_ptr: GlobalId,
    /// Bump allocator pointer — starts after string data segment.
    heap_ptr: GlobalId,
    source_map: SourceMap,
    #[allow(dead_code)]
    debug_mode: bool,
}

impl WasmCodegen {
    fn new(debug_mode: bool) -> Self {
        let mut module = Module::default();
        let memory = module.memories.add_local(false, false, 1, None, None);
        let stack_ptr = module.globals.add_local(
            ValType::I32,
            true,
            false,
            ConstExpr::Value(walrus::ir::Value::I32(65536)),
        );
        // Bump allocator pointer — initialized to 4096, after string data segment region.
        // Will be adjusted after string data is placed.
        let heap_ptr = module.globals.add_local(
            ValType::I32,
            true,
            false,
            ConstExpr::Value(walrus::ir::Value::I32(4096)),
        );
        Self {
            module,
            fn_ids: HashMap::new(),
            host_fn_ids: HashMap::new(),
            user_globals: HashMap::new(),
            string_offsets: Vec::new(),
            memory,
            stack_ptr,
            heap_ptr,
            source_map: SourceMap::new(),
            debug_mode,
        }
    }
}

// ---------------------------------------------------------------------------
// Module emission
// ---------------------------------------------------------------------------

impl WasmCodegen {
    fn emit_module(&mut self, ir: &IrModule) {
        // Register host imports.
        let print_i32_ty = self.module.types.add(&[ValType::I32], &[]);
        let (print_i32_fn, _) = self.module.add_import_func("env", "__print_i32", print_i32_ty);
        self.host_fn_ids.insert("__print_i32".into(), print_i32_fn);

        // String host imports.
        let str_new_ty = self.module.types.add(&[ValType::I32, ValType::I32], &[ValType::I32]);
        let (str_new_fn, _) = self.module.add_import_func("env", "__str_new", str_new_ty);
        self.host_fn_ids.insert("__str_new".into(), str_new_fn);

        let str_len_ty = self.module.types.add(&[ValType::I32], &[ValType::I32]);
        let (str_len_fn, _) = self.module.add_import_func("env", "__str_len", str_len_ty);
        self.host_fn_ids.insert("__str_len".into(), str_len_fn);

        let str_concat_ty = self.module.types.add(&[ValType::I32, ValType::I32], &[ValType::I32]);
        let (str_concat_fn, _) = self.module.add_import_func("env", "__str_concat", str_concat_ty);
        self.host_fn_ids.insert("__str_concat".into(), str_concat_fn);

        let str_print_ty = self.module.types.add(&[ValType::I32], &[]);
        let (str_print_fn, _) = self.module.add_import_func("env", "__str_print", str_print_ty);
        self.host_fn_ids.insert("__str_print".into(), str_print_fn);

        let str_eq_ty = self.module.types.add(&[ValType::I32, ValType::I32], &[ValType::I32]);
        let (str_eq_fn, _) = self.module.add_import_func("env", "__str_eq", str_eq_ty);
        self.host_fn_ids.insert("__str_eq".into(), str_eq_fn);

        // Array host imports.
        let arr_new_ty = self.module.types.add(&[], &[ValType::I32]);
        let (arr_new_fn, _) = self.module.add_import_func("env", "__arr_new", arr_new_ty);
        self.host_fn_ids.insert("__arr_new".into(), arr_new_fn);

        let arr_push_ty = self.module.types.add(&[ValType::I32, ValType::I32], &[]);
        let (arr_push_fn, _) = self.module.add_import_func("env", "__arr_push", arr_push_ty);
        self.host_fn_ids.insert("__arr_push".into(), arr_push_fn);

        let arr_len_ty = self.module.types.add(&[ValType::I32], &[ValType::I32]);
        let (arr_len_fn, _) = self.module.add_import_func("env", "__arr_len", arr_len_ty);
        self.host_fn_ids.insert("__arr_len".into(), arr_len_fn);

        let arr_get_ty = self.module.types.add(&[ValType::I32, ValType::I32], &[ValType::I32]);
        let (arr_get_fn, _) = self.module.add_import_func("env", "__arr_get", arr_get_ty);
        self.host_fn_ids.insert("__arr_get".into(), arr_get_fn);

        // __i32_to_str(value: i32) -> i32
        let i32_to_str_ty = self.module.types.add(&[ValType::I32], &[ValType::I32]);
        let (i32_to_str_fn, _) = self.module.add_import_func("env", "__i32_to_str", i32_to_str_ty);
        self.host_fn_ids.insert("__i32_to_str".into(), i32_to_str_fn);

        // __arr_sum(arr: i32) -> i32
        let arr_sum_ty = self.module.types.add(&[ValType::I32], &[ValType::I32]);
        let (arr_sum_fn, _) = self.module.add_import_func("env", "__arr_sum", arr_sum_ty);
        self.host_fn_ids.insert("__arr_sum".into(), arr_sum_fn);

        // __arr_contains(arr: i32, val: i32) -> i32
        let arr_contains_ty = self.module.types.add(&[ValType::I32, ValType::I32], &[ValType::I32]);
        let (arr_contains_fn, _) = self.module.add_import_func("env", "__arr_contains", arr_contains_ty);
        self.host_fn_ids.insert("__arr_contains".into(), arr_contains_fn);

        // __arr_reverse(arr: i32) -> i32
        let arr_reverse_ty = self.module.types.add(&[ValType::I32], &[ValType::I32]);
        let (arr_reverse_fn, _) = self.module.add_import_func("env", "__arr_reverse", arr_reverse_ty);
        self.host_fn_ids.insert("__arr_reverse".into(), arr_reverse_fn);

        // __arr_first(arr: i32) -> i32
        let arr_first_ty = self.module.types.add(&[ValType::I32], &[ValType::I32]);
        let (arr_first_fn, _) = self.module.add_import_func("env", "__arr_first", arr_first_ty);
        self.host_fn_ids.insert("__arr_first".into(), arr_first_fn);

        // __arr_last(arr: i32) -> i32
        let arr_last_ty = self.module.types.add(&[ValType::I32], &[ValType::I32]);
        let (arr_last_fn, _) = self.module.add_import_func("env", "__arr_last", arr_last_ty);
        self.host_fn_ids.insert("__arr_last".into(), arr_last_fn);

        // __arr_min(arr: i32) -> i32
        let arr_min_ty = self.module.types.add(&[ValType::I32], &[ValType::I32]);
        let (arr_min_fn, _) = self.module.add_import_func("env", "__arr_min", arr_min_ty);
        self.host_fn_ids.insert("__arr_min".into(), arr_min_fn);

        // __arr_max(arr: i32) -> i32
        let arr_max_ty = self.module.types.add(&[ValType::I32], &[ValType::I32]);
        let (arr_max_fn, _) = self.module.add_import_func("env", "__arr_max", arr_max_ty);
        self.host_fn_ids.insert("__arr_max".into(), arr_max_fn);

        // __arr_sort(arr: i32) -> i32
        let arr_sort_ty = self.module.types.add(&[ValType::I32], &[ValType::I32]);
        let (arr_sort_fn, _) = self.module.add_import_func("env", "__arr_sort", arr_sort_ty);
        self.host_fn_ids.insert("__arr_sort".into(), arr_sort_fn);

        // __arr_dedup(arr: i32) -> i32
        let arr_dedup_ty = self.module.types.add(&[ValType::I32], &[ValType::I32]);
        let (arr_dedup_fn, _) = self.module.add_import_func("env", "__arr_dedup", arr_dedup_ty);
        self.host_fn_ids.insert("__arr_dedup".into(), arr_dedup_fn);

        // __arr_join_str(arr: i32, sep: i32) -> i32
        let arr_join_str_ty = self.module.types.add(&[ValType::I32, ValType::I32], &[ValType::I32]);
        let (arr_join_str_fn, _) = self.module.add_import_func("env", "__arr_join_str", arr_join_str_ty);
        self.host_fn_ids.insert("__arr_join_str".into(), arr_join_str_fn);

        // ── New string host imports ──────────────────────────────────

        // __str_contains(s: i32, sub: i32) -> i32
        let str_contains_ty = self.module.types.add(&[ValType::I32, ValType::I32], &[ValType::I32]);
        let (str_contains_fn, _) = self.module.add_import_func("env", "__str_contains", str_contains_ty);
        self.host_fn_ids.insert("__str_contains".into(), str_contains_fn);

        // __str_starts_with(s: i32, prefix: i32) -> i32
        let str_starts_with_ty = self.module.types.add(&[ValType::I32, ValType::I32], &[ValType::I32]);
        let (str_starts_with_fn, _) = self.module.add_import_func("env", "__str_starts_with", str_starts_with_ty);
        self.host_fn_ids.insert("__str_starts_with".into(), str_starts_with_fn);

        // __str_ends_with(s: i32, suffix: i32) -> i32
        let str_ends_with_ty = self.module.types.add(&[ValType::I32, ValType::I32], &[ValType::I32]);
        let (str_ends_with_fn, _) = self.module.add_import_func("env", "__str_ends_with", str_ends_with_ty);
        self.host_fn_ids.insert("__str_ends_with".into(), str_ends_with_fn);

        // __str_trim(s: i32) -> i32
        let str_trim_ty = self.module.types.add(&[ValType::I32], &[ValType::I32]);
        let (str_trim_fn, _) = self.module.add_import_func("env", "__str_trim", str_trim_ty);
        self.host_fn_ids.insert("__str_trim".into(), str_trim_fn);

        // __str_to_upper(s: i32) -> i32
        let str_to_upper_ty = self.module.types.add(&[ValType::I32], &[ValType::I32]);
        let (str_to_upper_fn, _) = self.module.add_import_func("env", "__str_to_upper", str_to_upper_ty);
        self.host_fn_ids.insert("__str_to_upper".into(), str_to_upper_fn);

        // __str_to_lower(s: i32) -> i32
        let str_to_lower_ty = self.module.types.add(&[ValType::I32], &[ValType::I32]);
        let (str_to_lower_fn, _) = self.module.add_import_func("env", "__str_to_lower", str_to_lower_ty);
        self.host_fn_ids.insert("__str_to_lower".into(), str_to_lower_fn);

        // __str_replace(s: i32, from: i32, to: i32) -> i32
        let str_replace_ty = self.module.types.add(&[ValType::I32, ValType::I32, ValType::I32], &[ValType::I32]);
        let (str_replace_fn, _) = self.module.add_import_func("env", "__str_replace", str_replace_ty);
        self.host_fn_ids.insert("__str_replace".into(), str_replace_fn);

        // __str_split(s: i32, sep: i32) -> i32
        let str_split_ty = self.module.types.add(&[ValType::I32, ValType::I32], &[ValType::I32]);
        let (str_split_fn, _) = self.module.add_import_func("env", "__str_split", str_split_ty);
        self.host_fn_ids.insert("__str_split".into(), str_split_fn);

        // __str_char_count(s: i32) -> i32
        let str_char_count_ty = self.module.types.add(&[ValType::I32], &[ValType::I32]);
        let (str_char_count_fn, _) = self.module.add_import_func("env", "__str_char_count", str_char_count_ty);
        self.host_fn_ids.insert("__str_char_count".into(), str_char_count_fn);

        // __str_is_empty(s: i32) -> i32
        let str_is_empty_ty = self.module.types.add(&[ValType::I32], &[ValType::I32]);
        let (str_is_empty_fn, _) = self.module.add_import_func("env", "__str_is_empty", str_is_empty_ty);
        self.host_fn_ids.insert("__str_is_empty".into(), str_is_empty_fn);

        // __str_repeat(s: i32, n: i32) -> i32
        let str_repeat_ty = self.module.types.add(&[ValType::I32, ValType::I32], &[ValType::I32]);
        let (str_repeat_fn, _) = self.module.add_import_func("env", "__str_repeat", str_repeat_ty);
        self.host_fn_ids.insert("__str_repeat".into(), str_repeat_fn);

        // __print_f64(value_bits: i64)
        let print_f64_ty = self.module.types.add(&[ValType::I64], &[]);
        let (print_f64_fn, _) = self.module.add_import_func("env", "__print_f64", print_f64_ty);
        self.host_fn_ids.insert("__print_f64".into(), print_f64_fn);

        // __print_bool(val: i32)
        let print_bool_ty = self.module.types.add(&[ValType::I32], &[]);
        let (print_bool_fn, _) = self.module.add_import_func("env", "__print_bool", print_bool_ty);
        self.host_fn_ids.insert("__print_bool".into(), print_bool_fn);

        // __arr_to_str(arr: i32) -> i32
        let arr_to_str_ty = self.module.types.add(&[ValType::I32], &[ValType::I32]);
        let (arr_to_str_fn, _) = self.module.add_import_func("env", "__arr_to_str", arr_to_str_ty);
        self.host_fn_ids.insert("__arr_to_str".into(), arr_to_str_fn);

        // Map host imports.
        // __map_new() -> i32
        let map_new_ty = self.module.types.add(&[], &[ValType::I32]);
        let (map_new_fn, _) = self.module.add_import_func("env", "__map_new", map_new_ty);
        self.host_fn_ids.insert("__map_new".into(), map_new_fn);

        // __map_set(map: i32, key: i32, val: i32)
        let map_set_ty = self.module.types.add(&[ValType::I32, ValType::I32, ValType::I32], &[]);
        let (map_set_fn, _) = self.module.add_import_func("env", "__map_set", map_set_ty);
        self.host_fn_ids.insert("__map_set".into(), map_set_fn);

        // __map_get(map: i32, key: i32) -> i32
        let map_get_ty = self.module.types.add(&[ValType::I32, ValType::I32], &[ValType::I32]);
        let (map_get_fn, _) = self.module.add_import_func("env", "__map_get", map_get_ty);
        self.host_fn_ids.insert("__map_get".into(), map_get_fn);

        // __map_len(map: i32) -> i32
        let map_len_ty = self.module.types.add(&[ValType::I32], &[ValType::I32]);
        let (map_len_fn, _) = self.module.add_import_func("env", "__map_len", map_len_ty);
        self.host_fn_ids.insert("__map_len".into(), map_len_fn);

        // __panic(msg_handle: i32) — trap the script
        let panic_ty = self.module.types.add(&[ValType::I32], &[]);
        let (panic_fn, _) = self.module.add_import_func("env", "__panic", panic_ty);
        self.host_fn_ids.insert("__panic".into(), panic_fn);

        // User-registered host function imports (module: "host").
        for imp in &ir.user_host_imports {
            let params: Vec<ValType> = imp.params.iter().map(|t| ir_to_val(*t)).collect();
            let results: Vec<ValType> = match imp.ret {
                IrType::Unit => vec![],
                other => vec![ir_to_val(other)],
            };
            let ty = self.module.types.add(&params, &results);
            let (fid, _) = self.module.add_import_func("host", imp.name.as_str(), ty);
            self.host_fn_ids.insert(imp.name.clone(), fid);
        }

        // Embed string constants as a data segment in linear memory.
        // Place string data at offset 0 in memory (before the stack).
        let mut data_buf: Vec<u8> = Vec::new();
        for s in &ir.string_constants {
            let offset = data_buf.len() as u32;
            let bytes = s.as_bytes();
            data_buf.extend_from_slice(bytes);
            self.string_offsets.push((offset, bytes.len() as u32));
        }
        if !data_buf.is_empty() {
            let memory = self.memory;
            self.module.data.add(
                DataKind::Active {
                    memory,
                    offset: ConstExpr::Value(walrus::ir::Value::I32(0)),
                },
                data_buf,
            );
        }

        // User-declared globals: emit as WASM locals with const initializers
        // and export as `g_<name>` for host read/write via the Vm API.
        for g in &ir.globals {
            let vt = ir_to_val(g.ty);
            let init = const_expr_for_init(g.ty, g.init.as_ref());
            let gid = self.module.globals.add_local(vt, g.mutable, false, init);
            let export_name = format!("g_{}", g.name);
            self.module.exports.add(&export_name, gid);
            self.user_globals.insert(g.name.clone(), gid);
        }

        // Two-pass approach: first register all function signatures so that
        // forward references and self-recursion resolve correctly.

        // Pass 1: Create placeholder functions to reserve FunctionIds.
        // We store the allocated locals alongside each registration so
        // pass 2 can use them when emitting the real bodies.
        struct FuncReg {
            fid: FunctionId,
            all_locals: Vec<LocalId>,
            scratch_locals: Vec<LocalId>,
            result_tys: Vec<ValType>,
        }
        let mut registrations: Vec<FuncReg> = Vec::new();

        for func in &ir.functions {
            let param_tys: Vec<ValType> = func.params.iter().map(|p| ir_to_val(p.ty)).collect();
            let result_tys: Vec<ValType> = match func.ret_type {
                IrType::Unit => vec![],
                other => vec![ir_to_val(other)],
            };

            let mut builder =
                FunctionBuilder::new(&mut self.module.types, &param_tys, &result_tys);
            builder.name(func.name.to_string());

            // Allocate param locals.
            let param_locals: Vec<LocalId> = param_tys
                .iter()
                .map(|t| self.module.locals.add(*t))
                .collect();

            // Allocate extra locals (beyond params).
            let mut all_locals = param_locals.clone();
            for local in func.locals.iter().skip(func.params.len()) {
                let lid = self.module.locals.add(ir_to_val(local.ty));
                all_locals.push(lid);
            }

            // Preallocate a pool of i32 scratch locals used by StructNew to
            // hold the allocation base pointer across field stores. One per
            // nesting depth — StructNew can be emitted recursively when a
            // field expression is itself a struct literal, and each depth
            // must use its own slot so an inner emission doesn't clobber an
            // outer's saved base.
            const STRUCT_SCRATCH_DEPTH: usize = 8;
            let scratch_locals: Vec<LocalId> = (0..STRUCT_SCRATCH_DEPTH)
                .map(|_| self.module.locals.add(ValType::I32))
                .collect();

            // Emit a minimal placeholder body so we can finish the builder.
            {
                let mut body = builder.func_body();
                for &rt in &result_tys {
                    emit_default_val(&mut body, rt);
                }
            }
            let fid = builder.finish(param_locals, &mut self.module.funcs);
            self.fn_ids.insert(func.name.clone(), fid);
            registrations.push(FuncReg { fid, all_locals, scratch_locals, result_tys });
        }

        // Pass 2: Now that all FunctionIds are registered, replace each
        // placeholder body with the real instructions via builder_mut().
        for (func, reg) in ir.functions.iter().zip(registrations.iter()) {
            let local_fn = self.module.funcs.get_mut(reg.fid).kind.unwrap_local_mut();
            let entry = local_fn.entry_block();
            // Clear the placeholder instructions from pass 1.
            local_fn.block_mut(entry).instrs.clear();

            // Re-emit the real body through the builder's instr_seq API.
            let fn_ids = self.fn_ids.clone();
            let host_fn_ids = self.host_fn_ids.clone();
            let user_globals = self.user_globals.clone();
            let fn_ret_types: HashMap<SmolStr, IrType> = ir
                .functions
                .iter()
                .map(|f| (f.name.clone(), f.ret_type))
                .collect();
            let struct_depth = std::cell::Cell::new(0usize);
            let builder = local_fn.builder_mut();
            {
                let mut body = builder.instr_seq(entry);
                let ctx = EmitCtx {
                    locals: &reg.all_locals,
                    fn_ids: &fn_ids,
                    fn_ret_types: &fn_ret_types,
                    host_fn_ids: &host_fn_ids,
                    user_globals: &user_globals,
                    string_offsets: &self.string_offsets,
                    struct_layouts: &ir.struct_layouts,
                    struct_scratch: &reg.scratch_locals,
                    struct_depth: &struct_depth,
                    memory: self.memory,
                    stack_ptr: self.stack_ptr,
                    heap_ptr: self.heap_ptr,
                };
                emit_stmts(&func.body, &mut body, &ctx);

                // Emit a default return value to satisfy WASM validation
                // in case not all code paths return explicitly.
                if !reg.result_tys.is_empty() {
                    emit_default_val(&mut body, reg.result_tys[0]);
                }
            }

            // Record source map entry.
            if let Some(span) = func.source_span {
                self.source_map.add_entry(SourceMapEntry {
                    wasm_offset: 0,
                    span,
                    fn_name: Some(func.name.to_string()),
                    local_names: HashMap::new(),
                });
            }
        }

        // Wire the synthesized `__wscript_init_globals` fn as the WASM start
        // section so Wasmtime runs it automatically during instantiation.
        if let Some(&fid) = self.fn_ids.get(crate::compiler::lower::INIT_GLOBALS_FN) {
            self.module.start = Some(fid);
        }

        // Export functions marked for export.
        let exports: Vec<(SmolStr, FunctionId)> = ir
            .functions
            .iter()
            .filter(|f| f.is_export)
            .filter_map(|f| self.fn_ids.get(&f.name).map(|&id| (f.name.clone(), id)))
            .collect();
        for (name, fid) in exports {
            self.module.exports.add(name.as_str(), fid);
        }

        // Export memory.
        self.module.exports.add("memory", self.memory);
    }
}

// ---------------------------------------------------------------------------
// Emission context
// ---------------------------------------------------------------------------

struct EmitCtx<'a> {
    locals: &'a [LocalId],
    fn_ids: &'a HashMap<SmolStr, FunctionId>,
    fn_ret_types: &'a HashMap<SmolStr, IrType>,
    host_fn_ids: &'a HashMap<SmolStr, FunctionId>,
    user_globals: &'a HashMap<SmolStr, GlobalId>,
    string_offsets: &'a [(u32, u32)],
    struct_layouts: &'a [StructLayout],
    /// Pool of i32 scratch locals for StructNew allocation bookkeeping,
    /// one per nesting depth.
    struct_scratch: &'a [LocalId],
    /// Current StructNew nesting depth — indexes into `struct_scratch`.
    struct_depth: &'a std::cell::Cell<usize>,
    memory: MemoryId,
    #[allow(dead_code)]
    stack_ptr: GlobalId,
    /// Bump allocator global for heap allocation.
    heap_ptr: GlobalId,
}

impl<'a> EmitCtx<'a> {
    fn local(&self, idx: u32) -> LocalId {
        self.locals
            .get(idx as usize)
            .or_else(|| self.locals.first())
            .copied()
            .expect("EmitCtx::local called with no locals allocated")
    }
}

// ---------------------------------------------------------------------------
// Statement emission
// ---------------------------------------------------------------------------

fn emit_stmts(stmts: &[IrStmt], body: &mut InstrSeqBuilder, ctx: &EmitCtx) {
    for stmt in stmts {
        emit_stmt(stmt, body, ctx);
    }
}

fn emit_stmt(stmt: &IrStmt, body: &mut InstrSeqBuilder, ctx: &EmitCtx) {
    match stmt {
        IrStmt::Let { local, value, .. } => {
            if let Some(val) = value {
                emit_expr(val, body, ctx);
                body.local_set(ctx.local(*local));
            }
        }
        IrStmt::Assign { target, value } => {
            emit_expr(value, body, ctx);
            match target {
                IrLValue::Local(idx) => {
                    body.local_set(ctx.local(*idx));
                }
                IrLValue::Global(name) => {
                    if let Some(&gid) = ctx.user_globals.get(name) {
                        body.instr(walrus::ir::GlobalSet { global: gid });
                    } else {
                        body.drop();
                    }
                }
                _ => {
                    body.drop();
                }
            }
        }
        IrStmt::Expr(e) => {
            emit_expr(e, body, ctx);
            body.drop();
        }
        IrStmt::Return(val) => {
            if let Some(v) = val {
                emit_expr(v, body, ctx);
            }
            body.return_();
        }
        IrStmt::If { condition, then_body, else_body } => {
            emit_expr(condition, body, ctx);
            body.if_else(
                None, // no result type for statement if
                |then| {
                    emit_stmts(then_body, then, ctx);
                },
                |else_| {
                    emit_stmts(else_body, else_, ctx);
                },
            );
        }
        IrStmt::Loop { body: loop_body } => {
            // WASM loop pattern:
            //   block $break_target
            //     loop $continue_target
            //       <body>  (IrStmt::Break emits br to $break_target)
            //       br $continue_target  (implicit re-loop)
            //     end
            //   end
            body.block(None, |outer_block| {
                let break_target = outer_block.id();
                outer_block.loop_(None, |loop_block| {
                    let continue_target = loop_block.id();
                    for stmt in loop_body {
                        emit_stmt_with_labels(stmt, loop_block, ctx, break_target, continue_target);
                    }
                    // Re-loop: branch back to loop start
                    loop_block.br(continue_target);
                });
            });
        }
        IrStmt::Break | IrStmt::Continue => {
            // At top level (outside loop) these are no-ops.
            // Inside loops, emit_stmt_with_labels handles them.
        }
        IrStmt::DebugProbe { .. } => {
            // Debug probes are no-ops in WASM output for now
        }
    }
}

/// Like emit_stmt but with loop break/continue label targets.
fn emit_stmt_with_labels(
    stmt: &IrStmt,
    body: &mut InstrSeqBuilder,
    ctx: &EmitCtx,
    break_target: walrus::ir::InstrSeqId,
    continue_target: walrus::ir::InstrSeqId,
) {
    match stmt {
        IrStmt::Break => {
            body.br(break_target);
        }
        IrStmt::Continue => {
            body.br(continue_target);
        }
        IrStmt::If { condition, then_body, else_body } => {
            emit_expr(condition, body, ctx);
            body.if_else(
                None,
                |then| {
                    for s in then_body {
                        emit_stmt_with_labels(s, then, ctx, break_target, continue_target);
                    }
                },
                |else_| {
                    for s in else_body {
                        emit_stmt_with_labels(s, else_, ctx, break_target, continue_target);
                    }
                },
            );
        }
        // For all other statements, delegate to the regular emit_stmt
        other => emit_stmt(other, body, ctx),
    }
}

// ---------------------------------------------------------------------------
// Expression emission
// ---------------------------------------------------------------------------

fn emit_expr(expr: &IrExpr, body: &mut InstrSeqBuilder, ctx: &EmitCtx) {
    match expr {
        IrExpr::IntConst(v) => {
            body.i32_const(*v as i32);
        }
        IrExpr::FloatConst(v) => {
            body.f64_const(*v);
        }
        IrExpr::BoolConst(v) => {
            body.i32_const(if *v { 1 } else { 0 });
        }
        IrExpr::StringConst(idx) => {
            // Create a string handle via __str_new(ptr, len) host call.
            if let Some(&(offset, len)) = ctx.string_offsets.get(*idx as usize) {
                body.i32_const(offset as i32);
                body.i32_const(len as i32);
                if let Some(&fid) = ctx.host_fn_ids.get("__str_new") {
                    body.call(fid);
                } else {
                    // Fallback: drop len, keep ptr
                    body.drop();
                }
            } else {
                body.i32_const(0);
            }
        }
        IrExpr::GlobalGet(name) => {
            if let Some(&gid) = ctx.user_globals.get(name) {
                body.instr(walrus::ir::GlobalGet { global: gid });
            } else {
                body.i32_const(0);
            }
        }
        IrExpr::LocalGet(idx) => {
            body.local_get(ctx.local(*idx));
        }
        IrExpr::LocalSet(idx, val) => {
            emit_expr(val, body, ctx);
            body.local_tee(ctx.local(*idx));
        }
        IrExpr::BinOp { op, lhs, rhs } => {
            emit_expr(lhs, body, ctx);
            emit_expr(rhs, body, ctx);
            emit_binop(*op, body);
        }
        IrExpr::UnaryOp { op, operand } => {
            emit_expr(operand, body, ctx);
            emit_unop(*op, body);
        }
        IrExpr::Call { func, args } => {
            for arg in args {
                emit_expr(arg, body, ctx);
            }
            if let Some(&fid) = ctx.fn_ids.get(func) {
                body.call(fid);
                // Unit-returning fns produce nothing on the WASM stack, but
                // we're in expression position here — push a dummy 0 so the
                // surrounding drop/use sees something.
                if matches!(ctx.fn_ret_types.get(func), Some(IrType::Unit)) {
                    body.i32_const(0);
                }
            } else {
                // Unknown function — drop args and push default
                for _ in args {
                    body.drop();
                }
                body.i32_const(0);
            }
        }
        IrExpr::HostCall { name, args, ret, .. } => {
            for arg in args {
                emit_expr(arg, body, ctx);
            }
            if let Some(&fid) = ctx.host_fn_ids.get(name) {
                body.call(fid);
                // If the host call has no return value but HostCall is used in
                // expression position, push a dummy 0.
                if *ret == IrType::Unit {
                    body.i32_const(0);
                }
            } else {
                // Unknown host function — drop args and push 0
                for _ in args {
                    body.drop();
                }
                body.i32_const(0);
            }
        }
        IrExpr::IfExpr { condition, then_expr, else_expr } => {
            emit_expr(condition, body, ctx);
            body.if_else(
                Some(ValType::I32),
                |then_body| {
                    emit_expr(then_expr, then_body, ctx);
                },
                |else_body| {
                    emit_expr(else_expr, else_body, ctx);
                },
            );
        }
        IrExpr::Block { stmts, result } => {
            emit_stmts(stmts, body, ctx);
            emit_expr(result, body, ctx);
        }
        IrExpr::Seq(exprs) => {
            for (i, e) in exprs.iter().enumerate() {
                emit_expr(e, body, ctx);
                if i < exprs.len() - 1 {
                    body.drop();
                }
            }
            if exprs.is_empty() {
                body.i32_const(0);
            }
        }
        IrExpr::Cast { expr, .. } => {
            emit_expr(expr, body, ctx);
        }

        // ── Struct operations ─────────────────────────────────────────
        IrExpr::StructNew { layout_index, fields } => {
            if let Some(layout) = ctx.struct_layouts.get(*layout_index as usize) {
                let memory = ctx.memory;
                let total_size = 4 + layout.size; // 4 bytes refcount header + struct data

                // Acquire a scratch local for this nesting depth. Nested
                // StructNew in a field expression gets the next slot so it
                // can't clobber our saved alloc_addr.
                let depth = ctx.struct_depth.get();
                let scratch = ctx.struct_scratch.get(depth).copied().expect(
                    "StructNew nested deeper than preallocated scratch pool",
                );
                ctx.struct_depth.set(depth + 1);

                // alloc_addr = heap_ptr; heap_ptr += total_size
                body.instr(walrus::ir::GlobalGet { global: ctx.heap_ptr });
                body.local_tee(scratch);               // [alloc_addr], scratch=alloc_addr
                body.i32_const(total_size as i32);
                body.binop(walrus::ir::BinaryOp::I32Add);
                body.instr(walrus::ir::GlobalSet { global: ctx.heap_ptr });

                // Store refcount = 1 at alloc_addr.
                body.local_get(scratch);               // [alloc_addr]
                body.i32_const(1);
                emit_store(body, memory, IrType::I32, 0);

                // Store each field at (alloc_addr + 4 + field.offset).
                // Field addr is pushed BEFORE the field expression runs, so
                // even if the field expression recursively invokes StructNew
                // (and advances heap_ptr or reuses deeper scratch slots),
                // our addr is already on the WASM stack.
                for (i, field_expr) in fields.iter().enumerate() {
                    if let Some(fl) = layout.fields.get(i) {
                        body.local_get(scratch);
                        body.i32_const(4 + fl.offset as i32);
                        body.binop(walrus::ir::BinaryOp::I32Add);
                        // Stack: [field_addr]
                        emit_expr(field_expr, body, ctx);
                        emit_store(body, memory, fl.ty, 0);
                    }
                }

                // Push data_ptr = alloc_addr + 4 as the expression result.
                body.local_get(scratch);
                body.i32_const(4);
                body.binop(walrus::ir::BinaryOp::I32Add);

                ctx.struct_depth.set(depth);
            } else {
                body.i32_const(0);
            }
        }
        IrExpr::FieldGet { object, layout_index, field_index } => {
            emit_expr(object, body, ctx); // struct ptr on stack
            if let Some(layout) = ctx.struct_layouts.get(*layout_index as usize) {
                if let Some(fl) = layout.fields.get(*field_index as usize) {
                    // addr = struct_ptr + field_offset
                    body.i32_const(fl.offset as i32);
                    body.binop(walrus::ir::BinaryOp::I32Add);
                    emit_load(body, ctx.memory, fl.ty, 0);
                } else {
                    body.drop();
                    body.i32_const(0);
                }
            } else {
                body.drop();
                body.i32_const(0);
            }
        }
        IrExpr::FieldSet { object, layout_index, field_index, value } => {
            emit_expr(object, body, ctx); // struct ptr
            if let Some(layout) = ctx.struct_layouts.get(*layout_index as usize) {
                if let Some(fl) = layout.fields.get(*field_index as usize) {
                    body.i32_const(fl.offset as i32);
                    body.binop(walrus::ir::BinaryOp::I32Add);
                    emit_expr(value, body, ctx);
                    emit_store(body, ctx.memory, fl.ty, 0);
                } else {
                    body.drop();
                }
            } else {
                body.drop();
            }
            // Push struct ptr as result
            emit_expr(object, body, ctx);
        }

        // ── Heap allocation (bump allocator) ─────────────────────────
        IrExpr::HeapAlloc { size } => {
            // result = heap_ptr; heap_ptr += size
            body.instr(walrus::ir::GlobalGet { global: ctx.heap_ptr });
            // Advance heap_ptr
            body.instr(walrus::ir::GlobalGet { global: ctx.heap_ptr });
            body.i32_const(*size as i32);
            body.binop(walrus::ir::BinaryOp::I32Add);
            body.instr(walrus::ir::GlobalSet { global: ctx.heap_ptr });
            // Stack: [old heap_ptr] = allocated address
        }

        // ── Heap load ────────────────────────────────────────────────
        IrExpr::HeapLoad { addr, offset, ty } => {
            emit_expr(addr, body, ctx);
            if *offset > 0 {
                body.i32_const(*offset as i32);
                body.binop(walrus::ir::BinaryOp::I32Add);
            }
            emit_load(body, ctx.memory, *ty, 0);
        }

        // ── Heap store ───────────────────────────────────────────────
        IrExpr::HeapStore { addr, offset, value, ty } => {
            emit_expr(addr, body, ctx);
            if *offset > 0 {
                body.i32_const(*offset as i32);
                body.binop(walrus::ir::BinaryOp::I32Add);
            }
            emit_expr(value, body, ctx);
            emit_store(body, ctx.memory, *ty, 0);
            // HeapStore as expression pushes 0 (unit-like)
            body.i32_const(0);
        }

        // ── Reference counting ───────────────────────────────────────
        IrExpr::RcIncr(ptr_expr) => {
            // refcount is at ptr - 4; increment it; push ptr
            emit_expr(ptr_expr, body, ctx);
            // Duplicate ptr: compute rc_addr = ptr - 4
            // We need ptr on the stack at the end, so re-emit
            emit_expr(ptr_expr, body, ctx);
            body.i32_const(4);
            body.binop(walrus::ir::BinaryOp::I32Sub);
            // Stack: [ptr, rc_addr]
            // Load refcount
            emit_expr(ptr_expr, body, ctx);
            body.i32_const(4);
            body.binop(walrus::ir::BinaryOp::I32Sub);
            emit_load(body, ctx.memory, IrType::I32, 0);
            // Stack: [ptr, rc_addr, refcount]
            body.i32_const(1);
            body.binop(walrus::ir::BinaryOp::I32Add);
            // Stack: [ptr, rc_addr, refcount+1]
            emit_store(body, ctx.memory, IrType::I32, 0);
            // Stack: [ptr]
        }

        IrExpr::RcDecr(ptr_expr) => {
            // refcount is at ptr - 4; decrement it; push ptr
            emit_expr(ptr_expr, body, ctx);
            emit_expr(ptr_expr, body, ctx);
            body.i32_const(4);
            body.binop(walrus::ir::BinaryOp::I32Sub);
            emit_expr(ptr_expr, body, ctx);
            body.i32_const(4);
            body.binop(walrus::ir::BinaryOp::I32Sub);
            emit_load(body, ctx.memory, IrType::I32, 0);
            body.i32_const(1);
            body.binop(walrus::ir::BinaryOp::I32Sub);
            emit_store(body, ctx.memory, IrType::I32, 0);
            // Stack: [ptr]
        }

        // Enum/indirect call — placeholder: push 0
        IrExpr::EnumTag(_)
        | IrExpr::EnumPayload { .. }
        | IrExpr::CallIndirect { .. } => {
            body.i32_const(0);
        }
    }
}

// ---------------------------------------------------------------------------
// Binary operator emission
// ---------------------------------------------------------------------------

fn emit_binop(op: IrBinOp, body: &mut InstrSeqBuilder) {
    use walrus::ir::BinaryOp;
    let wasm_op = match op {
        IrBinOp::AddI32 => BinaryOp::I32Add,
        IrBinOp::AddI64 => BinaryOp::I64Add,
        IrBinOp::AddF32 => BinaryOp::F32Add,
        IrBinOp::AddF64 => BinaryOp::F64Add,
        IrBinOp::SubI32 => BinaryOp::I32Sub,
        IrBinOp::SubI64 => BinaryOp::I64Sub,
        IrBinOp::SubF32 => BinaryOp::F32Sub,
        IrBinOp::SubF64 => BinaryOp::F64Sub,
        IrBinOp::MulI32 => BinaryOp::I32Mul,
        IrBinOp::MulI64 => BinaryOp::I64Mul,
        IrBinOp::MulF32 => BinaryOp::F32Mul,
        IrBinOp::MulF64 => BinaryOp::F64Mul,
        IrBinOp::DivI32S => BinaryOp::I32DivS,
        IrBinOp::DivI64S => BinaryOp::I64DivS,
        IrBinOp::DivI32U => BinaryOp::I32DivU,
        IrBinOp::DivI64U => BinaryOp::I64DivU,
        IrBinOp::DivF32 => BinaryOp::F32Div,
        IrBinOp::DivF64 => BinaryOp::F64Div,
        IrBinOp::RemI32S => BinaryOp::I32RemS,
        IrBinOp::RemI64S => BinaryOp::I64RemS,
        IrBinOp::RemI32U => BinaryOp::I32RemU,
        IrBinOp::RemI64U => BinaryOp::I64RemU,
        IrBinOp::AndI32 => BinaryOp::I32And,
        IrBinOp::AndI64 => BinaryOp::I64And,
        IrBinOp::OrI32 => BinaryOp::I32Or,
        IrBinOp::OrI64 => BinaryOp::I64Or,
        IrBinOp::XorI32 => BinaryOp::I32Xor,
        IrBinOp::XorI64 => BinaryOp::I64Xor,
        IrBinOp::ShlI32 => BinaryOp::I32Shl,
        IrBinOp::ShlI64 => BinaryOp::I64Shl,
        IrBinOp::ShrI32S => BinaryOp::I32ShrS,
        IrBinOp::ShrI64S => BinaryOp::I64ShrS,
        IrBinOp::ShrI32U => BinaryOp::I32ShrU,
        IrBinOp::ShrI64U => BinaryOp::I64ShrU,
        IrBinOp::EqI32 => BinaryOp::I32Eq,
        IrBinOp::EqI64 => BinaryOp::I64Eq,
        IrBinOp::NeI32 => BinaryOp::I32Ne,
        IrBinOp::NeI64 => BinaryOp::I64Ne,
        IrBinOp::LtI32S => BinaryOp::I32LtS,
        IrBinOp::LtI64S => BinaryOp::I64LtS,
        IrBinOp::LtI32U => BinaryOp::I32LtU,
        IrBinOp::LtI64U => BinaryOp::I64LtU,
        IrBinOp::GtI32S => BinaryOp::I32GtS,
        IrBinOp::GtI64S => BinaryOp::I64GtS,
        IrBinOp::GtI32U => BinaryOp::I32GtU,
        IrBinOp::GtI64U => BinaryOp::I64GtU,
        IrBinOp::LeI32S => BinaryOp::I32LeS,
        IrBinOp::LeI64S => BinaryOp::I64LeS,
        IrBinOp::LeI32U => BinaryOp::I32LeU,
        IrBinOp::LeI64U => BinaryOp::I64LeU,
        IrBinOp::GeI32S => BinaryOp::I32GeS,
        IrBinOp::GeI64S => BinaryOp::I64GeS,
        IrBinOp::GeI32U => BinaryOp::I32GeU,
        IrBinOp::GeI64U => BinaryOp::I64GeU,
        IrBinOp::EqF32 => BinaryOp::F32Eq,
        IrBinOp::EqF64 => BinaryOp::F64Eq,
        IrBinOp::NeF32 => BinaryOp::F32Ne,
        IrBinOp::NeF64 => BinaryOp::F64Ne,
        IrBinOp::LtF32 => BinaryOp::F32Lt,
        IrBinOp::LtF64 => BinaryOp::F64Lt,
        IrBinOp::GtF32 => BinaryOp::F32Gt,
        IrBinOp::GtF64 => BinaryOp::F64Gt,
        IrBinOp::LeF32 => BinaryOp::F32Le,
        IrBinOp::LeF64 => BinaryOp::F64Le,
        IrBinOp::GeF32 => BinaryOp::F32Ge,
        IrBinOp::GeF64 => BinaryOp::F64Ge,
    };
    body.binop(wasm_op);
}

// ---------------------------------------------------------------------------
// Unary operator emission
// ---------------------------------------------------------------------------

fn emit_unop(op: IrUnaryOp, body: &mut InstrSeqBuilder) {
    use walrus::ir::UnaryOp;
    let wasm_op = match op {
        IrUnaryOp::NegI32 => UnaryOp::I32Eqz, // placeholder (real: 0 - x)
        IrUnaryOp::NegI64 => UnaryOp::I64Eqz,
        IrUnaryOp::NegF32 => UnaryOp::F32Neg,
        IrUnaryOp::NegF64 => UnaryOp::F64Neg,
        IrUnaryOp::NotI32 => UnaryOp::I32Eqz,
        IrUnaryOp::EqzI32 => UnaryOp::I32Eqz,
        IrUnaryOp::EqzI64 => UnaryOp::I64Eqz,
        IrUnaryOp::WrapI64ToI32 => UnaryOp::I32WrapI64,
        IrUnaryOp::ExtendI32SToI64 => UnaryOp::I64ExtendSI32,
        IrUnaryOp::ExtendI32UToI64 => UnaryOp::I64ExtendUI32,
        IrUnaryOp::ConvertI32SToF32 => UnaryOp::F32ConvertSI32,
        IrUnaryOp::ConvertI32SToF64 => UnaryOp::F64ConvertSI32,
        IrUnaryOp::ConvertI64SToF32 => UnaryOp::F32ConvertSI64,
        IrUnaryOp::ConvertI64SToF64 => UnaryOp::F64ConvertSI64,
        IrUnaryOp::ConvertI32UToF32 => UnaryOp::F32ConvertUI32,
        IrUnaryOp::ConvertI32UToF64 => UnaryOp::F64ConvertUI32,
        IrUnaryOp::ConvertI64UToF32 => UnaryOp::F32ConvertUI64,
        IrUnaryOp::ConvertI64UToF64 => UnaryOp::F64ConvertUI64,
        IrUnaryOp::TruncF32ToI32S => UnaryOp::I32TruncSF32,
        IrUnaryOp::TruncF32ToI64S => UnaryOp::I64TruncSF32,
        IrUnaryOp::TruncF64ToI32S => UnaryOp::I32TruncSF64,
        IrUnaryOp::TruncF64ToI64S => UnaryOp::I64TruncSF64,
        IrUnaryOp::PromoteF32ToF64 => UnaryOp::F64PromoteF32,
        IrUnaryOp::DemoteF64ToF32 => UnaryOp::F32DemoteF64,
    };
    body.unop(wasm_op);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn emit_default_val(body: &mut InstrSeqBuilder, vt: ValType) {
    match vt {
        ValType::I32 => { body.i32_const(0); }
        ValType::I64 => { body.i64_const(0); }
        ValType::F32 => { body.f32_const(0.0); }
        ValType::F64 => { body.f64_const(0.0); }
        _ => { body.i32_const(0); }
    }
}

fn const_expr_for_init(ty: IrType, init: Option<&IrExpr>) -> ConstExpr {
    let val = match (ty, init) {
        (IrType::I64, Some(IrExpr::IntConst(v))) => walrus::ir::Value::I64(*v),
        (_, Some(IrExpr::IntConst(v))) => walrus::ir::Value::I32(*v as i32),
        (_, Some(IrExpr::BoolConst(b))) => walrus::ir::Value::I32(if *b { 1 } else { 0 }),
        (IrType::F32, Some(IrExpr::FloatConst(f))) => walrus::ir::Value::F32(*f as f32),
        (_, Some(IrExpr::FloatConst(f))) => walrus::ir::Value::F64(*f),
        (IrType::I64, _) => walrus::ir::Value::I64(0),
        (IrType::F32, _) => walrus::ir::Value::F32(0.0),
        (IrType::F64, _) => walrus::ir::Value::F64(0.0),
        _ => walrus::ir::Value::I32(0),
    };
    ConstExpr::Value(val)
}

fn ir_to_val(ty: IrType) -> ValType {
    match ty {
        IrType::I32 | IrType::Bool | IrType::Ptr => ValType::I32,
        IrType::I64 => ValType::I64,
        IrType::F32 => ValType::F32,
        IrType::F64 => ValType::F64,
        IrType::Unit => ValType::I32,
    }
}

/// Emit a WASM store instruction for the given IrType at the given offset.
/// Expects [addr, value] already on the WASM stack.
fn emit_store(body: &mut InstrSeqBuilder, memory: MemoryId, ty: IrType, offset: u32) {
    use walrus::ir::{MemArg, StoreKind};
    match ty {
        IrType::F64 => {
            body.store(memory, StoreKind::F64, MemArg { align: 8, offset });
        }
        IrType::F32 => {
            body.store(memory, StoreKind::F32, MemArg { align: 4, offset });
        }
        IrType::I64 => {
            body.store(memory, StoreKind::I64 { atomic: false }, MemArg { align: 8, offset });
        }
        _ => {
            body.store(memory, StoreKind::I32 { atomic: false }, MemArg { align: 4, offset });
        }
    }
}

/// Emit a WASM load instruction for the given IrType at the given offset.
/// Expects [addr] already on the WASM stack.
fn emit_load(body: &mut InstrSeqBuilder, memory: MemoryId, ty: IrType, offset: u32) {
    use walrus::ir::{LoadKind, MemArg};
    match ty {
        IrType::F64 => {
            body.load(memory, LoadKind::F64, MemArg { align: 8, offset });
        }
        IrType::F32 => {
            body.load(memory, LoadKind::F32, MemArg { align: 4, offset });
        }
        IrType::I64 => {
            body.load(memory, LoadKind::I64 { atomic: false }, MemArg { align: 8, offset });
        }
        _ => {
            body.load(memory, LoadKind::I32 { atomic: false }, MemArg { align: 4, offset });
        }
    }
}
