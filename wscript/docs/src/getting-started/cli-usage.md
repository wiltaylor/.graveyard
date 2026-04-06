# Using the CLI

The `wscript` command-line tool provides commands for running scripts, checking them for errors, and starting the LSP and DAP servers. It is built from the `wscript-cli` crate and depends on the `wscript` library with all features enabled.

## Running a Script

```sh
wscript run <FILE>
```

This compiles the given `.ws` file and calls its `main` function. If the function returns a non-unit value, the result is printed to stdout. Diagnostics (warnings, errors) are printed to stderr.

### Options

| Option | Description |
|--------|-------------|
| `-f`, `--function <NAME>` | Call a function other than `main`. Default: `main` |
| `--debug` | Enable debug mode (inserts debug probes for breakpoints and tracing) |
| `--fuel <N>` | Set a maximum instruction budget. Execution traps when fuel is exhausted |

### Examples

```sh
# Run the main function
wscript run examples/hello.ws

# Run a specific exported function
wscript run examples/math.ws -f compute

# Run with debug probes enabled
wscript run examples/hello.ws --debug

# Run with a fuel limit of 1 million instructions
wscript run examples/hello.ws --fuel 1000000
```

### Shorthand

You can also pass a file directly without the `run` subcommand:

```sh
wscript examples/hello.ws
```

This is equivalent to `wscript run examples/hello.ws` -- it compiles the file and calls `main`.

## Checking a File

```sh
wscript check <FILE>
```

Compiles the file and runs the full type checker, but does not execute the script. This is useful for verifying correctness without side effects.

If the file has no errors, it prints `No errors found.` and exits with code 0. If there are errors, the diagnostics are printed to stderr and the exit code is non-zero.

```sh
# Check a single file for errors
wscript check src/logic.ws
```

## Starting the LSP Server

```sh
wscript lsp
```

Starts the Language Server Protocol server using stdio transport. This is the command your editor should be configured to run as the language server for `.ws` files. See [Editor Setup](./editor-setup.md) for configuration details.

The LSP server is only available if the `lsp` feature was enabled at build time. If it was not, the command prints an error message and exits.

## Starting the DAP Server

```sh
wscript dap --port <PORT>
```

Starts the Debug Adapter Protocol server, listening for TCP connections on the given port. The default port is 6009.

```sh
# Start on the default port (6009)
wscript dap

# Start on a custom port
wscript dap --port 9229
```

The DAP server compiles scripts in debug mode automatically. It waits for a debugger client (such as VS Code) to connect, then accepts launch, breakpoint, and stepping commands over the DAP protocol.

The DAP server is only available if the `dap` feature was enabled at build time.

## Running with `just`

If you are working within the Wscript repository, the `justfile` provides convenient shortcuts for all CLI operations:

```sh
# Run a script
just run examples/hello.ws

# Run with debug mode
just run-debug examples/hello.ws

# Check a file for errors
just check-file examples/hello.ws

# Start the LSP server
just lsp

# Start the DAP server (default port 6009)
just dap

# Start the DAP server on a custom port
just dap 9229

# Run all example scripts
just examples
```

The `just run` command is equivalent to `cargo run -p wscript-cli -- run <FILE>`, so you do not need to have the `wscript` binary installed globally when working from the repository.

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success -- the script compiled and executed without errors (or `check` found no errors) |
| 1 | Compilation failed -- one or more errors were found during parsing or type checking |
| 1 | Runtime panic -- the script panicked during execution (index out of bounds, `unwrap` on `None`, failed assertion, etc.) |
| 1 | I/O error -- the source file could not be read |
| 1 | Missing feature -- a command was used that requires a feature flag not compiled in |

All error output goes to stderr. Return values from scripts go to stdout.

## Environment Variables

| Variable | Effect |
|----------|--------|
| `RUST_LOG` | Controls log verbosity for the CLI and engine internals. Uses the `env_logger` format (e.g., `RUST_LOG=debug`, `RUST_LOG=wscript=trace`). |

```sh
# Run with verbose logging to see compiler internals
RUST_LOG=debug wscript run examples/hello.ws
```
