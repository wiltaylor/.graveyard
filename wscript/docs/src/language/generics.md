# Generics

Generics let you write functions and data structures that work with any type. The
compiler generates specialized code for each concrete type used, so there is no
runtime overhead from generics.

## Generic Functions

Declare type parameters in angle brackets after the function name.

```
fn identity<T>(x: T) -> T {
    return x;
}

identity(42)         // T = i32
identity("hello")   // T = String
identity(true)       // T = bool
```

The compiler infers the type parameter from the argument. Explicit type arguments
are rarely needed.

### Multiple Type Parameters

```
fn pair<A, B>(a: A, b: B) -> (A, B) {
    return (a, b);
}

let p = pair(42, "hello");   // (i32, String)
```

### Generic Functions with Arrays

```
fn first<T>(items: T[]) -> Option<T> {
    if items.is_empty() {
        return None;
    }
    return Some(items[0].clone());
}

first([1, 2, 3])              // Some(1)
first(["a", "b"])              // Some("a")

let empty: i32[] = [];
first(empty)                   // None
```

### Combining Generics with Closures

```
fn zip_with<A, B, C>(a: A[], b: B[], f: Fn(A, B) -> C) -> C[] {
    return a.zip(b).map(|(x, y)| f(x, y)).collect();
}

let sums = zip_with([1, 2, 3], [10, 20, 30], |a, b| a + b);
// [11, 22, 33]

let labels = zip_with(
    ["item1", "item2"],
    [100, 200],
    |name, val| `${name}: ${val}`,
);
// ["item1: 100", "item2: 200"]
```

## Generic Structs

Structs can declare type parameters, making them reusable containers.

```
struct Wrapper<T> {
    value: T,
    label: String,
}

let w1 = Wrapper { value: 42, label: "number" };       // Wrapper<i32>
let w2 = Wrapper { value: "hi", label: "greeting" };   // Wrapper<String>
```

### Implementing Methods on Generic Structs

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

    fn len(&self) -> u64 {
        return self.items.len();
    }
}
```

Using the generic stack:

```
let mut ints: Stack<i32> = Stack::new();
ints.push(1);
ints.push(2);
ints.push(3);
ints.pop()     // Some(3)
ints.peek()    // Some(2)

let mut names: Stack<String> = Stack::new();
names.push("Alice");
names.push("Bob");
names.len()    // 2
```

### Structs with Multiple Type Parameters

```
struct Pair<A, B> {
    first:  A,
    second: B,
}

impl Pair<A, B> {
    fn new(first: A, second: B) -> Pair<A, B> {
        return Pair { first, second };
    }

    fn swap(&self) -> Pair<B, A> {
        return Pair { first: self.second.clone(), second: self.first.clone() };
    }
}

let p = Pair::new(1, "hello");
let swapped = p.swap();   // Pair<String, i32> { first: "hello", second: 1 }
```

## Trait Bounds

Constrain type parameters to types that implement specific traits. Without bounds,
you can only perform operations available on all types (assignment, passing as
arguments). Bounds unlock trait methods.

```
fn largest<T: Comparable>(a: T, b: T) -> T {
    if a.compare(&b) > 0 {
        return a;
    }
    return b;
}

largest(10, 20)         // 20
largest("abc", "xyz")   // "xyz"
```

Multiple bounds with `+`:

```
fn dedup_sorted<T: Comparable + Eq>(items: T[]) -> T[] {
    return items.sort().dedup();
}
```

Bounds on struct impl blocks constrain which instantiations get certain methods:

```
struct SortedList<T> {
    items: T[],
}

impl SortedList<T: Comparable> {
    fn insert(&mut self, item: T) {
        self.items.push(item);
        self.items = self.items.sort();
    }

    fn min(&self) -> Option<T> {
        return self.items.first();
    }

    fn max(&self) -> Option<T> {
        return self.items.last();
    }
}
```

## Monomorphisation

Wscript compiles generics using **monomorphisation**. Each unique combination of
type arguments produces a distinct, specialized copy of the function or struct in the
compiled WASM output.

For example, calling `identity<i32>` and `identity<String>` generates two separate
WASM functions -- one operating on `i32` values and one on `String` references. This
means:

- There is no boxing or type-erasure overhead at runtime.
- Generic code runs at the same speed as hand-written specialized code.
- Each new type instantiation adds to the compiled binary size.

This is the same strategy used by Rust. Trait bound violations are caught at compile
time at the call site, not deferred to runtime.

## Complete Example

A generic key-value cache with a maximum size:

```
struct Cache<K, V> {
    entries: (K, V)[],
    max_size: u64,
}

impl Cache<K: Eq, V> {
    fn new(max_size: u64) -> Cache<K, V> {
        return Cache { entries: [], max_size };
    }

    fn get(&self, key: K) -> Option<V> {
        return self.entries
            .find(|(k, _)| k == key)
            .map(|(_, v)| v);
    }

    fn put(&mut self, key: K, value: V) {
        // Remove existing entry with same key
        self.entries = self.entries
            .filter(|(k, _)| k != key)
            .collect();

        // Evict oldest if at capacity
        if self.entries.len() >= self.max_size {
            self.entries.remove(0);
        }

        self.entries.push((key, value));
    }

    fn len(&self) -> u64 {
        return self.entries.len();
    }
}

let mut cache: Cache<String, i32> = Cache::new(3);
cache.put("a", 1);
cache.put("b", 2);
cache.put("c", 3);
cache.get("b")        // Some(2)

cache.put("d", 4);    // evicts "a"
cache.get("a")        // None
cache.len()           // 3
```
