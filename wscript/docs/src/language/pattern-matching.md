# Pattern Matching

Pattern matching in Wscript is performed with the `match` expression, `if let`, and
`while let`. Matches are exhaustive -- the compiler rejects any `match` that does not
cover every possible value of the scrutinee type.

## The `match` Expression

`match` is an expression: every arm must produce the same type, and the result can be
assigned to a binding.

```
let label = match status_code {
    200 => "ok",
    404 => "not found",
    500 => "server error",
    _   => "unknown",
};
```

Each arm has the form `pattern => expression`. Arms are evaluated top to bottom; the
first matching arm wins.

## Literal Patterns

Match against integer, float, boolean, string, or character literals.

```
match ch {
    'a' => "lowercase a",
    'A' => "uppercase a",
    _   => "something else",
}
```

## Range Patterns

A range pattern matches any value within the range. The `..` form is exclusive of the
upper bound; `..=` is inclusive.

```
match score {
    0       => "zero",
    1..50   => "low",
    50..=99 => "high",
    100     => "perfect",
    _       => "out of range",
}
```

## Enum Variant Patterns

Enums are the most common use of pattern matching. All three variant shapes are
supported: unit, tuple, and struct.

```
enum Shape {
    Circle(f64),
    Rectangle(f64, f64),
    Named { name: String },
    Point,
}

let description = match shape {
    Shape::Circle(r)       => `circle with radius ${r}`,
    Shape::Rectangle(w, h) => `${w} x ${h} rectangle`,
    Shape::Named { name }  => `shape called ${name}`,
    Shape::Point           => "a point",
};
```

Use `_` inside a variant to ignore fields you do not need:

```
match shape {
    Shape::Rectangle(w, _) => `width is ${w}`,
    _                      => "not a rectangle",
}
```

## Tuple Patterns

Destructure tuples directly inside a match arm.

```
let message = match (connected, retries) {
    (true, _)  => "online",
    (false, 0) => "offline, no retries left",
    (false, n) => `offline, ${n} retries remaining`,
};
```

## Guard Clauses

Add an `if` guard after the pattern to impose an additional condition. The guard has
access to any bindings introduced by the pattern.

```
match temperature {
    t if t < 0   => "freezing",
    0            => "zero",
    t if t > 100 => "boiling",
    _            => "moderate",
}
```

Guards do not affect exhaustiveness checking. You still need a wildcard or full
coverage even when guards are present.

## Binding with `@`

The `@` operator binds the matched value to a name while simultaneously testing the
pattern. This is useful when you need the whole value as well as its inner fields.

```
match shape {
    s @ Shape::Circle(_) => {
        print(`Got a circle: ${s.describe()}`);
        return s;
    },
    other => return other,
}
```

## Option Patterns

`Option<T>` values are matched with `Some` and `None`.

```
let display = match maybe_name {
    Some(name) => `Hello, ${name}`,
    None       => "Hello, stranger",
};
```

## Result Patterns

`Result<T, E>` values are matched with `Ok` and `Err`.

```
match parse_config(path) {
    Ok(config) => start(config),
    Err(e)     => print_err(`config error: ${e}`),
}
```

## `if let` Shorthand

When you only care about one variant, `if let` avoids a full match.

```
if let Some(val) = maybe_value {
    print(`Got: ${val}`);
}

if let Ok(data) = load_file("config.txt") {
    process(data);
}

if let Shape::Circle(r) = shape {
    print(`radius is ${r}`);
}
```

An optional `else` block handles the non-matching case:

```
if let Some(first) = items.first() {
    print(`first item: ${first}`);
} else {
    print("list is empty");
}
```

## `while let`

`while let` loops as long as the pattern continues to match. This is commonly used
to drain an iterator or repeatedly unwrap an `Option`.

```
let mut stack = [1, 2, 3, 4, 5];
while let Some(top) = stack.pop() {
    print(top);
}
```

## Wildcard `_`

The underscore `_` matches any value and discards it. Use it in positions where the
value is not needed.

```
match (x, y, z) {
    (_, 0, _) => "y is zero",
    (0, _, _) => "x is zero",
    _         => "neither is zero",
}
```

## Nesting Patterns

Patterns compose freely. You can nest enum, tuple, and Option/Result patterns to
match complex data in a single arm.

```
match result {
    Ok(Some(value)) => print(`got ${value}`),
    Ok(None)        => print("success but empty"),
    Err(e)          => print(`error: ${e}`),
}
```

## Exhaustiveness

The compiler verifies that every possible value of the scrutinee type is handled.
If coverage is incomplete, the compiler emits error E008 (non-exhaustive match) and
lists the missing variants. Adding a wildcard `_` arm satisfies exhaustiveness for
any type.

```
enum Color { Red, Green, Blue }

// Compile error E008: non-exhaustive match, missing Blue
match color {
    Color::Red   => "red",
    Color::Green => "green",
}

// Fix: add the missing variant or a wildcard
match color {
    Color::Red   => "red",
    Color::Green => "green",
    Color::Blue  => "blue",
}
```
