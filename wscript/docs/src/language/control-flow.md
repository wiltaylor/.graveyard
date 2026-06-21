# Control Flow

Wscript provides `if`/`else`, `match`, `for`, `while`, and `loop` constructs. Several of these can be used as both statements and expressions.

## `if` / `else if` / `else`

The basic `if` statement:

```wscript
if temperature > 100 {
    print("too hot!");
}
```

With `else if` and `else` branches:

```wscript
if score >= 90 {
    print("A");
} else if score >= 80 {
    print("B");
} else if score >= 70 {
    print("C");
} else {
    print("F");
}
```

Braces are always required around the body of each branch. There is no single-statement shorthand.

### `if` as an Expression

When used as an expression, `if` produces a value. All branches must produce the same type, and the `else` branch is required:

```wscript
let label = if x > 0 { "positive" } else if x < 0 { "negative" } else { "zero" };
```

```wscript
let abs_value = if n >= 0 { n } else { -n };
```

```wscript
let message = if count == 1 {
    "1 item"
} else {
    `${count} items`
};
```

When used as an expression, the last expression in each branch block is the value of that branch (this is the expression context exception to the explicit `return` rule):

```wscript
let discount = if is_member {
    0.20
} else if order_total > 100.0 {
    0.10
} else {
    0.0
};
```

## `match`

The `match` expression tests a value against a series of patterns. It is exhaustive -- the compiler requires that all possible values are covered:

```wscript
match direction {
    Direction::North => print("going north"),
    Direction::South => print("going south"),
    Direction::East  => print("going east"),
    Direction::West  => print("going west"),
}
```

### `match` as an Expression

`match` produces a value. All arms must return the same type:

```wscript
let description = match status_code {
    200 => "OK",
    404 => "Not Found",
    500 => "Internal Server Error",
    _   => "Unknown",
};
```

### Literal Patterns

```wscript
match x {
    0 => print("zero"),
    1 => print("one"),
    _ => print("other"),
}
```

### Range Patterns

```wscript
let category = match age {
    0..13  => "child",
    13..20 => "teenager",
    20..65 => "adult",
    _      => "senior",
};
```

### Tuple Patterns

```wscript
let msg = match (ok, count) {
    (true,  0) => "ok but empty",
    (true,  _) => "ok with data",
    (false, _) => "error",
};
```

### Guard Clauses

Add an `if` guard to a pattern for additional conditions:

```wscript
let description = match n {
    n if n < 0   => "negative",
    0            => "zero",
    n if n > 100 => "large",
    _            => "small positive",
};
```

### Enum Variant Patterns

```wscript
match shape {
    Shape::Circle(r)       => r * r * 3.14159,
    Shape::Rectangle(w, h) => w * h,
    Shape::Named { name }  => 0.0,
    Shape::Point           => 0.0,
}
```

### `Option` and `Result` Patterns

```wscript
match maybe_value {
    Some(val) => print(`got: ${val}`),
    None      => print("nothing"),
}

match load_file("config.txt") {
    Ok(data) => process(data),
    Err(e)   => print(`error: ${e}`),
}
```

### Binding with `@`

Bind the matched value to a name with `@`:

```wscript
match shape {
    s @ Shape::Circle(_) => {
        print(`matched a circle: ${s.describe()}`);
    },
    other => {
        print(`something else: ${other.describe()}`);
    },
}
```

### `if let`

`if let` is shorthand for matching a single pattern:

```wscript
if let Some(val) = maybe_value {
    print(`got: ${val}`);
}

if let Ok(data) = load_file("config.txt") {
    process(data);
}
```

### `while let`

`while let` loops as long as the pattern matches:

```wscript
while let Some(item) = iter.next() {
    process(item);
}
```

For full pattern matching details, see the [Pattern Matching](../pattern-matching.md) section of the specification.

## `for` Loops

`for` iterates over any iterable value -- arrays, maps, ranges, and pipeline results.

### Iterating Over Arrays

```wscript
let names = ["Alice", "Bob", "Carol"];

for name in names {
    print(`Hello, ${name}!`);
}
```

### Iterating Over Maps

Maps iterate as `(key, value)` tuples:

```wscript
let scores = #{ "alice": 95, "bob": 87, "carol": 92 };

for (name, score) in scores {
    print(`${name}: ${score}`);
}
```

### Iterating with Index

Use `.enumerate()` to get `(index, value)` pairs:

```wscript
let items = ["a", "b", "c"];

for (index, item) in items.enumerate() {
    print(`[${index}] ${item}`);
}
// [0] a
// [1] b
// [2] c
```

### Ranges

The `..` operator creates an exclusive range. The `..=` operator creates an inclusive range:

```wscript
// Exclusive range: 0, 1, 2, ..., 9
for i in 0..10 {
    print(i);
}

// Inclusive range: 0, 1, 2, ..., 10
for i in 0..=10 {
    print(i);
}
```

### Stepping

Use `.step_by()` to skip elements:

```wscript
// 0, 5, 10, 15, ..., 95
for i in (0..100).step_by(5) {
    print(i);
}
```

### Counting Down

Use `.reverse()` for descending iteration:

```wscript
// 10, 9, 8, ..., 1
for i in (1..=10).reverse() {
    print(i);
}
```

## `while` Loops

`while` repeats as long as its condition is true:

```wscript
let mut count = 0;

while count < 10 {
    print(count);
    count += 1;
}
```

```wscript
let mut input = read_line();

while input != "quit" {
    process(input);
    input = read_line();
}
```

## `loop`

`loop` creates an infinite loop. It must be exited with `break`:

```wscript
loop {
    let input = read_line();
    if input == "quit" {
        break;
    }
    process(input);
}
```

### `loop` as an Expression

`loop` can produce a value via `break`:

```wscript
let result = loop {
    let val = compute();
    if val > 100 {
        break val;
    }
};
```

The `break` with a value is only valid inside `loop`, not inside `for` or `while`.

### Retry Pattern

`loop` is useful for retry logic:

```wscript
let data = loop {
    match fetch_data() {
        Ok(d)  => break d,
        Err(e) => {
            print(`retrying: ${e}`);
            sleep(1000);
        },
    }
};
```

## `break` and `continue`

`break` exits the innermost loop. `continue` skips to the next iteration:

```wscript
for i in 0..100 {
    if i % 2 == 0 {
        continue;   // skip even numbers
    }
    if i > 20 {
        break;      // stop after 20
    }
    print(i);
}
// prints: 1, 3, 5, 7, 9, 11, 13, 15, 17, 19
```

`break` and `continue` work in `for`, `while`, and `loop`. They affect the innermost enclosing loop:

```wscript
for row in 0..10 {
    for col in 0..10 {
        if col > row {
            break;      // breaks inner loop only
        }
        print(`(${row}, ${col})`);
    }
    // execution continues here after inner break
}
```

## Nesting Control Flow

Control flow constructs compose naturally:

```wscript
fn find_first_match(grid: i32[][], target: i32) -> Option<(u64, u64)> {
    for (row_idx, row) in grid.enumerate() {
        for (col_idx, val) in row.enumerate() {
            if val == target {
                return Some((row_idx, col_idx));
            }
        }
    }
    return None;
}
```

```wscript
fn process_commands(commands: String[]) -> Result<()> {
    for cmd in commands {
        match cmd.split_once(" ") {
            Some(("add", rest))    => add_item(rest)?,
            Some(("remove", rest)) => remove_item(rest)?,
            Some(("list", _))      => list_items(),
            None if cmd == "quit"  => break,
            _                      => print(`unknown command: ${cmd}`),
        }
    }
    return Ok(());
}
```

## Summary

| Construct | As Statement | As Expression | Notes |
|-----------|:---:|:---:|-------|
| `if`/`else` | Yes | Yes | Expression form requires `else` and matching types |
| `match` | Yes | Yes | Always exhaustive; all arms must match types |
| `for` | Yes | No | Iterates arrays, maps, ranges, pipelines |
| `while` | Yes | No | Condition checked before each iteration |
| `loop` | Yes | Yes | Expression form uses `break value` |
| `break` | Yes | -- | Exits innermost loop; value only in `loop` |
| `continue` | Yes | -- | Skips to next iteration of innermost loop |
