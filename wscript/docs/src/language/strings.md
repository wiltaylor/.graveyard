# Strings

The `String` type in SpiteScript is a heap-allocated, UTF-8 encoded, growable string. Strings are reference-counted -- assignment shares the reference rather than copying the data. Use `.clone()` when you need an independent copy.

## String Literals

String literals are enclosed in double quotes and support escape sequences:

```spite
let greeting = "hello, world";
let with_newline = "line one\nline two";
let with_tab = "col1\tcol2";
let with_quote = "she said \"hi\"";
```

### Escape Sequences

String literals support the same escape sequences as character literals:

| Escape | Meaning |
|--------|---------|
| `\n` | Newline |
| `\r` | Carriage return |
| `\t` | Tab |
| `\\` | Backslash |
| `\"` | Double quote |
| `\0` | Null character |
| `\u{HHHHHH}` | Unicode scalar by hex code point |

```spite
let path = "C:\\Users\\alice\\docs";
let emoji = "\u{1F980}";    // crab emoji
let null_terminated = "data\0";
```

## Template String Literals

Template strings use backticks and support `${expr}` interpolation. Any expression can appear inside `${}` -- it is fully evaluated at runtime:

```spite
let name = "Alice";
let age = 30;

let msg = `Hello, ${name}!`;
let info = `${name} is ${age} years old`;
let math = `2 + 2 = ${2 + 2}`;
```

Interpolated expressions can be arbitrarily complex:

```spite
let label = `Status: ${if ok { "success" } else { "failure" }}`;
let summary = `Found ${items.len()} items totaling ${items.sum()}`;
let formatted = `Price: $${(price * 100.0).round() / 100.0}`;
```

Template strings can span multiple lines:

```spite
let html = `
<div class="user">
    <h1>${user.name}</h1>
    <p>Email: ${user.email}</p>
</div>
`;
```

## String Concatenation

Strings can be concatenated with the `+` operator. This creates a new string:

```spite
let first = "hello";
let second = " world";
let combined = first + second;   // "hello world"
```

You can also use `+=` for appending to a mutable binding:

```spite
let mut result = "items: ";
for item in items {
    result += item + ", ";
}
```

## String Slicing

Use range syntax to extract a substring by byte offsets:

```spite
let s = "hello, world";
let hello = s[0..5];      // "hello"
let world = s[7..12];     // "world"
```

Slicing panics if the indices do not fall on a UTF-8 character boundary. For safe slicing of multi-byte characters, work with `.chars()` instead.

## String Methods

### Length and Emptiness

```spite
let s = "hello";

s.len()          // 5 (byte length, u64)
s.char_count()   // 5 (Unicode scalar count, u64)
s.is_empty()     // false
"".is_empty()    // true
```

For ASCII strings, `len()` and `char_count()` return the same value. They differ for strings containing multi-byte UTF-8 characters:

```spite
let emoji = "🦀🦀🦀";
emoji.len()          // 12 (each crab emoji is 4 bytes)
emoji.char_count()   // 3
```

### Searching

```spite
let s = "hello, world";

s.contains("world")        // true
s.contains("xyz")          // false

s.starts_with("hello")     // true
s.ends_with("world")       // true

s.find("world")            // Some(7) -- byte offset of first occurrence
s.find("xyz")              // None
```

### Replacing

```spite
let s = "hello, world";
let replaced = s.replace("world", "SpiteScript");
// "hello, SpiteScript"
```

`replace` replaces all occurrences and returns a new string:

```spite
let s = "aaa bbb aaa";
s.replace("aaa", "ccc")   // "ccc bbb ccc"
```

### Case Conversion

```spite
let s = "Hello, World";

s.to_uppercase()    // "HELLO, WORLD"
s.to_lowercase()    // "hello, world"
```

### Trimming

```spite
let s = "  hello  ";

s.trim()            // "hello"
s.trim_start()      // "hello  "
s.trim_end()        // "  hello"
```

### Splitting

```spite
let csv = "alice,bob,carol";

let names = csv.split(",");
// ["alice", "bob", "carol"] -- returns String[]

let pair = csv.split_once(",");
// Some(("alice", "bob,carol")) -- splits at first occurrence only
```

`split` always returns a `String[]`. `split_once` returns `Option<(String, String)>` -- `None` if the separator is not found:

```spite
let no_comma = "hello world";
no_comma.split_once(",")    // None
```

### Characters and Bytes

```spite
let s = "hello";

let chars = s.chars();   // ['h', 'e', 'l', 'l', 'o'] -- char[]
let bytes = s.bytes();   // [104, 101, 108, 108, 111] -- u8[]
```

### Parsing

The `parse` method converts a string to a numeric or boolean type. It returns a `Result`:

```spite
let n = "42".parse::<i32>();       // Ok(42)
let f = "3.14".parse::<f64>();     // Ok(3.14)
let b = "true".parse::<bool>();    // Ok(true)
let bad = "abc".parse::<i32>();    // Err(...)
```

Use `?` or `unwrap` to extract the parsed value:

```spite
fn parse_config(s: String) -> Result<i32> {
    let value = s.trim().parse::<i32>()?;
    return Ok(value);
}
```

### Repeating

```spite
let stars = "*".repeat(10);     // "**********"
let dashes = "-".repeat(40);    // 40 dashes
```

### Padding

```spite
let num = "42";

num.pad_start(5, '0')     // "00042"
num.pad_end(5, ' ')       // "42   "
num.pad_start(5, ' ')     // "   42"
```

`pad_start` pads the beginning of the string until it reaches the target length. `pad_end` pads the end. If the string is already at or above the target length, it is returned unchanged:

```spite
let long = "already long enough";
long.pad_start(5, ' ')    // "already long enough" (unchanged)
```

## Reference Counting

Because strings are reference-counted, assignment shares the same underlying data:

```spite
let a = "hello";
let b = a;          // b and a share the same string data (ref count = 2)
```

If you need an independent copy, use `.clone()`:

```spite
let a = "hello";
let b = a.clone();  // b is a separate copy
```

For most use cases, the shared reference behavior is transparent -- strings are effectively immutable values. String methods like `replace`, `to_uppercase`, and `trim` all return new strings rather than modifying the original.

## Complete Method Reference

| Method | Return Type | Description |
|--------|-------------|-------------|
| `len()` | `u64` | Byte length |
| `char_count()` | `u64` | Number of Unicode scalars |
| `is_empty()` | `bool` | True if zero length |
| `contains(sub)` | `bool` | Substring search |
| `starts_with(prefix)` | `bool` | Prefix check |
| `ends_with(suffix)` | `bool` | Suffix check |
| `find(sub)` | `Option<u64>` | Byte offset of first occurrence |
| `replace(from, to)` | `String` | Replace all occurrences |
| `to_uppercase()` | `String` | Uppercase copy |
| `to_lowercase()` | `String` | Lowercase copy |
| `trim()` | `String` | Strip leading and trailing whitespace |
| `trim_start()` | `String` | Strip leading whitespace |
| `trim_end()` | `String` | Strip trailing whitespace |
| `split(sep)` | `String[]` | Split on separator |
| `split_once(sep)` | `Option<(String, String)>` | Split at first separator |
| `chars()` | `char[]` | Array of Unicode scalars |
| `bytes()` | `u8[]` | Array of raw bytes |
| `parse::<T>()` | `Result<T>` | Parse to numeric or bool type |
| `repeat(n)` | `String` | Repeat n times |
| `pad_start(n, ch)` | `String` | Pad beginning to length n |
| `pad_end(n, ch)` | `String` | Pad end to length n |
| `s + other` | `String` | Concatenation (new string) |
| `s[start..end]` | `String` | Byte slice (panics on bad boundary) |
