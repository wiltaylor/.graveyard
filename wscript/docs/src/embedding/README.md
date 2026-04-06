# Embedding Guide

Wscript is designed from the ground up to be embedded in Rust applications. The host application controls what scripts can do by registering functions, types, and global values before compilation.

This section covers:

- **[Host Functions](./host-functions.md)** — Register Rust closures that scripts can call
- **[Host Types](./host-types.md)** — Expose Rust types with methods to scripts
- **[Globals](./globals.md)** — Read and write script-declared top-level globals across calls

## How It Works

The embedding workflow follows three steps:

1. **Configure** — Create an `Engine`, register host bindings
2. **Compile** — Load source text, which runs the full compiler pipeline
3. **Execute** — Call exported script functions, passing and receiving values

```rust
use wscript::Engine;

let mut engine = Engine::new();

// 1. Configure
engine.register_fn_raw("get_time", /* ... */);

// 2. Compile
let result = engine.load_script(source)?;

// 3. Execute
let value = result.script.unwrap().call(&engine_ref, "main", &[])?;
```

Host bindings serve three purposes:

- **Runtime** — The registered Rust closures are called when scripts invoke host functions
- **Type checking** — Parameter and return types validate calls at compile time
- **LSP** — Type information and doc strings appear in completions, hover, and inlay hints
