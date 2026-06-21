# Wscript

A statically-typed, expression-oriented scripting language designed to be embedded in Rust applications. Compiles to WebAssembly in-memory and executes via Wasmtime JIT.

## Features

- **Static type system** with full bidirectional inference — types flow both ways
- **Compiles to WASM**, JIT'd by Wasmtime — no bytecode interpreter
- **Functional pipelines** with lazy iterators and LINQ-style operators
- **Full LSP server** for IDE support (completions, hover, diagnostics, go-to-definition)
- **Full DAP server** for step debugging in VS Code
- **Host binding system** — register Rust functions and types, call them from scripts
- **Safe by default** — reference-counted, no borrow checker, `Result`/`Option` first-class
- **Familiar syntax** — inspired by Rust, Rhai, and TypeScript

## Quick Start

### As a Library (embedding in your Rust app)

Add to your `Cargo.toml`:

```toml
[dependencies]
wscript = "0.1"
```

Basic usage:

```rust
use wscript::Engine;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();

    // Register host functions
    engine.register_fn_raw(
        "greet",
        vec![wscript::bindings::ParamInfo {
            name: "name".into(),
            ty: wscript::bindings::ScriptType::String,
        }],
        wscript::bindings::ScriptType::String,
        |args| {
            let name = match &args[0] {
                wscript::Value::String(s) => s.clone(),
                _ => return Err("expected string".into()),
            };
            Ok(wscript::Value::String(format!("Hello, {}!", name)))
        },
    );

    // Compile and run a script
    let result = engine.load(r#"
        @export
        fn main() -> String {
            return greet("World");
        }
    "#)?;

    println!("Compiled successfully!");
    Ok(())
}
```

### Using the CLI

Install the CLI:

```sh
cargo install --path crates/wscript-cli
```

Or run directly from the repo:

```sh
# Run a script
just run examples/hello.ws

# Check for errors without running
just check-file examples/hello.ws

# Start LSP server (for editor integration)
just lsp

# Start DAP server (for debugging, VS Code connects on port 6009)
just dap

# Or with a custom port:
just dap 9229
```

## Language Overview

### Variables and Types

```wscript
let x = 42;                  // immutable, inferred i32
let y: f64 = 3.14;           // explicit type
let mut z = 0;               // mutable
let name = "Alice";          // String
let items = [1, 2, 3];       // i32[]
let map = #{ "key": value }; // Map<String, T>
```

### Functions

```wscript
fn add(a: i32, b: i32) -> i32 {
    return a + b;
}

fn greet(name: String, greeting: String = "Hello") -> String {
    return `${greeting}, ${name}!`;
}
```

### Structs and Enums

```wscript
struct Point {
    x: f64,
    y: f64,
}

impl Point {
    fn distance(&self) -> f64 {
        return (self.x * self.x + self.y * self.y).sqrt();
    }
}

enum Shape {
    Circle(f64),
    Rectangle(f64, f64),
}
```

### Pipelines

```wscript
let result = items
    .filter(|x| x > 0)
    .map(|x| x * 2)
    .collect();

// Or with pipe operator:
let result = items
    |> filter(|x| x > 0)
    |> map(|x| x * 2)
    |> collect();
```

### Pattern Matching

```wscript
match shape {
    Shape::Circle(r) => r * r * 3.14159,
    Shape::Rectangle(w, h) => w * h,
}
```

### Error Handling

```wscript
fn load(path: String) -> Result<String> {
    let data = read_file(path)?;    // ? propagates errors
    return Ok(data.trim());
}

// Custom error types
@error
enum AppError {
    @msg("not found: '{path}'")
    NotFound { path: String },
}
```

## IDE Support

### VS Code Setup

1. Start the LSP server: `wscript lsp`
2. Configure your editor to use `wscript lsp` as the language server for `.ws` files

For debugging:

1. Start the DAP server: `wscript dap --port 6009`
2. Add a VS Code launch configuration:

```json
{
    "type": "wscript",
    "request": "launch",
    "name": "Debug Wscript",
    "program": "${file}",
    "debugServer": 6009
}
```

## Architecture

```
Source text → Lexer → Parser → Type Checker → IR Lowering → WASM Codegen → Wasmtime JIT
```

- **Lexer**: Hand-written single-pass scanner with template string support
- **Parser**: Recursive descent with error recovery (always produces a partial AST)
- **Type Checker**: Bidirectional inference with unification
- **IR Lowering**: Desugars pipelines, closures, generics (monomorphization), macros
- **WASM Codegen**: Produces WASM via the `walrus` crate
- **Runtime**: Executes via embedded Wasmtime, with host function imports

## Feature Flags

| Feature | Description |
|---------|-------------|
| `runtime` (default) | Execute compiled scripts via Wasmtime |
| `lsp` | LSP server for editor integration |
| `dap` | DAP server for step debugging |
| `full` | All features |

```toml
# Compiler only (no execution):
wscript = { version = "0.1", default-features = false }

# With LSP support:
wscript = { version = "0.1", features = ["lsp"] }

# Everything:
wscript = { version = "0.1", features = ["full"] }
```

## Building from Source

```sh
# Install just (if you don't have it)
cargo install just

# Build everything
just build

# Run all checks
just check

# Run tests
just test

# See all available commands
just --list
```

## License

MIT OR Apache-2.0
