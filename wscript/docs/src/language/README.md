# Language Overview

SpiteScript is a statically-typed, expression-oriented scripting language designed to be embedded in Rust host applications. It compiles source text to WebAssembly in memory at runtime and executes it via an embedded Wasmtime instance.

## Syntax Heritage

SpiteScript's syntax draws from three languages:

- **Rust** -- `let`/`let mut` bindings, `match` expressions, `enum` with data variants, `impl` blocks, `Option`/`Result` types, and trait-based generics.
- **Rhai** -- lightweight scripting feel, no borrow checker, ref-counted heap values, and embeddable-first design.
- **TypeScript** -- template string literals with `${expr}` interpolation, familiar operator set, and pragmatic type inference.

If you have experience with any of these languages, SpiteScript should feel immediately familiar.

## Key Characteristics

### Expression-Oriented

`if`/`else`, `match`, and `loop` can all be used as expressions that produce values:

```spite
let label = if x > 0 { "positive" } else { "negative" };

let area = match shape {
    Shape::Circle(r) => r * r * 3.14159,
    Shape::Rectangle(w, h) => w * h,
};

let result = loop {
    let val = compute();
    if val > threshold {
        break val;
    }
};
```

### Statically Typed with Inference

Types are checked at compile time, but explicit annotations are usually optional. The compiler uses bidirectional type inference to determine types from context:

```spite
let x = 42;                  // inferred as i32
let name = "Alice";           // inferred as String
let items = [1, 2, 3];       // inferred as i32[]
let doubled = items.map(|x| x * 2);  // inferred as i32[]
```

You only need explicit annotations at true ambiguity points, such as empty collections or `const` declarations.

### Safe by Default

There is no borrow checker. All heap values (strings, arrays, maps, structs) are reference-counted. The `Result` and `Option` types are first-class, with the `?` operator for ergonomic error propagation. Panics from the script are catchable by the host application.

### Embeddable

The entire runtime -- compiler, LSP server, and DAP server -- ships as a single Rust library crate. Host applications register their own functions and types, which become fully available in scripts with IDE support including completions, hover, and type checking.

## Language Tour

| Topic | Description |
|-------|-------------|
| [Variables](variables.md) | `let`, `let mut`, `const`, destructuring, shadowing |
| [Primitives](primitives.md) | Integers, floats, booleans, characters |
| [Strings](strings.md) | String type, template literals, methods |
| [Functions](functions.md) | Declaration, return rules, defaults, first-class functions |
| [Control Flow](control-flow.md) | `if`/`else`, `match`, `for`, `while`, `loop` |
| [Structs](structs.md) | Declaration, construction, `impl` blocks |
| [Enums](enums.md) | Unit, tuple, and struct variants, methods |

## Quick Example

```spite
struct Task {
    name: String,
    priority: i32,
    done: bool,
}

impl Task {
    fn new(name: String, priority: i32) -> Task {
        return Task { name, priority, done: false };
    }

    fn complete(&mut self) {
        self.done = true;
    }
}

@export
fn process_tasks(tasks: Task[]) -> String[] {
    return tasks
        .filter(|t| !t.done)
        .sort_by_key(|t| -t.priority)
        .map(|t| t.name.clone())
        .collect();
}
```
