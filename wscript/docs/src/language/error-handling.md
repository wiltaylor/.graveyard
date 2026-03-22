# Error Handling

SpiteScript uses `Option<T>` and `Result<T, E>` for error handling. There are no
exceptions. Errors are values that must be explicitly handled, propagated with `?`,
or deliberately discarded.

## Option\<T\>

`Option<T>` represents a value that may or may not exist.

```
let x: Option<i32> = Some(42);
let y: Option<i32> = None;
```

### Option Methods

| Method | Return | Description |
|--------|--------|-------------|
| `.is_some()` | `bool` | True if `Some` |
| `.is_none()` | `bool` | True if `None` |
| `.unwrap()` | `T` | Extract value; panics if `None` |
| `.unwrap_or(default)` | `T` | Extract value or use default |
| `.unwrap_or_else(\|\| expr)` | `T` | Extract value or compute default lazily |
| `.expect("msg")` | `T` | Extract value; panics with message if `None` |
| `.map(\|v\| expr)` | `Option<U>` | Transform the inner value |
| `.and_then(\|v\| opt)` | `Option<U>` | Flat-map (returns `Option`) |
| `.or(other)` | `Option<T>` | Return `self` if `Some`, otherwise `other` |
| `.or_else(\|\| opt)` | `Option<T>` | Lazy alternative |
| `.filter(\|v\| cond)` | `Option<T>` | Keep `Some` only if condition holds |
| `.ok_or(err)` | `Result<T, E>` | Convert to `Result` |
| `.ok_or_else(\|\| err)` | `Result<T, E>` | Convert to `Result` lazily |

```
let name: Option<String> = Some("Alice");

name.unwrap()                            // "Alice"
name.unwrap_or("stranger")               // "Alice"
name.map(|n| n.to_uppercase())           // Some("ALICE")

let empty: Option<String> = None;
empty.unwrap_or("stranger")              // "stranger"
empty.map(|n| n.to_uppercase())          // None
```

## Result\<T, E\>

`Result<T, E>` represents an operation that either succeeds with `Ok(T)` or fails
with `Err(E)`.

```
let r: Result<i32, String> = Ok(42);
let e: Result<i32, String> = Err("something went wrong");
```

**`Result<T>`** is a shorthand alias for `Result<T, Error>`, where `Error` is the
built-in type-erased error type. Use this in application-level code that does not
need callers to match on specific error variants.

### Result Methods

| Method | Return | Description |
|--------|--------|-------------|
| `.is_ok()` | `bool` | True if `Ok` |
| `.is_err()` | `bool` | True if `Err` |
| `.unwrap()` | `T` | Extract value; panics if `Err` |
| `.unwrap_or(default)` | `T` | Extract value or use default |
| `.unwrap_or_else(\|e\| expr)` | `T` | Extract value or compute from error |
| `.expect("msg")` | `T` | Extract value; panics with message if `Err` |
| `.map(\|v\| expr)` | `Result<U, E>` | Transform the success value |
| `.map_err(\|e\| expr)` | `Result<T, F>` | Transform the error value |
| `.and_then(\|v\| result)` | `Result<U, E>` | Chain operations that may fail |
| `.or(other)` | `Result<T, E>` | Use alternative if `Err` |
| `.ok()` | `Option<T>` | Discard error, keep success as `Option` |
| `.err()` | `Option<E>` | Discard success, keep error as `Option` |

```
fn parse_port(s: String) -> Result<u16> {
    return s.parse::<u16>();
}

let port = parse_port("8080").unwrap_or(80);    // 8080
let port = parse_port("nope").unwrap_or(80);    // 80
```

## The `?` Operator

The `?` operator provides concise error propagation. Applied to a `Result`, it
unwraps `Ok` values and returns `Err` values early from the enclosing function.
Applied to an `Option`, it unwraps `Some` and returns `None` early.

```
fn load_config(path: String) -> Result<Config> {
    let raw = read_file(path)?;           // returns Err early if this fails
    let parsed = parse_json(raw)?;        // same here
    return Ok(parsed);
}
```

Without `?`, the equivalent code would be:

```
fn load_config(path: String) -> Result<Config> {
    let raw = match read_file(path) {
        Ok(v) => v,
        Err(e) => return Err(e),
    };
    let parsed = match parse_json(raw) {
        Ok(v) => v,
        Err(e) => return Err(e),
    };
    return Ok(parsed);
}
```

`?` also works with `Option`:

```
fn first_word(text: Option<String>) -> Option<String> {
    let t = text?;                        // returns None if text is None
    let word = t.split(" ").first()?;     // returns None if no words
    return Some(word);
}
```

## `@error` -- Typed Errors

The `@error` attribute on an enum or struct auto-implements the `Error` trait and
enables the `@msg`, `@from`, `@source`, and `@transparent` sub-attributes.

### Error Enums

```
@error
enum ParseError {
    @msg("unexpected token '{token}' at line {line}")
    UnexpectedToken { token: String, line: u32 },

    @msg("unexpected end of input")
    UnexpectedEof,

    @msg("invalid number: '{0}'")
    InvalidNumber(String),
}
```

Each variant can carry data. The `@msg` attribute defines the human-readable format
string. Use `{field_name}` for struct variant fields and `{0}`, `{1}` for tuple
variant fields.

### Error Structs

For types with a single failure mode, apply `@error` to a struct:

```
@error
@msg("config error in '{file}': {reason}")
struct ConfigError {
    file:   String,
    reason: String,
}
```

### Using Typed Errors

```
fn parse_header(data: String) -> Result<Header, ParseError> {
    if data.is_empty() {
        return Err(ParseError::UnexpectedEof);
    }
    if !data.starts_with("HDR") {
        return Err(ParseError::UnexpectedToken {
            token: data[0..3],
            line: 1,
        });
    }
    return Ok(parse_header_inner(data));
}
```

## `@from` -- Automatic Error Conversion

`@from(T)` on a variant generates `impl From<T>` for the enclosing error enum.
This allows `?` to automatically convert errors between types.

```
@error
enum AppError {
    @msg("parse error: {0}")
    @from(ParseError)
    Parse(ParseError),

    @msg("config error: {0}")
    @from(ConfigError)
    Config(ConfigError),

    @msg("not found: '{path}'")
    NotFound { path: String },
}

fn startup() -> Result<(), AppError> {
    let config = load_config("app.toml")?;    // ConfigError -> AppError::Config
    let data = parse_input(config.input)?;     // ParseError -> AppError::Parse
    return Ok(());
}
```

## `@source` -- Error Cause Chain

Mark a field with `@source` to indicate it holds the root cause of the error. This
enables `.source()` and `.source_chain()` on the error.

```
@error
enum DbError {
    @msg("query failed: {reason}")
    QueryFailed {
        reason: String,
        @source cause: IoError,
    },
}
```

```
let err = DbError::QueryFailed {
    reason: "connection reset",
    cause: io_err,
};
err.source()          // Some(io_err)
err.source_chain()    // [io_err, ...]
```

## `@transparent` -- Delegate Display

`@transparent` makes a variant display as if it were the inner error directly. The
wrapping variant becomes invisible in error messages.

```
@error
enum AppError {
    @transparent
    @from(ParseError)
    Parse(ParseError),    // displays the ParseError message directly
}
```

## Dynamic `Error` Type

`Error` is the built-in type-erased error. Any `@error` type coerces to `Error`
automatically. Use it with `Result<T>` (the shorthand for `Result<T, Error>`).

Methods available on all error types:

| Method | Return | Description |
|--------|--------|-------------|
| `e.to_string()` | `String` | Formatted error message |
| `e.source()` | `Option<Error>` | Direct cause |
| `e.source_chain()` | `Error[]` | Full cause chain |
| `e.downcast::<T>()` | `Result<T, Error>` | Recover concrete type (consuming) |
| `e.downcast_ref::<T>()` | `Option<&T>` | Check concrete type (non-consuming) |
| `e.is::<T>()` | `bool` | Quick type check |

```
fn handle_error(e: Error) {
    if e.is::<ParseError>() {
        print("It was a parse error");
    }

    // Print the full cause chain
    for cause in e.source_chain() {
        print(`  caused by: ${cause}`);
    }
}
```

## Error Macros

SpiteScript provides convenience macros for common error patterns.

### `error!(msg)` -- Create an Error

```
let e = error!("something went wrong");
let e = error!("expected {expected}, got {actual}");
```

Produces an `Error` value with a formatted message. Variables in scope are
interpolated with `{name}` syntax.

### `bail!(msg)` -- Early Return with Error

```
fn process(x: i32) -> Result<i32> {
    if x < 0 {
        bail!("negative input: {x}");
    }
    return Ok(x * 2);
}
```

Equivalent to `return Err(error!(...))`. Also accepts typed error variants:

```
bail!(AppError::NotFound { path: path });
```

### `ensure!(condition, msg)` -- Assert or Bail

```
fn parse_header(bytes: u8[]) -> Result<Header> {
    ensure!(bytes.len() >= 8, "header too short: got {0} bytes", bytes.len());
    ensure!(bytes[0] == 0xFF, "invalid magic byte");
    return Ok(parse_header_inner(bytes));
}
```

Equivalent to `if !condition { bail!(msg); }`.

## Complete Example

```
@error
enum FileError {
    @msg("file not found: '{path}'")
    NotFound { path: String },

    @msg("permission denied: '{path}'")
    PermissionDenied { path: String },

    @msg("read error: {0}")
    @from(IoError)
    Io(IoError),
}

fn read_config(path: String) -> Result<Config, FileError> {
    if !file_exists(path) {
        return Err(FileError::NotFound { path });
    }

    let raw = read_file(path)?;     // IoError auto-converts via @from
    let config = parse_toml(raw).map_err(|e| FileError::PermissionDenied {
        path: path,
    })?;

    return Ok(config);
}

fn main() -> Result<()> {
    let config = match read_config("app.toml") {
        Ok(c)  => c,
        Err(e) => {
            print_err(`Config failed: ${e}`);
            return Err(e);
        },
    };

    print(`Loaded config: ${config.name}`);
    return Ok(());
}
```
