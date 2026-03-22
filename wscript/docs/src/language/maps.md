# Maps

Maps are heap-allocated, ref-counted hash maps that store key-value pairs. Keys must
implement `Hash` and `Eq` -- primitives, `String`, and enums without payloads satisfy
this automatically. The type is written `Map<K, V>`.

## Map Literals

Create maps with the `#{ }` syntax:

```
let ages: Map<String, i32> = #{
    "Alice": 30,
    "Bob":   25,
    "Carol": 28,
};

let empty: Map<String, i32> = #{};
```

Keys are separated from values by `:` and entries are separated by `,`. A trailing
comma is permitted.

## Accessing Values

There are three ways to read a value from a map:

```
let m = #{ "alice": 30, "bob": 25 };

// Direct index -- panics if key is missing
let age = m["alice"];              // 30

// Safe get -- returns Option<V>
let age = m.get("alice");          // Some(30)
let missing = m.get("dave");       // None

// Get with default -- returns V
let age = m.get_or("dave", 0);    // 0
```

Use `m.get()` or `m.get_or()` when the key might not exist.

## Inserting and Updating

```
let mut m = #{ "alice": 30 };

// Bracket assignment -- insert or update
m["bob"] = 25;

// Method form -- same behavior
m.insert("carol", 28);
```

Both forms insert the key if it does not exist, or overwrite the value if it does.

## Removing Entries

```
let mut m = #{ "alice": 30, "bob": 25 };
let removed = m.remove("bob");    // Some(25)
let missing = m.remove("dave");   // None
```

`remove` returns `Option<V>` -- the value that was removed, or `None` if the key
was not present.

## Methods

| Method | Return type | Description |
|--------|-------------|-------------|
| `m.len()` | `u64` | Number of entries |
| `m.is_empty()` | `bool` | True if the map has no entries |
| `m.contains(key)` | `bool` | True if the key exists |
| `m.keys()` | `K[]` | Array of all keys |
| `m.values()` | `V[]` | Array of all values |
| `m.entries()` | `(K, V)[]` | Array of all key-value pairs |
| `m.clone()` | `Map<K, V>` | Deep copy of the map |
| `m.merge(other)` | `Map<K, V>` | New map with entries from both; `other` wins on conflict |

```
let m = #{ "a": 1, "b": 2, "c": 3 };

m.len()            // 3
m.is_empty()       // false
m.contains("b")    // true
m.keys()           // ["a", "b", "c"]  (order is not guaranteed)
m.values()         // [1, 2, 3]
```

### Merging Maps

`merge` creates a new map. When both maps have the same key, the value from the
argument (`other`) takes precedence.

```
let defaults = #{ "host": "localhost", "port": "8080", "debug": "false" };
let overrides = #{ "port": "3000", "debug": "true" };

let config = defaults.merge(overrides);
// #{ "host": "localhost", "port": "3000", "debug": "true" }
```

## Iteration

Iterate over a map with a `for` loop. Each iteration yields a `(K, V)` tuple.

```
let scores = #{ "Alice": 95, "Bob": 87, "Carol": 92 };

for (name, score) in scores {
    print(`${name}: ${score}`);
}
```

Iteration order is not guaranteed to match insertion order.

## Pipelines on Map Entries

Convert the map to an entry array with `.entries()`, then use the full range of
pipeline operators.

```
let scores = #{ "Alice": 95, "Bob": 87, "Carol": 92, "Dave": 68 };

// Find names of everyone who scored above 90
let honors: String[] = scores.entries()
    .filter(|(_, score)| score > 90)
    .map(|(name, _)| name)
    .sort_by(|a, b| a <=> b)
    .collect();
// ["Alice", "Carol"]
```

### Building a Map from a Pipeline

When a pipeline produces `(K, V)` tuples, `collect()` gathers them into a map if
the target type is `Map<K, V>`.

```
let names = ["Alice", "Bob", "Carol"];
let name_lengths: Map<String, u64> = names
    .map(|n| (n, n.len()))
    .collect();
// #{ "Alice": 5, "Bob": 3, "Carol": 5 }
```

You can also use `to_map()` for an explicit key-value mapping:

```
let users = [
    User { id: 1, name: "Alice" },
    User { id: 2, name: "Bob" },
];

let by_id = users.to_map(|u| (u.id, u.name));
// Map<u64, String>: #{ 1: "Alice", 2: "Bob" }
```

## Reference Semantics

Like all heap types, maps are ref-counted. Assignment shares the reference.

```
let a = #{ "x": 1 };
let b = a;             // b and a point to the same map
```

Use `.clone()` for an independent copy:

```
let a = #{ "x": 1 };
let b = a.clone();     // b is a separate map
```

## Complete Example

```
fn word_frequency(text: String) -> Map<String, u64> {
    let freq: Map<String, u64> = #{};
    let mut counts = freq.clone();

    for word in text.split(" ") {
        if !word.is_empty() {
            let w = word.to_lowercase();
            let current = counts.get_or(w, 0);
            counts.insert(w, current + 1);
        }
    }

    return counts;
}

let freq = word_frequency("the cat sat on the mat");
// #{ "the": 2, "cat": 1, "sat": 1, "on": 1, "mat": 1 }
```
