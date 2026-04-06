# References

SpiteScript supports first-class reference types: `&T` for a shared (immutable) reference and `&mut T` for an exclusive (mutable) reference. References let you point at an existing value without copying it, and let functions accept arguments that can be read or mutated in place.

References are **not** the same thing as the `&self` / `&mut self` receivers on methods (which are a method-contract distinction, not standalone types). You can form a reference to any value and pass it around.

## Reference Types in Type Annotations

Write `&T` or `&mut T` wherever a type is expected:

```spite
fn read_hp(p: &PlayerState) -> i32 {
    return p.hp;
}

fn heal(p: &mut PlayerState, amount: i32) {
    p.hp = p.hp + amount;
}
```

## Taking a Reference

Use the `&` prefix operator to form a shared reference, and `&mut` for a mutable one. A `&mut` reference can only be taken from a `let mut` binding:

```spite
let mut state = PlayerState { hp: 100, score: 0 };

read_hp(&state);         // shared borrow, fine
heal(&mut state, 10);    // mutable borrow, requires `let mut state`
```

Taking `&mut` of an immutable binding is a compile error.

## Dereferencing

Use the prefix `*` operator to read through a reference. Field and method access through a reference is automatic, so `*` is only needed when you want the underlying value itself (for assignment through a `&mut T`, or when passing the pointee by value):

```spite
fn bump(counter: &mut i32) {
    *counter = *counter + 1;
}

let mut n = 0;
bump(&mut n);
// n is now 1
```

Dereferencing a non-reference type is a compile error.

## Memory Model

References are backed by a bump allocator inside the `Vm`'s linear memory. They are lightweight pointers; assigning a reference does not copy the pointee. Because references share the same underlying arena as the rest of script memory, they remain valid for the lifetime of the `Vm` instance.

SpiteScript does not have a borrow checker. The `&T` vs `&mut T` distinction is enforced at type-check time as a contract on callers (you cannot form `&mut` from an immutable binding, and you cannot assign through `&T`), but two mutable references to the same value are not prevented at runtime. Mutation follows evaluation order.
