# Getting Started

This section covers everything you need to start using Wscript, whether you are embedding it in a Rust application or using the command-line tool to run scripts directly.

## Overview

Wscript can be used in two primary ways:

1. **As a Rust library** -- add `wscript` as a dependency in your `Cargo.toml`, create an `Engine`, register any host functions or types you want scripts to access, then compile and execute `.ws` files from your application code.

2. **As a CLI tool** -- install the `wscript` command-line tool and use it to run scripts, check them for errors, or start the LSP/DAP servers for editor integration.

Both approaches use the same compiler and runtime under the hood. The CLI is itself a thin wrapper around the library.

## What to Read Next

- **[Installation](./installation.md)** -- how to add Wscript to your project or install the CLI
- **[Using as a Rust Library](./library-usage.md)** -- the embedding API: creating an engine, registering functions, compiling and running scripts
- **[Using the CLI](./cli-usage.md)** -- running scripts, checking for errors, and starting language servers from the command line
- **[Editor Setup](./editor-setup.md)** -- syntax highlighting, LSP configuration, and debugging in VS Code
