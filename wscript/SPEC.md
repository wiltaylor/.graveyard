# Language Specification
## Version 0.1 — Implementation Draft

---

## Table of Contents

1. [Overview](#1-overview)
2. [Architecture](#2-architecture)
3. [Lexical Structure](#3-lexical-structure)
4. [Type System](#4-type-system)
5. [Variables and Bindings](#5-variables-and-bindings)
6. [Primitive Types](#6-primitive-types)
7. [Strings](#7-strings)
8. [Tuples](#8-tuples)
9. [Arrays](#9-arrays)
10. [Maps](#10-maps)
11. [Enums](#11-enums)
12. [Structs](#12-structs)
13. [Traits](#13-traits)
14. [Generics](#14-generics)
15. [Functions](#15-functions)
16. [Closures and Lambdas](#16-closures-and-lambdas)
17. [Control Flow](#17-control-flow)
18. [Pattern Matching](#18-pattern-matching)
19. [Pipelines and Iterators](#19-pipelines-and-iterators)
20. [Error Handling](#20-error-handling)
21. [Built-in Macros](#21-built-in-macros)
22. [Attributes](#22-attributes)
23. [Standard Library](#23-standard-library)
24. [Host Bindings](#24-host-bindings)
25. [Memory Model](#25-memory-model)
26. [Compiler Architecture](#26-compiler-architecture)
27. [Runtime Architecture](#27-runtime-architecture)
28. [Debug Infrastructure](#28-debug-infrastructure)
29. [LSP Server](#29-lsp-server)
30. [DAP Server](#30-dap-server)
31. [Crate Structure and Cargo Features](#31-crate-structure-and-cargo-features)
32. [Error Messages](#32-error-messages)
33. [Grammar Reference](#33-grammar-reference)

---

## 1. Overview

This document specifies a statically-typed, expression-oriented scripting language designed to be embedded in Rust host applications. The language compiles source text to WebAssembly (WASM) in memory at runtime and executes it via an embedded Wasmtime instance. The WASM binary is never written to disk and is never intended to interoperate with external WASM modules.

### Design Goals

- **Embeddable**: The entire runtime, compiler, LSP server, and DAP server ship as a single Rust library crate.
- **Strongly typed with inference**: Types are inferred from usage. Explicit annotations are optional except at ambiguous boundaries.
- **Functional data manipulation as a first class citizen**: Lazy iterator pipelines with LINQ-style operators are a core language feature, not an afterthought.
- **Safe by default**: No borrow checker. All heap values are reference-counted. The `Result` and `Option` types are first-class. Panics are catchable by the host.
- **Host-aware tooling**: The LSP and DAP servers are aware of all host-registered functions and types, providing full IDE support including completions, hover, and type checking of host API calls.
- **Familiar syntax**: Syntax is inspired by Rhai, Rust, and TypeScript. Developers familiar with any of these should feel at home quickly.

### Non-Goals

- Multi-threading within scripts.
- Interoperability with external WASM modules.
- A standalone binary distribution (the language is a library).
- A borrow checker or Rust-style ownership.

---

## 2. Architecture

### Compilation Pipeline

```
Source text (String)
    │
    ▼  Lexer
Token stream
    │
    ▼  Parser (error-recovering)
AST (every node carries a Span)
    │
    ▼  Type checker + inference
Typed AST
    │
    ▼  Lowering pass
IR  (pipelines desugared, closures converted to explicit upvalue structs,
     generics monomorphised)
    │
    ▼  WASM codegen (walrus crate)
WASM module bytes (in memory)
    │
    ▼  wasm-opt (Binaryen, optional, release mode only)
Optimised WASM bytes
    │
    ▼  Wasmtime: compile to native code (JIT)
CompiledScript (ready to execute)
```

### Key Architectural Decisions

**WASM as internal IR**: The compiler targets WASM as its execution format. WASM is never written to disk. The host never sees the WASM bytes directly. This gives access to Wasmtime's JIT compiler without the complexity of building a bytecode VM.

**Single library crate**: The Rust crate exposes the compiler, runtime, LSP server, and DAP server behind Cargo feature flags. Embedders take only what they need.

**Wasmtime embedding**: The crate embeds Wasmtime as a dependency. One `Engine` instance owns the Wasmtime engine, the host binding registry, and optionally the LSP/DAP servers.

**QueryDb for incremental compilation**: The LSP server maintains a `QueryDb` — an incremental, cached view of compiler state. Only changed files are re-parsed and re-type-checked on each keystroke.

---

## 3. Lexical Structure

### Source Encoding

Source files are UTF-8. There is no BOM handling. Line endings may be `\n` or `\r\n`; they are normalised to `\n` during lexing.

### Keywords

```
let  mut  const  fn  return  if  else  match  for  in  while  loop
break  continue  struct  impl  trait  enum  true  false  as
and  or  not  pub  self  Self  None  Some  Ok  Err
```

### Identifiers

Identifiers begin with a Unicode letter or underscore, followed by any number of Unicode letters, digits, or underscores. Identifiers are case-sensitive.

```
valid_ident   _private   MyStruct   π   count2
```

### Operators

```
+  -  *  /  %       arithmetic
==  !=  <  >  <=  >=   comparison
&&  ||  !           logical (also: and, or, not)
&   |   ^   ~   <<  >>  bitwise
=   +=  -=  *=  /=  %=  &=  |=  ^=  <<=  >>=   assignment
|>              pipe
?               error propagation
<=>             three-way comparison (spaceship)
..   ..=        range (exclusive, inclusive)
.               member access
::              path separator
->              return type annotation
=>              match arm
@               attribute prefix
```

### Literals

**Integer literals**:
```
42          decimal
0xFF        hexadecimal
0b1010      binary
0o77        octal
1_000_000   underscores ignored
42i32       with type suffix
255u8
```

Valid integer suffixes: `i8`, `i16`, `i32`, `i64`, `i128`, `u8`, `u16`, `u32`, `u64`, `u128`.

**Float literals**:
```
3.14
6.022e23
1.0f64      with type suffix
3.14f32
```

Valid float suffixes: `f32`, `f64`.

**Bool literals**: `true`, `false`.

**Character literals**: Single Unicode scalar value in single quotes. Escape sequences: `\n`, `\r`, `\t`, `\\`, `\'`, `\0`, `\u{HHHHHH}`.
```
'a'   '🦀'   '\n'   '\u{1F980}'
```

**String literals**: UTF-8, double-quoted. Same escape sequences as chars plus `\"`.
```
"hello"
"line one\nline two"
```

**Template string literals**: Backtick-delimited. Supports `${expr}` interpolation. Expressions inside `${}` are fully evaluated.
```
`Hello, ${name}!`
`Result: ${a + b}`
`Nested: ${if x > 0 { "pos" } else { "neg" }}`
```

**Array literals**:
```
[1, 2, 3]
[]           empty (type must be inferrable from context)
```

**Map literals**:
```
#{ "key": value, "other": 42 }
#{}          empty map
```

**Tuple literals**:
```
(1, "hello")
(1, true, 3.14f64)
()           unit
```

### Comments

```
// line comment

/*
   block comment
   can span multiple lines
*/
```

Comments are stripped during lexing. Doc comments are `///` and `/**` — they are attached to the next declaration and preserved in the symbol table for LSP hover display.

```
/// This function reads a file from disk.
/// Returns an error if the file does not exist.
fn read_file(path: String) -> Result<String> { ... }
```

### Whitespace

Whitespace is insignificant except as a token separator. Semicolons are required to terminate statements. Missing semicolons are a parse error with a helpful diagnostic.

---

## 4. Type System

### Overview

The type system uses **bidirectional type inference**. Types flow both top-down (from annotations into expressions) and bottom-up (from expressions to infer binding types). Explicit annotations are optional where inference succeeds and mandatory only at true ambiguity points.

Generics are **monomorphised** at compile time. Each instantiation of a generic function or struct with distinct type arguments produces distinct WASM code. There is no type erasure, no boxing overhead for generic types.

### Type Universe

```
Types
├── Primitive (always copied on assignment)
│   ├── i8, i16, i32, i64, i128
│   ├── u8, u16, u32, u64, u128
│   ├── f32, f64
│   ├── bool
│   └── char
├── Heap (ref-counted, assignment shares reference)
│   ├── String
│   ├── T[]            (Array<T>)
│   ├── Map<K, V>
│   ├── (A, B, ...)    (Tuple)
│   ├── Struct instances
│   ├── Enum instances (if they contain heap fields)
│   └── Fn(A, B) -> C  (closure / function value)
├── Generic wrappers (always heap-allocated)
│   ├── Option<T>      = Some(T) | None
│   ├── Result<T, E>   = Ok(T) | Err(E)
│   └── Result<T>      = Result<T, Error>  (type alias)
├── References (bump-allocated pointers into Vm linear memory)
│   ├── &T             (shared reference)
│   └── &mut T         (exclusive reference)
└── Special
    ├── Error          (type-erased, heap error value)
    ├── Ref<T>         (shared mutable cell)
    └── ()             (unit — zero-sized return type)

References (`&T`, `&mut T`) are formed with the prefix `&` / `&mut` operators and dereferenced with prefix `*`. `&mut T` may only be formed from a `let mut` binding. References are backed by a bump allocator inside the `Vm`'s linear memory and remain valid for the lifetime of the `Vm`. There is no borrow checker — the `&T` vs `&mut T` distinction is enforced at type-check time only.
```

### Type Inference Rules

1. Integer literals default to `i32` unless a suffix is present or the context demands otherwise.
2. Float literals default to `f64` unless a suffix is present.
3. Empty array `[]` requires a type annotation or a context that forces the element type.
4. `collect()` on a pipeline infers its target type from the binding annotation or the surrounding call context.
5. Closures infer parameter types from the context they are passed into.
6. `match` arm expressions must all have the same type.
7. `if`/`else` branches used as expressions must have the same type.

### Implicit Coercions

The language is **not** implicitly coercive between numeric types. All numeric conversions require an explicit `as` cast:

```
let x: i32 = 42;
let y: i64 = x as i64;
let z: f64 = x as f64;
```

The only implicit coercions are:
- Any `@error` type coerces to `Error` when passed to a function expecting `Error` or when used with `?` in a `Result<T>` context.
- Named functions coerce to `Fn(...)` closure types.

### The `as` Cast

`as` performs value conversion between numeric types, truncating or sign-extending as needed. It does not perform checked conversion. For checked conversion, use the standard library methods (e.g. `i32::try_from(x)`).

```
let big: i64 = 1_000_000;
let small: i32 = big as i32;   // truncates if out of range
let f: f64 = small as f64;
```

---

## 5. Variables and Bindings

### `let` Bindings

```
let x = 42;              // immutable, inferred i32
let y: f64 = 3.14;       // immutable, explicit type
let mut z = 0;           // mutable, inferred i32
let mut w: String = "";  // mutable, explicit type
```

Bindings are **immutable by default**. The `mut` keyword is required to allow reassignment or to call `&mut self` methods.

Bindings are **block-scoped**. A binding shadows an outer binding of the same name within an inner block.

### Top-Level (Global) Declarations

`let`, `let mut`, and `const` may appear at the top level of a file, outside any function:

```
let mut tick_count: i32 = 0;
let mut greeting: str = "hello";

struct PlayerState { hp: i32, score: i32, name: str }
let mut world: PlayerState = PlayerState { hp: 50, score: 0, name: "world" };
```

Initializers for top-level bindings run once, at `Vm` instantiation, inside a synthesized `__wscript_init_globals` start function. Non-constant initializers (including struct construction and `str` interning) are allowed. Top-level mutable globals persist across calls to exported functions and are accessible from the host via `Vm::get_global` / `Vm::set_global` (primitives) and `Vm::read_global_struct` / `Vm::write_global_struct` (structs).

There is no host-registered globals API — the script declares its own globals.

### `const` Declarations

```
const MAX_SIZE: i32 = 1024;
const PI: f64 = 3.14159265358979;
```

Constants must have explicit type annotations. Their values must be compile-time expressions (literals and arithmetic on literals). Constants are inlined at every use site.

### Destructuring in `let`

```
let (a, b) = some_tuple;
let (x, _, z) = triple;          // _ discards a field
let (first, rest) = (1, 2, 3);   // ERROR — rest is not a spread
```

Destructuring is currently limited to tuples in `let` bindings. Struct destructuring and array destructuring are not in scope for v0.1.

### Variable Shadowing

```
let x = 5;
let x = x + 1;    // new binding, shadows the old one
{
    let x = x * 2;  // shadows within this block only
}
// x is 6 here
```

---

## 6. Primitive Types

### Integer Types

| Type  | Width | Range |
|-------|-------|-------|
| `i8`  | 8-bit signed | -128 to 127 |
| `i16` | 16-bit signed | -32,768 to 32,767 |
| `i32` | 32-bit signed | -2,147,483,648 to 2,147,483,647 |
| `i64` | 64-bit signed | -9,223,372,036,854,775,808 to 9,223,372,036,854,775,807 |
| `i128`| 128-bit signed | |
| `u8`  | 8-bit unsigned | 0 to 255 |
| `u16` | 16-bit unsigned | 0 to 65,535 |
| `u32` | 32-bit unsigned | 0 to 4,294,967,295 |
| `u64` | 64-bit unsigned | 0 to 18,446,744,073,709,551,615 |
| `u128`| 128-bit unsigned | |

Arithmetic overflow on integer types **panics** in debug mode and **wraps** in release mode. Use explicit wrapping methods (`wrapping_add`, `wrapping_mul`, etc.) for intentional wrap-around arithmetic.

### Float Types

| Type  | Width | Precision |
|-------|-------|-----------|
| `f32` | 32-bit | ~7 decimal digits |
| `f64` | 64-bit | ~15 decimal digits |

Floating-point operations follow IEEE 754. `NaN`, `+Inf`, `-Inf` are valid values. Division by zero produces `+Inf` or `-Inf`, not a panic.

### Bool

`true` or `false`. Logical operators: `&&`, `||`, `!` (also `and`, `or`, `not`). Short-circuit evaluation applies.

### Char

A Unicode scalar value (U+0000 to U+D7FF, U+E000 to U+10FFFF). Stored as a 32-bit value internally. Methods: `.is_alphabetic()`, `.is_numeric()`, `.is_whitespace()`, `.to_uppercase()`, `.to_lowercase()`, `.to_string()`.

---

## 7. Strings

### String Type

`String` is a heap-allocated, UTF-8 encoded, growable string. Assignment shares the reference (ref-counted). Use `.clone()` for an independent copy.

### String Literals and Templates

```
let s1 = "hello, world";
let name = "Alice";
let s2 = `Hello, ${name}! 2 + 2 = ${2 + 2}`;
```

### String Methods

```
s.len()                      // byte length (u64)
s.char_count()               // Unicode scalar count (u64)
s.is_empty()                 // bool
s.contains(sub: String)      // bool
s.starts_with(prefix)        // bool
s.ends_with(suffix)          // bool
s.find(sub)                  // Option<u64>  (byte offset)
s.replace(from, to)          // String
s.to_uppercase()             // String
s.to_lowercase()             // String
s.trim()                     // String
s.trim_start()               // String
s.trim_end()                 // String
s.split(sep: String)         // String[]
s.split_once(sep: String)    // Option<(String, String)>
s.chars()                    // char[]
s.bytes()                    // u8[]
s.parse::<T>()               // Result<T>   where T is a numeric or bool type
s.repeat(n: u64)             // String
s.pad_start(n, ch)           // String
s.pad_end(n, ch)             // String
s + other                    // String (concatenation, creates new string)
s[start..end]                // String (byte slice — panics if not on char boundary)
```

---

## 8. Tuples

Tuples are fixed-size, ordered, heterogeneous collections. They are heap-allocated and ref-counted.

### Syntax

```
let pair: (i32, String) = (42, "hello");
let triple = (1, true, 3.14f64);
let unit = ();
```

### Access

```
let first  = pair.0;     // i32
let second = pair.1;     // String
```

### Destructuring

```
let (a, b) = pair;
let (x, _, z) = triple;    // _ discards field
```

### Tuples in Functions

```
fn min_max(items: i32[]) -> (i32, i32) {
    return (items.min(), items.max());
}

let (lo, hi) = min_max([3, 1, 4, 1, 5, 9]);
```

### Tuples in Match

```
let msg = match (ok, count) {
    (true,  0) => "ok but empty",
    (true,  _) => "ok",
    (false, _) => "error",
};
```

### Tuples in Pipelines

Tuples appear naturally as the element type of zipped, enumerated, or grouped pipelines:

```
items.enumerate()                     // (u64, T)[]
items.zip(other)                      // (A, B)[]
items.group_by(|x| key(x))           // Map<K, T[]>  (not tuple, but related)

// Collecting (K, V) tuples into a map:
let m: Map<String, i32> = pairs.collect();
```

---

## 9. Arrays

Arrays are dynamic, heap-allocated, ref-counted sequences of a single element type. The type syntax is `T[]`. They grow and shrink at runtime.

### Syntax

```
let a: i32[] = [1, 2, 3];
let b = [1, 2, 3];           // inferred: i32[]
let c: String[] = [];        // empty — annotation required
```

### Indexing

```
let first = a[0];             // panics if out of bounds
let last  = a[a.len() - 1];
let slice = a[1..3];          // i32[] — new array, shares ref-counted backing
```

### Mutation

```
let mut a = [1, 2, 3];
a.push(4);                    // append
a.pop();                      // remove last, returns Option<T>
a.insert(0, 99);              // insert at index
a.remove(0);                  // remove at index, returns T
a[1] = 42;                    // set by index
a.clear();                    // remove all elements
```

### Methods

```
a.len()                       // u64
a.is_empty()                  // bool
a.contains(val)               // bool
a.first()                     // Option<T>
a.last()                      // Option<T>
a.get(i)                      // Option<T>
a.clone()                     // T[]  (deep copy)
a.reverse()                   // T[]  (new array)
a.sort()                      // T[]  (new array, T must implement Comparable)
a.sort_by(|a, b| a <=> b)     // T[]  (new array)
a.dedup()                     // T[]  (remove consecutive duplicates)
a + b                         // T[]  (concatenation — new array)
a.extend(b)                   // mutates a, appends all of b
a.join(sep: String)           // String  (T must be String)
```

All pipeline/iterator methods are also available on arrays (see Section 19).

### Multidimensional Arrays

```
let matrix: i32[][] = [[1, 2], [3, 4]];
let val = matrix[0][1];    // 2
```

---

## 10. Maps

Maps are heap-allocated, ref-counted hash maps. Keys must implement `Hash` and `Eq` (primitives, `String`, and enums without payload do so automatically).

### Syntax

```
let m: Map<String, i32> = #{
    "alice": 30,
    "bob":   25,
};
let empty: Map<String, i32> = #{};
```

### Access and Mutation

```
let age = m["alice"];             // panics if key missing
let age = m.get("alice");         // Option<i32>
let age = m.get_or("alice", 0);   // i32 — default if missing

m["carol"] = 28;                  // insert or update
m.insert("dave", 32);             // same
m.remove("bob");                  // remove, returns Option<V>
```

### Methods

```
m.len()                           // u64
m.is_empty()                      // bool
m.contains("alice")               // bool  (key check)
m.keys()                          // K[]
m.values()                        // V[]
m.entries()                       // (K, V)[]
m.clone()                         // Map<K, V>  (deep copy)
m.merge(other)                    // Map<K, V>  (new map, other wins on conflict)
```

### Iteration

```
for (key, val) in m {
    print(`${key}: ${val}`);
}

// Pipeline on entries:
m.entries()
    .filter(|(_, v)| v > 25)
    .map(|(k, _)| k)
    .collect()                    // String[]
```

---

## 11. Enums

### Basic Enum

```
enum Direction {
    North,
    South,
    East,
    West,
}

let d = Direction::North;
```

### Data-Carrying Enum

```
enum Shape {
    Circle(f64),                  // tuple variant
    Rectangle(f64, f64),
    Named { name: String },       // struct variant
    Point,                        // unit variant
}

let s = Shape::Circle(5.0);
let r = Shape::Rectangle(3.0, 4.0);
let n = Shape::Named { name: "triangle" };
```

### Enum Methods

```
impl Shape {
    fn area(&self) -> f64 {
        return match self {
            Shape::Circle(r)       => r * r * 3.14159,
            Shape::Rectangle(w, h) => w * h,
            Shape::Named { .. }    => 0.0,
            Shape::Point           => 0.0,
        };
    }
}
```

### Pattern Matching on Enums

See Section 18 for full pattern matching syntax.

---

## 12. Structs

### Declaration

```
struct Point {
    x: f64,
    y: f64,
}

struct User {
    id:    u64,
    name:  String,
    email: String,
    tags:  String[],
}
```

### Generic Structs

```
struct Pair<A, B> {
    first:  A,
    second: B,
}

struct Wrapper<T> {
    value: T,
    label: String,
}
```

### Construction

```
let p = Point { x: 3.0, y: 4.0 };

// Shorthand when variable names match field names:
let x = 3.0;
let y = 4.0;
let p = Point { x, y };

// Update syntax — copy from another struct, override some fields:
let p2 = Point { x: 10.0, ..p };
```

### `impl` Blocks

Methods are defined in `impl` blocks. Multiple `impl` blocks for the same type are allowed.

```
impl Point {
    // Static method (constructor)
    fn new(x: f64, y: f64) -> Point {
        return Point { x, y };
    }

    // Read-only method
    fn distance_from_origin(&self) -> f64 {
        return (self.x * self.x + self.y * self.y).sqrt();
    }

    // Mutating method
    fn translate(&mut self, dx: f64, dy: f64) {
        self.x += dx;
        self.y += dy;
    }

    // Consuming method (returns modified copy for immutable workflows)
    fn translated(&self, dx: f64, dy: f64) -> Point {
        return Point { x: self.x + dx, y: self.y + dy };
    }
}

let mut p = Point::new(3.0, 4.0);
p.translate(1.0, 0.0);
let dist = p.distance_from_origin();
```

### `self` Reference Rules

- `&self` — shared read-only reference to self. Requires the binding to exist but not be `mut`.
- `&mut self` — exclusive mutable reference. Requires the binding to be declared `let mut`.
- No `self` (static) — no receiver. Called as `TypeName::method(args)`.

Because the language uses reference counting (not borrow checking), `&self` and `&mut self` are enforced by the **type checker** only — they do not create actual exclusive borrows at runtime. Two `&mut self` calls interleaved on the same object are permitted; the mutation ordering is defined by evaluation order.

---

## 13. Traits

### Declaration

```
trait Describable {
    fn describe(&self) -> String;
}

trait Comparable {
    fn compare(&self, other: &Self) -> i32;
}

// Trait with a default method implementation
trait Printable {
    fn to_string(&self) -> String;

    fn print(&self) {
        print(self.to_string());
    }
}
```

### Implementing a Trait

```
impl Describable for Point {
    fn describe(&self) -> String {
        return `Point(${self.x}, ${self.y})`;
    }
}

impl Describable for User {
    fn describe(&self) -> String {
        return `User(${self.id}: ${self.name})`;
    }
}
```

### Trait Bounds

```
fn print_desc<T: Describable>(item: T) {
    print(item.describe());
}

fn largest<T: Comparable>(a: T, b: T) -> T {
    if a.compare(&b) > 0 {
        return a;
    }
    return b;
}

// Multiple bounds with +
fn describe_and_compare<T: Describable + Comparable>(a: T, b: T) -> String {
    return `${a.describe()} vs ${b.describe()}`;
}
```

### Built-in Traits

The following traits are defined in the standard library and may be derived or implemented manually:

| Trait | Description |
|-------|-------------|
| `Clone` | Provides `.clone()` for deep copy |
| `Eq` | Equality comparison (`==`, `!=`) |
| `Hash` | Required for use as a `Map` key |
| `Comparable` | Ordering comparison (`<`, `>`, `<=`, `>=`, `<=>`) |
| `Display` | Provides `.to_string()` and string interpolation |
| `Debug` | Provides debug-formatted string representation |
| `Error` | Implemented automatically by `@error` |
| `From<T>` | Conversion from `T` — generated by `@from` |
| `Iterator` | Enables use in `for` loops and pipelines |

### Derive

The `@derive` attribute generates trait implementations automatically:

```
@derive(Clone, Eq, Hash, Debug)
struct Point {
    x: i32,
    y: i32,
}
```

Derivable traits: `Clone`, `Eq`, `Hash`, `Comparable` (lexicographic on fields in declaration order), `Debug`, `Display` (format: `TypeName { field: value, ... }`).

---

## 14. Generics

### Generic Functions

```
fn identity<T>(x: T) -> T {
    return x;
}

fn first<T>(items: T[]) -> Option<T> {
    if items.is_empty() {
        return None;
    }
    return Some(items[0].clone());
}

fn zip_with<A, B, C>(a: A[], b: B[], f: Fn(A, B) -> C) -> C[] {
    return a.zip(b).map(|(x, y)| f(x, y)).collect();
}
```

### Generic Structs

```
struct Stack<T> {
    items: T[],
}

impl Stack<T> {
    fn new() -> Stack<T> {
        return Stack { items: [] };
    }

    fn push(&mut self, item: T) {
        self.items.push(item);
    }

    fn pop(&mut self) -> Option<T> {
        return self.items.pop();
    }

    fn peek(&self) -> Option<T> {
        return self.items.last();
    }

    fn is_empty(&self) -> bool {
        return self.items.is_empty();
    }
}

let mut s: Stack<i32> = Stack::new();
s.push(1);
s.push(2);
let top = s.pop();    // Some(2)
```

### Monomorphisation

Each unique instantiation of a generic function or struct generates distinct WASM code. There is no shared generic implementation — this is identical to Rust's monomorphisation strategy. The type checker catches trait bound violations at the call site.

---

## 15. Functions

### Declaration

```
fn name(param: Type, param2: Type) -> ReturnType {
    // body
    return expression;
}
```

### No Implicit Returns

**All functions require an explicit `return` statement.** There is no implicit return from the final expression. This rule applies to all multi-line function bodies.

```
fn add(a: i32, b: i32) -> i32 {
    return a + b;      // explicit return required
}

// WRONG — compile error:
fn add(a: i32, b: i32) -> i32 {
    a + b              // error: missing return statement
}
```

The only exceptions are:
1. Single-line lambdas (see Section 16).
2. `match` arm expressions (the arm expression is the value, not a statement).
3. `if`/`else` expressions used as values (the branch expression is the value).

### Unit Return

Functions that return nothing have no return type annotation. They implicitly return `()`.

```
fn greet(name: String) {
    print(`Hello, ${name}!`);
    // implicit return ()
}
```

### Default Parameter Values

```
fn connect(host: String, port: u16 = 8080, timeout_ms: u64 = 5000) -> Result<()> {
    // ...
    return Ok(());
}

connect("localhost");
connect("localhost", 3000);
connect("localhost", port: 3000, timeout_ms: 1000);
```

Named arguments are supported when calling with non-trailing defaults. Positional arguments must appear before named arguments.

### Function as Value

Named functions coerce to `Fn(...)` types:

```
fn double(x: i32) -> i32 {
    return x * 2;
}

fn apply(f: Fn(i32) -> i32, x: i32) -> i32 {
    return f(x);
}

apply(double, 5);        // 10
apply(|x| x + 1, 5);    // 6
```

### The `@export` Attribute

Functions marked `@export` are callable from the host after compilation:

```
@export
fn process(items: i32[]) -> i32 {
    return items.filter(|x| x > 0).sum();
}
```

Without `@export`, functions may be optimised away or inlined. Only `@export` functions are guaranteed to be addressable by name from the host.

---

## 16. Closures and Lambdas

### Lambda Syntax

```
|params| expr              // single-line — implicit return
|params| { statements }    // multi-line — explicit return required
```

### Type Inference

Lambda parameter and return types are inferred from context. Explicit annotations are allowed:

```
let double = |x| x * 2;                        // inferred: Fn(i32) -> i32
let add    = |a, b| a + b;                     // inferred: Fn(i32, i32) -> i32
let typed  = |x: i64, y: i64| -> i64 { return x + y; };
```

### Multi-line Lambdas

```
let process = |x: i32| {
    let doubled = x * 2;
    let shifted = doubled + 1;
    return shifted;
};
```

### Closure Capture

Closures capture their environment at the point of definition:

- **Primitive types** are captured by copy. Mutations inside the closure do not affect the outer binding.
- **Heap types** (String, arrays, maps, structs) are captured by shared reference. The ref count is incremented. The closure and the outer scope share the same object.

```
let offset = 10;
let shift = |x| x + offset;    // captures copy of offset (i32 is primitive)
offset = 20;                    // does not affect shift's captured copy

let data = [1, 2, 3];
let count = || data.len();      // shares data's ref-counted allocation
data.push(4);                   // count() will now return 4
```

### Shared Mutable State Between Closures

When two closures need to mutate the same value, use `Ref<T>` — a ref-counted mutable cell:

```
let counter = Ref::new(0);

let increment = || { counter.set(counter.get() + 1); };
let get_count = || counter.get();

increment();
increment();
get_count()     // 2
```

`Ref<T>` methods:
```
Ref::new(value)     // create
r.get()             // T  (clones the inner value)
r.set(value)        // update
r.update(|v| ...)   // apply a function to the inner value
```

### Function Type Annotations

```
fn apply_twice(f: Fn(i32) -> i32, x: i32) -> i32 {
    return f(f(x));
}

fn make_adder(n: i32) -> Fn(i32) -> i32 {
    return |x| x + n;
}

let add5 = make_adder(5);
add5(3)    // 8
```

---

## 17. Control Flow

### `if` / `else if` / `else`

```
if condition {
    // ...
} else if other_condition {
    // ...
} else {
    // ...
}
```

`if` can be used as an expression. When used as an expression, all branches must produce the same type and the `else` branch is required:

```
let label = if x > 0 { "positive" } else if x < 0 { "negative" } else { "zero" };
```

### `match`

See Section 18.

### `for` Loops

```
// Iterate over any iterable (arrays, maps, ranges, pipeline results)
for item in collection {
    // ...
}

// With destructuring:
for (key, value) in map {
    print(`${key}: ${value}`);
}

for (index, value) in items.enumerate() {
    print(`[${index}] ${value}`);
}

// Ranges:
for i in 0..10 {   // 0 to 9 (exclusive)
    // ...
}

for i in 0..=10 {  // 0 to 10 (inclusive)
    // ...
}

// With step (using range + filter or stdlib):
for i in (0..100).step_by(5) {
    // ...
}
```

### `while` Loops

```
while condition {
    // ...
}
```

### `loop`

Infinite loop. Must be exited with `break`.

```
loop {
    if done {
        break;
    }
}

// loop as expression — break with a value:
let result = loop {
    let val = compute();
    if val > 100 {
        break val;
    }
};
```

### `break` and `continue`

```
for i in 0..10 {
    if i == 5 { break; }
    if i % 2 == 0 { continue; }
    print(i);
}
```

`break` with a value is only valid inside `loop`, not inside `for` or `while`.

---

## 18. Pattern Matching

### `match` Expression

`match` is exhaustive — the compiler errors if not all variants are covered.

```
match value {
    pattern1 => expr1,
    pattern2 => expr2,
    _        => fallback_expr,   // wildcard, matches anything
}
```

`match` is an expression. All arms must produce the same type.

### Pattern Types

**Literal patterns**:
```
match x {
    0    => "zero",
    1    => "one",
    2..5 => "two to four",    // range pattern
    _    => "other",
}
```

**Enum variant patterns**:
```
match shape {
    Shape::Circle(r)       => r * r * 3.14159,
    Shape::Rectangle(w, h) => w * h,
    Shape::Named { name }  => 0.0,
    Shape::Point           => 0.0,
}
```

**Tuple patterns**:
```
match (ok, count) {
    (true, 0)  => "ok but empty",
    (true, _)  => "ok",
    (false, _) => "error",
}
```

**Guard clauses** (`if`):
```
match n {
    n if n < 0  => "negative",
    0           => "zero",
    n if n > 100 => "large",
    _           => "small positive",
}
```

**Binding with `@`**:
```
match shape {
    s @ Shape::Circle(_) => {
        print(`Got a circle: ${s.describe()}`);
        return s;
    },
    other => return other,
}
```

**`Option` patterns**:
```
match opt {
    Some(value) => use(value),
    None        => fallback(),
}
```

**`Result` patterns**:
```
match result {
    Ok(value)  => use(value),
    Err(e)     => handle(e),
}
```

**`if let`** — shorthand match for a single variant:
```
if let Some(val) = maybe_value {
    use(val);
}

if let Ok(data) = load_file("config.txt") {
    process(data);
}
```

**`while let`**:
```
while let Some(item) = iter.next() {
    process(item);
}
```

---

## 19. Pipelines and Iterators

Pipelines are the primary mechanism for functional data transformation. They are **lazy** — no intermediate collection is allocated until a terminal operation is called. The pipe operator `|>` and method chaining syntax are equivalent.

### Pipe Operator

```
let result = source
    |> filter(|x| x > 0)
    |> map(|x| x * 2)
    |> collect();

// Equivalent method chaining:
let result = source
    .filter(|x| x > 0)
    .map(|x| x * 2)
    .collect();
```

### Transformation Operators

```
.map(|x| expr)                // transform each element
.flat_map(|x| array_expr)     // transform then flatten one level
.flatten()                    // flatten T[][] → T[]
.inspect(|x| side_effect)     // peek at each element without consuming
```

### Filtering Operators

```
.filter(|x| condition)        // keep elements where condition is true
.take(n: u64)                 // keep first n elements
.skip(n: u64)                 // skip first n elements
.take_while(|x| condition)    // take while condition holds
.skip_while(|x| condition)    // skip while condition holds
.distinct()                   // remove duplicates (T must implement Eq + Hash)
```

### Aggregation Operators (Terminal)

```
.sum()                        // T  (T must implement numeric addition)
.product()                    // T
.count()                      // u64
.min()                        // Option<T>
.max()                        // Option<T>
.min_by(|x| key_expr)         // Option<T>
.max_by(|x| key_expr)         // Option<T>
.fold(init, |acc, x| expr)    // accumulate with initial value
.reduce(|a, b| expr)          // accumulate without initial value → Option<T>
```

### Search Operators (Terminal)

```
.find(|x| condition)          // Option<T>  — first match
.find_last(|x| condition)     // Option<T>  — last match
.any(|x| condition)           // bool
.all(|x| condition)           // bool
.none(|x| condition)          // bool
.position(|x| condition)      // Option<u64>  — index of first match
```

### Ordering Operators

```
.sort_by(|a, b| a <=> b)      // T[]  (new collection, stable sort)
.sort_by_key(|x| key_expr)    // T[]
.reverse()                    // T[]
```

The `<=>` spaceship operator returns `i32`: negative if left < right, zero if equal, positive if left > right.

### Grouping and Zipping Operators

```
.group_by(|x| key_expr)       // Map<K, T[]>  (terminal)
.zip(other: U[])              // (T, U)[]
.unzip()                      // on (A, B)[] → (A[], B[])
.enumerate()                  // (u64, T)[]
.chunks(n: u64)               // T[][]  (split into chunks of n)
.windows(n: u64)              // T[][]  (sliding windows of size n)
.partition(|x| condition)     // (T[], T[])  — matching and non-matching
.step_by(n: u64)              // takes every nth element
```

### Collection Operators (Terminal)

```
.collect()                    // infers T[], Map<K,V>, or String from context
.collect(sep: String)         // String  — join with separator
.for_each(|x| side_effect)    // ()  — consume for side effects
.to_map(|x| (key, val))       // Map<K, V>  — explicit key-value mapping
```

**`collect()` type inference rules**:
- If the binding type annotation is `T[]` → collects to array.
- If the binding type annotation is `Map<K, V>` and the element type is `(K, V)` → collects to map.
- If the binding type annotation is `String` → joins with no separator (elements must be `String`).
- If no annotation, infers from the element type: `(K, V)` → `Map`, otherwise → `T[]`.
- `collect(sep: "...")` always produces `String`.

### Chaining Example

```
let word_freq: Map<String, u64> = text
    .split(" ")
    .filter(|w| !w.is_empty())
    .map(|w| w.to_lowercase())
    .group_by(|w| w.clone())
    .entries()
    .map(|(word, group)| (word, group.count() as u64))
    .sort_by_key(|(_, count)| -(count as i64))
    .take(10)
    .collect();
```

### Custom Iterators

A type can be made iterable by implementing the `Iterator` trait:

```
trait Iterator {
    type Item;
    fn next(&mut self) -> Option<Self::Item>;
}
```

Any type implementing `Iterator` can be used in `for` loops and all pipeline operators.

---

## 20. Error Handling

### `Option<T>`

```
Some(value)        // contains a value
None               // absent

let x: Option<i32> = Some(42);
let y: Option<i32> = None;
```

**Methods**:
```
.is_some()                    // bool
.is_none()                    // bool
.unwrap()                     // T — panics if None
.unwrap_or(default)           // T
.unwrap_or_else(|| expr)      // T — lazy default
.expect("message")            // T — panics with message if None
.map(|v| expr)                // Option<U>
.and_then(|v| Option<U>)      // Option<U> — flatmap
.or(other: Option<T>)         // Option<T>
.or_else(|| Option<T>)        // Option<T> — lazy
.filter(|v| condition)        // Option<T>
.ok_or(err)                   // Result<T, E>
.ok_or_else(|| err)           // Result<T, E> — lazy
```

### `Result<T, E>`

```
Ok(value)          // success
Err(error)         // failure

let r: Result<i32, String> = Ok(42);
let e: Result<i32, String> = Err("oops");
```

**`Result<T>`** — shorthand alias for `Result<T, Error>` using the dynamic `Error` type. This is the return type to use in application-level code that does not need callers to match on specific error variants.

**Methods**:
```
.is_ok()                      // bool
.is_err()                     // bool
.unwrap()                     // T — panics if Err
.unwrap_or(default)           // T
.unwrap_or_else(|e| expr)     // T
.expect("message")            // T
.map(|v| expr)                // Result<U, E>
.map_err(|e| expr)            // Result<T, F>
.and_then(|v| Result<U, E>)   // Result<U, E>
.or(other)                    // Result<T, E>
.ok()                         // Option<T>
.err()                        // Option<E>
```

### The `?` Operator

`?` on a `Result` value: if `Err`, returns early from the current function with that error. If `Ok`, unwraps the value and continues.

`?` on an `Option` value: if `None`, returns early from the current function with `None`. If `Some`, unwraps and continues.

```
fn load(path: String) -> Result<String> {
    let raw = read_file(path)?;    // returns Err early if read_file fails
    return Ok(raw.trim());
}
```

When `?` is used in a `Result<T>` context and the error type is a concrete `@error` type, it is automatically coerced to `Error` via the `From` implementation. When `@from(T)` is declared on a `Result<T, AppError>` variant, `?` converts `T` errors to `AppError` automatically.

### `@error` — Typed Errors

Apply `@error` to an enum or struct to auto-implement the `Error` trait.

**`@error` enum**:
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

**`@error` struct** (single failure mode):
```
@error
@msg("config error in '{file}': {reason}")
struct ConfigError {
    file:   String,
    reason: String,
}
```

**`@msg` format strings**: Use `{field_name}` for named struct variant fields, `{0}`, `{1}`, etc. for tuple variant fields.

### `@from` — Automatic Error Conversion

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
```

`@from(T)` on a variant generates `impl From<T> for AppError`. This allows `?` to automatically convert `ParseError` → `AppError::Parse` and `ConfigError` → `AppError::Config` without explicit `map_err`.

### `@source` — Error Cause Chain

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

`@source` marks the field holding the root cause. Enables `.source()` and `.source_chain()`.

### `@transparent` — Delegate Display to Inner Error

```
@error
enum AppError {
    @transparent
    @from(ParseError)
    Parse(ParseError),    // displays as if it were a ParseError directly
}
```

### Dynamic `Error` Type

`Error` is a built-in type-erased error. Any `@error` type coerces to `Error`. Used with `Result<T>` (the shorthand alias).

**Methods on all error types** (both `@error` concrete types and `Error`):
```
e.to_string()             // String — the formatted message
e.source()                // Option<Error> — the direct cause
e.source_chain()          // Error[] — full cause chain, shallowest first
e.downcast::<T>()         // Result<T, Error> — consuming type recovery
e.downcast_ref::<T>()     // Option<&T> — non-consuming type check
e.is::<T>()               // bool — quick type check
```

---

## 21. Built-in Macros

Macros are invoked with `name!(...)` syntax. The `!` distinguishes them from function calls.

### `error!(msg)` — Create an `Error` Inline

```
return Err(error!("expected positive value, got {x}"));
```

Produces an `Error` from a format string. `{name}` interpolates in-scope variables.

### `bail!(msg)` — Early Return with Error

```
fn divide(a: f64, b: f64) -> Result<f64> {
    if b == 0.0 {
        bail!("cannot divide by zero");
    }
    return Ok(a / b);
}

// Also accepts typed error variants:
bail!(AppError::NotFound { path: path });
```

Sugar for `return Err(error!(...))` or `return Err(TypedVariant)`.

### `ensure!(condition, msg)` — Assert or Bail

```
fn parse_header(bytes: u8[]) -> Result<Header> {
    ensure!(bytes.len() >= 8, "header too short: got {0} bytes", bytes.len());
    ensure!(bytes[0] == 0xFF, "invalid magic byte");
    return Ok(parse_header_inner(bytes));
}
```

Sugar for `if !condition { bail!(msg); }`.

### `dbg!(expr)` — Debug Print and Pass Through

```
let x = dbg!(compute(a));
// prints: [script.al:12] compute(a) = 42
// x is still the result of compute(a)
```

Returns its argument unchanged. Removed in release mode (becomes a no-op).

### `assert!(condition)` and `assert!(condition, msg)`

```
assert!(x > 0);
assert!(x > 0, "expected positive, got {x}");
```

Panics with location info if the condition is false.

### `assert_eq!(left, right)` and `assert_ne!(left, right)`

```
assert_eq!(result, 42);
assert_ne!(a, b);
assert_eq!(got, expected, "custom message");
```

On failure, shows both values:
```
panic: assertion failed: result == 42
  left:  37
  right: 42
  at script.al:18:4
```

### `todo!()`

```
fn not_implemented_yet() -> i32 {
    return todo!();
}
```

Panics with "not yet implemented" and the source location if reached.

### `unreachable!()`

```
match direction {
    Direction::North => handle_north(),
    Direction::South => handle_south(),
    _ => unreachable!("all directions should be handled above"),
}
```

Panics if reached. Documents impossible code paths.

---

## 22. Attributes

Attributes annotate declarations. They use the `@` prefix.

### Declaration Attributes

| Attribute | Applies to | Effect |
|-----------|-----------|--------|
| `@export` | `fn` | Makes the function callable from the host after compilation |
| `@error` | `struct`, `enum` | Implements the `Error` trait, enables `@msg`, `@from`, `@source`, `@transparent` |
| `@msg("...")` | enum variant, `@error` struct | Sets the display format string |
| `@from(Type)` | enum variant | Generates `From<Type>` for the enclosing enum |
| `@source` | struct field | Marks the cause field in an `@error` type |
| `@transparent` | enum variant | Delegates `Display` to the inner error |
| `@derive(...)` | `struct`, `enum` | Auto-implements listed traits |
| `@trace` | `fn` | In debug mode, logs entry, args, exit, and duration |
| `@trace(args: false)` | `fn` | Logs entry and exit without argument values |
| `@deprecated("msg")` | any declaration | Emits a warning at call sites |
| `@allow(warning_name)` | any declaration | Suppresses a specific warning |

### Attribute Stacking

Multiple attributes may appear on a single declaration:

```
@error
@derive(Clone)
@msg("io error: {0}")
struct IoError(String);
```

---

## 23. Standard Library

The standard library is available in all scripts without any import. It consists of the built-in methods on all types (described above) plus the following free functions and types.

### I/O

```
print(value)                  // prints value.to_string() + newline
print_err(value)              // prints to stderr
eprint(value)                 // alias for print_err
```

These route through the host's configured output handler (default: stdout/stderr).

### Math

```
abs(x)                        // same type as x
min(a, b)                     // same type
max(a, b)                     // same type
clamp(x, lo, hi)              // same type
pow(base: f64, exp: f64)      // f64
sqrt(x: f64)                  // f64
cbrt(x: f64)                  // f64
floor(x: f64)                 // f64
ceil(x: f64)                  // f64
round(x: f64)                 // f64
ln(x: f64)                    // f64
log2(x: f64)                  // f64
log10(x: f64)                 // f64
sin(x: f64)                   // f64
cos(x: f64)                   // f64
tan(x: f64)                   // f64
PI: f64                       // constant
E: f64                        // constant
```

### Type Conversion

```
i8(x)   i16(x)   i32(x)   i64(x)   i128(x)    // convert, panic on overflow
u8(x)   u16(x)   u32(x)   u64(x)   u128(x)
f32(x)  f64(x)
bool(x)                               // 0 → false, nonzero → true
string(x)                             // calls .to_string()

// Checked conversion:
i32::try_from(x)              // Result<i32>
u8::try_from(x)               // Result<u8>
// etc.
```

### Ranges

```
0..10                         // Range<i32> — exclusive, iterable
0..=10                        // RangeInclusive<i32> — inclusive
'a'..'z'                      // Range<char>
```

Ranges implement `Iterator` and support all pipeline operators.

### `Ref<T>`

```
Ref::new(value: T) -> Ref<T>
ref.get() -> T
ref.set(value: T)
ref.update(|v| new_value)
```

### Option and Result Constructors

These are in scope without any qualification:

```
Some(value)
None
Ok(value)
Err(error)
```

---

## 24. Host Bindings

### Overview

The host application registers functions, types, and global values with the engine before compiling or running any scripts. These registrations serve three purposes:

1. **Runtime**: The registered Rust closures are called when scripts invoke the host functions.
2. **Type checking**: The parameter and return types are used by the compiler to type-check calls to host functions.
3. **LSP**: The type information and doc strings appear in completions, hover, and inlay hints.

### Registering Functions

```rust
engine
    .register_fn("read_file", |path: String| -> Result<String, String> {
        std::fs::read_to_string(&path).map_err(|e| e.to_string())
    })
    .doc("Read a UTF-8 file from disk.")
    .param("path", "The file path to read")
    .returns("The file contents")
    .example(r#"
let contents = read_file("data.txt")?;
    "#)
```

**Type mapping** (Rust ↔ language types):

| Rust type | Language type |
|-----------|--------------|
| `i8`, `i16`, `i32`, `i64`, `i128` | `i8`, `i16`, `i32`, `i64`, `i128` |
| `u8`, `u16`, `u32`, `u64`, `u128` | `u8`, `u16`, `u32`, `u64`, `u128` |
| `f32`, `f64` | `f32`, `f64` |
| `bool` | `bool` |
| `String` or `&str` | `String` |
| `Vec<T>` | `T[]` |
| `HashMap<K, V>` | `Map<K, V>` |
| `Option<T>` | `Option<T>` |
| `Result<T, E: ToString>` | `Result<T>` |
| `()` | `()` |
| `Box<dyn Any>` | registered type name |

### Registering Types

```rust
engine
    .register_type::<DbConnection>()
    .doc("A connection to the application database.")
    .method("query", |conn: &DbConnection, sql: String| -> Result<Vec<HashMap<String, String>>, String> {
        conn.query(&sql).map_err(|e| e.to_string())
    })
    .method_doc("query", "Execute a SQL SELECT and return rows as an array of maps.")
    .method("execute", |conn: &mut DbConnection, sql: String| -> Result<u64, String> {
        conn.execute(&sql).map_err(|e| e.to_string())
    })
    .method_doc("execute", "Execute a SQL statement and return the number of rows affected.")
    .debug_display(|conn: &DbConnection| {
        format!("DbConnection(host={}, db={})", conn.host, conn.db_name)
    })
    .debug_children(|conn: &DbConnection| vec![
        ("host".into(),    DebugValue::String(conn.host.clone())),
        ("db".into(),      DebugValue::String(conn.db_name.clone())),
        ("is_open".into(), DebugValue::Bool(conn.is_open())),
    ])
    .done()
```

In scripts, `DbConnection` appears as a first-class type with the registered methods available for call and completion.

### Registering Globals

```rust
engine.register_global("db", db_connection_instance);
engine.register_global("config", config_map);
```

Globals appear in scripts as pre-bound identifiers. Their type is inferred from the registered value.

### Reading and Writing Globals from the Host

```rust
// Write before execution
engine.set_global("input", my_value)?;

// Read after execution
let result: i32 = script.get_global("output")?;
```

### Host Function Visibility in Scripts

Host functions are called without any special syntax. The type checker validates them at compile time using the registered type information. An unregistered function call is a compile error.

```
// Just call it — no @import or declaration needed in the script:
let data = read_file("config.json")?;
let rows = db.query("SELECT * FROM users")?;
```

---

## 25. Memory Model

### Reference Counting

All heap-allocated values (strings, arrays, maps, tuples, structs, closures) use reference counting. Assignment increments the reference count; going out of scope decrements it. When the count reaches zero the value is freed.

```
let a = [1, 2, 3];     // ref count = 1
let b = a;             // ref count = 2 — b and a share the same array
let c = a.clone();     // ref count stays 2 — c is a new, independent array
// b goes out of scope: ref count = 1
// a goes out of scope: ref count = 0 — freed
```

### No Borrow Checker

There is no borrow checker. The type checker enforces `&self` vs `&mut self` as a convention — `&mut self` methods require a `let mut` binding — but at runtime all method calls go through the same ref-counted pointer. This means:

- You can alias freely.
- Two closures can both hold references to the same mutable struct.
- There is no protection against logical mutation conflicts at runtime.

### Cycles

Reference-counted values with cycles will leak. The language does not include a cycle collector in v0.1. Embedders should avoid creating circular struct references in script code.

### Primitives

`i8`, `i16`, `i32`, `i64`, `i128`, `u8`, `u16`, `u32`, `u64`, `u128`, `f32`, `f64`, `bool`, `char` are stored by value in WASM locals and on the WASM stack. They are always copied on assignment. There is no boxing of primitives.

### WASM Linear Memory

The language runtime manages a linear memory inside the WASM module. A small allocator (wee_alloc or a custom bump allocator) is linked into every compiled module. Ref count operations, string data, array backing buffers, and struct fields all live in this linear memory. The host does not access this memory directly — all host↔script data exchange goes through the registered binding layer.

---

## 26. Compiler Architecture

### Lexer

The lexer converts source text to a flat token stream. It is a hand-written, single-pass scanner. All lexer errors (invalid characters, unclosed strings) produce an error token rather than aborting — the parser handles recovery.

**Output**: `Vec<Token>` where `Token = { kind: TokenKind, span: Span }`.

### Parser

The parser is a hand-written recursive descent parser producing an AST. It is **error-recovering**: on a syntax error it emits a diagnostic, inserts an `Error` AST node, and advances to a synchronisation point (`;`, `}`, `fn`, end of file) before continuing. This ensures the parser always produces a full (possibly partial) AST for the whole file.

Every AST node carries a `Span { start: u32, end: u32, line: u32, col: u32 }`.

### Type Checker

Bidirectional type inference. The checker walks the typed AST in two passes:
1. **Forward pass**: infer types bottom-up from literals and known bindings.
2. **Backward pass**: propagate annotations top-down to resolve ambiguous types.

The type checker produces a fully annotated `TypedAst` where every node has a resolved type. It validates:
- Type compatibility at assignment and argument sites.
- `&mut self` method calls require `let mut` bindings.
- Exhaustiveness of `match` expressions.
- Trait bound satisfaction for generic instantiations.
- Host function signatures against registered bindings.

Type errors are collected (not fatal) and all continue to produce `Error` nodes. This prevents cascading errors.

### Lowering Pass

Transforms the `TypedAst` to a simpler `IR`:
- **Pipelines**: desugared into explicit iterator state machine structs.
- **Closures**: upvalue analysis; upvalues converted to explicit struct fields; `Ref<T>` cells inserted for shared mutable upvalues.
- **Generics**: monomorphised; each instantiation becomes a concrete IR function.
- **`@error` types**: `Error` trait impl and `From` impls generated.
- **Macros**: expanded (`bail!`, `ensure!`, `dbg!`, `assert!`, etc.).
- **String interpolation**: desugared to concatenation.
- **Range literals**: desugared to `Range::new(start, end)`.

### WASM Codegen

The IR is lowered to WASM using the `walrus` crate. The codegen maintains a `SourceMap` — a mapping from WASM byte offsets to `Span` values — alongside the module being built.

In debug mode, the codegen inserts a probe call (`call $__debug_probe(location_id)`) at every statement boundary. `location_id` is an index into the source map table.

### Source Map

```
SourceMap {
    entries: Vec<SourceMapEntry>,
}

SourceMapEntry {
    wasm_offset: u32,
    span:        Span,
    fn_name:     Option<String>,
    local_names: HashMap<u32, String>,   // wasm local index → source name
}
```

The source map is stored in the `CompiledScript` alongside the WASM bytes. It is never written to disk.

### DWARF Emission (Debug Mode, Optional)

When `debug_mode` is enabled and `emit_dwarf` is set in the engine config, the compiler emits DWARF debug information into custom WASM sections using the `gimli` crate. This enables native debugger support (LLDB, Chrome DevTools) without requiring the source map layer.

### Optimisation

In release mode, after WASM generation, the binary is passed through `wasm-opt` (Binaryen) for optimisation. The source map is **not** updated after `wasm-opt` — source-level debugging is only supported in debug mode.

---

## 27. Runtime Architecture

### Engine

The `Engine` struct is the main entry point. It owns:
- The Wasmtime `Engine` instance.
- The host binding registry (`BindingRegistry`).
- The `QueryDb` (if LSP or DAP is enabled).
- Configuration (debug mode, fuel limit, callbacks).

One `Engine` instance can compile and run multiple scripts. It is designed to be long-lived for the host application lifetime.

### CompiledScript

`engine.load(source: &str) -> Result<CompiledScript>` runs the full compilation pipeline and produces a `CompiledScript`. This includes:
- The Wasmtime-compiled native module (JIT'd).
- The source map.
- The set of exported function names and their signatures.

`CompiledScript` instances are lightweight to clone — the underlying module is reference-counted by Wasmtime.

### Execution

```rust
let value: i32 = script.call("main", &[])?;
let result: Vec<i32> = script.call("process", &[Value::Array(input)])?;
```

Each `call` creates a new Wasmtime `Store` with a fresh linear memory and fresh copies of all global values. Scripts are stateless between calls unless globals are explicitly set.

### Fuel Metering

When `max_fuel` is configured, Wasmtime's fuel metering is enabled. Fuel is consumed approximately one unit per WASM instruction. When fuel is exhausted, the `on_fuel_exhausted` callback is invoked. The callback may add more fuel or return `FuelAction::Trap` to abort execution.

### Debug Probes

In debug mode, every compiled script contains probe calls at statement boundaries. The probe function is a host import that checks a breakpoint table on every call. When no breakpoints are set, the probe is a near-zero-cost table lookup and branch-not-taken. When a breakpoint is hit, the probe suspends execution, collects the current frame (locals, call stack), and calls the `on_break` handler.

### Panic Handling

Panics in scripts (index out of bounds, unwrap on None, assert failure, etc.) produce a Wasmtime trap. The runtime catches the trap, translates the WASM backtrace to source-level frames using the source map, and returns a `ScriptPanic` error to the host.

```rust
ScriptPanic {
    message: String,
    trace:   Vec<SourceFrame>,
}

SourceFrame {
    fn_name: String,
    file:    String,
    line:    u32,
    col:     u32,
}
```

---

## 28. Debug Infrastructure

### Source Maps

Every `CompiledScript` carries a `SourceMap`. This is the foundation of all debug features. It maps WASM byte offsets to source locations.

### Stack Traces

On any panic or unhandled error, the runtime produces a source-level stack trace by walking the Wasmtime `WasmBacktrace` and looking up each frame's offset in the source map.

In debug mode, `?` also captures a lightweight frame. The error's cause chain therefore carries both the error context messages and the propagation call stack.

**Example output**:
```
error: failed to load config
  caused by: file not found: 'config.toml'

stack trace:
  at load_config(path: String)   app.al:24:16
  at startup(args: String[])     app.al:10:4
  at main()                      app.al:3:2
```

### Breakpoints

Set by source line. The engine maps the line to the nearest probe point at or after that line. If no probe point exists on the requested line, the breakpoint is set on the next available line and the host is informed.

```rust
script.set_breakpoint(line: 12);
script.clear_breakpoint(line: 12);
script.clear_all_breakpoints();
```

### BreakpointFrame

Passed to the `on_break` callback:

```rust
struct BreakpointFrame {
    location:  Span,
    fn_name:   String,
    locals:    HashMap<String, DebugValue>,
    callstack: Vec<SourceFrame>,
}

enum DebugValue {
    I32(i32), I64(i64), F64(f64), Bool(bool),
    String(String),
    Array(Vec<DebugValue>),
    Map(Vec<(DebugValue, DebugValue)>),
    Struct { type_name: String, fields: Vec<(String, DebugValue)> },
    HostObject { display: String, children: Vec<(String, DebugValue)> },
    Null,
}
```

### Debug Actions

```rust
enum DebugAction {
    Continue,
    StepOver,
    StepInto,
    StepOut,
    Stop,
    SetValue { name: String, value: DebugValue },
}
```

The `on_break` callback returns a `DebugAction` to control execution after the breakpoint.

### `@trace` Attribute

In debug mode, `@trace` on a function inserts entry and exit logging routed through the host's configured log handler:

```
→ process(items: [1, 2, 3])     script.al:5
← process = 6                   script.al:5  (0.12ms)
```

In release mode, `@trace` is a no-op.

---

## 29. LSP Server

### Overview

The LSP server is enabled by the `lsp` Cargo feature. It implements the Language Server Protocol, speaking JSON-RPC over stdio or a TCP socket. It is built on the `tower-lsp` crate.

The LSP server is hosted inside the embedding application. The application registers all its host bindings, then calls `engine.serve_lsp()`. The application binary becomes the language server.

### QueryDb

The `QueryDb` is the incremental compiler state maintained by the LSP server. It holds the current source text, AST, type-checked IR, symbol table, scope tree, and diagnostics for every open file.

On every `textDocument/didChange` notification, the affected file is re-parsed and re-type-checked (with a 150ms debounce). Only the changed file is invalidated; other files are unaffected.

### Host Binding Awareness

The `QueryDb` is pre-seeded with all registered host functions, types, and globals at startup. They appear in the symbol table as first-class definitions. The type checker uses them when checking host function calls in scripts. This means:

- Completions include host functions alongside stdlib.
- Hover shows host function signatures and doc strings.
- Type errors are reported for incorrect host function argument types.
- Inlay hints show types inferred from host function return types.

### Supported LSP Features

| Feature | LSP Method | Implementation notes |
|---------|------------|---------------------|
| Diagnostics | `textDocument/publishDiagnostics` | Parse errors, type errors, warnings — published after each recheck |
| Semantic tokens | `textDocument/semanticTokens/full` | Keywords, types, functions, variables, strings — enables rich highlighting |
| Hover | `textDocument/hover` | Shows inferred type and doc comment for symbol under cursor |
| Go to definition | `textDocument/definition` | Works for local bindings, struct fields, trait impls, and host types |
| Go to type definition | `textDocument/typeDefinition` | |
| Find references | `textDocument/references` | |
| Document symbols | `textDocument/documentSymbol` | Outline: fns, structs, enums, traits |
| Workspace symbols | `workspace/symbol` | Fuzzy search across all open files |
| Completion | `textDocument/completion` | Variables in scope, struct fields after `.`, enum variants after `::`, host fns, trait methods |
| Signature help | `textDocument/signatureHelp` | Shows parameter names and types as you type a call |
| Inlay hints | `textDocument/inlayHint` | Inferred types on `let` bindings, inferred param types on lambdas |
| Rename | `textDocument/rename` | Renames all references in the file |
| Formatting | `textDocument/formatting` | Canonical formatting pass |
| Code actions | `textDocument/codeAction` | Quick fixes: add `mut`, wrap in `match`, add `?`, implement trait stub |

### Error Recovery Requirements

The parser **must** produce a partial, valid AST on syntax errors for the LSP to be useful. A parser that returns `None` on any syntax error is not acceptable. Error recovery strategy:

1. On a syntax error, emit a diagnostic with the span of the unexpected token.
2. Insert an `Expr::Error(span)` or `Stmt::Error(span)` node.
3. Skip forward to the next synchronisation point: `;`, `}`, `fn`, `struct`, `impl`, `enum`, `trait`, or EOF.
4. Continue parsing from the synchronisation point.

The type checker must propagate `Error` nodes as the `Unknown` type to prevent cascading errors.

### Starting the LSP

```rust
// In the host application's main:
if args.contains("--lsp") {
    Engine::new()
        .register_fn("my_api", my_api_impl)
        .register_type::<MyType>()
        .done()
        .lsp(LspConfig { transport: LspTransport::Stdio })
        .build()?
        .serve_lsp()
        .await?;
    return Ok(());
}
```

---

## 30. DAP Server

### Overview

The DAP server is enabled by the `dap` Cargo feature. It implements the Debug Adapter Protocol, speaking JSON-RPC over a TCP socket. It is built on the `dap-rs` crate.

The DAP server owns a `ScriptEngine` in debug mode. It compiles and runs scripts under the control of the connected client (VS Code).

### DAP ↔ Debug API Mapping

| DAP request | Host debug API |
|-------------|---------------|
| `launch` / `attach` | `engine.load(source)` with debug mode |
| `setBreakpoints` | `script.set_breakpoint(line)` per breakpoint; responds `verified: true/false` |
| `configurationDone` | Start execution on a background thread |
| `threads` | Always returns a single thread (scripts are single-threaded) |
| `stackTrace` | `frame.callstack` mapped to DAP `StackFrame[]` with source paths, lines, cols |
| `scopes` | Returns one scope ("Locals") per frame |
| `variables` | `frame.locals` as DAP `Variable[]`; nested values get a `variablesReference` |
| `continue` | `DebugAction::Continue` sent to paused probe |
| `next` | `DebugAction::StepOver` |
| `stepIn` | `DebugAction::StepInto` |
| `stepOut` | `DebugAction::StepOut` |
| `setVariable` | `DebugAction::SetValue` |
| `evaluate` | Compile and execute expression in current scope |
| `disconnect` | Stop execution, shut down |

### Host Type Rendering

Host types registered with `debug_display` and `debug_children` are rendered in the Variables panel. The `debug_children` entries are expandable sub-nodes.

### Starting the DAP Server

```rust
if args.contains("--dap") {
    Engine::new()
        .register_fn("my_api", my_api_impl)
        .register_type::<MyType>()
            .debug_display(|t| format!("MyType({})", t.name))
            .debug_children(|t| vec![
                ("name".into(), DebugValue::String(t.name.clone())),
            ])
            .done()
        .dap(DapConfig { transport: DapTransport::Socket(6009) })
        .build()?
        .serve_dap()
        .await?;
    return Ok(());
}
```

### Combined LSP + DAP

Both servers can run simultaneously in the same process:

```rust
Engine::new()
    .register_fn("my_api", my_api_impl)
    .lsp(LspConfig { transport: LspTransport::Stdio })
    .dap(DapConfig { transport: DapTransport::Socket(6009) })
    .build()?
    .serve()
    .await?;   // runs both concurrently on separate async tasks
```

---

## 31. Crate Structure and Cargo Features

### Features

| Feature | Dependencies enabled | Description |
|---------|---------------------|-------------|
| `runtime` (default) | `wasmtime` | Execute compiled scripts |
| `lsp` | `tower-lsp`, `tokio` | LSP server |
| `dap` | `dap-rs`, `tokio` | DAP server |
| `full` | all of the above | Everything |
| *(none)* | — | Compile and type-check only; no execution |

### Module Layout

```
src/
├── lib.rs                   — public API
├── engine.rs                — Engine builder, config, serve()
│
├── compiler/
│   ├── lexer.rs             — tokeniser
│   ├── token.rs             — TokenKind enum
│   ├── parser.rs            — recursive descent, error-recovering
│   ├── ast.rs               — AST node types, Span
│   ├── tycheck.rs           — type inference, trait checking
│   ├── lower.rs             — IR lowering, monomorphisation
│   ├── ir.rs                — IR types
│   ├── codegen.rs           — IR → WASM via walrus
│   └── source_map.rs        — SourceMap, SourceMapEntry
│
├── query_db.rs              — incremental compiler state for LSP
├── bindings.rs              — HostFnBinding, HostTypeBinding, BindingRegistry
│
├── runtime/                 — feature: runtime
│   ├── vm.rs                — Wasmtime setup, script execution
│   ├── value.rs             — Value enum (script ↔ host data exchange)
│   └── debug.rs             — probe instrumentation, BreakpointFrame
│
├── lsp/                     — feature: lsp
│   ├── server.rs            — tower-lsp LanguageServer impl
│   ├── completions.rs
│   ├── hover.rs
│   ├── diagnostics.rs
│   ├── inlay_hints.rs
│   ├── semantic_tokens.rs
│   └── formatting.rs
│
└── dap/                     — feature: dap
    └── server.rs            — dap-rs impl
```

### Key External Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `wasmtime` | latest stable | WASM execution (feature: runtime) |
| `walrus` | latest stable | WASM IR construction |
| `wasm-encoder` | latest stable | Low-level WASM encoding |
| `tower-lsp` | latest stable | LSP server framework (feature: lsp) |
| `dap-rs` | latest stable | DAP server framework (feature: dap) |
| `tokio` | latest stable, `full` | Async runtime for LSP/DAP (feature: lsp or dap) |
| `gimli` | latest stable | DWARF debug info generation |

---

## 32. Error Messages

Error messages must be precise, include the source location, and where possible suggest a fix.

### Format

```
error[E001]: type mismatch
  --> script.al:12:8
   |
12 |     let x: String = 42;
   |                     ^^ expected String, found i32
   |
   = hint: use string(42) to convert
```

### Error Codes

| Code | Category | Description |
|------|----------|-------------|
| E001 | Type | Type mismatch |
| E002 | Type | Cannot infer type — annotation required |
| E003 | Type | Trait bound not satisfied |
| E004 | Type | Missing field in struct literal |
| E005 | Type | Unknown field or method |
| E006 | Type | Wrong number of arguments |
| E007 | Borrow | `&mut self` method called on immutable binding |
| E008 | Pattern | Non-exhaustive match |
| E009 | Pattern | Unreachable match arm |
| E010 | Name | Undefined variable |
| E011 | Name | Undefined function |
| E012 | Name | Undefined type |
| E013 | Name | Undefined host function (registered binding not found) |
| E014 | Syntax | Unexpected token |
| E015 | Syntax | Missing semicolon |
| E016 | Syntax | Unclosed delimiter |
| W001 | Warning | Unused variable (prefix with `_` to suppress) |
| W002 | Warning | Unused function |
| W003 | Warning | Unused import |
| W004 | Warning | Deprecated symbol |
| W005 | Warning | Unreachable code |

---

## 33. Grammar Reference

The following is an informal EBNF grammar for the language. This is non-normative — the parser implementation is authoritative.

```ebnf
program         = item* EOF

item            = fn_decl
                | struct_decl
                | enum_decl
                | trait_decl
                | impl_block
                | const_decl
                | global_decl

global_decl     = 'let' 'mut'? IDENT (':' type)? '=' expr ';'

fn_decl         = attr* 'fn' IDENT generic_params? '(' params? ')' return_type? block

struct_decl     = attr* 'struct' IDENT generic_params? '{' struct_fields '}'
struct_fields   = (IDENT ':' type (',' IDENT ':' type)* ','?)?

enum_decl       = attr* 'enum' IDENT generic_params? '{' enum_variants '}'
enum_variants   = (enum_variant (',' enum_variant)* ','?)?
enum_variant    = attr* IDENT                              // unit
                | attr* IDENT '(' type (',' type)* ')'    // tuple
                | attr* IDENT '{' struct_fields '}'        // struct

trait_decl      = attr* 'trait' IDENT '{' trait_item* '}'
trait_item      = fn_decl | fn_sig ';'
fn_sig          = attr* 'fn' IDENT generic_params? '(' params? ')' return_type?

impl_block      = 'impl' generic_params? type ('for' type)? '{' fn_decl* '}'

const_decl      = 'const' IDENT ':' type '=' expr ';'

params          = param (',' param)*
param           = ('&' 'mut'? 'self') | ('&' 'self') | (IDENT ':' type ('=' expr)?)
return_type     = '->' type

type            = 'i8' | 'i16' | 'i32' | 'i64' | 'i128'
                | 'u8' | 'u16' | 'u32' | 'u64' | 'u128'
                | 'f32' | 'f64' | 'bool' | 'char' | 'String' | 'str'
                | '&' 'mut'? type
                | type '[]'
                | 'Map' '<' type ',' type '>'
                | 'Option' '<' type '>'
                | 'Result' '<' type (',' type)? '>'
                | 'Fn' '(' type_list? ')' '->' type
                | 'Ref' '<' type '>'
                | '(' type_list? ')'                       // tuple
                | IDENT generic_args?                      // named type
                | '()'                                     // unit

generic_params  = '<' IDENT (':' trait_bound ('+' trait_bound)*)? (',' ...)* '>'
generic_args    = '<' type (',' type)* '>'
trait_bound     = IDENT generic_args?

block           = '{' stmt* '}'
stmt            = let_stmt | expr_stmt | return_stmt | for_stmt | while_stmt | loop_stmt
let_stmt        = 'let' 'mut'? (IDENT | tuple_pattern) (':' type)? '=' expr ';'
expr_stmt       = expr ';'
return_stmt     = 'return' expr? ';'
for_stmt        = 'for' (IDENT | tuple_pattern) 'in' expr block
while_stmt      = 'while' expr block
loop_stmt       = 'loop' block

tuple_pattern   = '(' (IDENT | '_') (',' (IDENT | '_'))* ')'

expr            = assignment_expr
assignment_expr = pipe_expr ('=' | '+=' | '-=' | '*=' | '/=' | '%=') assignment_expr
                | pipe_expr
pipe_expr       = or_expr ('|>' call_tail)*
or_expr         = and_expr ('||' and_expr)*
and_expr        = eq_expr ('&&' eq_expr)*
eq_expr         = cmp_expr (('==' | '!=') cmp_expr)*
cmp_expr        = add_expr (('<' | '>' | '<=' | '>=' | '<=>') add_expr)*
add_expr        = mul_expr (('+' | '-') mul_expr)*
mul_expr        = unary_expr (('*' | '/' | '%') unary_expr)*
unary_expr      = ('-' | '!' | 'not' | '*' | '&' 'mut'?) unary_expr | postfix_expr
postfix_expr    = primary_expr (method_call | index | field | '?')*
method_call     = '.' IDENT generic_args? '(' args? ')'
index           = '[' expr ']'
field           = '.' (IDENT | INT)
primary_expr    = literal | ident_expr | call_expr | if_expr | match_expr
                | lambda | block | range | paren_expr | array_lit | map_lit
                | macro_call

ident_expr      = IDENT (('::' IDENT)* struct_init?)?
call_expr       = ident_expr '(' named_args? ')'
call_tail       = IDENT generic_args? '(' named_args? ')'
named_args      = named_arg (',' named_arg)* ','?
named_arg       = (IDENT ':')? expr

if_expr         = 'if' expr block ('else' 'if' expr block)* ('else' block)?
match_expr      = 'match' expr '{' match_arm (',' match_arm)* ','? '}'
match_arm       = pattern ('if' expr)? '=>' (expr | block)

lambda          = '|' lambda_params? '|' (expr | block)
lambda_params   = lambda_param (',' lambda_param)*
lambda_param    = IDENT (':' type)?

range           = expr '..' expr | expr '..=' expr

paren_expr      = '(' expr (',' expr)* ')'    // tuple if >1 expr
array_lit       = '[' (expr (',' expr)* ','?)? ']'
map_lit         = '#' '{' (expr ':' expr (',' expr ':' expr)* ','?)? '}'

struct_init     = '{' (IDENT (':' expr)? (',' IDENT (':' expr)?)* ','? ('..' expr)?)? '}'

pattern         = '_' | literal | IDENT | tuple_pattern
                | IDENT '::' IDENT pattern_payload?
                | IDENT '{' field_patterns '}'
                | pattern '@' IDENT
pattern_payload = '(' pattern (',' pattern)* ')' | '{' field_patterns '}'
field_patterns  = (IDENT (':' pattern)?)* (',' '..')?

attr            = '@' IDENT ('(' attr_args ')')?
attr_args       = attr_arg (',' attr_arg)*
attr_arg        = IDENT (':' literal)? | literal

macro_call      = IDENT '!' '(' macro_args ')'
macro_args      = (expr (',' expr)* ','?)?

literal         = INT_LIT | FLOAT_LIT | BOOL_LIT | CHAR_LIT | STR_LIT | TEMPLATE_LIT
```

---

*End of specification.*

*This document describes language version 0.1. All features described herein are intended for implementation. Features not described are not part of the language.*
