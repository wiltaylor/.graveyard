# Editor Setup

SpiteScript provides a VS Code extension for syntax highlighting, an LSP server for full IDE support, and a DAP server for step debugging.

## VS Code Extension

The repository includes a TextMate grammar and VS Code extension in the `editors/vscode/` directory. This extension provides:

- Syntax highlighting for `.spite` files
- Bracket matching and auto-closing pairs
- Comment toggling (`//` and `/* */`)
- Indentation rules

### Installing the Extension

The extension is not yet published to the VS Code marketplace. To use it locally:

1. Open VS Code
2. Open the Command Palette (`Ctrl+Shift+P` / `Cmd+Shift+P`)
3. Run **Developer: Install Extension from Location...**
4. Select the `editors/vscode/` directory from the SpiteScript repository

Alternatively, you can symlink the extension into your VS Code extensions directory:

```sh
ln -s /path/to/spite-script/editors/vscode ~/.vscode/extensions/spite-script
```

After installing, VS Code will recognize `.spite` files and apply syntax highlighting automatically.

## Configuring the LSP Server

The LSP server provides completions, hover information, diagnostics, go-to-definition, find references, rename, inlay hints, and more. It communicates over stdio.

### VS Code Configuration

Add the following to your VS Code `settings.json` to use the SpiteScript LSP:

```json
{
    "spite.lsp.serverPath": "spite",
    "spite.lsp.serverArgs": ["lsp"]
}
```

If you are using a generic LSP client extension (such as [vscode-languageclient](https://github.com/microsoft/vscode-languageserver-node) or a custom extension), configure it to launch `spite lsp` as the language server for files with the `spite` language ID:

```json
{
    "languageServerExample.trace.server": "verbose",
    "[spite]": {
        "editor.semanticHighlighting.enabled": true
    }
}
```

### Running from the Repository

If you have not installed the `spite` binary globally, you can point the LSP configuration at the Cargo build:

```json
{
    "spite.lsp.serverPath": "cargo",
    "spite.lsp.serverArgs": ["run", "-p", "spite-cli", "--", "lsp"]
}
```

Or use `just`:

```sh
just lsp
```

### LSP Features

Once the LSP server is running, you get the following IDE features:

| Feature | Description |
|---------|-------------|
| Diagnostics | Parse errors, type errors, and warnings shown inline as you type |
| Completions | Variables in scope, struct fields after `.`, enum variants after `::`, host functions |
| Hover | Inferred type and doc comment for the symbol under the cursor |
| Go to definition | Jump to the definition of a variable, function, struct, or trait |
| Go to type definition | Jump to the type declaration of a binding |
| Find references | Find all uses of a symbol across the file |
| Rename | Rename a symbol and all its references |
| Signature help | Shows parameter names and types as you type a function call |
| Inlay hints | Shows inferred types on `let` bindings and lambda parameters |
| Document symbols | Outline view of functions, structs, enums, and traits |
| Semantic tokens | Rich syntax highlighting based on the type checker's analysis |
| Formatting | Canonical code formatting |
| Code actions | Quick fixes such as adding `mut`, wrapping in `match`, adding `?` |

### Host Binding Awareness

A key feature of the SpiteScript LSP is that it is aware of all host-registered functions and types. When you run the LSP server from your application binary (rather than the standalone `spite` CLI), every function and type you registered with the engine appears in completions, hover, and type checking. See the [Embedding Guide](../embedding/README.md) for details on registering host bindings.

When using the standalone `spite lsp` command, only the standard library is available since no host bindings are registered.

## Configuring the DAP Server

The DAP server enables step debugging in VS Code and other editors that support the Debug Adapter Protocol.

### Starting the DAP Server

Start the server on a TCP port (default 6009):

```sh
spite dap --port 6009
```

Or from the repository:

```sh
just dap
just dap 9229   # custom port
```

The server waits for a debugger client to connect before doing anything.

### VS Code launch.json

Add the following configuration to your `.vscode/launch.json`:

```json
{
    "version": "0.2.0",
    "configurations": [
        {
            "type": "spite",
            "request": "launch",
            "name": "Debug SpiteScript",
            "program": "${file}",
            "debugServer": 6009
        }
    ]
}
```

If the `spite` debug adapter type is not recognized, you can use the generic `debugServer` approach. Add this to your `launch.json`:

```json
{
    "version": "0.2.0",
    "configurations": [
        {
            "name": "Debug SpiteScript",
            "type": "node",
            "request": "launch",
            "debugServer": 6009,
            "program": "${file}"
        }
    ]
}
```

Then start the DAP server manually before launching the debug session.

### Debug Features

Once connected, the debugger supports:

| Feature | Description |
|---------|-------------|
| Breakpoints | Set breakpoints by clicking in the gutter. The server maps them to the nearest valid source line. |
| Step Over | Execute the current statement and stop at the next one (`F10` in VS Code) |
| Step Into | Step into function calls (`F11`) |
| Step Out | Run until the current function returns (`Shift+F11`) |
| Continue | Resume execution until the next breakpoint (`F5`) |
| Variables | Inspect local variables and their values in the Variables panel |
| Call stack | View the source-level call stack in the Call Stack panel |
| Evaluate | Evaluate expressions in the Debug Console |
| Set variable | Modify variable values during a paused session |

Host types that were registered with `debug_display` and `debug_children` render with meaningful labels and expandable child properties in the Variables panel.

### Workflow

A typical debugging workflow:

1. Start the DAP server: `spite dap --port 6009`
2. Open your `.spite` file in VS Code
3. Set breakpoints by clicking in the editor gutter
4. Press `F5` to launch the debug session (using the `launch.json` configuration above)
5. The script compiles in debug mode and runs until a breakpoint is hit
6. Inspect variables, step through code, and evaluate expressions in the Debug Console
7. Press `F5` to continue or `Shift+F5` to stop

## Other Editors

### Neovim

For Neovim with `nvim-lspconfig`, add the following to your configuration:

```lua
local lspconfig = require('lspconfig')
local configs = require('lspconfig.configs')

if not configs.spite then
    configs.spite = {
        default_config = {
            cmd = { 'spite', 'lsp' },
            filetypes = { 'spite' },
            root_dir = lspconfig.util.find_git_ancestor,
            settings = {},
        },
    }
end

lspconfig.spite.setup{}
```

You will also need to add a filetype detection rule:

```lua
vim.filetype.add({
    extension = {
        spite = 'spite',
    },
})
```

### Helix

Add to your `~/.config/helix/languages.toml`:

```toml
[[language]]
name = "spite"
scope = "source.spite"
file-types = ["spite"]
language-servers = ["spite-lsp"]
comment-token = "//"
indent = { tab-width = 4, unit = "    " }

[language-server.spite-lsp]
command = "spite"
args = ["lsp"]
```

### Zed

Zed supports custom language servers. Add to your Zed settings:

```json
{
    "lsp": {
        "spite": {
            "binary": {
                "path": "spite",
                "arguments": ["lsp"]
            }
        }
    },
    "languages": {
        "SpiteScript": {
            "language_servers": ["spite"]
        }
    }
}
```
