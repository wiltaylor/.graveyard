//! Example: Hosting SpiteScript in a Rust Application
//!
//! This demonstrates how to embed SpiteScript as a scripting engine
//! inside a Rust application. The host registers custom functions,
//! compiles scripts, and calls exported script functions.
//!
//! Run with:
//!   cargo run -p example-hosted
//!
//! Or from the workspace root:
//!   just example-hosted

use spite_script::bindings::{ParamInfo, ScriptType};
use spite_script::{Engine, Value};

fn main() {
    println!("=== SpiteScript Hosted Example ===");
    println!();

    // ── 1. Create the engine ────────────────────────────────────────

    let mut engine = Engine::new();

    // ── 2. Register host functions ──────────────────────────────────
    //
    // These Rust functions become available to all scripts without
    // any import statement. The type checker validates calls at
    // compile time.

    // log_event(event_id: i32) — scripts call this to notify the host
    engine.register_fn_raw(
        "log_event",
        vec![ParamInfo {
            name: "event_id".into(),
            ty: ScriptType::I32,
        }],
        ScriptType::Unit,
        |args| {
            let event_id = match &args[0] {
                Value::I32(v) => *v,
                _ => return Err("expected i32".into()),
            };
            let event_name = match event_id {
                1 => "TurnStarted",
                2 => "LevelUp",
                3 => "ScoreUpdated",
                4 => "TurnEnded",
                5 => "PlayerDied",
                _ => "Unknown",
            };
            println!("  [HOST] Event: {} (id={})", event_name, event_id);
            Ok(Value::Unit)
        },
    );

    // get_config(key: i32) -> i32 — scripts can read host configuration
    engine.register_fn_raw(
        "get_config",
        vec![ParamInfo {
            name: "key".into(),
            ty: ScriptType::I32,
        }],
        ScriptType::I32,
        |args| {
            let key = match &args[0] {
                Value::I32(v) => *v,
                _ => return Err("expected i32".into()),
            };
            // Simulate a config store
            let value = match key {
                0 => 100, // max_health
                1 => 10,  // base_damage
                2 => 50,  // level_up_threshold
                _ => 0,
            };
            Ok(Value::I32(value))
        },
    );

    // ── 3. Load and run scripts ─────────────────────────────────────

    run_game_logic(&engine);
    println!();
    run_config_processor(&engine);
    println!();
    run_inline_script(&engine);
}

/// Demonstrate loading a script from a file and calling multiple exports.
fn run_game_logic(engine: &Engine) {
    println!("--- Game Logic Script ---");

    let source = include_str!("../scripts/game_logic.spite");

    // Compile the script
    let load_result = match engine.load_script(source) {
        Ok(r) => r,
        Err(diags) => {
            eprintln!("Compilation failed:");
            for d in &diags {
                eprintln!("  {}", d);
            }
            return;
        }
    };

    // Print any warnings
    for d in &load_result.diagnostics {
        println!("  [WARN] {}", d);
    }

    let script = match &load_result.script {
        Some(s) => s,
        None => {
            eprintln!("  No executable module produced.");
            return;
        }
    };

    let script_engine = engine.script_engine().unwrap();

    // Call add_scores(30, 12) — simple function call
    println!();
    println!("  Calling add_scores(30, 12)...");
    match script.call(script_engine, "add_scores", &[Value::I32(30), Value::I32(12)]) {
        Ok(Value::I32(result)) => println!("  Result: {}", result),
        Ok(other) => println!("  Unexpected result: {:?}", other),
        Err(e) => eprintln!("  Error: {}", e.message),
    }

    // Call factorial(10) — demonstrates recursion
    println!();
    println!("  Calling factorial(10)...");
    match script.call(script_engine, "factorial", &[Value::I32(10)]) {
        Ok(Value::I32(result)) => println!("  Result: {} (10! = 3628800)", result),
        Ok(other) => println!("  Unexpected result: {:?}", other),
        Err(e) => eprintln!("  Error: {}", e.message),
    }

    // Call game_turn(health=100, level=3, enemy_power=5)
    println!();
    println!("  Calling game_turn(health=100, level=3, enemy_power=5)...");
    match script.call(
        script_engine,
        "game_turn",
        &[Value::I32(100), Value::I32(3), Value::I32(5)],
    ) {
        Ok(Value::I32(score)) => println!("  Final score: {}", score),
        Ok(other) => println!("  Unexpected result: {:?}", other),
        Err(e) => eprintln!("  Error: {}", e.message),
    }

    // Call game_turn with lethal damage
    println!();
    println!("  Calling game_turn(health=5, level=1, enemy_power=50)...");
    match script.call(
        script_engine,
        "game_turn",
        &[Value::I32(5), Value::I32(1), Value::I32(50)],
    ) {
        Ok(Value::I32(score)) => {
            if score == 0 {
                println!("  Player died! Score: 0");
            } else {
                println!("  Final score: {}", score);
            }
        }
        Ok(other) => println!("  Unexpected result: {:?}", other),
        Err(e) => eprintln!("  Error: {}", e.message),
    }
}

/// Demonstrate loading a second script with different logic.
fn run_config_processor(engine: &Engine) {
    println!("--- Config Processor Script ---");

    let source = include_str!("../scripts/config_processor.spite");

    let load_result = match engine.load_script(source) {
        Ok(r) => r,
        Err(diags) => {
            eprintln!("Compilation failed:");
            for d in &diags {
                eprintln!("  {}", d);
            }
            return;
        }
    };

    let script = match &load_result.script {
        Some(s) => s,
        None => {
            eprintln!("  No executable module produced.");
            return;
        }
    };

    let se = engine.script_engine().unwrap();

    // Count values above threshold
    println!();
    println!("  Calling count_above_threshold(50)...");
    match script.call(se, "count_above_threshold", &[Value::I32(50)]) {
        Ok(Value::I32(count)) => println!("  {} values exceed 50", count),
        Ok(other) => println!("  Unexpected: {:?}", other),
        Err(e) => eprintln!("  Error: {}", e.message),
    }

    // Compute weighted score
    println!();
    println!("  Calling weighted_score(quality=80, speed=60, reliability=90)...");
    match script.call(
        se,
        "weighted_score",
        &[Value::I32(80), Value::I32(60), Value::I32(90)],
    ) {
        Ok(Value::I32(score)) => println!("  Weighted score: {}", score),
        Ok(other) => println!("  Unexpected: {:?}", other),
        Err(e) => eprintln!("  Error: {}", e.message),
    }

    // Classify values
    println!();
    let labels = ["low", "medium", "high", "critical"];
    for &value in &[10, 30, 60, 90] {
        match script.call(se, "classify", &[Value::I32(value)]) {
            Ok(Value::I32(cat)) => {
                let label = labels.get(cat as usize).unwrap_or(&"unknown");
                println!("  classify({}) = {} ({})", value, cat, label);
            }
            Ok(other) => println!("  Unexpected: {:?}", other),
            Err(e) => eprintln!("  Error: {}", e.message),
        }
    }
}

/// Demonstrate compiling and running an inline script string.
fn run_inline_script(engine: &Engine) {
    println!("--- Inline Script ---");

    let source = r#"
        fn fib(n: i32) -> i32 {
            if n <= 1 { return n; }
            return fib(n - 1) + fib(n - 2);
        }

        @export
        fn compute() -> i32 {
            let mut sum = 0;
            for i in 0..10 {
                sum = sum + fib(i);
            }
            return sum;
        }
    "#;

    println!();
    println!("  Compiling inline script...");

    match engine.run(source, "compute", &[]) {
        Ok(Value::I32(result)) => println!("  sum of fib(0)..fib(9) = {}", result),
        Ok(other) => println!("  Unexpected: {:?}", other),
        Err(e) => eprintln!("  Error: {}", e),
    }
}
