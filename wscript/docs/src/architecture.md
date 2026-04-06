# Architecture

Wscript compiles source text to WebAssembly (WASM) in memory and executes it via an embedded Wasmtime instance. The WASM binary is never written to disk.

## Compilation Pipeline

```
Source text (String)
    |
    v  Lexer (compiler/lexer.rs)
Token stream (Vec<Token>)
    |
    v  Parser (compiler/parser.rs) — error-recovering
AST (every node carries a Span)
    |
    v  Type checker (compiler/tycheck.rs) — bidirectional inference
Typed AST + TypeInfo
    |
    v  Lowering pass (compiler/lower.rs)
IR  (pipelines desugared, closures converted, macros expanded)
    |
    v  WASM codegen (compiler/codegen.rs) — walrus crate
WASM module bytes (in memory)
    |
    v  Wasmtime: compile to native code (JIT)
CompiledScript (ready to execute)
```

## Key Design Decisions

### WASM as Internal IR

The compiler targets WASM as its execution format rather than building a custom bytecode VM. This gives access to Wasmtime's JIT compiler, WASM validation, and fuel metering without the complexity of designing a VM from scratch.

### Single Library Crate

The Rust crate exposes the compiler, runtime, LSP server, and DAP server behind Cargo feature flags. Embedders take only what they need:

| Feature | What it enables |
|---------|----------------|
| `runtime` (default) | Execute compiled scripts via Wasmtime |
| `lsp` | LSP server for editor integration |
| `dap` | DAP server for step debugging |
| `full` | All of the above |

### Reference Counting

All heap-allocated values (strings, arrays, maps, structs, closures) use reference counting. There is no borrow checker — the type checker enforces `&self` vs `&mut self` as a convention, but at runtime all access goes through ref-counted pointers.

### Monomorphisation

Generics are monomorphised at compile time. Each instantiation of a generic function or struct with distinct type arguments produces distinct WASM code. There is no type erasure or boxing overhead.

## Module Layout

```
crates/wscript/src/
├── lib.rs                   — public API
├── engine.rs                — Engine builder, config
├── bindings.rs              — host binding registry
├── query_db.rs              — incremental state for LSP
│
├── compiler/
│   ├── token.rs             — TokenKind, Span
│   ├── lexer.rs             — hand-written scanner
│   ├── parser.rs            — recursive descent, error-recovering
│   ├── ast.rs               — AST node types
│   ├── tycheck.rs           — type inference, trait checking
│   ├── lower.rs             — IR lowering, monomorphisation
│   ├── ir.rs                — IR types
│   ├── codegen.rs           — IR to WASM via walrus
│   └── source_map.rs        — WASM offset to source location
│
├── runtime/
│   ├── vm.rs                — Wasmtime setup, script execution
│   ├── value.rs             — Value enum for data exchange
│   └── debug.rs             — breakpoints, stepping, stack traces
│
├── lsp/                     — LSP server (feature: lsp)
│   ├── server.rs            — tower-lsp LanguageServer impl
│   ├── completions.rs
│   ├── hover.rs
│   ├── diagnostics.rs
│   ├── semantic_tokens.rs
│   ├── inlay_hints.rs
│   └── formatting.rs
│
└── dap/                     — DAP server (feature: dap)
    └── server.rs
```

## Runtime Architecture

Each `script.call()` creates a fresh Wasmtime `Store` with a new linear memory. Scripts are stateless between calls unless globals are explicitly set.

Host functions are registered as WASM imports in the `"env"` module. The runtime provides built-in imports for:

- `__print_i32`, `__str_print` — output
- `__str_new`, `__str_len`, `__str_concat`, etc. — string operations
- `__arr_new`, `__arr_push`, `__arr_get`, etc. — array operations
- `__map_new`, `__map_set`, `__map_get`, etc. — map operations
- `__panic` — script panics
- `__debug_probe` — breakpoint checks (debug mode)

## Error Recovery

The parser is error-recovering: on a syntax error it emits a diagnostic, inserts an `Error` AST node, and advances to a synchronisation point before continuing. This ensures the parser always produces a full AST for the LSP to work with.

The type checker similarly collects errors without aborting, propagating `Unknown` types to prevent cascading diagnostics.
