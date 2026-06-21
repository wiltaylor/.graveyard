//! Wine-driven integration test runner for the Windows COM feature.
//!
//! Compiled to `x86_64-pc-windows-gnu` and executed under `wine`, this
//! embeds the wscript engine with the `com` feature enabled and runs each
//! `.ws` file in `tests/com_scripts/`. Scripts signal success/failure by
//! calling the host-registered `test_pass()` / `test_fail(msg)` functions;
//! the driver inspects a shared counter after each run.
//!
//! The scripts target `Scripting.Dictionary` from `scrrun.dll` since wine
//! ships it out of the box — no external COM servers required.

use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use wscript::{Engine, ParamInfo, ScriptType, Value};

struct Case {
    name: &'static str,
    source: &'static str,
}

const CASES: &[Case] = &[
    Case {
        name: "dictionary_basic",
        source: include_str!("../tests/com_scripts/dictionary_basic.ws"),
    },
    Case {
        name: "missing_method",
        source: include_str!("../tests/com_scripts/missing_method.ws"),
    },
    Case {
        name: "release_then_use",
        source: include_str!("../tests/com_scripts/release_then_use.ws"),
    },
    Case {
        name: "drop_on_vm_exit",
        source: include_str!("../tests/com_scripts/drop_on_vm_exit.ws"),
    },
];

fn run_case(case: &Case) -> Result<(), String> {
    let pass_count = Arc::new(AtomicUsize::new(0));
    let fail_count = Arc::new(AtomicUsize::new(0));

    let mut engine = Engine::new();
    {
        let pc = Arc::clone(&pass_count);
        engine.register_fn_raw("test_pass", vec![], ScriptType::Unit, move |_args| {
            pc.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        });
    }
    {
        let fc = Arc::clone(&fail_count);
        engine.register_fn_raw(
            "test_fail",
            vec![ParamInfo {
                name: "msg".into(),
                ty: ScriptType::Str,
            }],
            ScriptType::Unit,
            move |args| {
                if let Some(Value::Str(s)) = args.first().cloned() {
                    eprintln!("    script reported failure: {s}");
                }
                fc.fetch_add(1, Ordering::SeqCst);
                Ok(None)
            },
        );
    }

    engine
        .run(case.source, "test", &[])
        .map_err(|e| format!("run error: {e}"))?;

    let passes = pass_count.load(Ordering::SeqCst);
    let fails = fail_count.load(Ordering::SeqCst);
    if fails > 0 {
        return Err(format!("{fails} assertion(s) failed"));
    }
    if passes == 0 {
        return Err("script never called test_pass()".into());
    }
    Ok(())
}

fn main() -> ExitCode {
    let mut failed = 0usize;
    for case in CASES {
        print!("[{}] ... ", case.name);
        match run_case(case) {
            Ok(()) => println!("OK"),
            Err(e) => {
                println!("FAIL: {e}");
                failed += 1;
            }
        }
    }
    if failed == 0 {
        println!("all {} COM test cases passed", CASES.len());
        ExitCode::SUCCESS
    } else {
        println!("{failed} of {} COM test cases failed", CASES.len());
        ExitCode::FAILURE
    }
}
