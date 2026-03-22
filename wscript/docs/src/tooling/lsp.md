# LSP Server

The SpiteScript LSP server provides IDE features for any editor that supports the Language Server Protocol. It is enabled by the `lsp` Cargo feature.

## Starting the Server

### From the CLI

```bash
spite lsp
```

This starts the LSP server on stdio, which is the standard transport for most editors.

### From a Rust Application

```rust
use spite_script::Engine;

let engine = Engine::new();
// Register host bindings first, so the LSP knows about them
// engine.register_fn_raw(...);

// The LSP server will include host bindings in completions and type checking
// Start via the CLI: your-app --lsp
```

## Supported Features

| Feature | LSP Method | Description |
|---------|-----------|-------------|
| Diagnostics | `textDocument/publishDiagnostics` | Parse errors, type errors, warnings |
| Completions | `textDocument/completion` | Keywords, types, functions, host bindings, macros |
| Hover | `textDocument/hover` | Type info and documentation for symbol under cursor |
| Semantic Tokens | `textDocument/semanticTokens/full` | Rich syntax highlighting |
| Document Symbols | `textDocument/documentSymbol` | Outline of functions, structs, enums, traits |
| Formatting | `textDocument/formatting` | Canonical code formatting |
| Inlay Hints | `textDocument/inlayHint` | Inferred types on `let` bindings |
| Go to Definition | `textDocument/definition` | Navigate to symbol definitions |

## Incremental Compilation

The LSP maintains a `QueryDb` — an incremental compiler state that caches parsed ASTs, type information, and diagnostics for each open file. On every keystroke, only the changed file is re-parsed and re-type-checked.

## Host Binding Awareness

The `QueryDb` is pre-seeded with all registered host functions, types, and globals. This means:

- Completions include host functions alongside stdlib
- Hover shows host function signatures and doc strings
- Type errors are reported for incorrect host function argument types
- Inlay hints show types inferred from host function return types

## Editor Configuration

### VS Code

Install the TextMate grammar from `editors/vscode/`, then configure the language server:

```json
{
    "spite.languageServer.path": "spite",
    "spite.languageServer.args": ["lsp"]
}
```

### Neovim (nvim-lspconfig)

```lua
vim.api.nvim_create_autocmd("FileType", {
    pattern = "spite",
    callback = function()
        vim.lsp.start({
            name = "spite-lsp",
            cmd = { "spite", "lsp" },
        })
    end,
})
```
