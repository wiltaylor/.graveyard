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

    // ── Top-level globals: host ↔ script. ───────────────────────────
    // Script declares `let mut tick_count: i32 = 0;` and
    // `let mut difficulty: i32 = 1;`. Host pokes `difficulty` before each
    // tick, script updates `tick_count`, host reads it back.
    vm.set_global("difficulty", Value::I32(3)).expect("set difficulty");
    assert_eq!(
        vm.get_global("difficulty").unwrap(),
        Value::I32(3),
        "difficulty roundtrip"
    );
    let t1 = vm.call("tick", &[]).expect("tick1");
    let t2 = vm.call("tick", &[]).expect("tick2");
    println!("  tick 1 -> {t1:?}, tick 2 -> {t2:?}");
    assert_eq!(t1, Some(Value::I32(3)));
    assert_eq!(t2, Some(Value::I32(6)));
    let tick_count = vm.get_global("tick_count").expect("get tick_count");
    println!("  host-observed tick_count = {tick_count:?}");
    assert_eq!(tick_count, Value::I32(6));

    // Script-side struct field mutation (now that lower_assign handles it).
    let hp_after = vm
        .call("damage", &[Value::I32(state_ptr), Value::I32(25)])
        .expect("damage");
    println!("  damage(25) -> hp {hp_after:?}");
    assert_eq!(hp_after, Some(Value::I32(75)));
    let view_dmg = vm.read_struct_at(state_ptr, "PlayerState").expect("re-read");
    if let Some(FieldValue::Primitive(Value::I32(h))) = view_dmg.get("hp") {
        assert_eq!(*h, 75, "script-side field mutation must survive in memory");
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

    // ── Struct-typed top-level global. ──────────────────────────────
    // `world: PlayerState` initializer runs in __spite_init_globals at
    // instantiation time; host reads/writes via read_global_struct /
    // write_global_struct.
    println!("  --- global structs ---");
    for info in vm.globals() {
        println!("    global {} : {:?} (mutable={})", info.name, info.kind, info.mutable);
    }
    let world_hp = vm.call("world_hp", &[]).expect("world_hp");
    println!("  world_hp -> {world_hp:?}");
    assert_eq!(world_hp, Some(Value::I32(50)));

    let world_view = vm.read_global_struct("world").expect("read_global_struct");
    print_view(&world_view, 1);
    if let Some(FieldValue::Primitive(Value::Str(name))) = world_view.get("name") {
        assert_eq!(name, "world");
    }
    if let Some(FieldValue::Nested(inner)) = world_view.get("inner") {
        if let Some(FieldValue::Primitive(Value::Bool(flag))) = inner.get("flag") {
            assert!(*flag, "nested init'd flag must be true");
        }
    }

    vm.write_global_struct(
        "world",
        &[
            ("hp", Value::I32(7)),
            ("name", Value::Str("Flintpebble's realm".into())),
        ],
    )
    .expect("write_global_struct");
    let new_hp = vm.call("world_hp", &[]).expect("world_hp2");
    assert_eq!(new_hp, Some(Value::I32(7)));
    let bumped = vm.call("world_bump_score", &[]).expect("bump");
    assert_eq!(bumped, Some(Value::I32(1)));
    println!("  after host write, world_hp -> {new_hp:?}, score bump -> {bumped:?}");

    // Str global roundtrip.
    let greeting = vm.get_global("greeting").expect("greeting");
    println!("  greeting global -> {greeting:?}");
    assert_eq!(greeting, Value::Str("hello".into()));
    vm.set_global("greeting", Value::Str("salutations".into()))
        .expect("set greeting");
    assert_eq!(
        vm.get_global("greeting").unwrap(),
        Value::Str("salutations".into())
    );

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
    let tc2 = vm2.get_global("tick_count").expect("tick_count global");
    assert_eq!(tc2, Value::I32(0), "fresh Vm must reset tick_count global");
    println!("  fresh Vm tick_count -> {tc2:?}");
    // Struct/str globals are re-initialized by the start fn on the new Vm.
    let hp2 = vm2.call("world_hp", &[]).expect("world_hp");
    assert_eq!(hp2, Some(Value::I32(50)), "fresh Vm must reset world.hp");
    let g2 = vm2.get_global("greeting").unwrap();
    assert_eq!(g2, Value::Str("hello".into()), "fresh Vm must reset greeting");
    println!("  fresh Vm world_hp -> {hp2:?}, greeting -> {g2:?}");

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
