//! Example: embedding SpiteScript with the new long-lived `Vm` API.
//!
//! Demonstrates:
//!   - Persistent script state across multiple `Vm::call` invocations
//!     (linear memory is retained for the life of the `Vm`).
//!   - Host-registered functions called from scripts (`log_event`).
//!   - Struct reflection via `Vm::read_struct_at` / `write_struct_at`,
//!     including recursion into nested struct fields.
//!   - Disposing the `Vm` and starting fresh to prove state resets.

use spite_script::reflect::{FieldValue, StructView};
use spite_script::{Engine, ParamInfo, ScriptType, Value};

fn main() {
    let mut engine = Engine::new();

    // Register a user host function that scripts call by name.
    engine.register_fn_raw(
        "log_event",
        vec![ParamInfo { name: "msg".into(), ty: ScriptType::Str }],
        ScriptType::Unit,
        |args| {
            if let Some(Value::Str(s)) = args.first().cloned() {
                println!("  [HOST log_event] {s}");
            }
            Ok(None)
        },
    );

    let source = include_str!("../scripts/state.spite");
    let load = engine.load_script(source).expect("load failed");
    for d in &load.diagnostics {
        println!("  [diag] {d}");
    }
    let script = load.script.expect("no compiled script");
    let script_engine = engine.script_engine().expect("engine");

    // ── Reflection: enumerate known struct types. ────────────────────
    println!("=== reflected types ===");
    for info in script.types() {
        println!("  {} (size={})", info.name, info.size);
        for f in &info.fields {
            println!("    .{} @ {} : {:?}", f.name, f.offset, f.ty);
        }
    }

    // ── First Vm: build state, mutate from host, read back. ─────────
    println!("=== first Vm ===");
    let mut vm = script.instantiate(script_engine).expect("instantiate");

    let state_ptr = match vm.call("init_state", &[]).expect("init_state") {
        Some(Value::I32(p)) => p,
        other => panic!("expected i32 ptr, got {other:?}"),
    };
    println!("  init_state -> ptr {state_ptr}");

    // Host writes `name` and `score` into the struct in linear memory.
    vm.write_struct_at(
        state_ptr,
        "PlayerState",
        &[
            ("name", Value::Str("Flintpebble".into())),
            ("score", Value::I32(42)),
        ],
    )
    .expect("write name/score");

    // Script reads what the host wrote — linear memory persists.
    let score = vm.call("read_score", &[Value::I32(state_ptr)]).expect("read_score");
    println!("  script read_score -> {score:?}");
    assert_eq!(score, Some(Value::I32(42)));

    // Call a script fn that invokes the user host fn log_event.
    let hp = vm.call("announce", &[Value::I32(state_ptr)]).expect("announce");
    println!("  announce -> {hp:?}");
    assert_eq!(hp, Some(Value::I32(100)));

    // Full recursive read via reflection.
    let view = vm.read_struct_at(state_ptr, "PlayerState").expect("read_struct");
    print_view(&view, 1);

    // Validate every primitive field.
    if let Some(FieldValue::Primitive(Value::I32(hp))) = view.get("hp") {
        assert_eq!(*hp, 100);
    } else {
        panic!("hp missing");
    }
    if let Some(FieldValue::Primitive(Value::I32(s))) = view.get("score") {
        assert_eq!(*s, 42);
    } else {
        panic!("score missing");
    }
    if let Some(FieldValue::Primitive(Value::Str(name))) = view.get("name") {
        assert_eq!(name, "Flintpebble");
    } else {
        panic!("name missing");
    }
    match view.get("inner") {
        Some(FieldValue::Nested(inner)) => {
            println!("  inner nested struct with {} field(s)", inner.fields.len());
            if let Some(FieldValue::Primitive(Value::Bool(flag))) = inner.get("flag") {
                assert_eq!(*flag, false);
            } else {
                panic!("inner.flag missing");
            }
        }
        other => panic!("expected nested inner, got {other:?}"),
    }

    // Second pointer from a different allocation — Vm persistence check.
    let state2 = match vm.call("init_state", &[]).expect("init2") {
        Some(Value::I32(p)) => p,
        _ => panic!(),
    };
    assert_ne!(state2, state_ptr, "second allocation must get a fresh ptr");
    println!("  second init_state -> ptr {state2} (distinct)");

    // Original state must still be intact because the Vm kept memory alive.
    let view_again = vm.read_struct_at(state_ptr, "PlayerState").expect("re-read");
    if let Some(FieldValue::Primitive(Value::I32(s))) = view_again.get("score") {
        assert_eq!(*s, 42, "first state score should survive a second allocation");
    }

    drop(vm);

    // ── Second Vm: fresh state. ─────────────────────────────────────
    println!("=== second Vm (fresh state) ===");
    let mut vm2 = script.instantiate(script_engine).expect("instantiate2");
    let ptr2 = match vm2.call("init_state", &[]).expect("init") {
        Some(Value::I32(p)) => p,
        _ => panic!(),
    };
    let score2 = vm2.call("read_score", &[Value::I32(ptr2)]).expect("read");
    assert_eq!(score2, Some(Value::I32(0)), "fresh Vm must start with score=0");
    println!("  fresh Vm read_score -> {score2:?}");

    println!("OK");
}

fn print_view(v: &StructView, indent: usize) {
    let pad = "  ".repeat(indent);
    println!("{pad}{}:", v.type_name);
    for (name, val) in &v.fields {
        match val {
            FieldValue::Primitive(p) => println!("{pad}  .{name} = {p}"),
            FieldValue::Nested(n) => {
                println!("{pad}  .{name} =");
                print_view(n, indent + 2);
            }
        }
    }
}
