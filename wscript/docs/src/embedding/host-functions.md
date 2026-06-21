# Host Functions

Host functions are Rust closures that scripts can call as if they were built-in. They appear in the script's namespace without any import statement.

## Registering a Function

Use `Engine::register_fn_raw` to register a host function:

```rust
use wscript::{Engine, Value};
use wscript::bindings::{ParamInfo, ScriptType};

let mut engine = Engine::new();

engine.register_fn_raw(
    "read_file",
    vec![
        ParamInfo { name: "path".into(), ty: ScriptType::String },
    ],
    ScriptType::String,
    |args| {
        let path = match &args[0] {
            Value::String(s) => s.clone(),
            _ => return Err("expected string".into()),
        };
        match std::fs::read_to_string(&path) {
            Ok(contents) => Ok(Value::String(contents)),
            Err(e) => Err(e.to_string()),
        }
    },
);
```

## Type Mapping

Host functions declare their parameter and return types using `ScriptType`:

| Rust Type | ScriptType | Script Type |
|-----------|-----------|-------------|
| `i32` | `ScriptType::I32` | `i32` |
| `i64` | `ScriptType::I64` | `i64` |
| `f64` | `ScriptType::F64` | `f64` |
| `bool` | `ScriptType::Bool` | `bool` |
| `String` | `ScriptType::String` | `String` |
| `Vec<T>` | `ScriptType::Array(Box::new(T))` | `T[]` |
| `Option<T>` | `ScriptType::Option(Box::new(T))` | `Option<T>` |
| `()` | `ScriptType::Unit` | `()` |

## Calling from Scripts

Host functions are called without any special syntax:

```wscript
// Just call it — no import needed:
let data = read_file("config.json");
```

The type checker validates calls at compile time using the registered type information. An unregistered function call is a compile error.

## Multiple Functions

Chain multiple registrations:

```rust
engine.register_fn_raw("get_time", vec![], ScriptType::I64, |_| {
    Ok(Value::I64(chrono::Utc::now().timestamp()))
});

engine.register_fn_raw(
    "log",
    vec![ParamInfo { name: "msg".into(), ty: ScriptType::String }],
    ScriptType::Unit,
    |args| {
        if let Value::String(msg) = &args[0] {
            println!("[LOG] {}", msg);
        }
        Ok(Value::Unit)
    },
);
```
