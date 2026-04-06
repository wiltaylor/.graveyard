# Globals

Globals are top-level `let`, `let mut`, or `const` declarations in a script. They are declared by the script itself — the host does not pre-register them. After a script is instantiated into a `Vm`, the host can read and write mutable globals across calls, and the `Vm` retains linear memory and global state until it is dropped.

## Declaring Globals in a Script

Top-level declarations live outside any function. Their initializers run once, inside a synthesized `__spite_init_globals` start function, when the `Vm` is instantiated:

```spite
let mut tick_count: i32 = 0;
let mut difficulty: i32 = 1;
let mut greeting: str = "hello";

const MAX_HP: i32 = 100;

@export
fn tick() -> i32 {
    tick_count = tick_count + difficulty;
    return tick_count;
}
```

Struct-typed globals are allowed as well; their non-constant initializers run in the same start function:

```spite
struct PlayerState {
    hp: i32,
    score: i32,
    name: str,
}

let mut world: PlayerState = PlayerState {
    hp: 50,
    score: 0,
    name: "world",
};
```

## Accessing Globals from the Host

Instantiate the compiled script into a long-lived `Vm`, then use `get_global` / `set_global` for primitive globals and `read_global_struct` / `write_global_struct` for struct-typed globals:

```rust
use spite_script::{Engine, Value};

let mut engine = Engine::new();
let result = engine.load_script(source)?;
let script = result.script.expect("compilation succeeded");

let script_engine = engine.script_engine().expect("runtime available");
let mut vm = script.instantiate(script_engine)?;

// Read a primitive global.
let diff = vm.get_global("difficulty")?;        // Value::I32(1)

// Write a primitive global (including `str`, which is interned into the
// host string table).
vm.set_global("difficulty", Value::I32(3))?;
vm.set_global("greeting", Value::Str("hi there".into()))?;

// Call an exported function — global state persists across calls.
let tick1 = vm.call("tick", &[])?;              // Some(I32(3))
let tick2 = vm.call("tick", &[])?;              // Some(I32(6))
```

`set_global` rejects struct-typed globals; use `write_global_struct` to update struct fields in place:

```rust
let view = vm.read_global_struct("world")?;    // StructView (recursive)
// ... inspect fields via reflection ...
```

## Iterating Reflection Metadata

`Vm::globals()` yields a `GlobalInfo` per declared top-level global, including its name and kind (primitive `ScriptType` or struct name). This is useful for debuggers and editors that want to enumerate script state.

## Notes and Limits

- There is **no host-registered globals API**. The host cannot inject a new global name into the script's scope; the script must declare it.
- A `Vm` owns its linear memory and global storage. Drop the `Vm` to reset state, or create a new one from the same `CompiledScript`.
- `str` globals are stored as `i32` handles into a host-side string table. `get_global` / `set_global` transparently resolve and intern strings for you.
- `bool` globals are stored as `i32` (0 or 1) but surface as `Value::Bool`.
