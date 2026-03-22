# Variables and Bindings

SpiteScript provides three ways to introduce named values: `let` bindings, `let mut` bindings, and `const` declarations. Bindings are block-scoped and immutable by default.

## `let` Bindings

The `let` keyword introduces an immutable binding. Once assigned, the value cannot be changed:

```spite
let x = 42;
let name = "Alice";
let items = [1, 2, 3];
```

Attempting to reassign an immutable binding is a compile error:

```spite
let x = 42;
x = 100;  // ERROR: cannot assign to immutable binding 'x'
```

## `let mut` -- Mutable Bindings

Add the `mut` keyword to allow reassignment:

```spite
let mut count = 0;
count = count + 1;
count += 1;
```

The `mut` keyword is also required to call methods that take `&mut self`:

```spite
let mut items = [1, 2, 3];
items.push(4);        // push requires &mut self
items[0] = 99;        // index assignment requires mut

let frozen = [1, 2, 3];
frozen.push(4);       // ERROR: cannot call &mut self method on immutable binding
```

## Type Annotations

Type annotations are optional when the compiler can infer the type. You can add them for clarity or when inference needs help:

```spite
let x = 42;              // inferred as i32
let y: f64 = 3.14;       // explicit type annotation
let mut z = 0;            // inferred as i32
let mut w: String = "";   // explicit type annotation
```

Type annotations are required in a few situations:

```spite
// Empty collections need annotations so the compiler knows the element type
let empty: i32[] = [];
let lookup: Map<String, i32> = #{};

// const declarations always require type annotations
const MAX: i32 = 1024;
```

The annotation syntax places the type after a colon, following the binding name:

```spite
let name: Type = value;
```

## `const` Declarations

Constants are compile-time values. They must have an explicit type annotation and their value must be a compile-time expression (literals and arithmetic on literals):

```spite
const MAX_SIZE: i32 = 1024;
const PI: f64 = 3.14159265358979;
const BUFFER_SIZE: u64 = 4 * 1024;
const GREETING: String = "hello";
```

Constants are inlined at every use site. They cannot be modified and do not occupy a runtime binding:

```spite
const LIMIT: i32 = 100;

fn check(n: i32) -> bool {
    return n <= LIMIT;  // LIMIT is replaced with 100 at compile time
}
```

Constants differ from immutable `let` bindings in that they are evaluated at compile time rather than at runtime, and they require explicit type annotations.

## Destructuring

Tuple bindings can be destructured in a `let` statement:

```spite
let pair = (42, "hello");
let (number, greeting) = pair;
// number is 42, greeting is "hello"
```

Use `_` to discard fields you do not need:

```spite
let triple = (1, true, 3.14);
let (first, _, third) = triple;
// first is 1, third is 3.14
```

Destructuring also works with `let mut`:

```spite
let mut pair = (0, 0);
let (mut x, mut y) = some_function();
x += 1;
y += 1;
```

The destructured shape must exactly match the tuple arity:

```spite
let triple = (1, 2, 3);
let (a, b) = triple;         // ERROR: expected 2 elements, found 3
let (a, b, c, d) = triple;   // ERROR: expected 4 elements, found 3
```

Destructuring in `let` bindings is limited to tuples in SpiteScript v0.1. Struct destructuring and array destructuring are not supported.

## Variable Shadowing

A new `let` binding with the same name as an existing one creates a new binding that shadows the old one. The old binding still exists but is no longer accessible by that name:

```spite
let x = 5;
let x = x + 1;    // new binding shadows the old one; x is now 6
let x = x * 2;    // shadows again; x is now 12
```

Shadowing is useful for transforming a value through several steps without needing distinct names:

```spite
let input = "  42  ";
let input = input.trim();
let input = input.parse::<i32>().unwrap();
// input is now the integer 42
```

Unlike mutation, shadowing creates an entirely new binding. The new binding can even have a different type:

```spite
let value = "100";          // String
let value = value.parse::<i32>().unwrap();  // i32
```

## Block Scoping

Bindings are scoped to the block in which they are declared. A block is delimited by `{` and `}`. Inner blocks can access bindings from outer blocks, but not the reverse:

```spite
let outer = 10;

{
    let inner = 20;
    let sum = outer + inner;  // outer is accessible here
    // sum is 30
}

// inner and sum are no longer accessible here
// print(inner);  // ERROR: undefined variable 'inner'
```

Shadowing within a block does not affect the outer binding:

```spite
let x = 5;
{
    let x = x * 2;   // shadows x within this block
    print(x);         // prints 10
}
print(x);             // prints 5 -- the outer x is unchanged
```

Block scoping applies to all control flow constructs:

```spite
for i in 0..10 {
    let squared = i * i;
    // squared is only available within the for body
}
// i and squared are not accessible here

if condition {
    let result = compute();
    // result is scoped to this if-branch
}
```

## Putting It All Together

Here is a more complete example demonstrating several binding concepts together:

```spite
const TAX_RATE: f64 = 0.08;

fn calculate_total(items: (String, f64)[]) -> f64 {
    let mut total = 0.0;

    for (name, price) in items {
        let discounted = if price > 100.0 {
            price * 0.9
        } else {
            price
        };

        total += discounted;
    }

    let total = total * (1.0 + TAX_RATE);  // shadow with final value
    return total;
}
```
