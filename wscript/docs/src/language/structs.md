# Structs

Structs are Wscript's primary way to define custom data types with named fields. They are heap-allocated, reference-counted, and support methods through `impl` blocks.

## Declaration

A struct declaration defines a type with named, typed fields:

```wscript
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

Each field must have an explicit type annotation. Fields are separated by commas; a trailing comma is permitted.

## Generic Structs

Structs can be parameterized over one or more type variables:

```wscript
struct Pair<A, B> {
    first:  A,
    second: B,
}

struct Wrapper<T> {
    value: T,
    label: String,
}

struct Stack<T> {
    items: T[],
}
```

Type parameters are specified in angle brackets after the struct name. They can be used as field types throughout the struct body.

When constructing or annotating a generic struct, provide concrete type arguments:

```wscript
let p: Pair<i32, String> = Pair { first: 42, second: "hello" };
let w = Wrapper { value: 3.14, label: "pi" };  // Wrapper<f64> inferred
```

## Construction

### Full Construction

Specify all fields by name:

```wscript
let p = Point { x: 3.0, y: 4.0 };

let user = User {
    id: 1,
    name: "Alice",
    email: "alice@example.com",
    tags: ["admin", "active"],
};
```

Every field must be provided. Missing fields are a compile error.

### Shorthand Construction

When a local variable has the same name as a field, you can omit the `: value` part:

```wscript
let x = 3.0;
let y = 4.0;
let p = Point { x, y };    // equivalent to Point { x: x, y: y }
```

```wscript
let name = "Alice";
let email = "alice@example.com";
let user = User {
    id: 1,
    name,           // shorthand for name: name
    email,          // shorthand for email: email
    tags: [],
};
```

### Update Syntax

Create a new struct by copying fields from an existing one and overriding specific fields with `..`:

```wscript
let p1 = Point { x: 1.0, y: 2.0 };
let p2 = Point { x: 10.0, ..p1 };   // p2.x = 10.0, p2.y = 2.0

let alice = User {
    id: 1,
    name: "Alice",
    email: "alice@example.com",
    tags: ["admin"],
};

let bob = User {
    id: 2,
    name: "Bob",
    email: "bob@example.com",
    ..alice    // copies tags from alice
};
```

The `..source` must appear at the end of the field list. It fills in any fields not explicitly set.

## Field Access

Access fields with dot notation:

```wscript
let p = Point { x: 3.0, y: 4.0 };
let x_val = p.x;     // 3.0
let y_val = p.y;     // 4.0
```

## Field Assignment

Assign directly to a struct field with `=`, `+=`, `-=`, etc. The receiver must be a `let mut` binding (or reached through another mutable path such as `self` inside a `&mut self` method, or a struct-typed top-level global):

```wscript
let mut p = Point { x: 3.0, y: 4.0 };
p.x = 10.0;
p.y += 1.0;
```

Field assignment also works on struct-typed top-level globals:

```wscript
struct PlayerState { hp: i32, score: i32 }

let mut world: PlayerState = PlayerState { hp: 50, score: 0 };

@export
fn damage(amount: i32) -> i32 {
    world.hp = world.hp - amount;
    return world.hp;
}
```

Assigning to a field of an immutable binding is a compile error.

## `impl` Blocks

Methods are defined in `impl` blocks, separate from the struct declaration. You can have multiple `impl` blocks for the same type:

```wscript
impl Point {
    fn new(x: f64, y: f64) -> Point {
        return Point { x, y };
    }

    fn distance_from_origin(&self) -> f64 {
        return (self.x * self.x + self.y * self.y).sqrt();
    }

    fn translate(&mut self, dx: f64, dy: f64) {
        self.x += dx;
        self.y += dy;
    }

    fn translated(&self, dx: f64, dy: f64) -> Point {
        return Point { x: self.x + dx, y: self.y + dy };
    }
}
```

### Static Methods

Methods without a `self` parameter are static. They are called on the type with `::`:

```wscript
impl Point {
    fn new(x: f64, y: f64) -> Point {
        return Point { x, y };
    }

    fn origin() -> Point {
        return Point { x: 0.0, y: 0.0 };
    }
}

let p = Point::new(3.0, 4.0);
let o = Point::origin();
```

Static methods are often used as constructors.

### `&self` Methods

Methods taking `&self` have read-only access to the struct. They are called with dot notation on any binding (mutable or immutable):

```wscript
impl Point {
    fn distance_from_origin(&self) -> f64 {
        return (self.x * self.x + self.y * self.y).sqrt();
    }

    fn distance_to(&self, other: Point) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        return (dx * dx + dy * dy).sqrt();
    }

    fn to_string(&self) -> String {
        return `(${self.x}, ${self.y})`;
    }
}

let p = Point::new(3.0, 4.0);
let dist = p.distance_from_origin();    // 5.0
let s = p.to_string();                  // "(3.0, 4.0)"
```

### `&mut self` Methods

Methods taking `&mut self` can modify the struct's fields. The caller must have a `let mut` binding:

```wscript
impl Point {
    fn translate(&mut self, dx: f64, dy: f64) {
        self.x += dx;
        self.y += dy;
    }

    fn scale(&mut self, factor: f64) {
        self.x *= factor;
        self.y *= factor;
    }

    fn reset(&mut self) {
        self.x = 0.0;
        self.y = 0.0;
    }
}

let mut p = Point::new(3.0, 4.0);
p.translate(1.0, -1.0);    // p is now (4.0, 3.0)
p.scale(2.0);               // p is now (8.0, 6.0)

let frozen = Point::new(1.0, 2.0);
frozen.translate(1.0, 0.0); // ERROR: cannot call &mut self method on immutable binding
```

## `self` Reference Rules

Wscript uses reference counting rather than borrow checking. The `&self` and `&mut self` distinctions are enforced by the type checker as a contract, not as runtime exclusive borrows:

- **`&self`** -- shared, read-only access. Works on any binding.
- **`&mut self`** -- mutable access. Requires a `let mut` binding.
- **No `self` (static)** -- no receiver. Called as `TypeName::method(args)`.

Because there is no borrow checker, two `&mut self` calls interleaved on the same object are permitted at runtime. The mutation ordering follows evaluation order.

## Generic `impl` Blocks

When implementing methods for a generic struct, include the type parameters:

```wscript
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

    fn len(&self) -> u64 {
        return self.items.len();
    }
}

let mut s: Stack<i32> = Stack::new();
s.push(1);
s.push(2);
s.push(3);
let top = s.pop();      // Some(3)
let peek = s.peek();    // Some(2)
let empty = s.is_empty(); // false
```

## Complete Example

Here is a complete example showing struct declaration, multiple `impl` blocks, and various method types:

```wscript
struct Rectangle {
    width: f64,
    height: f64,
}

impl Rectangle {
    fn new(width: f64, height: f64) -> Rectangle {
        return Rectangle { width, height };
    }

    fn square(side: f64) -> Rectangle {
        return Rectangle { width: side, height: side };
    }

    fn area(&self) -> f64 {
        return self.width * self.height;
    }

    fn perimeter(&self) -> f64 {
        return 2.0 * (self.width + self.height);
    }

    fn is_square(&self) -> bool {
        return self.width == self.height;
    }
}

impl Rectangle {
    fn scale(&mut self, factor: f64) {
        self.width *= factor;
        self.height *= factor;
    }

    fn scaled(&self, factor: f64) -> Rectangle {
        return Rectangle {
            width: self.width * factor,
            height: self.height * factor,
        };
    }

    fn contains(&self, other: Rectangle) -> bool {
        return self.width >= other.width && self.height >= other.height;
    }
}

let mut rect = Rectangle::new(10.0, 5.0);
let area = rect.area();            // 50.0
let perim = rect.perimeter();      // 30.0

rect.scale(2.0);
// rect is now 20.0 x 10.0

let small = Rectangle::square(3.0);
let big = rect.contains(small);    // true

let doubled = small.scaled(2.0);   // 6.0 x 6.0 -- original unchanged
```
