# Example: Hosting Wscript

This is a complete example of a Rust application that embeds Wscript as its scripting engine.

## What It Demonstrates

1. **Creating an Engine** and configuring it
2. **Registering host functions** (`log_event`, `get_config`) that scripts can call
3. **Loading scripts from files** (`scripts/game_logic.ws`, `scripts/config_processor.ws`)
4. **Calling exported script functions** with arguments and receiving results
5. **Running inline scripts** compiled from string literals
6. **Error handling** for compilation failures and runtime panics

## Running

From the workspace root:

```sh
just example-hosted
```

Or directly:

```sh
cargo run -p example-hosted
```

## Project Structure

```
example-hosted/
├── Cargo.toml              # Depends on wscript with "runtime" feature
├── README.md
├── src/
│   └── main.rs             # Host application
└── scripts/
    ├── game_logic.ws     # Game simulation script
    └── config_processor.ws  # Configuration processing script
```

## Key Code Patterns

### Registering a host function

```rust
engine.register_fn_raw(
    "log_event",                                    // name visible to scripts
    vec![ParamInfo { name: "id".into(), ty: ScriptType::I32 }],  // params
    ScriptType::Unit,                               // return type
    |args| {                                        // implementation
        let id = match &args[0] { Value::I32(v) => *v, _ => return Err("bad arg".into()) };
        println!("Event: {}", id);
        Ok(Value::Unit)
    },
);
```

### Loading and calling a script

```rust
let source = std::fs::read_to_string("script.ws")?;
let result = engine.load_script(&source)?;
let script = result.script.unwrap();
let se = engine.script_engine().unwrap();

match script.call(se, "my_function", &[Value::I32(42)]) {
    Ok(Value::I32(n)) => println!("Got: {}", n),
    Err(e) => eprintln!("Error: {}", e.message),
}
```

### Running an inline script

```rust
match engine.run("@export fn main() -> i32 { return 42; }", "main", &[]) {
    Ok(Value::I32(n)) => println!("Got: {}", n),
    Err(e) => eprintln!("Error: {}", e),
}
```
