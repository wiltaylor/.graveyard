# Primitive Types

Primitive types in Wscript are always copied on assignment. They live on the stack (in WASM locals) and are never heap-allocated or reference-counted.

## Integer Types

Wscript provides ten integer types, five signed and five unsigned:

| Type | Width | Range |
|------|-------|-------|
| `i8` | 8-bit signed | -128 to 127 |
| `i16` | 16-bit signed | -32,768 to 32,767 |
| `i32` | 32-bit signed | -2,147,483,648 to 2,147,483,647 |
| `i64` | 64-bit signed | -9,223,372,036,854,775,808 to 9,223,372,036,854,775,807 |
| `i128` | 128-bit signed | full 128-bit range |
| `u8` | 8-bit unsigned | 0 to 255 |
| `u16` | 16-bit unsigned | 0 to 65,535 |
| `u32` | 32-bit unsigned | 0 to 4,294,967,295 |
| `u64` | 64-bit unsigned | 0 to 18,446,744,073,709,551,615 |
| `u128` | 128-bit unsigned | full 128-bit range |

Integer literals without a type suffix default to `i32`:

```wscript
let x = 42;        // i32
let y: i64 = 42;   // i64 because of the annotation
```

### Integer Arithmetic

Standard arithmetic operators work on all integer types:

```wscript
let a = 10;
let b = 3;

let sum = a + b;        // 13
let diff = a - b;       // 7
let prod = a * b;       // 30
let quot = a / b;       // 3 (integer division, truncates toward zero)
let rem = a % b;        // 1
```

### Overflow Behavior

Arithmetic overflow on integer types **panics in debug mode** and **wraps in release mode**. If you need intentional wrap-around arithmetic, use the explicit wrapping methods:

```wscript
let x: u8 = 255;
// x + 1 would panic in debug mode, wrap to 0 in release mode

let wrapped = x.wrapping_add(1);  // always 0, no panic
```

## Float Types

| Type | Width | Precision |
|------|-------|-----------|
| `f32` | 32-bit | approximately 7 decimal digits |
| `f64` | 64-bit | approximately 15 decimal digits |

Float literals without a type suffix default to `f64`:

```wscript
let x = 3.14;        // f64
let y: f32 = 3.14;   // f32 because of the annotation
```

Floating-point operations follow IEEE 754. `NaN`, `+Inf`, and `-Inf` are valid values. Division by zero produces infinity, not a panic:

```wscript
let inf = 1.0 / 0.0;     // +Inf
let neg_inf = -1.0 / 0.0; // -Inf
let nan = 0.0 / 0.0;      // NaN
```

### Float Arithmetic

```wscript
let a = 10.0;
let b = 3.0;

let sum = a + b;     // 13.0
let diff = a - b;    // 7.0
let prod = a * b;    // 30.0
let quot = a / b;    // 3.3333...
let rem = a % b;     // 1.0
```

## Bool

The `bool` type has exactly two values: `true` and `false`.

### Logical Operators

Wscript supports both symbolic and keyword-style logical operators:

```wscript
let a = true;
let b = false;

// Symbolic operators
let and_result = a && b;    // false
let or_result = a || b;     // true
let not_result = !a;        // false

// Keyword operators (equivalent)
let and_result = a and b;   // false
let or_result = a or b;     // true
let not_result = not a;     // false
```

Logical `&&` and `||` (and their keyword equivalents `and` and `or`) use **short-circuit evaluation**. The right-hand side is only evaluated if the left-hand side does not already determine the result:

```wscript
// safe_check() is only called if items is not empty
let ok = !items.is_empty() && safe_check(items[0]);

// default_value() is only called if primary() returns false
let result = primary() || default_value();
```

## Char

The `char` type represents a single Unicode scalar value (U+0000 to U+D7FF, U+E000 to U+10FFFF). It is stored as a 32-bit value internally.

```wscript
let letter = 'a';
let emoji = '🦀';
let newline = '\n';
let unicode = '\u{1F980}';
```

### Escape Sequences

Character literals support the following escape sequences:

| Escape | Meaning |
|--------|---------|
| `\n` | Newline |
| `\r` | Carriage return |
| `\t` | Tab |
| `\\` | Backslash |
| `\'` | Single quote |
| `\0` | Null character |
| `\u{HHHHHH}` | Unicode scalar by hex code point |

### Char Methods

```wscript
let ch = 'A';
ch.is_alphabetic();   // true
ch.is_numeric();      // false
ch.is_whitespace();   // false
ch.to_uppercase();    // 'A'
ch.to_lowercase();    // 'a'
ch.to_string();       // "A"
```

## Numeric Literals

Wscript supports several numeric literal formats.

### Decimal

```wscript
let x = 42;
let big = 1_000_000;    // underscores are ignored, used for readability
let neg = -17;
```

### Hexadecimal

Prefixed with `0x` or `0X`:

```wscript
let hex = 0xFF;          // 255
let mask = 0x00FF_00FF;  // underscores allowed
```

### Binary

Prefixed with `0b` or `0B`:

```wscript
let bits = 0b1010;       // 10
let byte = 0b1111_0000;  // 240
```

### Octal

Prefixed with `0o` or `0O`:

```wscript
let octal = 0o77;        // 63
let perms = 0o755;       // 493
```

### Float Literals

Float literals require a decimal point or use scientific notation:

```wscript
let pi = 3.14;
let avogadro = 6.022e23;
let tiny = 1.0e-10;
```

### Type Suffixes

You can append a type suffix directly to any numeric literal to specify its type without a separate annotation:

```wscript
let a = 42i32;       // i32
let b = 42u8;        // u8
let c = 42i64;       // i64
let d = 255u8;       // u8

let e = 3.14f64;     // f64
let f = 3.14f32;     // f32
let g = 1.0f64;      // f64
```

Valid integer suffixes: `i8`, `i16`, `i32`, `i64`, `i128`, `u8`, `u16`, `u32`, `u64`, `u128`.

Valid float suffixes: `f32`, `f64`.

## Type Defaults

When no suffix or annotation is present, numeric literals use these defaults:

- Integer literals default to `i32`
- Float literals default to `f64`

```wscript
let x = 42;       // i32
let y = 3.14;     // f64
```

The default can be overridden by context:

```wscript
fn takes_i64(n: i64) { }
takes_i64(42);    // 42 is treated as i64 here due to the parameter type
```

## The `as` Cast

Wscript does not perform implicit coercions between numeric types. All numeric conversions require an explicit `as` cast:

```wscript
let x: i32 = 42;
let y: i64 = x as i64;    // widen
let z: f64 = x as f64;    // int to float
```

The `as` cast performs value conversion, truncating or sign-extending as needed. It does not check for overflow:

```wscript
let big: i64 = 1_000_000;
let small: i32 = big as i32;   // truncates if out of i32 range

let f: f64 = 3.99;
let i: i32 = f as i32;         // truncates to 3

let signed: i32 = -1;
let unsigned: u32 = signed as u32;  // reinterprets bits: 4294967295
```

For checked conversion that returns a `Result` instead of silently truncating, use the standard library:

```wscript
let big: i64 = 1_000_000_000_000;
let result = i32::try_from(big);   // Err -- value out of range
```

## Comparison Operators

All primitive types support comparison:

```wscript
let a = 10;
let b = 20;

a == b    // false
a != b    // true
a < b     // true
a > b     // false
a <= b    // true
a >= b    // true
```

## Bitwise Operators

Integer types support bitwise operations:

```wscript
let a: u8 = 0b1100;
let b: u8 = 0b1010;

let and = a & b;     // 0b1000 (8)
let or = a | b;      // 0b1110 (14)
let xor = a ^ b;     // 0b0110 (6)
let not = ~a;        // bitwise complement
let shl = a << 2;    // 0b110000 (48)
let shr = a >> 1;    // 0b0110 (6)
```
