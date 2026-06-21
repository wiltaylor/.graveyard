# Pipelines and Iterators

Pipelines are Wscript's primary mechanism for functional data transformation.
They are **lazy** -- no intermediate collection is allocated until a terminal
operation is called. The pipe operator `|>` and method chaining syntax are equivalent.

## Pipe Operator

The `|>` operator passes the left-hand value as the receiver to the right-hand
function call. These two forms are interchangeable:

```
// Pipe operator style
let result = source
    |> filter(|x| x > 0)
    |> map(|x| x * 2)
    |> collect();

// Method chaining style
let result = source
    .filter(|x| x > 0)
    .map(|x| x * 2)
    .collect();
```

Use whichever reads better in context. Both produce identical compiled code.

## Transformation Operators

Transform each element in the pipeline without changing the number of elements (except
`flat_map` and `flatten`).

| Operator | Description |
|----------|-------------|
| `.map(\|x\| expr)` | Transform each element |
| `.flat_map(\|x\| array_expr)` | Transform each element into an array, then flatten one level |
| `.flatten()` | Flatten `T[][]` into `T[]` |
| `.inspect(\|x\| side_effect)` | Peek at each element without consuming; useful for debugging |

```
let doubled = [1, 2, 3].map(|x| x * 2).collect();
// [2, 4, 6]

let words = ["hello world", "foo bar"]
    .flat_map(|s| s.split(" "))
    .collect();
// ["hello", "world", "foo", "bar"]

let nested = [[1, 2], [3, 4], [5]];
let flat = nested.flatten().collect();
// [1, 2, 3, 4, 5]

// inspect for debugging -- does not alter the pipeline
[1, 2, 3]
    .inspect(|x| print(`before filter: ${x}`))
    .filter(|x| x > 1)
    .inspect(|x| print(`after filter: ${x}`))
    .collect();
```

## Filtering Operators

Control which elements pass through the pipeline.

| Operator | Description |
|----------|-------------|
| `.filter(\|x\| condition)` | Keep elements where condition is true |
| `.take(n)` | Keep only the first `n` elements |
| `.skip(n)` | Skip the first `n` elements |
| `.take_while(\|x\| condition)` | Take while condition holds, stop at first false |
| `.skip_while(\|x\| condition)` | Skip while condition holds, take everything after |
| `.distinct()` | Remove duplicates (`T` must implement `Eq + Hash`) |

```
let evens = [1, 2, 3, 4, 5, 6].filter(|x| x % 2 == 0).collect();
// [2, 4, 6]

let first_three = (0..100).take(3).collect();
// [0, 1, 2]

let unique = [1, 2, 2, 3, 3, 3].distinct().collect();
// [1, 2, 3]

let after_zeros = [0, 0, 0, 1, 2, 3].skip_while(|x| x == 0).collect();
// [1, 2, 3]
```

## Aggregation Operators (Terminal)

These consume the entire pipeline and produce a single value.

| Operator | Return | Description |
|----------|--------|-------------|
| `.sum()` | `T` | Sum of all elements (numeric types) |
| `.product()` | `T` | Product of all elements |
| `.count()` | `u64` | Number of elements |
| `.min()` | `Option<T>` | Smallest element |
| `.max()` | `Option<T>` | Largest element |
| `.min_by(\|x\| key)` | `Option<T>` | Element with smallest key |
| `.max_by(\|x\| key)` | `Option<T>` | Element with largest key |
| `.fold(init, \|acc, x\| expr)` | `A` | Accumulate with initial value |
| `.reduce(\|a, b\| expr)` | `Option<T>` | Accumulate without initial value |

```
[1, 2, 3, 4, 5].sum()          // 15
[1, 2, 3, 4, 5].product()      // 120
[1, 2, 3].count()               // 3

[5, 2, 8, 1].min()              // Some(1)
[5, 2, 8, 1].max()              // Some(8)

let factorial = [1, 2, 3, 4, 5].reduce(|a, b| a * b);
// Some(120)

let sum_of_squares = [1, 2, 3, 4].fold(0, |acc, x| acc + x * x);
// 30

let names = ["Alice", "Bob", "Christopher"];
let longest = names.max_by(|n| n.len());
// Some("Christopher")
```

## Search Operators (Terminal)

Find specific elements or test conditions.

| Operator | Return | Description |
|----------|--------|-------------|
| `.find(\|x\| condition)` | `Option<T>` | First element matching condition |
| `.find_last(\|x\| condition)` | `Option<T>` | Last element matching condition |
| `.any(\|x\| condition)` | `bool` | True if any element matches |
| `.all(\|x\| condition)` | `bool` | True if all elements match |
| `.none(\|x\| condition)` | `bool` | True if no element matches |
| `.position(\|x\| condition)` | `Option<u64>` | Index of first matching element |

```
let first_even = [1, 3, 4, 6, 7].find(|x| x % 2 == 0);
// Some(4)

[1, 2, 3].any(|x| x > 2)       // true
[1, 2, 3].all(|x| x > 0)       // true
[1, 2, 3].none(|x| x < 0)      // true

[10, 20, 30].position(|x| x == 20)   // Some(1)
```

## Ordering Operators

Sort and reverse the pipeline elements.

| Operator | Description |
|----------|-------------|
| `.sort_by(\|a, b\| a <=> b)` | Stable sort by comparator (returns new collection) |
| `.sort_by_key(\|x\| key_expr)` | Sort by a derived key |
| `.reverse()` | Reverse element order |

The `<=>` spaceship operator returns a negative `i32` if left < right, zero if
equal, and positive if left > right.

```
let sorted = [3, 1, 4, 1, 5].sort_by(|a, b| a <=> b).collect();
// [1, 1, 3, 4, 5]

let by_length = ["banana", "fig", "apple"]
    .sort_by_key(|s| s.len())
    .collect();
// ["fig", "apple", "banana"]

let descending = [1, 2, 3, 4, 5].sort_by(|a, b| b <=> a).collect();
// [5, 4, 3, 2, 1]
```

## Grouping and Zipping Operators

Combine, split, and restructure pipeline elements.

| Operator | Return | Description |
|----------|--------|-------------|
| `.group_by(\|x\| key)` | `Map<K, T[]>` | Group elements by key (terminal) |
| `.zip(other)` | `(T, U)[]` | Pair elements from two sequences |
| `.unzip()` | `(A[], B[])` | Split pairs into two arrays |
| `.enumerate()` | `(u64, T)[]` | Attach index to each element |
| `.chunks(n)` | `T[][]` | Split into groups of `n` |
| `.windows(n)` | `T[][]` | Sliding windows of size `n` |
| `.partition(\|x\| cond)` | `(T[], T[])` | Split into matching and non-matching |
| `.step_by(n)` | pipeline | Take every nth element |

```
let groups = ["apple", "avocado", "banana", "blueberry"]
    .group_by(|w| w.chars()[0]);
// Map<char, String[]>: #{ 'a': ["apple", "avocado"], 'b': ["banana", "blueberry"] }

let (evens, odds) = [1, 2, 3, 4, 5, 6].partition(|x| x % 2 == 0);
// evens = [2, 4, 6], odds = [1, 3, 5]

let pairs = [1, 2, 3].zip(["a", "b", "c"]).collect();
// [(1, "a"), (2, "b"), (3, "c")]

let indexed = ["x", "y", "z"].enumerate().collect();
// [(0, "x"), (1, "y"), (2, "z")]

let chunked = [1, 2, 3, 4, 5].chunks(2).collect();
// [[1, 2], [3, 4], [5]]

let wins = [1, 2, 3, 4, 5].windows(3).collect();
// [[1, 2, 3], [2, 3, 4], [3, 4, 5]]

let every_third = (0..20).step_by(3).collect();
// [0, 3, 6, 9, 12, 15, 18]
```

## Collection Operators (Terminal)

Materialize the lazy pipeline into a concrete collection.

| Operator | Description |
|----------|-------------|
| `.collect()` | Collect into an inferred collection type |
| `.collect(sep)` | Join elements into a `String` with separator |
| `.for_each(\|x\| effect)` | Consume for side effects, returns `()` |
| `.to_map(\|x\| (key, val))` | Build a `Map<K, V>` from a mapping function |

### `collect()` Type Inference Rules

The target collection type is inferred from context:

- Binding annotated as `T[]` -- collects to an array.
- Binding annotated as `Map<K, V>` and elements are `(K, V)` -- collects to a map.
- Binding annotated as `String` -- joins with no separator (elements must be `String`).
- No annotation and elements are `(K, V)` -- infers `Map`.
- No annotation otherwise -- infers `T[]`.
- `collect(sep)` with a string separator always produces `String`.

```
let nums: i32[] = [1, 2, 3].map(|x| x * 2).collect();
// [2, 4, 6]

let lookup: Map<String, i32> = [("a", 1), ("b", 2)].collect();
// #{ "a": 1, "b": 2 }

let csv = ["Alice", "Bob", "Carol"].collect(", ");
// "Alice, Bob, Carol"
```

## Chaining Example: Word Frequency

A complete pipeline that computes the top 10 most frequent words in a text:

```
let word_freq: Map<String, u64> = text
    .split(" ")
    .filter(|w| !w.is_empty())
    .map(|w| w.to_lowercase())
    .group_by(|w| w.clone())
    .entries()
    .map(|(word, group)| (word, group.count() as u64))
    .sort_by_key(|(_, count)| -(count as i64))
    .take(10)
    .collect();
```

This pipeline:
1. Splits the text into words.
2. Removes empty strings.
3. Normalizes to lowercase.
4. Groups identical words together.
5. Converts each group to a `(word, count)` pair.
6. Sorts by count descending.
7. Takes the top 10.
8. Collects into a `Map<String, u64>`.

## Custom Iterators

Any type can participate in pipelines by implementing the `Iterator` trait:

```
trait Iterator {
    type Item;
    fn next(&mut self) -> Option<Self::Item>;
}
```

Example -- a struct that yields values in a range with a custom step:

```
struct CountBy {
    current: i32,
    step:    i32,
    limit:   i32,
}

impl Iterator for CountBy {
    type Item = i32;

    fn next(&mut self) -> Option<i32> {
        if self.current >= self.limit {
            return None;
        }
        let val = self.current;
        self.current += self.step;
        return Some(val);
    }
}

let by_fives = CountBy { current: 0, step: 5, limit: 30 };
let result = by_fives.filter(|x| x % 2 == 0).collect();
// [0, 10, 20]
```

Once a type implements `Iterator`, it gains access to all pipeline operators
(`map`, `filter`, `fold`, `collect`, etc.) and can be used in `for` loops.
