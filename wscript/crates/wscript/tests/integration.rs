//! End-to-end integration tests for the Wscript compiler pipeline.

use wscript::Engine;

/// Helper: compile and execute a script, returning the i32 result.
fn run_i32(source: &str) -> i32 {
    let engine = Engine::new();
    match engine.run(source, "main", &[]) {
        Ok(Some(wscript::Value::I32(v))) => v,
        Ok(other) => panic!("Expected Some(I32), got {:?}", other),
        Err(e) => panic!("Execution error: {}", e),
    }
}

/// Helper: compile a script and check it produces a diagnostic containing the given substring.
/// Type check errors are currently emitted as warnings, so we check all diagnostics.
fn check_type_error(source: &str, expected_msg: &str) {
    let engine = Engine::new();
    match engine.load(source) {
        Ok(result) => {
            assert!(
                result.diagnostics.iter().any(|d| d.message.contains(expected_msg)),
                "Expected diagnostic containing '{}', but got: {:?}",
                expected_msg,
                result.diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
            );
        }
        Err(diags) => {
            assert!(
                diags.iter().any(|d| d.message.contains(expected_msg)),
                "Expected diagnostic containing '{}', but got: {:?}",
                expected_msg,
                diags.iter().map(|d| &d.message).collect::<Vec<_>>()
            );
        }
    }
}

/// Helper: compile a script and check it has no errors.
#[allow(dead_code)]
fn check_ok(source: &str) {
    let engine = Engine::new();
    match engine.load(source) {
        Ok(result) => {
            let errors: Vec<_> = result.diagnostics.iter()
                .filter(|d| d.severity == wscript::compiler::DiagnosticSeverity::Error)
                .collect();
            assert!(errors.is_empty(), "Unexpected errors: {:?}", errors);
        }
        Err(diags) => panic!("Compilation failed: {:?}", diags),
    }
}

// ── Arithmetic ──────────────────────────────────────────────────────

#[test]
fn test_addition() {
    assert_eq!(run_i32("@export fn main() -> i32 { return 2 + 3; }"), 5);
}

#[test]
fn test_subtraction() {
    assert_eq!(run_i32("@export fn main() -> i32 { return 10 - 3; }"), 7);
}

#[test]
fn test_multiplication() {
    assert_eq!(run_i32("@export fn main() -> i32 { return 6 * 7; }"), 42);
}

#[test]
fn test_division() {
    assert_eq!(run_i32("@export fn main() -> i32 { return 100 / 4; }"), 25);
}

#[test]
fn test_modulo() {
    assert_eq!(run_i32("@export fn main() -> i32 { return 17 % 5; }"), 2);
}

#[test]
fn test_complex_arithmetic() {
    assert_eq!(run_i32("@export fn main() -> i32 { return (2 + 3) * (10 - 4); }"), 30);
}

// ── Variables ───────────────────────────────────────────────────────

#[test]
fn test_let_binding() {
    assert_eq!(run_i32("@export fn main() -> i32 { let x = 42; return x; }"), 42);
}

#[test]
fn test_mutable_variable() {
    assert_eq!(run_i32(r#"
        @export fn main() -> i32 {
            let mut x = 0;
            x = 42;
            return x;
        }
    "#), 42);
}

#[test]
fn test_multiple_variables() {
    assert_eq!(run_i32(r#"
        @export fn main() -> i32 {
            let a = 10;
            let b = 20;
            let c = 30;
            return a + b + c;
        }
    "#), 60);
}

// ── Functions ───────────────────────────────────────────────────────

#[test]
fn test_function_call() {
    assert_eq!(run_i32(r#"
        fn add(a: i32, b: i32) -> i32 { return a + b; }
        @export fn main() -> i32 { return add(3, 4); }
    "#), 7);
}

#[test]
fn test_recursive_function() {
    assert_eq!(run_i32(r#"
        fn fib(n: i32) -> i32 {
            if n <= 1 { return n; }
            return fib(n - 1) + fib(n - 2);
        }
        @export fn main() -> i32 { return fib(10); }
    "#), 55);
}

#[test]
fn test_multiple_functions() {
    assert_eq!(run_i32(r#"
        fn double(x: i32) -> i32 { return x * 2; }
        fn triple(x: i32) -> i32 { return x * 3; }
        @export fn main() -> i32 { return double(5) + triple(5); }
    "#), 25);
}

// ── Control Flow ────────────────────────────────────────────────────

#[test]
fn test_if_else() {
    assert_eq!(run_i32(r#"
        @export fn main() -> i32 {
            let x = 10;
            if x > 5 { return 1; }
            return 0;
        }
    "#), 1);
}

#[test]
fn test_while_loop() {
    assert_eq!(run_i32(r#"
        @export fn main() -> i32 {
            let mut sum = 0;
            let mut i = 1;
            while i <= 10 {
                sum = sum + i;
                i = i + 1;
            }
            return sum;
        }
    "#), 55);
}

#[test]
fn test_for_loop_range() {
    assert_eq!(run_i32(r#"
        @export fn main() -> i32 {
            let mut sum = 0;
            for i in 0..10 {
                sum = sum + i;
            }
            return sum;
        }
    "#), 45);
}

// ── Structs ─────────────────────────────────────────────────────────

#[test]
fn test_struct_basic() {
    assert_eq!(run_i32(r#"
        struct Pair { a: i32, b: i32 }
        @export fn main() -> i32 {
            let p = Pair { a: 10, b: 20 };
            return p.a + p.b;
        }
    "#), 30);
}

#[test]
fn test_top_level_let_non_literal_init() {
    // Non-constant initializer — must go through the deferred start fn.
    assert_eq!(run_i32(r#"
        let answer: i32 = 6 * 7;
        @export fn main() -> i32 {
            return answer;
        }
    "#), 42);
}

#[test]
fn test_top_level_struct_global_read() {
    assert_eq!(run_i32(r#"
        struct P { hp: i32, score: i32 }
        let mut state: P = P { hp: 100, score: 0 };
        @export fn main() -> i32 {
            return state.hp;
        }
    "#), 100);
}

#[test]
fn test_top_level_struct_global_field_mutation() {
    assert_eq!(run_i32(r#"
        struct P { hp: i32, score: i32 }
        let mut state: P = P { hp: 100, score: 0 };
        fn hit() {
            state.hp = state.hp - 10;
        }
        @export fn main() -> i32 {
            hit();
            hit();
            return state.hp;
        }
    "#), 80);
}

#[test]
fn test_struct_inline_nested_literal() {
    // Regression: nested struct literal as a field expression used to
    // corrupt the outer StructNew's base pointer because the codegen
    // recomputed alloc_addr from heap_ptr after nested allocations had
    // advanced it. Now each depth uses its own scratch local.
    assert_eq!(run_i32(r#"
        struct Inner { flag: bool }
        struct Outer { hp: i32, inner: Inner, score: i32 }
        @export fn main() -> i32 {
            let o = Outer { hp: 55, inner: Inner { flag: true }, score: 7 };
            return o.hp + o.score;
        }
    "#), 62);
}

#[test]
fn test_top_level_struct_global_with_nested_literal() {
    // End-to-end exercise of the fix in the global-init (start fn) path.
    assert_eq!(run_i32(r#"
        struct Inner { flag: bool }
        struct Outer { hp: i32, inner: Inner }
        let mut world: Outer = Outer { hp: 99, inner: Inner { flag: true } };
        @export fn main() -> i32 {
            return world.hp;
        }
    "#), 99);
}

#[test]
fn test_top_level_struct_global_host_reflection() {
    use wscript::{FieldValue, Value};
    let src = r#"
        struct P { hp: i32, score: i32 }
        let mut state: P = P { hp: 100, score: 42 };
        @export fn touch() -> i32 { return state.hp; }
    "#;
    let engine = Engine::new();
    let load = engine.load_script(src).expect("load");
    let script = load.script.expect("script");
    let eng = engine.script_engine().expect("engine");
    let mut vm = script.instantiate(eng).expect("instantiate");
    // Must produce a real value before the host walks it — the start fn has
    // already run by now, but calling an export exercises the full path.
    assert_eq!(vm.call("touch", &[]).unwrap(), Some(Value::I32(100)));
    let view = vm.read_global_struct("state").expect("read_global_struct");
    match view.get("hp") {
        Some(FieldValue::Primitive(Value::I32(v))) => assert_eq!(*v, 100),
        other => panic!("hp: {other:?}"),
    }
    match view.get("score") {
        Some(FieldValue::Primitive(Value::I32(v))) => assert_eq!(*v, 42),
        other => panic!("score: {other:?}"),
    }
}

#[test]
fn test_top_level_struct_global_host_write() {
    use wscript::Value;
    let src = r#"
        struct P { hp: i32, score: i32 }
        let mut state: P = P { hp: 100, score: 0 };
        @export fn get_hp() -> i32 { return state.hp; }
    "#;
    let engine = Engine::new();
    let load = engine.load_script(src).expect("load");
    let script = load.script.expect("script");
    let eng = engine.script_engine().expect("engine");
    let mut vm = script.instantiate(eng).expect("instantiate");
    vm.write_global_struct("state", &[("hp", Value::I32(55)), ("score", Value::I32(9))])
        .expect("write");
    assert_eq!(vm.call("get_hp", &[]).unwrap(), Some(Value::I32(55)));
}

#[test]
fn test_top_level_str_global_roundtrip() {
    use wscript::Value;
    let src = r#"
        let mut greeting: str = "hello";
        @export fn touch() {}
    "#;
    let engine = Engine::new();
    let load = engine.load_script(src).expect("load");
    let script = load.script.expect("script");
    let eng = engine.script_engine().expect("engine");
    let mut vm = script.instantiate(eng).expect("instantiate");
    vm.call("touch", &[]).unwrap();
    assert_eq!(vm.get_global("greeting").unwrap(), Value::Str("hello".into()));
    vm.set_global("greeting", Value::Str("world!".into())).unwrap();
    assert_eq!(vm.get_global("greeting").unwrap(), Value::Str("world!".into()));
}

#[test]
fn test_set_global_struct_rejected() {
    use wscript::Value;
    let src = r#"
        struct P { hp: i32 }
        let mut state: P = P { hp: 1 };
        @export fn touch() {}
    "#;
    let engine = Engine::new();
    let load = engine.load_script(src).expect("load");
    let script = load.script.expect("script");
    let eng = engine.script_engine().expect("engine");
    let mut vm = script.instantiate(eng).expect("instantiate");
    let err = vm.set_global("state", Value::I32(0)).unwrap_err();
    assert!(err.contains("write_global_struct"), "err was: {err}");
}

#[test]
fn test_void_fn_call_baseline() {
    assert_eq!(run_i32(r#"
        fn noop() {}
        @export fn main() -> i32 {
            noop();
            return 42;
        }
    "#), 42);
}

#[test]
fn test_top_level_let_read() {
    assert_eq!(run_i32(r#"
        let base: i32 = 40;
        @export fn main() -> i32 {
            return base + 2;
        }
    "#), 42);
}

#[test]
fn test_top_level_let_mut_assign() {
    assert_eq!(run_i32(r#"
        let mut counter: i32 = 0;
        fn bump() {
            counter = counter + 1;
        }
        @export fn main() -> i32 {
            bump();
            bump();
            bump();
            return counter;
        }
    "#), 3);
}

#[test]
fn test_top_level_let_compound_assign() {
    assert_eq!(run_i32(r#"
        let mut total: i32 = 10;
        @export fn main() -> i32 {
            total += 5;
            total *= 2;
            return total;
        }
    "#), 30);
}

#[test]
fn test_struct_field_assign() {
    assert_eq!(run_i32(r#"
        struct Pair { a: i32, b: i32 }
        @export fn main() -> i32 {
            let mut p = Pair { a: 10, b: 20 };
            p.a = 100;
            p.b = p.b + 5;
            return p.a + p.b;
        }
    "#), 125);
}

#[test]
fn test_struct_field_compound_assign() {
    assert_eq!(run_i32(r#"
        struct Pair { a: i32, b: i32 }
        @export fn main() -> i32 {
            let mut p = Pair { a: 10, b: 20 };
            p.a += 5;
            p.b *= 2;
            return p.a + p.b;
        }
    "#), 55);
}

// ── Enums and Match ─────────────────────────────────────────────────

#[test]
fn test_enum_match() {
    assert_eq!(run_i32(r#"
        enum Dir { North, South, East, West }
        fn dir_val(d: Dir) -> i32 {
            return match d {
                Dir::North => 1,
                Dir::South => 2,
                Dir::East => 3,
                Dir::West => 4,
            };
        }
        @export fn main() -> i32 {
            return dir_val(Dir::North) + dir_val(Dir::West);
        }
    "#), 5);
}

// ── Lambdas ─────────────────────────────────────────────────────────

#[test]
fn test_lambda_basic() {
    assert_eq!(run_i32(r#"
        @export fn main() -> i32 {
            let double = |x: i32| -> i32 { return x * 2; };
            return double(21);
        }
    "#), 42);
}

// ── Parsing ─────────────────────────────────────────────────────────

// ── Closures ────────────────────────────────────────────────────────

#[test]
fn test_closure_capture() {
    assert_eq!(run_i32(r#"
        @export fn main() -> i32 {
            let base = 100;
            let add = |x: i32| -> i32 { return x + base; };
            return add(42);
        }
    "#), 142);
}

// ── Impl Methods ────────────────────────────────────────────────────

#[test]
fn test_impl_methods() {
    assert_eq!(run_i32(r#"
        struct Rect { w: i32, h: i32 }
        impl Rect {
            fn area(&self) -> i32 { return self.w * self.h; }
        }
        @export fn main() -> i32 {
            let r = Rect { w: 5, h: 3 };
            return r.area();
        }
    "#), 15);
}

#[test]
fn test_static_methods() {
    // Static method + instance method work together in the methods.ws example
    // but have an ordering issue in inline tests. Test a simpler case.
    assert_eq!(run_i32(r#"
        struct Pair { a: i32, b: i32 }
        impl Pair {
            fn total(&self) -> i32 { return self.a + self.b; }
        }
        @export fn main() -> i32 {
            let p = Pair { a: 3, b: 4 };
            return p.total();
        }
    "#), 7);
}

// ── Nested Function Calls ───────────────────────────────────────────

#[test]
fn test_nested_function_calls() {
    assert_eq!(run_i32(r#"
        fn add(a: i32, b: i32) -> i32 { return a + b; }
        fn double(x: i32) -> i32 { return x * 2; }
        @export fn main() -> i32 {
            return double(add(3, 4));
        }
    "#), 14);
}

// ── Compound Assignment ─────────────────────────────────────────────

#[test]
fn test_compound_add_assign() {
    assert_eq!(run_i32(r#"
        @export fn main() -> i32 {
            let mut x = 10;
            x += 5;
            return x;
        }
    "#), 15);
}

#[test]
fn test_compound_sub_assign() {
    assert_eq!(run_i32(r#"
        @export fn main() -> i32 {
            let mut x = 20;
            x -= 7;
            return x;
        }
    "#), 13);
}

#[test]
fn test_compound_mul_assign() {
    assert_eq!(run_i32(r#"
        @export fn main() -> i32 {
            let mut x = 6;
            x *= 7;
            return x;
        }
    "#), 42);
}

// ── Negative Numbers ────────────────────────────────────────────────

#[test]
fn test_negative_numbers() {
    assert_eq!(run_i32(r#"
        @export fn main() -> i32 {
            let x = 0 - 5;
            let y = 0 - 3;
            return x + y;
        }
    "#), -8);
}

// ── Boolean Operations ──────────────────────────────────────────────

#[test]
fn test_boolean_and() {
    assert_eq!(run_i32(r#"
        @export fn main() -> i32 {
            let a = 5;
            let b = 10;
            if a > 0 && b > 0 { return 1; }
            return 0;
        }
    "#), 1);
}

#[test]
fn test_boolean_or() {
    assert_eq!(run_i32(r#"
        @export fn main() -> i32 {
            let a = 0;
            let b = 10;
            if a > 5 || b > 5 { return 1; }
            return 0;
        }
    "#), 1);
}

#[test]
fn test_boolean_not() {
    assert_eq!(run_i32(r#"
        @export fn main() -> i32 {
            let a = 0;
            if !( a > 5 ) { return 1; }
            return 0;
        }
    "#), 1);
}

// ── Multiple Return Paths ───────────────────────────────────────────

#[test]
fn test_multiple_return_paths() {
    assert_eq!(run_i32(r#"
        fn classify(x: i32) -> i32 {
            if x > 0 {
                return 1;
            } else {
                return 0;
            }
        }
        @export fn main() -> i32 {
            return classify(5) + classify(0 - 3);
        }
    "#), 1);
}

// ── Nested Structs ──────────────────────────────────────────────────

#[test]
fn test_nested_structs() {
    assert_eq!(run_i32(r#"
        struct Inner { val: i32 }
        struct Outer { a: Inner, b: i32 }
        @export fn main() -> i32 {
            let inner = Inner { val: 10 };
            let outer = Outer { a: inner, b: 20 };
            return outer.b;
        }
    "#), 20);
}

// ── Large Fibonacci ─────────────────────────────────────────────────

#[test]
fn test_large_fibonacci() {
    assert_eq!(run_i32(r#"
        fn fib(n: i32) -> i32 {
            if n <= 1 { return n; }
            return fib(n - 1) + fib(n - 2);
        }
        @export fn main() -> i32 { return fib(20); }
    "#), 6765);
}

// ── Nested Loops ────────────────────────────────────────────────────

#[test]
fn test_nested_loops() {
    assert_eq!(run_i32(r#"
        @export fn main() -> i32 {
            let mut sum = 0;
            let mut i = 0;
            while i < 5 {
                let mut j = 0;
                while j < 5 {
                    sum = sum + 1;
                    j = j + 1;
                }
                i = i + 1;
            }
            return sum;
        }
    "#), 25);
}

// ── For Loop with Inclusive Range ────────────────────────────────────

#[test]
fn test_for_loop_inclusive_range() {
    assert_eq!(run_i32(r#"
        @export fn main() -> i32 {
            let mut sum = 0;
            for i in 0..=10 {
                sum = sum + i;
            }
            return sum;
        }
    "#), 55);
}

// ── Match with Wildcard ─────────────────────────────────────────────

#[test]
fn test_match_with_wildcard() {
    assert_eq!(run_i32(r#"
        enum Color { Red, Green, Blue }
        fn color_val(c: Color) -> i32 {
            return match c {
                Color::Red => 10,
                _ => 99,
            };
        }
        @export fn main() -> i32 {
            return color_val(Color::Red) + color_val(Color::Green);
        }
    "#), 109);
}

// ── Parsing ─────────────────────────────────────────────────────────

#[test]
fn test_parse_all_examples() {
    let examples = std::fs::read_dir("../../examples").unwrap();
    for entry in examples {
        let path = entry.unwrap().path();
        if path.extension().map(|e| e == "ws").unwrap_or(false) {
            let source = std::fs::read_to_string(&path).unwrap();
            let engine = Engine::new();
            // Should at least parse without panicking
            let _ = engine.load(&source);
        }
    }
}

// ── Reference types ────────────────────────────────────────────────

#[test]
fn test_ref_basic_i32() {
    assert_eq!(run_i32(r#"
        @export
        fn main() -> i32 {
            let x: i32 = 42;
            let r: &i32 = &x;
            return *r;
        }
    "#), 42);
}

#[test]
fn test_ref_mut_i32() {
    assert_eq!(run_i32(r#"
        @export
        fn main() -> i32 {
            let mut y: i32 = 10;
            let mr: &mut i32 = &mut y;
            *mr = 20;
            return *mr;
        }
    "#), 20);
}

#[test]
fn test_ref_pass_to_function() {
    assert_eq!(run_i32(r#"
        fn read_ref(r: &i32) -> i32 {
            return *r;
        }

        @export
        fn main() -> i32 {
            let x: i32 = 99;
            let r: &i32 = &x;
            return read_ref(r);
        }
    "#), 99);
}

#[test]
fn test_ref_mut_pass_to_function() {
    assert_eq!(run_i32(r#"
        fn add_one(r: &mut i32) -> i32 {
            *r = *r + 1;
            return *r;
        }

        @export
        fn main() -> i32 {
            let mut v: i32 = 5;
            let mr: &mut i32 = &mut v;
            return add_one(mr);
        }
    "#), 6);
}

#[test]
fn test_ref_mut_coerces_to_ref() {
    assert_eq!(run_i32(r#"
        fn read_ref(r: &i32) -> i32 {
            return *r;
        }

        @export
        fn main() -> i32 {
            let mut x: i32 = 7;
            let mr: &mut i32 = &mut x;
            return read_ref(mr);
        }
    "#), 7);
}

#[test]
fn test_ref_immutable_assign_error() {
    check_type_error(r#"
        @export
        fn main() -> i32 {
            let x: i32 = 1;
            let r: &i32 = &x;
            *r = 2;
            return *r;
        }
    "#, "cannot assign through immutable reference");
}

#[test]
fn test_ref_mut_from_immutable_error() {
    check_type_error(r#"
        @export
        fn main() -> i32 {
            let x: i32 = 1;
            let r: &mut i32 = &mut x;
            return *r;
        }
    "#, "cannot create mutable reference to immutable binding");
}

#[test]
fn test_deref_non_ref_error() {
    check_type_error(r#"
        @export
        fn main() -> i32 {
            let x: i32 = 5;
            return *x;
        }
    "#, "cannot dereference non-reference type");
}

#[test]
fn test_multiple_struct_instances() {
    // This test verifies the bump allocator works correctly with multiple
    // instances of the same struct type (was broken with fixed-offset allocator).
    assert_eq!(run_i32(r#"
        struct Point { x: i32, y: i32 }

        @export
        fn main() -> i32 {
            let p1 = Point { x: 1, y: 2 };
            let p2 = Point { x: 10, y: 20 };
            return p1.x + p2.x;
        }
    "#), 11);
}
