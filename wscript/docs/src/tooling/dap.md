# DAP Server

The SpiteScript DAP server enables step-through debugging in editors that support the Debug Adapter Protocol, such as VS Code. It is enabled by the `dap` Cargo feature.

## Starting the Server

```bash
spite dap --port 6009
```

The server listens on the specified TCP port for a single debug client connection.

## VS Code Configuration

Add a launch configuration to `.vscode/launch.json`:

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

## Supported DAP Requests

| DAP Request | Description |
|-------------|-------------|
| `initialize` | Negotiate capabilities |
| `launch` | Load a script for debugging |
| `setBreakpoints` | Set breakpoints by source line |
| `configurationDone` | Signal ready to start |
| `threads` | List threads (always 1 — scripts are single-threaded) |
| `stackTrace` | Get the current call stack |
| `scopes` | Get variable scopes for a frame |
| `variables` | Inspect local variables |
| `continue` | Resume execution |
| `next` | Step over |
| `stepIn` | Step into |
| `stepOut` | Step out |
| `disconnect` | End the debug session |

## Debug Mode

When scripts are compiled with debug mode enabled, the compiler inserts probe calls at every statement boundary. These probes check a breakpoint table and, when a breakpoint is hit, pause execution and report the current state.

```rust
let engine = Engine::new().debug_mode(true);
```

## Breakpoints

Breakpoints are set by source line number. The engine maps each line to the nearest probe point:

```rust
script.set_breakpoint(12);     // Set breakpoint at line 12
script.clear_breakpoint(12);   // Remove it
script.clear_all_breakpoints(); // Remove all
```

## Variable Inspection

When paused at a breakpoint, the DAP server reports local variables with their current values. Host objects registered with `debug_display` and `debug_children` are rendered with expandable sub-nodes in the Variables panel.
