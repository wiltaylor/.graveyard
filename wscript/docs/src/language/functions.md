# Functions

Functions in Wscript are declared with the `fn` keyword. They are statically typed, support default parameters and named arguments, and can be passed as values.

## Declaration Syntax

A function declaration specifies a name, parameters with types, an optional return type, and a body:

```wscript
fn add(a: i32, b: i32) -> i32 {
    return a + b;
}

fn greet(name: String) {
    print(`Hello, ${name}!`);
}
```

Parameters must always have explicit type annotations. The return type appears after `->`. If no return type is specified, the function returns unit `()`.

## No Implicit Returns

Wscript requires an explicit `return` statement in all function bodies. There is no implicit return from the final expression. This is a deliberate design choice for clarity:

```wscript
// Correct -- explicit return
fn add(a: i32, b: i32) -> i32 {
    return a + b;
}

// WRONG -- compile error
fn add(a: i32, b: i32) -> i32 {
    a + b    // ERROR: missing return statement
}
```

The only contexts where an expression produces a value without `return` are:

1. **Single-line lambdas**: `|x| x * 2` (the body expression is the return value).
2. **`match` arm expressions**: each arm's expression is the value of that arm.
3. **`if`/`else` used as expressions**: each branch's expression is the value.

These exceptions exist because they are expression contexts, not statement contexts. Inside a function body, you always need `return`.

### Multiple Return Points

Functions can have multiple `return` statements for early exit:

```wscript
fn classify(n: i32) -> String {
    if n < 0 {
        return "negative";
    }
    if n == 0 {
        return "zero";
    }
    return "positive";
}
```

## Unit Return

Functions that perform side effects and return nothing omit the return type annotation. They implicitly return `()` (the unit type):

```wscript
fn log_message(msg: String) {
    print(`[LOG] ${msg}`);
    // implicit return ()
}

fn update_counter(counter: Ref<i32>) {
    counter.update(|n| n + 1);
    // no return needed
}
```

You may write an explicit `return;` (without a value) to exit early from a unit-returning function:

```wscript
fn maybe_log(msg: String, verbose: bool) {
    if !verbose {
        return;
    }
    print(`[LOG] ${msg}`);
}
```

## Default Parameter Values

Parameters can have default values. Callers may omit arguments for parameters with defaults:

```wscript
fn connect(host: String, port: u16 = 8080, timeout_ms: u64 = 5000) -> Result<()> {
    // ...
    return Ok(());
}

// All of these are valid calls:
connect("localhost");                            // port=8080, timeout_ms=5000
connect("localhost", 3000);                      // port=3000, timeout_ms=5000
connect("localhost", 3000, 1000);                // port=3000, timeout_ms=1000
```

Default values must be compile-time expressions. Parameters with defaults must come after parameters without defaults:

```wscript
// Correct
fn create_user(name: String, role: String = "viewer", active: bool = true) -> User {
    return User { name, role, active };
}

// WRONG -- non-default parameter after default
fn bad(x: i32 = 0, y: i32) -> i32 {   // ERROR
    return x + y;
}
```

## Named Arguments

When calling functions with default parameters, you can use named arguments to skip over defaults or call in a different order. Positional arguments must appear before named arguments:

```wscript
fn connect(host: String, port: u16 = 8080, timeout_ms: u64 = 5000) -> Result<()> {
    // ...
    return Ok(());
}

// Skip port, only specify timeout
connect("localhost", timeout_ms: 1000);

// Specify both by name
connect("localhost", port: 3000, timeout_ms: 1000);
```

Named arguments are particularly useful for functions with several defaulted parameters:

```wscript
fn create_window(
    title: String,
    width: u32 = 800,
    height: u32 = 600,
    resizable: bool = true,
    fullscreen: bool = false,
) {
    // ...
}

create_window("My App", fullscreen: true);
create_window("Editor", width: 1200, height: 900);
```

## Functions as Values

Named functions can be used as values. They coerce to the `Fn(...)` type:

```wscript
fn double(x: i32) -> i32 {
    return x * 2;
}

fn triple(x: i32) -> i32 {
    return x * 3;
}

// Pass a function as an argument
fn apply(f: Fn(i32) -> i32, x: i32) -> i32 {
    return f(x);
}

apply(double, 5);     // 10
apply(triple, 5);     // 15
```

Functions can be stored in variables:

```wscript
let operation: Fn(i32) -> i32 = double;
let result = operation(21);    // 42
```

Functions can be stored in arrays:

```wscript
let transforms: Fn(i32) -> i32[] = [double, triple, |x| x + 1];

for f in transforms {
    print(f(10));
}
// prints: 20, 30, 11
```

Functions can be returned from other functions:

```wscript
fn make_multiplier(factor: i32) -> Fn(i32) -> i32 {
    return |x| x * factor;
}

let times_five = make_multiplier(5);
times_five(3)    // 15
```

### The `Fn` Type

The `Fn` type describes the signature of a callable value. The syntax is:

```wscript
Fn(ParamType1, ParamType2) -> ReturnType
```

For functions that take no arguments:

```wscript
Fn() -> i32
```

For functions that return unit:

```wscript
Fn(String)          // implicitly returns ()
Fn(String) -> ()    // equivalent, explicit
```

Both named functions and lambdas/closures satisfy `Fn` types. The `Fn` type does not distinguish between them.

## Higher-Order Function Patterns

Functions that take or return other functions are common in Wscript. Here are typical patterns:

```wscript
// Predicate function
fn filter_items(items: i32[], predicate: Fn(i32) -> bool) -> i32[] {
    return items.filter(predicate).collect();
}

let positives = filter_items([-1, 2, -3, 4], |x| x > 0);
// [2, 4]
```

```wscript
// Transformation chain
fn compose(f: Fn(i32) -> i32, g: Fn(i32) -> i32) -> Fn(i32) -> i32 {
    return |x| f(g(x));
}

let add_one_then_double = compose(|x| x * 2, |x| x + 1);
add_one_then_double(3)   // 8  (3+1=4, 4*2=8)
```

## The `@export` Attribute

Functions marked with `@export` are callable from the Rust host application after the script is compiled:

```wscript
@export
fn process(items: i32[]) -> i32 {
    return items.filter(|x| x > 0).sum();
}

@export
fn greet(name: String) -> String {
    return `Hello, ${name}!`;
}
```

Without `@export`, functions may be inlined or optimized away by the compiler. Only `@export` functions are guaranteed to be addressable by name from the host.

If your script defines a single entry point, mark it with `@export`:

```wscript
@export
fn main() {
    let data = load_data();
    let result = analyze(data);
    save_report(result);
}

// Helper functions do not need @export
fn analyze(data: i32[]) -> String {
    let avg = data.sum() as f64 / data.len() as f64;
    return `Average: ${avg}`;
}
```

## Complete Example

Here is a function that demonstrates several features together -- default parameters, named arguments, and functions as values:

```wscript
fn process_records(
    records: String[],
    transform: Fn(String) -> String = |s| s,
    filter_fn: Fn(String) -> bool = |_| true,
    max_results: u64 = 100,
) -> String[] {
    return records
        .filter(filter_fn)
        .map(transform)
        .take(max_results)
        .collect();
}

// Use defaults
let all = process_records(data);

// Use named arguments to skip to max_results
let first_ten = process_records(data, max_results: 10);

// Provide custom transform and filter
let result = process_records(
    data,
    transform: |s| s.to_uppercase(),
    filter_fn: |s| s.len() > 3,
    max_results: 50,
);
```
