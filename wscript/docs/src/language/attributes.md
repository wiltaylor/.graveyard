# Attributes

Attributes annotate declarations with metadata that affects compilation, code
generation, or tooling behavior. They use the `@` prefix and are placed on the line
before (or stacked above) the declaration they modify.

## Syntax

```
@attribute_name
@attribute_name(arg1, arg2)
@attribute_name(key: value)
```

Attributes appear before the declaration they apply to. Multiple attributes can be
stacked on a single declaration.

## `@export`

Marks a function as callable from the host application. Without `@export`, functions
may be inlined or optimized away. Only `@export` functions are guaranteed to be
addressable by name from the host.

```
@export
fn process(items: i32[]) -> i32 {
    return items.filter(|x| x > 0).sum();
}

@export
fn greet(name: String) -> String {
    return `Hello, ${name}!`;
}
```

The host calls these functions by name after compilation:

```rust
// In Rust host code:
let result: i32 = script.call("process", &[Value::Array(vec![1, -2, 3])])?;
```

## `@error`

Applied to an enum or struct to auto-implement the `Error` trait. Enables the use of
`@msg`, `@from`, `@source`, and `@transparent` on the type and its variants.

### On an Enum

```
@error
enum AppError {
    @msg("not found: '{path}'")
    NotFound { path: String },

    @msg("access denied")
    AccessDenied,

    @msg("invalid input: '{0}'")
    InvalidInput(String),
}
```

### On a Struct

For types representing a single failure mode:

```
@error
@msg("timeout after {duration_ms}ms")
struct TimeoutError {
    duration_ms: u64,
}
```

## `@msg` -- Error Format String

Defines the human-readable error message for an `@error` variant or struct. Field
values are interpolated using `{field_name}` for struct fields and `{0}`, `{1}` for
tuple fields.

```
@error
enum ParseError {
    @msg("unexpected token '{token}' at line {line}")
    UnexpectedToken { token: String, line: u32 },

    @msg("invalid number: '{0}'")
    InvalidNumber(String),

    @msg("unexpected end of input")
    UnexpectedEof,
}
```

The interpolated message is returned by `.to_string()` on the error:

```
let e = ParseError::UnexpectedToken { token: "+", line: 42 };
e.to_string()    // "unexpected token '+' at line 42"
```

## `@from` -- Automatic Error Conversion

Applied to an enum variant to generate `impl From<T>` for the enclosing error type.
This enables the `?` operator to automatically convert errors between types.

```
@error
enum AppError {
    @msg("parse error: {0}")
    @from(ParseError)
    Parse(ParseError),

    @msg("io error: {0}")
    @from(IoError)
    Io(IoError),
}
```

With `@from`, the `?` operator converts automatically:

```
fn load(path: String) -> Result<Data, AppError> {
    let raw = read_file(path)?;       // IoError -> AppError::Io
    let parsed = parse(raw)?;          // ParseError -> AppError::Parse
    return Ok(parsed);
}
```

## `@source` -- Error Cause Chain

Marks a field as the source (root cause) of an error. Enables `.source()` and
`.source_chain()` on the error value.

```
@error
enum DbError {
    @msg("query failed: {reason}")
    QueryFailed {
        reason: String,
        @source cause: IoError,
    },

    @msg("connection lost")
    ConnectionLost {
        @source inner: IoError,
    },
}
```

```
let err = DbError::QueryFailed {
    reason: "connection reset",
    cause: io_error,
};

err.source()          // Some(io_error)
err.source_chain()    // [io_error, ...]
```

## `@transparent`

Delegates the `Display` implementation of a variant to its inner error. The wrapping
variant becomes invisible in error messages -- the inner error's message is shown
directly.

```
@error
enum AppError {
    @transparent
    @from(ParseError)
    Parse(ParseError),

    @msg("application error: {0}")
    Other(String),
}

let e = AppError::Parse(ParseError::UnexpectedEof);
e.to_string()    // "unexpected end of input" (shows ParseError's message)
```

## `@derive` -- Auto-implement Traits

Generates trait implementations based on the type's structure. Takes a list of trait
names.

```
@derive(Clone, Eq, Hash, Debug)
struct Point {
    x: i32,
    y: i32,
}
```

### Derivable Traits

| Trait | Generated behavior |
|-------|-------------------|
| `Clone` | Deep-copies each field |
| `Eq` | Field-by-field equality |
| `Hash` | Hashes fields in declaration order |
| `Comparable` | Lexicographic comparison on fields in declaration order |
| `Debug` | Formats as `TypeName { field: value, ... }` |
| `Display` | Same format as `Debug` |

Works on both structs and enums:

```
@derive(Clone, Eq, Hash, Comparable, Debug)
enum Priority {
    Low,
    Medium,
    High,
}

// Derived Comparable: Low < Medium < High
assert!(Priority::Low.compare(&Priority::High) < 0);
```

All fields must themselves implement the derived trait, or the compiler reports an
error.

## `@trace` -- Function Tracing

In debug mode, logs function entry (with arguments), exit (with return value), and
execution duration. Routed through the host's configured log handler.

```
@trace
fn process(items: i32[]) -> i32 {
    return items.filter(|x| x > 0).sum();
}
```

Output when `process([1, -2, 3])` is called:

```
-> process(items: [1, -2, 3])     script.al:2
<- process = 4                    script.al:2  (0.05ms)
```

To omit argument values from the trace (useful for large or sensitive data):

```
@trace(args: false)
fn process_sensitive(data: String) -> Result<()> {
    // ...
    return Ok(());
}
```

Output:

```
-> process_sensitive(...)         script.al:8
<- process_sensitive = Ok(())    script.al:8  (1.23ms)
```

In release mode, `@trace` is a no-op and adds no overhead.

## `@deprecated` -- Deprecation Warning

Marks a declaration as deprecated. The compiler emits warning W004 at every call
site.

```
@deprecated("use process_v2 instead")
fn process(items: i32[]) -> i32 {
    return items.sum();
}

// Calling process() anywhere will produce:
// warning[W004]: use of deprecated function 'process': use process_v2 instead
```

Works on functions, structs, enum variants, and trait methods.

## `@allow` -- Suppress Warnings

Suppresses a specific compiler warning on the annotated declaration.

```
@allow(unused_variable)
fn example() {
    let x = 42;    // no W001 warning
}

@allow(deprecated)
fn legacy_caller() {
    process([1, 2, 3]);    // no W004 warning
}
```

Common warning names:

| Warning | Code | Description |
|---------|------|-------------|
| `unused_variable` | W001 | Variable declared but never used |
| `unused_function` | W002 | Function declared but never called |
| `deprecated` | W004 | Use of a deprecated symbol |
| `unreachable_code` | W005 | Code after a return, break, or unconditional bail |

## Attribute Stacking

Multiple attributes can be applied to a single declaration. They are processed in
order, top to bottom.

```
@error
@derive(Clone)
@msg("io error: {0}")
struct IoError(String);
```

```
@export
@trace
@deprecated("use process_v2 instead")
fn process(items: i32[]) -> i32 {
    return items.sum();
}
```

Stacking is commonly used with `@error` types, where `@error`, `@derive`, `@msg`,
`@from`, and `@transparent` often appear together:

```
@error
@derive(Clone, Debug)
enum ServiceError {
    @msg("timeout after {ms}ms")
    Timeout { ms: u64 },

    @transparent
    @from(IoError)
    Io(IoError),

    @msg("rate limited: retry after {seconds}s")
    RateLimited { seconds: u64 },
}
```
