# Built-in Macros

Macros are invoked with `name!(...)` syntax. The `!` distinguishes them from regular
function calls. SpiteScript provides a fixed set of built-in macros -- user-defined
macros are not supported.

## `error!(msg)` -- Create an Error

Creates an `Error` value from a format string. In-scope variables are interpolated
using `{name}` syntax.

```
let e = error!("something went wrong");

let expected = "number";
let got = "string";
let e = error!("expected {expected}, got {got}");
// Error message: "expected number, got string"
```

This produces a value of type `Error` (the dynamic, type-erased error type). Use it
with `Err(...)` to return from a function:

```
fn validate(x: i32) -> Result<i32> {
    if x < 0 {
        return Err(error!("value must be non-negative, got {x}"));
    }
    return Ok(x);
}
```

## `bail!(msg)` -- Early Return with Error

`bail!` is sugar for `return Err(error!(...))`. It immediately returns an error from
the enclosing function.

```
fn divide(a: f64, b: f64) -> Result<f64> {
    if b == 0.0 {
        bail!("cannot divide by zero");
    }
    return Ok(a / b);
}
```

`bail!` also accepts typed error variants instead of format strings:

```
fn lookup(id: u64) -> Result<User, AppError> {
    if id == 0 {
        bail!(AppError::NotFound { path: `user/${id}` });
    }
    return Ok(find_user(id));
}
```

Format string interpolation works the same as `error!`:

```
fn parse(input: String) -> Result<Ast> {
    if input.is_empty() {
        bail!("cannot parse empty input");
    }
    let token = next_token(input);
    if token.is_none() {
        bail!("unexpected end of input after: {input}");
    }
    return Ok(parse_inner(input));
}
```

## `ensure!(condition, msg)` -- Assert or Bail

`ensure!` checks a condition and bails with an error if it is false. It is sugar for
`if !condition { bail!(msg); }`.

```
fn parse_header(bytes: u8[]) -> Result<Header> {
    ensure!(bytes.len() >= 8, "header too short: got {0} bytes", bytes.len());
    ensure!(bytes[0] == 0xFF, "invalid magic byte: {0}", bytes[0]);
    ensure!(bytes[1] >= 1, "unsupported version");
    return Ok(parse_header_inner(bytes));
}
```

The first argument is the condition. The remaining arguments form the error message.
Use `{0}`, `{1}`, etc. to refer to additional arguments by position.

## `dbg!(expr)` -- Debug Print and Pass Through

`dbg!` prints the expression text, its value, and the source location to stderr.
It returns the value unchanged, so it can be inserted into any expression without
altering program behavior.

```
let x = dbg!(2 + 3);
// prints: [script.al:1] 2 + 3 = 5
// x = 5
```

Chain `dbg!` in the middle of expressions:

```
let result = dbg!(items.len()) * 2;
// prints: [script.al:1] items.len() = 10
// result = 20
```

Use it in pipelines to inspect intermediate values:

```
let total = [1, 2, 3, 4, 5]
    .filter(|x| dbg!(x) > 2)
    .sum();
// prints each element as it passes through filter
```

In release mode, `dbg!` is a no-op -- it returns its argument without printing.

## `assert!(condition)` -- Runtime Assertion

Panics with source location information if the condition is false. Optionally takes
a custom message.

```
assert!(x > 0);
assert!(x > 0, "expected positive value, got {x}");
```

Without a message, the panic output includes the condition text:

```
panic: assertion failed: x > 0
  at script.al:5:4
```

With a message:

```
panic: assertion failed: expected positive value, got -3
  at script.al:5:4
```

Use assertions for conditions that should never be false in correct code. They are
active in both debug and release mode.

## `assert_eq!(left, right)` -- Equality Assertion

Panics if the two values are not equal. On failure, displays both values for easy
comparison. Optionally takes a custom message.

```
assert_eq!(result, 42);
assert_eq!(name, "Alice");
assert_eq!(got, expected, "output mismatch");
```

Failure output:

```
panic: assertion failed: result == 42
  left:  37
  right: 42
  at script.al:18:4
```

## `assert_ne!(left, right)` -- Inequality Assertion

Panics if the two values are equal. The inverse of `assert_eq!`.

```
assert_ne!(a, b);
assert_ne!(result, 0, "result should not be zero");
```

Failure output:

```
panic: assertion failed: a != b
  left:  5
  right: 5
  at script.al:22:4
```

## `todo!()` -- Placeholder for Unfinished Code

Panics with "not yet implemented" and the source location. Use it as a placeholder
while developing, so the code compiles but any execution path reaching the `todo!`
is clearly marked.

```
fn complex_algorithm(data: i32[]) -> i32 {
    return todo!();
}
```

The panic message:

```
panic: not yet implemented
  at script.al:2:12
```

`todo!()` satisfies any return type, so it can stand in for any expression.

## `unreachable!()` -- Impossible Code Path

Panics if execution reaches it. Use it to document code paths that should be
logically impossible. Optionally takes a message.

```
fn direction_name(d: Direction) -> String {
    return match d {
        Direction::North => "north",
        Direction::South => "south",
        Direction::East  => "east",
        Direction::West  => "west",
    };
    // If the match is exhaustive, this is never reached.
    // But if you need it after non-exhaustive logic:
    unreachable!("all directions handled above");
}
```

The panic message:

```
panic: entered unreachable code: all directions handled above
  at script.al:10:4
```

Without a message:

```
unreachable!();
// panic: entered unreachable code
//   at script.al:10:4
```

## Summary

| Macro | Purpose | Returns |
|-------|---------|---------|
| `error!(msg)` | Create an `Error` value | `Error` |
| `bail!(msg)` | Return early with error | (never -- returns from function) |
| `ensure!(cond, msg)` | Assert condition or bail | `()` if condition is true |
| `dbg!(expr)` | Debug print and pass through | Same type as `expr` |
| `assert!(cond)` | Panic if false | `()` |
| `assert_eq!(a, b)` | Panic if not equal | `()` |
| `assert_ne!(a, b)` | Panic if equal | `()` |
| `todo!()` | Mark unfinished code | Any type (panics) |
| `unreachable!()` | Mark impossible path | Any type (panics) |
