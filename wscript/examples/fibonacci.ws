/// Compute fibonacci numbers.

fn fib(n: i32) -> i32 {
    if n <= 1 {
        return n;
    }
    return fib(n - 1) + fib(n - 2);
}

@export
fn main() {
    for i in 0..10 {
        print(`fib(${i}) = ${fib(i)}`);
    }
}
