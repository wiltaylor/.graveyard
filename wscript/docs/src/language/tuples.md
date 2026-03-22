# Tuples

Tuples are fixed-size, ordered, heterogeneous collections. Each element can have a
different type. Tuples are heap-allocated and reference-counted.

## Syntax and Types

Create a tuple by listing values in parentheses, separated by commas. The type is
written as a parenthesized list of types.

```
let pair: (i32, String) = (42, "hello");
let triple = (1, true, 3.14f64);          // inferred: (i32, bool, f64)
let single_element = (42,);               // trailing comma for 1-element tuples
```

Type annotations are optional when the types can be inferred.

## The Unit Type

The empty tuple `()` is called the **unit type**. It represents the absence of a
meaningful value and is the implicit return type of functions that do not return
anything.

```
let unit = ();

fn greet(name: String) {
    print(`Hello, ${name}!`);
    // implicitly returns ()
}
```

## Accessing Elements

Access tuple elements with dot-syntax and a zero-based numeric index.

```
let pair = (42, "hello");

let number = pair.0;    // 42
let text = pair.1;      // "hello"
```

The index must be a literal integer known at compile time. Accessing an index
beyond the tuple's length is a compile error.

```
let triple = ("a", "b", "c");
let first = triple.0;     // "a"
let second = triple.1;    // "b"
let third = triple.2;     // "c"
```

## Destructuring

Destructure tuples in `let` bindings to extract all elements at once.

```
let (a, b) = (42, "hello");
// a = 42, b = "hello"

let (x, y, z) = (1.0, 2.0, 3.0);
// x = 1.0, y = 2.0, z = 3.0
```

Use `_` to discard elements you do not need:

```
let (first, _, third) = ("a", "b", "c");
// first = "a", third = "c", middle is discarded
```

The number of bindings must match the tuple length exactly.

## Tuples in Functions

Tuples are commonly used to return multiple values from a function.

```
fn min_max(items: i32[]) -> (i32, i32) {
    return (items.min(), items.max());
}

let (lo, hi) = min_max([3, 1, 4, 1, 5, 9]);
// lo = 1, hi = 9
```

Another example -- returning a status and a value:

```
fn divide_checked(a: f64, b: f64) -> (bool, f64) {
    if b == 0.0 {
        return (false, 0.0);
    }
    return (true, a / b);
}

let (ok, result) = divide_checked(10.0, 3.0);
if ok {
    print(`Result: ${result}`);
}
```

## Tuples in Match

Tuples can be used as the scrutinee in `match` expressions. This is useful for
branching on multiple values simultaneously.

```
let msg = match (ok, count) {
    (true,  0) => "ok but empty",
    (true,  _) => "ok with data",
    (false, _) => "error",
};
```

Combine tuple patterns with guards for more refined matching:

```
let category = match (role, level) {
    ("admin", _)             => "full access",
    ("user", n) if n >= 10   => "power user",
    ("user", _)              => "regular user",
    ("guest", _)             => "read only",
    _                        => "unknown",
};
```

## Tuples in Pipelines

Several pipeline operations produce tuples as their element type.

**`enumerate()`** yields `(u64, T)` pairs -- the index and the element:

```
let items = ["apple", "banana", "cherry"];
for (i, item) in items.enumerate() {
    print(`${i}: ${item}`);
}
// 0: apple
// 1: banana
// 2: cherry
```

**`zip()`** pairs elements from two sequences into `(A, B)` tuples:

```
let names = ["Alice", "Bob", "Carol"];
let scores = [95, 87, 92];

let pairs = names.zip(scores).collect();
// [("Alice", 95), ("Bob", 87), ("Carol", 92)]
```

**Collecting tuples into a Map**: when a pipeline produces `(K, V)` tuples,
`collect()` can gather them into a `Map<K, V>`:

```
let scores = [("Alice", 95), ("Bob", 87), ("Carol", 92)];

let score_map: Map<String, i32> = scores.collect();
// #{ "Alice": 95, "Bob": 87, "Carol": 92 }
```

**`partition()`** splits a collection into a tuple of two arrays:

```
let (evens, odds) = [1, 2, 3, 4, 5, 6].partition(|x| x % 2 == 0);
// evens = [2, 4, 6]
// odds = [1, 3, 5]
```

**`unzip()`** converts an array of pairs into a pair of arrays:

```
let pairs = [(1, "a"), (2, "b"), (3, "c")];
let (numbers, letters) = pairs.unzip();
// numbers = [1, 2, 3]
// letters = ["a", "b", "c"]
```
