# Using as a Rust Library

This chapter walks through the SpiteScript embedding API: creating an engine, registering host functions, compiling scripts, and calling exported functions from Rust.

## Creating an Engine

The `Engine` is the main entry point for SpiteScript. It owns the Wasmtime engine, the host binding registry, and configuration state.

```rust
use spite_script::Engine;

let engine = Engine::new();
```

The engine can be configured with builder-style methods:

```rust
let engine = Engine::new()
    .debug_mode(true)   // Enable debug probes and source maps
    .max_fuel(1_000_000); // Limit instruction budget
```

- **`debug_mode(true)`** inserts debug probe calls at every statement boundary in compiled scripts. This is required for breakpoint-based debugging but adds overhead. Leave it off for production execution.
- **`max_fuel(n)`** enables Wasmtime's fuel metering. Each WASM instruction consumes approximately one unit of fuel. When fuel is exhausted, execution traps. This prevents runaway scripts.

## Registering Host Functions

Before compiling any scripts, register the Rust functions you want scripts to be able to call. Registered functions are available in scripts without any import statement -- the type checker validates them at compile time using the registered type information.

Use `register_fn_raw` to register a function with explicit parameter and return type declarations:

```rust
use spite_script::{Engine, Value};
use spite_script::bindings::{ParamInfo, ScriptType};

let mut engine = Engine::new();

engine.register_fn_raw(
    "read_file",
    vec![
        ParamInfo {
            name: "path".into(),
            ty: ScriptType::String,
        },
    ],
    ScriptType::Result(Box::new(ScriptType::String)),
    |args| {
        let path = match &args[0] {
            Value::String(s) => s.clone(),
            _ => return Err("expected string argument".into()),
        };
        match std::fs::read_to_string(&path) {
            Ok(contents) => Ok(Value::String(contents)),
            Err(e) => Err(format!("failed to read {}: {}", path, e).into()),
        }
    },
);
```

The type mapping between Rust and SpiteScript is:

| Rust Type | SpiteScript Type |
|-----------|-----------------|
| `i8`, `i16`, `i32`, `i64`, `i128` | `i8`, `i16`, `i32`, `i64`, `i128` |
| `u8`, `u16`, `u32`, `u64`, `u128` | `u8`, `u16`, `u32`, `u64`, `u128` |
| `f32`, `f64` | `f32`, `f64` |
| `bool` | `bool` |
| `String` or `&str` | `String` |
| `Vec<T>` | `T[]` |
| `HashMap<K, V>` | `Map<K, V>` |
| `Option<T>` | `Option<T>` |
| `Result<T, E: ToString>` | `Result<T>` |
| `()` | `()` |

## Loading and Compiling Scripts

There are two methods for compiling a script:

### `engine.load()`

The `load` method compiles the source and returns a `CompileResult` containing diagnostics and optionally a compiled script:

```rust
let source = r#"
    @export
    fn greet(name: String) -> String {
        return `Hello, ${name}!`;
    }
"#;

match engine.load(source) {
    Ok(result) => {
        // Print any warnings
        for diag in &result.diagnostics {
            eprintln!("{}", diag);
        }
        if result.has_errors() {
            eprintln!("Compilation failed");
        }
    }
    Err(diags) => {
        for diag in &diags {
            eprintln!("{}", diag);
        }
    }
}
```

### `engine.load_script()`

The `load_script` method is similar to `load` but returns a result that includes both diagnostics and the compiled script in a single struct:

```rust
match engine.load_script(source) {
    Ok(load_result) => {
        for diag in &load_result.diagnostics {
            eprintln!("{}", diag);
        }
        if load_result.has_errors() {
            eprintln!("Compilation failed with errors");
            return;
        }
        if let Some(script) = &load_result.script {
            // Script compiled successfully, ready to execute
        }
    }
    Err(diags) => {
        for diag in &diags {
            eprintln!("{}", diag);
        }
    }
}
```

## Calling Exported Functions

Once you have a compiled script, call its `@export` functions using `script.call()`. You need a reference to the script engine (obtained from the `Engine`):

```rust
let script_engine = engine.script_engine()
    .expect("runtime engine not available");

match script.call(script_engine, "greet", &[Value::String("World".into())]) {
    Ok(value) => {
        println!("Result: {}", value); // Result: Hello, World!
    }
    Err(panic) => {
        eprintln!("Script panicked: {}", panic.message);
        for frame in &panic.trace {
            eprintln!("  at {}", frame);
        }
    }
}
```

Each `call` creates a fresh Wasmtime `Store` with a new linear memory. Scripts are stateless between calls unless you explicitly set globals.

### Return Values

The return value is a `Value` enum that mirrors the SpiteScript type system:

```rust
use spite_script::Value;

match value {
    Value::Unit => { /* () return */ }
    Value::I32(n) => println!("got i32: {}", n),
    Value::I64(n) => println!("got i64: {}", n),
    Value::F64(f) => println!("got f64: {}", f),
    Value::Bool(b) => println!("got bool: {}", b),
    Value::String(s) => println!("got string: {}", s),
    // ... other variants for arrays, maps, etc.
    _ => println!("got: {}", value),
}
```

## Complete Working Example

Here is a full example that registers a host function, compiles a script, and calls an exported function:

```rust
use spite_script::{Engine, Value};
use spite_script::bindings::{ParamInfo, ScriptType};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();

    // Register a host function that scripts can call.
    engine.register_fn_raw(
        "get_username",
        vec![],
        ScriptType::String,
        |_args| {
            Ok(Value::String(
                std::env::var("USER").unwrap_or_else(|_| "anonymous".into())
            ))
        },
    );

    // Register another host function with parameters.
    engine.register_fn_raw(
        "log_message",
        vec![
            ParamInfo { name: "level".into(), ty: ScriptType::String },
            ParamInfo { name: "msg".into(),   ty: ScriptType::String },
        ],
        ScriptType::Unit,
        |args| {
            let level = match &args[0] {
                Value::String(s) => s.clone(),
                _ => return Err("expected string".into()),
            };
            let msg = match &args[1] {
                Value::String(s) => s.clone(),
                _ => return Err("expected string".into()),
            };
            println!("[{}] {}", level, msg);
            Ok(Value::Unit)
        },
    );

    // Compile a script that uses both host functions.
    let source = r#"
        @export
        fn main() -> String {
            let user = get_username();
            log_message("info", `Script started by ${user}`);

            let items = [10, 20, 30, 40, 50];
            let total = items
                .filter(|x| x > 15)
                .map(|x| x * 2)
                .sum();

            log_message("info", `Computed total: ${total}`);
            return `Done! Total = ${total}`;
        }
    "#;

    match engine.load_script(source) {
        Ok(load_result) => {
            // Print any diagnostics (warnings, etc.)
            for diag in &load_result.diagnostics {
                eprintln!("{}", diag);
            }

            if load_result.has_errors() {
                return Err("compilation failed".into());
            }

            if let Some(script) = &load_result.script {
                let script_engine = engine.script_engine()
                    .ok_or("runtime not available")?;

                match script.call(script_engine, "main", &[]) {
                    Ok(value) => {
                        if !matches!(value, Value::Unit) {
                            println!("Return value: {}", value);
                        }
                    }
                    Err(panic) => {
                        eprintln!("Script panicked: {}", panic.message);
                        for frame in &panic.trace {
                            eprintln!("  at {}", frame);
                        }
                        return Err(panic.message.into());
                    }
                }
            }
        }
        Err(diags) => {
            for diag in &diags {
                eprintln!("{}", diag);
            }
            return Err("compilation failed".into());
        }
    }

    Ok(())
}
```

## Error Handling

The SpiteScript API surfaces errors at two levels:

### Compile Errors (`CompileResult`)

Compilation produces a list of diagnostics. Each diagnostic includes:
- A severity (error or warning)
- A message
- A source span (file, line, column, length)
- Optionally, a hint or suggested fix

When `has_errors()` returns `true`, the compiled script may be `None` or may be incomplete. Always check for errors before executing.

### Runtime Errors (`ScriptPanic`)

When a script panics at runtime (array index out of bounds, `unwrap()` on `None`, failed assertion, etc.), the `call` method returns a `ScriptPanic`:

```rust
pub struct ScriptPanic {
    pub message: String,
    pub trace: Vec<SourceFrame>,
}

pub struct SourceFrame {
    pub fn_name: String,
    pub file: String,
    pub line: u32,
    pub col: u32,
}
```

The `trace` field contains a source-level stack trace, translated from the WASM backtrace using the source map generated during compilation. This gives you human-readable function names, file paths, and line numbers.

```rust
match script.call(script_engine, "main", &[]) {
    Ok(value) => { /* success */ }
    Err(panic) => {
        eprintln!("Script panicked: {}", panic.message);
        for frame in &panic.trace {
            eprintln!("  at {}  {}:{}:{}", frame.fn_name, frame.file, frame.line, frame.col);
        }
    }
}
```

Example output:

```
Script panicked: index out of bounds: index 10, length 3
  at process  script.spite:15:8
  at main  script.spite:5:4
```
