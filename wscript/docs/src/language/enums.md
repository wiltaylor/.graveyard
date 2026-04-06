# Enums

Enums in Wscript define types that can be one of several named variants. Variants can carry no data, tuple data, or named-field data. Enums support methods through `impl` blocks and are a natural fit for pattern matching.

## Basic Enums

A simple enum with unit variants (no associated data):

```wscript
enum Direction {
    North,
    South,
    East,
    West,
}

let d = Direction::North;
```

Unit enums are useful for representing a fixed set of options:

```wscript
enum Color {
    Red,
    Green,
    Blue,
    Yellow,
}

enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}
```

Variants are accessed via the `::` path separator:

```wscript
let level = LogLevel::Warn;
let color = Color::Blue;
```

## Data-Carrying Enums

Enum variants can carry data. Wscript supports three variant kinds in a single enum.

### Tuple Variants

A variant with positional data fields:

```wscript
enum Shape {
    Circle(f64),               // one field: radius
    Rectangle(f64, f64),       // two fields: width, height
}

let circle = Shape::Circle(5.0);
let rect = Shape::Rectangle(3.0, 4.0);
```

### Struct Variants

A variant with named fields:

```wscript
enum Event {
    Click { x: i32, y: i32, button: String },
    KeyPress { key: char, modifiers: String[] },
    Resize { width: u32, height: u32 },
}

let evt = Event::Click { x: 100, y: 200, button: "left" };
let key = Event::KeyPress { key: 'a', modifiers: ["ctrl"] };
```

### Mixed Variant Kinds

A single enum can mix unit, tuple, and struct variants:

```wscript
enum Shape {
    Circle(f64),                   // tuple variant
    Rectangle(f64, f64),           // tuple variant
    Named { name: String },        // struct variant
    Point,                         // unit variant
}

let s1 = Shape::Circle(5.0);
let s2 = Shape::Named { name: "triangle" };
let s3 = Shape::Point;
```

## Pattern Matching on Enums

The `match` expression is the primary way to work with enum values. The compiler requires that all variants are covered (exhaustive matching):

```wscript
fn describe(d: Direction) -> String {
    return match d {
        Direction::North => "heading north",
        Direction::South => "heading south",
        Direction::East  => "heading east",
        Direction::West  => "heading west",
    };
}
```

### Matching Tuple Variants

Bind the inner fields to names:

```wscript
fn area(shape: Shape) -> f64 {
    return match shape {
        Shape::Circle(r)       => r * r * 3.14159,
        Shape::Rectangle(w, h) => w * h,
        Shape::Named { .. }    => 0.0,
        Shape::Point           => 0.0,
    };
}
```

### Matching Struct Variants

Bind fields by name. Use `..` to ignore remaining fields:

```wscript
fn handle_event(event: Event) {
    match event {
        Event::Click { x, y, button } => {
            print(`click at (${x}, ${y}) with ${button}`);
        },
        Event::KeyPress { key, .. } => {
            print(`key pressed: ${key}`);
        },
        Event::Resize { width, height } => {
            print(`resized to ${width}x${height}`);
        },
    }
}
```

### Using `_` and Wildcards

Use `_` to match any value without binding it, or as a catch-all arm:

```wscript
fn is_circle(shape: Shape) -> bool {
    return match shape {
        Shape::Circle(_) => true,
        _                => false,
    };
}
```

### Guard Clauses

Add conditions with `if`:

```wscript
fn describe_shape(shape: Shape) -> String {
    return match shape {
        Shape::Circle(r) if r > 100.0 => "large circle",
        Shape::Circle(r) if r > 10.0  => "medium circle",
        Shape::Circle(_)               => "small circle",
        Shape::Rectangle(w, h) if w == h => "square",
        Shape::Rectangle(_, _)         => "rectangle",
        _                              => "other",
    };
}
```

### `if let` with Enums

For matching a single variant, `if let` is more concise than a full `match`:

```wscript
if let Shape::Circle(radius) = shape {
    print(`circle with radius ${radius}`);
}

if let Event::Click { x, y, .. } = event {
    handle_click(x, y);
}
```

For a complete guide to patterns, including `@` bindings and `while let`, see the [Control Flow](control-flow.md) page.

## Enum Methods

Define methods on enums with `impl` blocks, just like structs:

```wscript
enum Shape {
    Circle(f64),
    Rectangle(f64, f64),
    Named { name: String },
    Point,
}

impl Shape {
    fn area(&self) -> f64 {
        return match self {
            Shape::Circle(r)       => r * r * 3.14159,
            Shape::Rectangle(w, h) => w * h,
            Shape::Named { .. }    => 0.0,
            Shape::Point           => 0.0,
        };
    }

    fn describe(&self) -> String {
        return match self {
            Shape::Circle(r)        => `Circle(radius=${r})`,
            Shape::Rectangle(w, h)  => `Rectangle(${w}x${h})`,
            Shape::Named { name }   => `Named(${name})`,
            Shape::Point            => "Point",
        };
    }

    fn is_zero_area(&self) -> bool {
        return self.area() == 0.0;
    }
}

let s = Shape::Circle(5.0);
print(s.describe());        // "Circle(radius=5.0)"
print(s.area());            // 78.53975
print(s.is_zero_area());   // false
```

### Static Methods on Enums

Enums can also have static methods for constructing commonly used variants:

```wscript
impl Shape {
    fn unit_circle() -> Shape {
        return Shape::Circle(1.0);
    }

    fn square(side: f64) -> Shape {
        return Shape::Rectangle(side, side);
    }
}

let uc = Shape::unit_circle();
let sq = Shape::square(5.0);
```

## Built-in Enums: `Option` and `Result`

Wscript's `Option` and `Result` types are enums. They follow the same patterns:

### `Option<T>`

```wscript
let some_val: Option<i32> = Some(42);
let no_val: Option<i32> = None;

match some_val {
    Some(n) => print(`got ${n}`),
    None    => print("nothing"),
}
```

### `Result<T, E>`

```wscript
let ok_val: Result<i32, String> = Ok(42);
let err_val: Result<i32, String> = Err("something went wrong");

match ok_val {
    Ok(n)  => print(`success: ${n}`),
    Err(e) => print(`error: ${e}`),
}
```

These types are used throughout Wscript for optional values and error handling. See the error handling section of the specification for details on `?`, `@error`, and related features.

## Complete Example

Here is a complete example showing an enum with mixed variant types, methods, and pattern matching:

```wscript
enum Expression {
    Number(f64),
    Add(Expression, Expression),
    Multiply(Expression, Expression),
    Variable { name: String },
}

impl Expression {
    fn evaluate(&self, vars: Map<String, f64>) -> Result<f64> {
        return match self {
            Expression::Number(n) => Ok(n),
            Expression::Add(left, right) => {
                let l = left.evaluate(vars.clone())?;
                let r = right.evaluate(vars)?;
                return Ok(l + r);
            },
            Expression::Multiply(left, right) => {
                let l = left.evaluate(vars.clone())?;
                let r = right.evaluate(vars)?;
                return Ok(l * r);
            },
            Expression::Variable { name } => {
                return match vars.get(name) {
                    Some(val) => Ok(val),
                    None      => Err(error!("undefined variable: {name}")),
                };
            },
        };
    }

    fn to_string(&self) -> String {
        return match self {
            Expression::Number(n)          => `${n}`,
            Expression::Add(l, r)          => `(${l.to_string()} + ${r.to_string()})`,
            Expression::Multiply(l, r)     => `(${l.to_string()} * ${r.to_string()})`,
            Expression::Variable { name }  => name.clone(),
        };
    }
}

let expr = Expression::Add(
    Expression::Number(2.0),
    Expression::Multiply(
        Expression::Variable { name: "x" },
        Expression::Number(3.0),
    ),
);

let vars = #{ "x": 5.0 };
let result = expr.evaluate(vars);   // Ok(17.0)
print(expr.to_string());            // "(2.0 + (x * 3.0))"
```
