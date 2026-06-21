# Closures and Lambdas

Closures (also called lambdas) are anonymous functions that can capture values from
the surrounding scope. They are first-class values -- you can assign them to variables,
pass them to functions, and return them from functions.

## Lambda Syntax

There are two forms. The single-expression form has an implicit return:

```
|params| expr
```

The block form requires an explicit `return`:

```
|params| { statements; return value; }
```

Examples:

```
let double = |x| x * 2;
let add = |a, b| a + b;
let greet = |name| `Hello, ${name}!`;

let result = double(21);    // 42
let sum = add(3, 4);        // 7
```

## Type Inference for Parameters

Lambda parameter types and the return type are inferred from context. When a lambda
is passed to a function with a known `Fn(...)` signature, the parameter types flow in
automatically.

```
let numbers = [1, 2, 3, 4, 5];

// The compiler knows .filter takes Fn(i32) -> bool,
// so |x| is inferred as i32 and the condition as bool.
let evens = numbers.filter(|x| x % 2 == 0).collect();
```

You can add explicit type annotations when needed or for clarity:

```
let typed = |x: i64, y: i64| -> i64 { return x + y; };
```

## Multi-line Lambdas

Multi-line lambdas use a block body and require an explicit `return` statement, just
like regular functions.

```
let process = |x: i32| {
    let doubled = x * 2;
    let shifted = doubled + 1;
    return shifted;
};

let result = process(10);    // 21
```

Lambdas without a return value implicitly return `()`:

```
let log_item = |item| {
    print(`Processing: ${item}`);
};
```

## Closure Capture Rules

Closures capture variables from their defining scope. The capture semantics depend on
the type of the captured value:

**Primitive types** (`i32`, `f64`, `bool`, `char`, etc.) are captured **by copy**.
Changes to the original variable after the closure is defined do not affect the
captured copy, and vice versa.

```
let mut offset = 10;
let shift = |x| x + offset;    // captures a copy of offset (10)

offset = 20;                    // does NOT affect the closure
shift(5)                        // 15, not 25
```

**Heap types** (`String`, arrays, maps, structs, closures) are captured **by shared
reference**. The reference count is incremented. The closure and the outer scope see
the same underlying object.

```
let mut data = [1, 2, 3];
let count = || data.len();     // shares data's ref-counted allocation

data.push(4);                  // the closure sees this change
count()                        // 4
```

## Shared Mutable State with `Ref<T>`

When two or more closures need to read and write the same primitive value, wrap it in
`Ref<T>` -- a ref-counted mutable cell.

```
let counter = Ref::new(0);

let increment = || { counter.set(counter.get() + 1); };
let decrement = || { counter.set(counter.get() - 1); };
let get_count = || counter.get();

increment();
increment();
increment();
decrement();
get_count()    // 2
```

`Ref<T>` methods:

| Method | Description |
|--------|-------------|
| `Ref::new(value)` | Create a new ref cell |
| `r.get()` | Clone and return the inner value |
| `r.set(value)` | Replace the inner value |
| `r.update(\|v\| expr)` | Apply a function to the inner value in place |

The `update` method is convenient for read-modify-write operations:

```
let total = Ref::new(0);
let add_to_total = |n| { total.update(|v| v + n); };

add_to_total(10);
add_to_total(20);
total.get()    // 30
```

## Function Type Annotations

Function types are written as `Fn(ParamTypes) -> ReturnType`. Use them in parameter
lists, return types, and variable annotations.

```
fn apply_twice(f: Fn(i32) -> i32, x: i32) -> i32 {
    return f(f(x));
}

apply_twice(|x| x + 1, 0)    // 2
apply_twice(|x| x * 2, 3)    // 12
```

Functions that take no parameters: `Fn() -> T`. Functions that return nothing:
`Fn(A, B) -> ()` (though the `-> ()` is typically omitted in practice).

## Higher-Order Functions

Functions can return closures. The returned closure captures values from the
enclosing function's scope.

```
fn make_adder(n: i32) -> Fn(i32) -> i32 {
    return |x| x + n;
}

let add5 = make_adder(5);
let add10 = make_adder(10);

add5(3)     // 8
add10(3)    // 13
```

A more complex example -- a function that composes two functions:

```
fn compose<A, B, C>(f: Fn(A) -> B, g: Fn(B) -> C) -> Fn(A) -> C {
    return |x| g(f(x));
}

let double = |x: i32| x * 2;
let to_string = |x: i32| `${x}`;

let double_then_string = compose(double, to_string);
double_then_string(21)    // "42"
```

## Named Functions as Values

Named functions automatically coerce to `Fn(...)` types. You can pass them anywhere
a closure is expected.

```
fn square(x: i32) -> i32 {
    return x * x;
}

let numbers = [1, 2, 3, 4, 5];
let squares = numbers.map(square).collect();    // [1, 4, 9, 16, 25]
```

## Closures in Pipelines

Closures are used extensively in pipeline operations like `map`, `filter`, `fold`,
and others. The pipeline context provides full type inference for closure parameters.

```
let result = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
    .filter(|x| x % 2 == 0)
    .map(|x| x * x)
    .fold(0, |acc, x| acc + x);

// result = 4 + 16 + 36 + 64 + 100 = 220
```
