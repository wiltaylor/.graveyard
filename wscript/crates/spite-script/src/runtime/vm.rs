//! Wasmtime-based script execution.

use crate::bindings::{BindingRegistry, ScriptType};
use crate::compiler::source_map::SourceMap;
use crate::reflect::{FieldType, FieldValue, StructTypeInfo, StructView, TypeLayouts};
use crate::runtime::debug::*;
use crate::runtime::value::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use wasmtime::{Caller, Engine as WasmEngine, Instance, Linker, Module, Store};

/// Host store data for string, array, and map operations.
pub(crate) struct StoreData {
    pub(crate) strings: Vec<String>,
    pub(crate) arrays: Vec<Vec<i32>>,
    pub(crate) maps: Vec<HashMap<i32, i32>>,
}

impl StoreData {
    fn intern_string(&mut self, s: String) -> i32 {
        let idx = self.strings.len() as i32;
        self.strings.push(s);
        idx
    }
}

/// Configuration for the script engine.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Whether to enable debug instrumentation (breakpoints, stepping).
    pub debug_mode: bool,
    /// Maximum fuel (instruction budget) before pausing. `None` = unlimited.
    pub max_fuel: Option<u64>,
    /// Whether to emit DWARF debug info in compiled Wasm.
    pub emit_dwarf: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            debug_mode: false,
            max_fuel: None,
            emit_dwarf: false,
        }
    }
}

/// What to do when fuel runs out.
#[derive(Debug, Clone)]
pub enum FuelAction {
    /// Add more fuel and continue execution.
    AddFuel(u64),
    /// Trap the execution with an error.
    Trap,
}

/// Wraps a Wasmtime Engine with SpiteScript-specific configuration.
pub struct ScriptEngine {
    wasm_engine: WasmEngine,
}

impl ScriptEngine {
    /// Create a new ScriptEngine with default configuration.
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let mut config = wasmtime::Config::new();
        config.wasm_memory64(false);
        config.consume_fuel(false);
        let engine = WasmEngine::new(&config)?;
        Ok(Self { wasm_engine: engine })
    }

    /// Return a reference to the underlying Wasmtime engine.
    pub fn wasm_engine(&self) -> &WasmEngine {
        &self.wasm_engine
    }
}

/// An exported function's metadata.
#[derive(Debug, Clone)]
pub struct ExportedFn {
    pub name: String,
    pub param_count: usize,
    pub result_count: usize,
}

/// A compiled script ready for execution.
pub struct CompiledScript {
    module: Module,
    source_map: SourceMap,
    exports: Vec<ExportedFn>,
    source: String,
    breakpoints: Arc<Mutex<BreakpointTable>>,
    layouts: Arc<TypeLayouts>,
    bindings: Arc<BindingRegistry>,
}

impl CompiledScript {
    /// Create a compiled script from raw WASM bytes.
    pub fn from_wasm_bytes(
        engine: &ScriptEngine,
        wasm_bytes: &[u8],
        source: String,
        source_map: SourceMap,
        layouts: Arc<TypeLayouts>,
        bindings: Arc<BindingRegistry>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let module = Module::new(&engine.wasm_engine, wasm_bytes)?;

        let exports = module
            .exports()
            .filter_map(|export| {
                let ty = export.ty();
                let func_ty = ty.func()?;
                Some(ExportedFn {
                    name: export.name().to_string(),
                    param_count: func_ty.params().len(),
                    result_count: func_ty.results().len(),
                })
            })
            .collect();

        Ok(Self {
            module,
            source_map,
            exports,
            source,
            breakpoints: Arc::new(Mutex::new(BreakpointTable::new())),
            layouts,
            bindings,
        })
    }

    /// Reflection: look up a user struct type by name.
    pub fn type_info(&self, name: &str) -> Option<&StructTypeInfo> {
        self.layouts.get(name)
    }

    /// Reflection: iterate all user struct types.
    pub fn types(&self) -> &[StructTypeInfo] {
        &self.layouts.structs
    }

    /// Return the list of exported functions.
    pub fn exports(&self) -> &[ExportedFn] {
        &self.exports
    }

    /// Return a reference to the source map.
    pub fn source_map(&self) -> &SourceMap {
        &self.source_map
    }

    /// Return the original source code.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Return a reference to the underlying Wasmtime module.
    pub fn module(&self) -> &Module {
        &self.module
    }

    /// Instantiate the script into a long-lived `Vm`. The returned `Vm` owns
    /// its own `Store` and `Instance` — all state (linear memory, globals,
    /// host string table) persists across `Vm::call` invocations until the
    /// `Vm` is dropped.
    pub fn instantiate(&self, engine: &ScriptEngine) -> Result<Vm, ScriptPanic> {
        let mut store = Store::new(
            &engine.wasm_engine,
            StoreData {
                strings: Vec::new(),
                arrays: Vec::new(),
                maps: Vec::new(),
            },
        );

        let mut linker: Linker<StoreData> = Linker::new(&engine.wasm_engine);

        // Register __print(ptr: i32, len: i32) — reads a UTF-8 string from WASM memory and prints it.
        linker
            .func_wrap("env", "__print", |mut caller: Caller<'_, StoreData>, ptr: i32, len: i32| {
                let memory = caller
                    .get_export("memory")
                    .and_then(|ext| ext.into_memory());
                if let Some(memory) = memory {
                    let data = memory.data(&caller);
                    let start = ptr as usize;
                    let end = start + len as usize;
                    if end <= data.len() {
                        if let Ok(s) = std::str::from_utf8(&data[start..end]) {
                            print!("{}", s);
                        }
                    }
                }
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __print: {e}"),
                trace: vec![],
            })?;

        // Register __print_i32(value: i32) — prints an integer to stdout.
        linker
            .func_wrap("env", "__print_i32", |_caller: Caller<'_, StoreData>, value: i32| {
                println!("{}", value);
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __print_i32: {e}"),
                trace: vec![],
            })?;

        // Register __debug_probe(location_id: i32) — breakpoint check hook.
        let breakpoints = Arc::clone(&self.breakpoints);
        linker
            .func_wrap("env", "__debug_probe", move |_caller: Caller<'_, StoreData>, location_id: i32| {
                let table = breakpoints.lock().unwrap();
                if table.is_active(location_id as u32) {
                    log::debug!("Breakpoint hit at probe location {}", location_id);
                }
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __debug_probe: {e}"),
                trace: vec![],
            })?;

        // ── String host imports ────────────────────────────────────────

        // __str_new(ptr: i32, len: i32) -> i32: read bytes from WASM memory, create string handle.
        linker
            .func_wrap("env", "__str_new", |mut caller: Caller<'_, StoreData>, ptr: i32, len: i32| -> i32 {
                let memory = caller
                    .get_export("memory")
                    .and_then(|ext| ext.into_memory());
                let s = if let Some(memory) = memory {
                    let data = memory.data(&caller);
                    let start = ptr as usize;
                    let end = start + len as usize;
                    if end <= data.len() {
                        String::from_utf8_lossy(&data[start..end]).into_owned()
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };
                let idx = caller.data().strings.len() as i32;
                caller.data_mut().strings.push(s);
                idx
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __str_new: {e}"),
                trace: vec![],
            })?;

        // __str_len(handle: i32) -> i32: get string length.
        linker
            .func_wrap("env", "__str_len", |caller: Caller<'_, StoreData>, handle: i32| -> i32 {
                caller
                    .data()
                    .strings
                    .get(handle as usize)
                    .map(|s| s.len() as i32)
                    .unwrap_or(0)
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __str_len: {e}"),
                trace: vec![],
            })?;

        // __str_concat(a: i32, b: i32) -> i32: concatenate two strings.
        linker
            .func_wrap("env", "__str_concat", |mut caller: Caller<'_, StoreData>, a: i32, b: i32| -> i32 {
                let sa = caller.data().strings.get(a as usize).cloned().unwrap_or_default();
                let sb = caller.data().strings.get(b as usize).cloned().unwrap_or_default();
                let combined = sa + &sb;
                let idx = caller.data().strings.len() as i32;
                caller.data_mut().strings.push(combined);
                idx
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __str_concat: {e}"),
                trace: vec![],
            })?;

        // __str_print(handle: i32): print a string.
        linker
            .func_wrap("env", "__str_print", |caller: Caller<'_, StoreData>, handle: i32| {
                if let Some(s) = caller.data().strings.get(handle as usize) {
                    println!("{}", s);
                }
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __str_print: {e}"),
                trace: vec![],
            })?;

        // __str_eq(a: i32, b: i32) -> i32: compare two strings.
        linker
            .func_wrap("env", "__str_eq", |caller: Caller<'_, StoreData>, a: i32, b: i32| -> i32 {
                let sa = caller.data().strings.get(a as usize).cloned().unwrap_or_default();
                let sb = caller.data().strings.get(b as usize).cloned().unwrap_or_default();
                if sa == sb { 1 } else { 0 }
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __str_eq: {e}"),
                trace: vec![],
            })?;

        // ── New string host imports ───────────────────────────────────

        // __str_contains(s: i32, sub: i32) -> i32
        linker
            .func_wrap("env", "__str_contains", |caller: Caller<'_, StoreData>, s: i32, sub: i32| -> i32 {
                let sa = caller.data().strings.get(s as usize).cloned().unwrap_or_default();
                let sb = caller.data().strings.get(sub as usize).cloned().unwrap_or_default();
                if sa.contains(&*sb) { 1 } else { 0 }
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __str_contains: {e}"),
                trace: vec![],
            })?;

        // __str_starts_with(s: i32, prefix: i32) -> i32
        linker
            .func_wrap("env", "__str_starts_with", |caller: Caller<'_, StoreData>, s: i32, prefix: i32| -> i32 {
                let sa = caller.data().strings.get(s as usize).cloned().unwrap_or_default();
                let sb = caller.data().strings.get(prefix as usize).cloned().unwrap_or_default();
                if sa.starts_with(&*sb) { 1 } else { 0 }
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __str_starts_with: {e}"),
                trace: vec![],
            })?;

        // __str_ends_with(s: i32, suffix: i32) -> i32
        linker
            .func_wrap("env", "__str_ends_with", |caller: Caller<'_, StoreData>, s: i32, suffix: i32| -> i32 {
                let sa = caller.data().strings.get(s as usize).cloned().unwrap_or_default();
                let sb = caller.data().strings.get(suffix as usize).cloned().unwrap_or_default();
                if sa.ends_with(&*sb) { 1 } else { 0 }
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __str_ends_with: {e}"),
                trace: vec![],
            })?;

        // __str_trim(s: i32) -> i32
        linker
            .func_wrap("env", "__str_trim", |mut caller: Caller<'_, StoreData>, s: i32| -> i32 {
                let trimmed = caller.data().strings.get(s as usize).map(|v| v.trim().to_string()).unwrap_or_default();
                let idx = caller.data().strings.len() as i32;
                caller.data_mut().strings.push(trimmed);
                idx
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __str_trim: {e}"),
                trace: vec![],
            })?;

        // __str_to_upper(s: i32) -> i32
        linker
            .func_wrap("env", "__str_to_upper", |mut caller: Caller<'_, StoreData>, s: i32| -> i32 {
                let upper = caller.data().strings.get(s as usize).map(|v| v.to_uppercase()).unwrap_or_default();
                let idx = caller.data().strings.len() as i32;
                caller.data_mut().strings.push(upper);
                idx
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __str_to_upper: {e}"),
                trace: vec![],
            })?;

        // __str_to_lower(s: i32) -> i32
        linker
            .func_wrap("env", "__str_to_lower", |mut caller: Caller<'_, StoreData>, s: i32| -> i32 {
                let lower = caller.data().strings.get(s as usize).map(|v| v.to_lowercase()).unwrap_or_default();
                let idx = caller.data().strings.len() as i32;
                caller.data_mut().strings.push(lower);
                idx
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __str_to_lower: {e}"),
                trace: vec![],
            })?;

        // __str_replace(s: i32, from: i32, to: i32) -> i32
        linker
            .func_wrap("env", "__str_replace", |mut caller: Caller<'_, StoreData>, s: i32, from: i32, to: i32| -> i32 {
                let ss = caller.data().strings.get(s as usize).cloned().unwrap_or_default();
                let sf = caller.data().strings.get(from as usize).cloned().unwrap_or_default();
                let st = caller.data().strings.get(to as usize).cloned().unwrap_or_default();
                let replaced = ss.replace(&*sf, &st);
                let idx = caller.data().strings.len() as i32;
                caller.data_mut().strings.push(replaced);
                idx
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __str_replace: {e}"),
                trace: vec![],
            })?;

        // __str_split(s: i32, sep: i32) -> i32: split string, return array of string handles
        linker
            .func_wrap("env", "__str_split", |mut caller: Caller<'_, StoreData>, s: i32, sep: i32| -> i32 {
                let ss = caller.data().strings.get(s as usize).cloned().unwrap_or_default();
                let sf = caller.data().strings.get(sep as usize).cloned().unwrap_or_default();
                let parts: Vec<String> = ss.split(&*sf).map(|p| p.to_string()).collect();
                let mut handles = Vec::new();
                for part in parts {
                    let h = caller.data().strings.len() as i32;
                    caller.data_mut().strings.push(part);
                    handles.push(h);
                }
                let arr_idx = caller.data().arrays.len() as i32;
                caller.data_mut().arrays.push(handles);
                arr_idx
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __str_split: {e}"),
                trace: vec![],
            })?;

        // __str_char_count(s: i32) -> i32
        linker
            .func_wrap("env", "__str_char_count", |caller: Caller<'_, StoreData>, s: i32| -> i32 {
                caller.data().strings.get(s as usize).map(|v| v.chars().count() as i32).unwrap_or(0)
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __str_char_count: {e}"),
                trace: vec![],
            })?;

        // __str_is_empty(s: i32) -> i32
        linker
            .func_wrap("env", "__str_is_empty", |caller: Caller<'_, StoreData>, s: i32| -> i32 {
                caller.data().strings.get(s as usize).map(|v| if v.is_empty() { 1 } else { 0 }).unwrap_or(1)
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __str_is_empty: {e}"),
                trace: vec![],
            })?;

        // __str_repeat(s: i32, n: i32) -> i32
        linker
            .func_wrap("env", "__str_repeat", |mut caller: Caller<'_, StoreData>, s: i32, n: i32| -> i32 {
                let repeated = caller.data().strings.get(s as usize).map(|v| v.repeat(n.max(0) as usize)).unwrap_or_default();
                let idx = caller.data().strings.len() as i32;
                caller.data_mut().strings.push(repeated);
                idx
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __str_repeat: {e}"),
                trace: vec![],
            })?;

        // __print_f64(value_bits: i64): print a float (passed as i64 bits)
        linker
            .func_wrap("env", "__print_f64", |_caller: Caller<'_, StoreData>, value_bits: i64| {
                let v = f64::from_bits(value_bits as u64);
                println!("{}", v);
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __print_f64: {e}"),
                trace: vec![],
            })?;

        // __print_bool(val: i32): print "true" or "false"
        linker
            .func_wrap("env", "__print_bool", |_caller: Caller<'_, StoreData>, val: i32| {
                if val != 0 {
                    println!("true");
                } else {
                    println!("false");
                }
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __print_bool: {e}"),
                trace: vec![],
            })?;

        // __arr_to_str(arr: i32) -> i32: create string representation of array
        linker
            .func_wrap("env", "__arr_to_str", |mut caller: Caller<'_, StoreData>, arr: i32| -> i32 {
                let elements = caller.data().arrays.get(arr as usize).cloned().unwrap_or_default();
                let parts: Vec<String> = elements.iter().map(|v| v.to_string()).collect();
                let s = format!("[{}]", parts.join(", "));
                let idx = caller.data().strings.len() as i32;
                caller.data_mut().strings.push(s);
                idx
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __arr_to_str: {e}"),
                trace: vec![],
            })?;

        // ── Array host imports ─────────────────────────────────────────

        // __arr_new() -> i32: create an empty array handle.
        linker
            .func_wrap("env", "__arr_new", |mut caller: Caller<'_, StoreData>| -> i32 {
                let idx = caller.data().arrays.len() as i32;
                caller.data_mut().arrays.push(Vec::new());
                idx
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __arr_new: {e}"),
                trace: vec![],
            })?;

        // __arr_push(arr: i32, val: i32): push i32 to array.
        linker
            .func_wrap("env", "__arr_push", |mut caller: Caller<'_, StoreData>, arr: i32, val: i32| {
                if let Some(a) = caller.data_mut().arrays.get_mut(arr as usize) {
                    a.push(val);
                }
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __arr_push: {e}"),
                trace: vec![],
            })?;

        // __arr_len(arr: i32) -> i32: get array length.
        linker
            .func_wrap("env", "__arr_len", |caller: Caller<'_, StoreData>, arr: i32| -> i32 {
                caller
                    .data()
                    .arrays
                    .get(arr as usize)
                    .map(|a| a.len() as i32)
                    .unwrap_or(0)
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __arr_len: {e}"),
                trace: vec![],
            })?;

        // __arr_get(arr: i32, idx: i32) -> i32: get element at index.
        linker
            .func_wrap("env", "__arr_get", |caller: Caller<'_, StoreData>, arr: i32, idx: i32| -> i32 {
                caller
                    .data()
                    .arrays
                    .get(arr as usize)
                    .and_then(|a| a.get(idx as usize))
                    .copied()
                    .unwrap_or(0)
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __arr_get: {e}"),
                trace: vec![],
            })?;

        // __i32_to_str(value: i32) -> i32: convert integer to string handle.
        linker
            .func_wrap("env", "__i32_to_str", |mut caller: Caller<'_, StoreData>, value: i32| -> i32 {
                let s = value.to_string();
                let idx = caller.data().strings.len() as i32;
                caller.data_mut().strings.push(s);
                idx
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __i32_to_str: {e}"),
                trace: vec![],
            })?;

        // __arr_sum(arr: i32) -> i32: sum all elements of an array.
        linker
            .func_wrap("env", "__arr_sum", |caller: Caller<'_, StoreData>, arr: i32| -> i32 {
                caller
                    .data()
                    .arrays
                    .get(arr as usize)
                    .map(|a| a.iter().sum())
                    .unwrap_or(0)
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __arr_sum: {e}"),
                trace: vec![],
            })?;

        // __arr_contains(arr: i32, val: i32) -> i32: check if array contains value.
        linker
            .func_wrap("env", "__arr_contains", |caller: Caller<'_, StoreData>, arr: i32, val: i32| -> i32 {
                caller
                    .data()
                    .arrays
                    .get(arr as usize)
                    .map(|a| if a.contains(&val) { 1 } else { 0 })
                    .unwrap_or(0)
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __arr_contains: {e}"),
                trace: vec![],
            })?;

        // __arr_reverse(arr: i32) -> i32: reverse array and return new handle.
        linker
            .func_wrap("env", "__arr_reverse", |mut caller: Caller<'_, StoreData>, arr: i32| -> i32 {
                let reversed = caller
                    .data()
                    .arrays
                    .get(arr as usize)
                    .map(|a| a.iter().rev().copied().collect::<Vec<i32>>())
                    .unwrap_or_default();
                let idx = caller.data().arrays.len() as i32;
                caller.data_mut().arrays.push(reversed);
                idx
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __arr_reverse: {e}"),
                trace: vec![],
            })?;

        // __arr_first(arr: i32) -> i32: first element or 0.
        linker
            .func_wrap("env", "__arr_first", |caller: Caller<'_, StoreData>, arr: i32| -> i32 {
                caller
                    .data()
                    .arrays
                    .get(arr as usize)
                    .and_then(|a| a.first())
                    .copied()
                    .unwrap_or(0)
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __arr_first: {e}"),
                trace: vec![],
            })?;

        // __arr_last(arr: i32) -> i32: last element or 0.
        linker
            .func_wrap("env", "__arr_last", |caller: Caller<'_, StoreData>, arr: i32| -> i32 {
                caller
                    .data()
                    .arrays
                    .get(arr as usize)
                    .and_then(|a| a.last())
                    .copied()
                    .unwrap_or(0)
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __arr_last: {e}"),
                trace: vec![],
            })?;

        // __arr_min(arr: i32) -> i32: minimum element.
        linker
            .func_wrap("env", "__arr_min", |caller: Caller<'_, StoreData>, arr: i32| -> i32 {
                caller
                    .data()
                    .arrays
                    .get(arr as usize)
                    .and_then(|a| a.iter().copied().min())
                    .unwrap_or(0)
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __arr_min: {e}"),
                trace: vec![],
            })?;

        // __arr_max(arr: i32) -> i32: maximum element.
        linker
            .func_wrap("env", "__arr_max", |caller: Caller<'_, StoreData>, arr: i32| -> i32 {
                caller
                    .data()
                    .arrays
                    .get(arr as usize)
                    .and_then(|a| a.iter().copied().max())
                    .unwrap_or(0)
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __arr_max: {e}"),
                trace: vec![],
            })?;

        // __arr_sort(arr: i32) -> i32: return sorted copy.
        linker
            .func_wrap("env", "__arr_sort", |mut caller: Caller<'_, StoreData>, arr: i32| -> i32 {
                let mut sorted = caller
                    .data()
                    .arrays
                    .get(arr as usize)
                    .cloned()
                    .unwrap_or_default();
                sorted.sort();
                let idx = caller.data().arrays.len() as i32;
                caller.data_mut().arrays.push(sorted);
                idx
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __arr_sort: {e}"),
                trace: vec![],
            })?;

        // __arr_dedup(arr: i32) -> i32: remove consecutive duplicates, return new handle.
        linker
            .func_wrap("env", "__arr_dedup", |mut caller: Caller<'_, StoreData>, arr: i32| -> i32 {
                let mut deduped = caller
                    .data()
                    .arrays
                    .get(arr as usize)
                    .cloned()
                    .unwrap_or_default();
                deduped.dedup();
                let idx = caller.data().arrays.len() as i32;
                caller.data_mut().arrays.push(deduped);
                idx
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __arr_dedup: {e}"),
                trace: vec![],
            })?;

        // __arr_join_str(arr: i32, sep: i32) -> i32: join i32 array as string with separator.
        linker
            .func_wrap("env", "__arr_join_str", |mut caller: Caller<'_, StoreData>, arr: i32, sep: i32| -> i32 {
                let elements = caller
                    .data()
                    .arrays
                    .get(arr as usize)
                    .cloned()
                    .unwrap_or_default();
                let separator = caller
                    .data()
                    .strings
                    .get(sep as usize)
                    .cloned()
                    .unwrap_or_default();
                let joined = elements
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(&separator);
                let idx = caller.data().strings.len() as i32;
                caller.data_mut().strings.push(joined);
                idx
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __arr_join_str: {e}"),
                trace: vec![],
            })?;

        // ── Map host imports ──────────────────────────────────────────

        // __map_new() -> i32: create an empty map handle.
        linker
            .func_wrap("env", "__map_new", |mut caller: Caller<'_, StoreData>| -> i32 {
                let idx = caller.data().maps.len() as i32;
                caller.data_mut().maps.push(HashMap::new());
                idx
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __map_new: {e}"),
                trace: vec![],
            })?;

        // __map_set(map: i32, key: i32, val: i32): set key-value pair.
        linker
            .func_wrap("env", "__map_set", |mut caller: Caller<'_, StoreData>, map: i32, key: i32, val: i32| {
                if let Some(m) = caller.data_mut().maps.get_mut(map as usize) {
                    m.insert(key, val);
                }
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __map_set: {e}"),
                trace: vec![],
            })?;

        // __map_get(map: i32, key: i32) -> i32: get value by key.
        linker
            .func_wrap("env", "__map_get", |caller: Caller<'_, StoreData>, map: i32, key: i32| -> i32 {
                caller
                    .data()
                    .maps
                    .get(map as usize)
                    .and_then(|m| m.get(&key))
                    .copied()
                    .unwrap_or(0)
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __map_get: {e}"),
                trace: vec![],
            })?;

        // __map_len(map: i32) -> i32: get number of entries.
        linker
            .func_wrap("env", "__map_len", |caller: Caller<'_, StoreData>, map: i32| -> i32 {
                caller
                    .data()
                    .maps
                    .get(map as usize)
                    .map(|m| m.len() as i32)
                    .unwrap_or(0)
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __map_len: {e}"),
                trace: vec![],
            })?;

        // __panic(msg_handle: i32) — trap the script with an error message.
        linker
            .func_wrap("env", "__panic", |caller: Caller<'_, StoreData>, msg_handle: i32| -> wasmtime::Result<()> {
                let msg = caller
                    .data()
                    .strings
                    .get(msg_handle as usize)
                    .cloned()
                    .unwrap_or_else(|| "script panicked".to_string());
                Err(wasmtime::Error::msg(format!("panic: {}", msg)))
            })
            .map_err(|e| ScriptPanic {
                message: format!("Failed to register __panic: {e}"),
                trace: vec![],
            })?;

        // ── User-registered host functions (module "host") ─────────────
        for (name, hf) in &self.bindings.functions {
            let closure = Arc::clone(&hf.closure);
            let param_tys: Vec<ScriptType> = hf.params.iter().map(|p| p.ty.clone()).collect();
            let ret_ty = hf.return_type.clone();
            let fn_name = name.clone();
            let param_count = param_tys.len();

            // Build a dynamic wasmtime Func using typed I/O from ScriptType.
            // Use wasmtime::Func::new with a dynamic signature.
            use wasmtime::{Func, FuncType, Val, ValType};
            let params_val: Vec<ValType> = param_tys
                .iter()
                .map(|st| match st {
                    ScriptType::I64 => ValType::I64,
                    ScriptType::F32 => ValType::F32,
                    ScriptType::F64 => ValType::F64,
                    _ => ValType::I32,
                })
                .collect();
            let results_val: Vec<ValType> = match &ret_ty {
                ScriptType::Unit => vec![],
                ScriptType::I64 => vec![ValType::I64],
                ScriptType::F32 => vec![ValType::F32],
                ScriptType::F64 => vec![ValType::F64],
                _ => vec![ValType::I32],
            };
            let ty = FuncType::new(&engine.wasm_engine, params_val, results_val);
            let param_tys_clone = param_tys.clone();
            let ret_ty_clone = ret_ty.clone();
            let fn_name_for_err = fn_name.clone();
            let func = Func::new(
                &mut store,
                ty,
                move |mut caller, params: &[Val], results: &mut [Val]| {
                    if params.len() != param_count {
                        return Err(wasmtime::Error::msg(format!(
                            "host fn '{}' arity mismatch",
                            fn_name_for_err
                        )));
                    }
                    let mut values: Vec<Value> = Vec::with_capacity(param_count);
                    for (i, st) in param_tys_clone.iter().enumerate() {
                        let v = match (st, &params[i]) {
                            (ScriptType::I32, Val::I32(x)) => Value::I32(*x),
                            (ScriptType::Bool, Val::I32(x)) => Value::Bool(*x != 0),
                            (ScriptType::I64, Val::I64(x)) => Value::I64(*x),
                            (ScriptType::F32, Val::F32(bits)) => Value::F32(f32::from_bits(*bits)),
                            (ScriptType::F64, Val::F64(bits)) => Value::F64(f64::from_bits(*bits)),
                            (ScriptType::Str, Val::I32(handle)) => {
                                let s = caller
                                    .data()
                                    .strings
                                    .get(*handle as usize)
                                    .cloned()
                                    .unwrap_or_default();
                                Value::Str(s)
                            }
                            _ => {
                                return Err(wasmtime::Error::msg(format!(
                                    "host fn '{}' argument type mismatch at {}",
                                    fn_name_for_err, i
                                )));
                            }
                        };
                        values.push(v);
                    }
                    let out = closure(&values).map_err(wasmtime::Error::msg)?;
                    match (&ret_ty_clone, out) {
                        (ScriptType::Unit, _) => {}
                        (ScriptType::I32, Some(Value::I32(v))) => results[0] = Val::I32(v),
                        (ScriptType::Bool, Some(Value::Bool(v))) => {
                            results[0] = Val::I32(if v { 1 } else { 0 })
                        }
                        (ScriptType::I64, Some(Value::I64(v))) => results[0] = Val::I64(v),
                        (ScriptType::F32, Some(Value::F32(v))) => {
                            results[0] = Val::F32(v.to_bits())
                        }
                        (ScriptType::F64, Some(Value::F64(v))) => {
                            results[0] = Val::F64(v.to_bits())
                        }
                        (ScriptType::Str, Some(Value::Str(s))) => {
                            let h = caller.data_mut().intern_string(s);
                            results[0] = Val::I32(h);
                        }
                        (ret, got) => {
                            return Err(wasmtime::Error::msg(format!(
                                "host fn '{}' return type mismatch: expected {:?}, got {:?}",
                                fn_name_for_err, ret, got
                            )));
                        }
                    }
                    Ok(())
                },
            );
            linker
                .define(&mut store, "host", &fn_name, func)
                .map_err(|e| ScriptPanic {
                    message: format!("Failed to register host fn {fn_name}: {e}"),
                    trace: vec![],
                })?;
        }

        // Instantiate the module.
        let instance = linker
            .instantiate(&mut store, &self.module)
            .map_err(|e| self.trap_to_panic(&e.to_string()))?;

        Ok(Vm {
            store,
            instance,
            layouts: Arc::clone(&self.layouts),
        })
    }

    /// Convenience: instantiate + call + discard. Prefer `instantiate` for
    /// stateful usage.
    pub fn call(
        &self,
        engine: &ScriptEngine,
        fn_name: &str,
        args: &[Value],
    ) -> Result<Option<Value>, ScriptPanic> {
        let mut vm = self.instantiate(engine)?;
        vm.call(fn_name, args)
    }

    /// Convert a WASM trap error message into a ScriptPanic with source-mapped trace.
    fn trap_to_panic(&self, message: &str) -> ScriptPanic {
        // Try to extract source-level information from the error message.
        // In a more sophisticated implementation, we would parse wasm frame offsets
        // and map them through the source map.
        let trace = Vec::new();
        ScriptPanic {
            message: message.to_string(),
            trace,
        }
    }

    /// Set a breakpoint at the given source line.
    pub fn set_breakpoint(&self, line: u32) {
        let entries = self.source_map.lookup_by_source_line(line);
        if let Some(entry) = entries.first() {
            let probe_id = self
                .source_map
                .entries
                .iter()
                .position(|e| std::ptr::eq(e, *entry))
                .unwrap_or(0) as u32;
            self.breakpoints
                .lock()
                .unwrap()
                .set_breakpoint(line, probe_id);
        }
    }

    /// Clear the breakpoint at the given source line.
    pub fn clear_breakpoint(&self, line: u32) {
        self.breakpoints.lock().unwrap().clear_breakpoint(line);
    }

    /// Clear all breakpoints.
    pub fn clear_all_breakpoints(&self) {
        self.breakpoints.lock().unwrap().clear_all();
    }
}

/// Convert a `Value` into a `wasmtime::Val`. Strings must be handled
/// separately against a `StoreData` — this helper only covers primitives.
fn value_to_wasm_val(value: &Value) -> wasmtime::Val {
    match value {
        Value::I32(v) => wasmtime::Val::I32(*v),
        Value::I64(v) => wasmtime::Val::I64(*v),
        Value::F32(v) => wasmtime::Val::F32(v.to_bits()),
        Value::F64(v) => wasmtime::Val::F64(v.to_bits()),
        Value::Bool(v) => wasmtime::Val::I32(if *v { 1 } else { 0 }),
        Value::Str(_) => wasmtime::Val::I32(0),
    }
}

fn wasm_val_to_value(val: &wasmtime::Val) -> Value {
    match val {
        wasmtime::Val::I32(v) => Value::I32(*v),
        wasmtime::Val::I64(v) => Value::I64(*v),
        wasmtime::Val::F32(v) => Value::F32(f32::from_bits(*v)),
        wasmtime::Val::F64(v) => Value::F64(f64::from_bits(*v)),
        _ => Value::I32(0),
    }
}

// ---------------------------------------------------------------------------
// Long-lived Vm
// ---------------------------------------------------------------------------

/// A live script instance. Owns its own `Store` and `Instance`; dropping it
/// disposes the script state.
pub struct Vm {
    store: Store<StoreData>,
    instance: Instance,
    layouts: Arc<TypeLayouts>,
}

impl Vm {
    /// Call an exported function by name. Strings in `args` are interned into
    /// the store's string table and the handle is passed to the script. A
    /// return value of `None` means unit.
    pub fn call(&mut self, fn_name: &str, args: &[Value]) -> Result<Option<Value>, ScriptPanic> {
        let func = self
            .instance
            .get_func(&mut self.store, fn_name)
            .ok_or_else(|| ScriptPanic {
                message: format!("Function '{}' not found in module exports", fn_name),
                trace: vec![],
            })?;

        let params: Vec<wasmtime::Val> = args
            .iter()
            .map(|v| match v {
                Value::Str(s) => {
                    let h = self.store.data_mut().intern_string(s.clone());
                    wasmtime::Val::I32(h)
                }
                other => value_to_wasm_val(other),
            })
            .collect();

        let func_ty = func.ty(&self.store);
        let result_count = func_ty.results().len();
        let mut results = vec![wasmtime::Val::I32(0); result_count];

        func.call(&mut self.store, &params, &mut results)
            .map_err(|e| ScriptPanic { message: e.to_string(), trace: vec![] })?;

        if results.is_empty() {
            Ok(None)
        } else {
            Ok(Some(wasm_val_to_value(&results[0])))
        }
    }

    /// Read a primitive WASM global exported as `g_<name>`.
    pub fn get_global(&mut self, name: &str) -> Result<Value, String> {
        let export_name = format!("g_{name}");
        let g = self
            .instance
            .get_global(&mut self.store, &export_name)
            .ok_or_else(|| format!("global '{name}' not found"))?;
        Ok(wasm_val_to_value(&g.get(&mut self.store)))
    }

    /// Write a primitive WASM global exported as `g_<name>`.
    pub fn set_global(&mut self, name: &str, v: Value) -> Result<(), String> {
        let export_name = format!("g_{name}");
        let g = self
            .instance
            .get_global(&mut self.store, &export_name)
            .ok_or_else(|| format!("global '{name}' not found"))?;
        let val = match v {
            Value::Str(s) => {
                let h = self.store.data_mut().intern_string(s);
                wasmtime::Val::I32(h)
            }
            other => value_to_wasm_val(&other),
        };
        g.set(&mut self.store, val).map_err(|e| e.to_string())
    }

    /// Look up reflection info for a type.
    pub fn type_info(&self, name: &str) -> Option<&StructTypeInfo> {
        self.layouts.get(name)
    }

    /// Read a script struct starting at linear-memory pointer `base`.
    /// Walks `FieldInfo` recursively: primitives load directly, `str` fields
    /// load the handle and resolve through the host string table, nested
    /// struct fields recurse.
    pub fn read_struct_at(&mut self, base: i32, type_name: &str) -> Result<StructView, String> {
        let info = self
            .layouts
            .get(type_name)
            .ok_or_else(|| format!("unknown struct type '{type_name}'"))?
            .clone();
        self.read_struct_info(base, &info)
    }

    fn read_struct_info(
        &mut self,
        base: i32,
        info: &StructTypeInfo,
    ) -> Result<StructView, String> {
        let memory = self
            .instance
            .get_memory(&mut self.store, "memory")
            .ok_or_else(|| "script has no exported memory".to_string())?;
        let mut fields = Vec::with_capacity(info.fields.len());
        for field in &info.fields {
            let addr = (base as u32 + field.offset) as usize;
            let data = memory.data(&self.store);
            let value = match &field.ty {
                FieldType::Primitive(ScriptType::I32) => {
                    FieldValue::Primitive(Value::I32(read_i32(data, addr)?))
                }
                FieldType::Primitive(ScriptType::Bool) => FieldValue::Primitive(Value::Bool(
                    read_i32(data, addr)? != 0,
                )),
                FieldType::Primitive(ScriptType::I64) => {
                    FieldValue::Primitive(Value::I64(read_i64(data, addr)?))
                }
                FieldType::Primitive(ScriptType::F32) => FieldValue::Primitive(Value::F32(
                    f32::from_bits(read_i32(data, addr)? as u32),
                )),
                FieldType::Primitive(ScriptType::F64) => FieldValue::Primitive(Value::F64(
                    f64::from_bits(read_i64(data, addr)? as u64),
                )),
                FieldType::Primitive(ScriptType::Str) => {
                    let handle = read_i32(data, addr)?;
                    let s = self
                        .store
                        .data()
                        .strings
                        .get(handle as usize)
                        .cloned()
                        .unwrap_or_default();
                    FieldValue::Primitive(Value::Str(s))
                }
                FieldType::Primitive(ScriptType::Unit) => FieldValue::Primitive(Value::I32(0)),
                FieldType::Struct(nested_name) => {
                    let ptr = read_i32(data, addr)?;
                    let nested = self
                        .layouts
                        .get(nested_name)
                        .ok_or_else(|| format!("unknown nested struct '{nested_name}'"))?
                        .clone();
                    FieldValue::Nested(self.read_struct_info(ptr, &nested)?)
                }
            };
            fields.push((field.name.clone(), value));
        }
        Ok(StructView { type_name: info.name.clone(), fields })
    }

    /// Write a set of primitive/string fields into a struct at linear-memory
    /// pointer `base`. Nested struct fields and unknown fields return an
    /// error.
    pub fn write_struct_at(
        &mut self,
        base: i32,
        type_name: &str,
        fields: &[(&str, Value)],
    ) -> Result<(), String> {
        let info = self
            .layouts
            .get(type_name)
            .ok_or_else(|| format!("unknown struct type '{type_name}'"))?
            .clone();
        for (name, value) in fields {
            let field = info
                .fields
                .iter()
                .find(|f| f.name == *name)
                .ok_or_else(|| format!("unknown field '{name}' on {type_name}"))?;
            let addr = (base as u32 + field.offset) as usize;
            match (&field.ty, value.clone()) {
                (FieldType::Primitive(ScriptType::I32), Value::I32(v)) => {
                    self.write_i32(addr, v)?
                }
                (FieldType::Primitive(ScriptType::Bool), Value::Bool(v)) => {
                    self.write_i32(addr, if v { 1 } else { 0 })?
                }
                (FieldType::Primitive(ScriptType::I64), Value::I64(v)) => {
                    self.write_i64(addr, v)?
                }
                (FieldType::Primitive(ScriptType::F32), Value::F32(v)) => {
                    self.write_i32(addr, v.to_bits() as i32)?
                }
                (FieldType::Primitive(ScriptType::F64), Value::F64(v)) => {
                    self.write_i64(addr, v.to_bits() as i64)?
                }
                (FieldType::Primitive(ScriptType::Str), Value::Str(s)) => {
                    let h = self.store.data_mut().intern_string(s);
                    self.write_i32(addr, h)?;
                }
                (FieldType::Struct(_), _) => {
                    return Err(format!(
                        "write_struct_at: nested struct field '{name}' unsupported"
                    ))
                }
                (ft, v) => {
                    return Err(format!(
                        "write_struct_at: type mismatch for '{name}': field {:?}, got {}",
                        ft,
                        v.type_name()
                    ))
                }
            }
        }
        Ok(())
    }

    fn write_i32(&mut self, addr: usize, v: i32) -> Result<(), String> {
        let memory = self
            .instance
            .get_memory(&mut self.store, "memory")
            .ok_or_else(|| "script has no exported memory".to_string())?;
        let data = memory.data_mut(&mut self.store);
        if addr + 4 > data.len() {
            return Err("address out of bounds".into());
        }
        data[addr..addr + 4].copy_from_slice(&v.to_le_bytes());
        Ok(())
    }

    fn write_i64(&mut self, addr: usize, v: i64) -> Result<(), String> {
        let memory = self
            .instance
            .get_memory(&mut self.store, "memory")
            .ok_or_else(|| "script has no exported memory".to_string())?;
        let data = memory.data_mut(&mut self.store);
        if addr + 8 > data.len() {
            return Err("address out of bounds".into());
        }
        data[addr..addr + 8].copy_from_slice(&v.to_le_bytes());
        Ok(())
    }
}

fn read_i32(data: &[u8], addr: usize) -> Result<i32, String> {
    if addr + 4 > data.len() {
        return Err("address out of bounds".into());
    }
    Ok(i32::from_le_bytes(data[addr..addr + 4].try_into().unwrap()))
}

fn read_i64(data: &[u8], addr: usize) -> Result<i64, String> {
    if addr + 8 > data.len() {
        return Err("address out of bounds".into());
    }
    Ok(i64::from_le_bytes(data[addr..addr + 8].try_into().unwrap()))
}
