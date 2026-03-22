# Tooling

SpiteScript ships with built-in tooling for a first-class development experience:

- **[LSP Server](./lsp.md)** — Language Server Protocol for editor integration (completions, hover, diagnostics, and more)
- **[DAP Server](./dap.md)** — Debug Adapter Protocol for step-through debugging in VS Code

Both servers are designed to be **host-aware** — they know about all registered host functions and types, providing full IDE support including completions and type checking of host API calls.

## Quick Start

```bash
# Start the LSP server (editors connect via stdio)
spite lsp

# Start the DAP server (VS Code connects via TCP)
spite dap --port 6009
```

Both servers can also be started programmatically from within a Rust application that embeds SpiteScript, ensuring the IDE tooling has access to all host bindings.
