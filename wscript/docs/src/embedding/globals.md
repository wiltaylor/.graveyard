# Globals

Globals are pre-bound values available to scripts without any declaration. They appear as identifiers in the script's top-level scope.

## Registering Globals

```rust
use spite_script::bindings::{GlobalBinding, ScriptType};
use spite_script::Value;

engine.bindings_mut().globals.insert(
    "config_path".to_string(),
    GlobalBinding {
        name: "config_path".to_string(),
        value: Value::String("/etc/app/config.toml".to_string()),
        ty: ScriptType::String,
    },
);
```

## Using in Scripts

Globals are available without declaration:

```spite
// config_path is available as a pre-bound identifier
let path = config_path;
```

## Reading Globals After Execution

After a script runs, you can read globals that the script may have modified:

```rust
// The script's global state is isolated per call,
// but you can read return values from exported functions.
let result: i32 = script.call(&engine, "compute", &[])?;
```

## Common Patterns

Globals are useful for:

- **Configuration** — Pass settings into scripts (`config`, `env`)
- **Service handles** — Provide database connections, HTTP clients
- **Constants** — Application-specific constants (`VERSION`, `APP_NAME`)

```rust
engine.bindings_mut().globals.insert("VERSION".into(), GlobalBinding {
    name: "VERSION".into(),
    value: Value::String("1.0.0".into()),
    ty: ScriptType::String,
});

engine.bindings_mut().globals.insert("MAX_RETRIES".into(), GlobalBinding {
    name: "MAX_RETRIES".into(),
    value: Value::I32(3),
    ty: ScriptType::I32,
});
```
