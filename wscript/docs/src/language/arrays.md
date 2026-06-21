# Arrays

Arrays are dynamic, heap-allocated, reference-counted sequences of a single element
type. They grow and shrink at runtime. The type syntax is `T[]`.

## Array Literals

Create arrays with square bracket syntax:

```
let numbers = [1, 2, 3, 4, 5];         // inferred: i32[]
let names: String[] = ["Alice", "Bob"]; // explicit type
let empty: f64[] = [];                  // empty array requires type annotation
```

The element type is inferred from the contents. An empty array literal `[]` requires
a type annotation or surrounding context that determines the element type.

## Indexing

Access elements by zero-based index. Out-of-bounds access panics at runtime.

```
let a = [10, 20, 30];
let first = a[0];           // 10
let last = a[a.len() - 1];  // 30
```

Set elements by index on a mutable array:

```
let mut a = [10, 20, 30];
a[1] = 42;
// a is now [10, 42, 30]
```

## Slicing with Ranges

Slice an array with a range expression to get a new array containing the selected
elements.

```
let a = [0, 1, 2, 3, 4, 5];
let middle = a[1..4];       // [1, 2, 3]   (exclusive upper bound)
let tail = a[3..a.len()];   // [3, 4, 5]
```

Slicing produces a new array that shares the ref-counted backing storage.

## Mutation Methods

These methods require the array binding to be declared `let mut`.

```
let mut items = [1, 2, 3];

items.push(4);              // append to end       -> [1, 2, 3, 4]
items.pop();                // remove last element -> Some(4)
items.insert(0, 99);        // insert at index     -> [99, 1, 2, 3]
items.remove(0);            // remove at index     -> 99, array is [1, 2, 3]
items[1] = 42;              // set by index        -> [1, 42, 3]
items.clear();              // remove all          -> []
```

`push` and `insert` grow the array. `pop` returns `Option<T>` (returns `None` on
an empty array). `remove` returns the removed element and panics if the index is
out of bounds.

## Extending and Concatenating

```
let mut a = [1, 2, 3];
let b = [4, 5, 6];

a.extend(b);               // mutates a: [1, 2, 3, 4, 5, 6]

let c = [1, 2] + [3, 4];   // new array: [1, 2, 3, 4]
```

`extend` appends all elements of another array in place. The `+` operator creates a
new array without modifying either operand.

## Query Methods

These methods do not mutate the array.

| Method | Return type | Description |
|--------|-------------|-------------|
| `a.len()` | `u64` | Number of elements |
| `a.is_empty()` | `bool` | True if length is zero |
| `a.contains(val)` | `bool` | True if any element equals `val` |
| `a.first()` | `Option<T>` | First element, or `None` if empty |
| `a.last()` | `Option<T>` | Last element, or `None` if empty |
| `a.get(i)` | `Option<T>` | Element at index, or `None` if out of bounds |

```
let a = [10, 20, 30];

a.len()              // 3
a.is_empty()         // false
a.contains(20)       // true
a.first()            // Some(10)
a.last()             // Some(30)
a.get(5)             // None
```

## Transformation Methods

These methods return new arrays without modifying the original.

| Method | Description |
|--------|-------------|
| `a.clone()` | Deep copy of the array |
| `a.reverse()` | New array with elements in reverse order |
| `a.sort()` | New sorted array (`T` must implement `Comparable`) |
| `a.sort_by(\|a, b\| a <=> b)` | New array sorted by a custom comparator |
| `a.dedup()` | New array with consecutive duplicates removed |
| `a.join(sep)` | Join elements into a `String` (elements must be `String`) |

```
let a = [3, 1, 4, 1, 5, 9];

let sorted = a.sort();          // [1, 1, 3, 4, 5, 9]
let reversed = a.reverse();     // [9, 5, 1, 4, 1, 3]
let deduped = sorted.dedup();   // [1, 3, 4, 5, 9]

let words = ["hello", "world"];
let sentence = words.join(" "); // "hello world"
```

Custom sorting with `sort_by`:

```
let names = ["Charlie", "Alice", "Bob"];
let by_length = names.sort_by(|a, b| a.len() <=> b.len());
// ["Bob", "Alice", "Charlie"]
```

## Multidimensional Arrays

Nest array types to create matrices and higher-dimensional structures.

```
let matrix: i32[][] = [
    [1, 2, 3],
    [4, 5, 6],
    [7, 8, 9],
];

let val = matrix[1][2];    // 6

for row in matrix {
    for cell in row {
        print(cell);
    }
}
```

## Pipeline Methods

All pipeline and iterator operators are available on arrays. Arrays are the most
common starting point for a pipeline chain. See the
[Pipelines and Iterators](pipelines.md) page for the full list.

```
let top_scores = scores
    .filter(|s| s > 50)
    .sort_by(|a, b| b <=> a)
    .take(3)
    .collect();
```

## Reference Semantics

Arrays are ref-counted. Assignment shares the reference -- both bindings point to the
same underlying data.

```
let a = [1, 2, 3];
let b = a;           // b and a share the same array
```

Use `.clone()` to create an independent copy:

```
let a = [1, 2, 3];
let b = a.clone();   // b is a separate array
```
