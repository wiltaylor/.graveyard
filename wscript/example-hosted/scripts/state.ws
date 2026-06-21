// Example script for the long-lived Vm embedding API.
//
// State mutation can happen either from the host (via write_struct_at /
// set_global) or from the script (via struct field assignment and top-level
// globals). The Vm retains linear memory and globals across calls.

let mut tick_count: i32 = 0;
let mut difficulty: i32 = 1;
let mut greeting: str = "hello";

@export
fn tick() -> i32 {
    tick_count = tick_count + difficulty;
    return tick_count;
}

struct Inner {
    flag: bool,
}

struct PlayerState {
    hp: i32,
    score: i32,
    name: str,
    inner: Inner,
}

@export
fn init_state() -> PlayerState {
    let inner = Inner { flag: false };
    let s = PlayerState {
        hp: 100,
        score: 0,
        name: "unnamed",
        inner: inner,
    };
    return s;
}

@export
fn read_score(state: PlayerState) -> i32 {
    return state.score;
}

@export
fn announce(state: PlayerState) -> i32 {
    log_event("turn-start");
    return state.hp;
}

@export
fn damage(state: PlayerState, amount: i32) -> i32 {
    state.hp = state.hp - amount;
    return state.hp;
}

// Struct-typed top-level global. Its initializer (StructNew + nested struct
// + str field) is non-constant and runs inside the synthesized
// __wscript_init_globals start function.
let mut world: PlayerState = PlayerState {
    hp: 50,
    score: 0,
    name: "world",
    inner: Inner { flag: true },
};

@export
fn world_hp() -> i32 {
    return world.hp;
}

@export
fn world_bump_score() -> i32 {
    world.score = world.score + 1;
    return world.score;
}
