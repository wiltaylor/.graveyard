use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "wscript", about = "Wscript language runner and tooling")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Run a .ws file directly
    #[arg(value_name = "FILE")]
    file: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Command {
    /// Run a .ws file
    Run {
        #[arg(value_name = "FILE")]
        file: PathBuf,
        /// Function to call (default: "main")
        #[arg(short, long, default_value = "main")]
        function: String,
        /// Enable debug mode
        #[arg(long)]
        debug: bool,
        /// Maximum fuel (instruction budget)
        #[arg(long)]
        fuel: Option<u64>,
    },
    /// Start LSP server (stdio transport)
    Lsp,
    /// Start DAP server (TCP transport)
    Dap {
        /// Port to listen on
        #[arg(short, long, default_value = "6009")]
        port: u16,
    },
    /// Check a file for errors without running
    Check {
        #[arg(value_name = "FILE")]
        file: PathBuf,
    },
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    env_logger::init();
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Run {
            file,
            function,
            debug,
            fuel,
        }) => {
            cmd_run(file, &function, debug, fuel)?;
        }
        Some(Command::Lsp) => {
            cmd_lsp().await?;
        }
        Some(Command::Dap { port }) => {
            cmd_dap(port).await?;
        }
        Some(Command::Check { file }) => {
            cmd_check(file)?;
        }
        None => {
            if let Some(file) = cli.file {
                cmd_run(file, "main", false, None)?;
            } else {
                eprintln!("Usage: wscript <file> or wscript <command>");
                eprintln!("Run 'wscript --help' for more information.");
                std::process::exit(1);
            }
        }
    }
    Ok(())
}

fn cmd_run(file: PathBuf, function: &str, debug: bool, fuel: Option<u64>) -> miette::Result<()> {
    let source = std::fs::read_to_string(&file)
        .map_err(|e| miette::miette!("Failed to read {}: {}", file.display(), e))?;

    let mut engine = wscript::Engine::new();
    if debug {
        engine = engine.debug_mode(true);
    }
    if let Some(f) = fuel {
        engine = engine.max_fuel(f);
    }

    match engine.load_script(&source) {
        Ok(load_result) => {
            // Print any warnings/diagnostics.
            for diag in &load_result.diagnostics {
                eprintln!("{}", diag);
            }
            if load_result.has_errors() {
                return Err(miette::miette!("Compilation failed with errors"));
            }

            // If we have a compiled script, execute it.
            if let Some(script) = &load_result.script {
                let script_engine = engine
                    .script_engine()
                    .ok_or_else(|| miette::miette!("Runtime engine not available"))?;

                match script.call(script_engine, function, &[]) {
                    Ok(Some(value)) => {
                        println!("{}", value);
                    }
                    Ok(None) => {}
                    Err(panic) => {
                        eprintln!("script panic: {}", panic.message);
                        for frame in &panic.trace {
                            eprintln!("{}", frame);
                        }
                        return Err(miette::miette!("Script panicked: {}", panic.message));
                    }
                }
            } else {
                println!(
                    "Compiled successfully (no WASM bytes produced; codegen may not be implemented yet)."
                );
            }

            Ok(())
        }
        Err(diags) => {
            for diag in &diags {
                eprintln!("{}", diag);
            }
            Err(miette::miette!(
                "Compilation failed with {} error(s)",
                diags.len()
            ))
        }
    }
}

fn cmd_check(file: PathBuf) -> miette::Result<()> {
    let source = std::fs::read_to_string(&file)
        .map_err(|e| miette::miette!("Failed to read {}: {}", file.display(), e))?;

    let engine = wscript::Engine::new();
    match engine.load(&source) {
        Ok(result) => {
            for diag in &result.diagnostics {
                eprintln!("{}", diag);
            }
            if result.has_errors() {
                Err(miette::miette!("Check failed"))
            } else {
                println!("No errors found.");
                Ok(())
            }
        }
        Err(diags) => {
            for diag in &diags {
                eprintln!("{}", diag);
            }
            Err(miette::miette!(
                "Check failed with {} error(s)",
                diags.len()
            ))
        }
    }
}

async fn cmd_lsp() -> miette::Result<()> {
    #[cfg(feature = "lsp")]
    {
        use std::sync::Arc;
        use tokio::sync::RwLock;
        use tower_lsp::{LspService, Server};
        use wscript::lsp::WscriptLspServer;
        use wscript::query_db::QueryDb;

        let bindings = Arc::new(wscript::BindingRegistry::new());
        let db = Arc::new(RwLock::new(QueryDb::new(bindings)));

        let (service, socket) = LspService::new(|client| WscriptLspServer::new(client, db.clone()));
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();
        Server::new(stdin, stdout, socket).serve(service).await;
        Ok(())
    }
    #[cfg(not(feature = "lsp"))]
    {
        Err(miette::miette!(
            "LSP support not compiled in. Build with --features lsp"
        ))
    }
}

async fn cmd_dap(_port: u16) -> miette::Result<()> {
    #[cfg(feature = "dap")]
    {
        let engine = wscript::Engine::new().debug_mode(true);
        let mut server = wscript::dap::WscriptDapServer::new(engine, _port);
        server
            .serve()
            .await
            .map_err(|e| miette::miette!("DAP server error: {}", e))?;
        Ok(())
    }
    #[cfg(not(feature = "dap"))]
    {
        Err(miette::miette!(
            "DAP support not compiled in. Build with --features dap"
        ))
    }
}
