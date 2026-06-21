# Introduction

Wscript is a statically-typed, expression-oriented scripting language designed to be embedded in Rust host applications. It compiles source code to WebAssembly (WASM) in memory at runtime and executes it via an embedded Wasmtime JIT compiler. The WASM binary is never written to disk -- Wscript uses WASM purely as an internal execution format to get high-performance native code generation without the complexity of building a custom bytecode VM.

## Design Goals

Wscript is built around six core principles:

- **Embeddable**: The entire runtime, compiler, LSP server, and DAP server ship as a single Rust library crate. Your application takes only what it needs via Cargo feature flags.

- **Strongly typed with inference**: Types are inferred from usage using bidirectional type inference. Explicit annotations are optional except at genuinely ambiguous boundaries. Integer literals default to `i32`, floats to `f64`, and closures infer their parameter types from context.

- **Functional data manipulation as a first-class citizen**: Lazy iterator pipelines with LINQ-style operators are a core language feature. The pipe operator (`|>`) and method chaining let you write expressive data transformation chains that read top to bottom.

- **Safe by default**: There is no borrow checker. All heap values are reference-counted. `Result` and `Option` are first-class types with the `?` operator for ergonomic error propagation. Panics from scripts are caught by the host and reported with full source-level stack traces.

- **Host-aware tooling**: The LSP and DAP servers are aware of all host-registered functions and types. This means full IDE support -- completions, hover, type checking, and debugging -- even for the custom API your Rust application exposes to scripts.

- **Familiar syntax**: The syntax is inspired by Rust, Rhai, and TypeScript. Developers familiar with any of these languages should feel productive quickly.

## Non-Goals

Wscript intentionally does not pursue:

- **Multi-threading within scripts.** Scripts execute single-threaded. Concurrency belongs in the host application.
- **Interoperability with external WASM modules.** The WASM output is an internal detail, not a distribution format.
- **A standalone binary distribution.** Wscript is a library, not a stand-alone language runtime.
- **A borrow checker or Rust-style ownership.** Memory safety comes from reference counting, not lifetime analysis.

## A Taste of Wscript

Here is a small program that demonstrates variables, structs, functions, pipelines, pattern matching, and error handling:

```wscript
struct Task {
    title: String,
    priority: i32,
    done: bool,
}

impl Task {
    fn new(title: String, priority: i32) -> Task {
        return Task { title, priority, done: false };
    }

    fn describe(&self) -> String {
        let status = if self.done { "done" } else { "pending" };
        return `[${status}] ${self.title} (priority ${self.priority})`;
    }
}

@error
enum TaskError {
    @msg("no tasks found matching '{query}'")
    NotFound { query: String },

    @msg("duplicate task: '{title}'")
    Duplicate { title: String },
}

fn find_top_pending(tasks: Task[], n: i32) -> Result<Task[]> {
    let results = tasks
        .filter(|t| !t.done)
        .sort_by(|a, b| b.priority <=> a.priority)
        .take(n as u64)
        .collect();

    if results.is_empty() {
        return Err(error!("no pending tasks found"));
    }
    return Ok(results);
}

@export
fn main() -> Result<String> {
    let tasks = [
        Task::new("Write docs", 3),
        Task::new("Fix bug", 5),
        Task::new("Add tests", 4),
        Task::new("Ship release", 2),
    ];

    let top = find_top_pending(tasks, 2)?;

    let summary = top
        .map(|t| t.describe())
        .collect(", ");

    return Ok(`Top tasks: ${summary}`);
}
```

This example shows:

- **Structs** with fields, constructors, and methods
- **Template strings** with `${}` interpolation
- **If/else as expressions** for inline conditionals
- **Custom error types** with `@error` and `@msg` attributes
- **Pipelines** chaining `.filter()`, `.sort_by()`, `.take()`, and `.collect()`
- **The `?` operator** for early error propagation
- **`@export`** to make `main` callable from the Rust host

## How This Book Is Organized

This book is divided into several sections:

- **[Getting Started](./getting-started/README.md)** covers installation, using Wscript as a Rust library, the CLI tool, and editor setup.

- **[Language Guide](./language/README.md)** is a comprehensive reference for every language feature: variables, types, functions, structs, enums, pattern matching, closures, pipelines, error handling, macros, and attributes.

- **[Embedding Guide](./embedding/README.md)** explains how to register host functions, types, and globals from your Rust application so that scripts can call into your codebase.

- **[Tooling](./tooling/README.md)** documents the LSP server for IDE integration and the DAP server for step debugging.

- **[Architecture](./architecture.md)** describes the compilation pipeline, runtime design, and internal crate structure.

- **[Grammar Reference](./grammar.md)** provides the full EBNF grammar for the language.
