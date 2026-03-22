# Installation

## Adding SpiteScript as a Cargo Dependency

Add `spite-script` to your project's `Cargo.toml`:

```toml
[dependencies]
spite-script = "0.1"
```

This pulls in the default feature set, which includes the `runtime` feature for compiling and executing scripts via Wasmtime.

### Feature Flags

SpiteScript uses Cargo feature flags to let you include only the components you need:

| Feature | Default | Dependencies Enabled | Description |
|---------|---------|---------------------|-------------|
| `runtime` | Yes | `wasmtime` | Compile and execute scripts via Wasmtime JIT |
| `lsp` | No | `tower-lsp`, `tokio` | LSP server for editor integration |
| `dap` | No | `dap-rs`, `tokio` | DAP server for step debugging |
| `full` | No | All of the above | Everything in one flag |
| *(none)* | -- | -- | Compile and type-check only; no execution |

### Examples

```toml
# Compiler only -- type-check scripts without executing them:
spite-script = { version = "0.1", default-features = false }

# Runtime + LSP support (for an application that runs scripts and serves an LSP):
spite-script = { version = "0.1", features = ["lsp"] }

# Everything:
spite-script = { version = "0.1", features = ["full"] }
```

When no features are enabled (not even `runtime`), the crate still provides the full compiler pipeline -- lexer, parser, type checker, and IR lowering -- so you can check scripts for errors without needing the Wasmtime dependency.

## Installing the CLI

The SpiteScript CLI is a separate crate called `spite-cli` that produces a binary named `spite`. It depends on the `spite-script` library with all features enabled.

### From the Repository

If you have the SpiteScript repository cloned locally:

```sh
cargo install --path crates/spite-cli
```

This installs the `spite` binary into your Cargo bin directory (typically `~/.cargo/bin/`).

### Running Without Installing

You can also run the CLI directly from the repository without installing it globally:

```sh
cargo run -p spite-cli -- run examples/hello.spite
```

## Building from Source

SpiteScript uses [just](https://github.com/casey/just) as a command runner. If you do not have `just` installed:

```sh
cargo install just
```

Then, from the repository root:

```sh
# Build all crates (library + CLI)
just build

# Build in release mode
just build-release

# Build only the compiler (no runtime, LSP, or DAP dependencies)
just build-compiler

# Run all checks (formatting, clippy, tests)
just check

# Run the test suite
just test

# See all available commands
just --list
```

### Workspace Structure

The repository is a Cargo workspace with two crates:

| Crate | Path | Description |
|-------|------|-------------|
| `spite-script` | `crates/spite-script/` | The core library -- compiler, runtime, LSP, DAP |
| `spite-cli` | `crates/spite-cli/` | The `spite` command-line tool |

### Key Dependencies

| Crate | Purpose |
|-------|---------|
| `wasmtime` | WASM execution (JIT compilation to native code) |
| `walrus` | WASM IR construction during code generation |
| `tower-lsp` | LSP server framework |
| `dap` | DAP server framework |
| `tokio` | Async runtime for LSP and DAP servers |
| `miette` | Diagnostic rendering (fancy error messages) |
| `clap` | CLI argument parsing |
