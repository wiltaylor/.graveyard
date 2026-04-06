@export
fn add(a: i32, b: i32) -> i32 {
    return a + b;
}

@export
fn multiply(a: i32, b: i32) -> i32 {
    return a * b;
}

fn square(x: i32) -> i32 {
    return x * x;
}

@export
fn main() -> i32 {
    let a = 10;
    let b = 20;
    let sum = add(a, b);
    let prod = multiply(a, b);
    let sq = square(5);
    return sum + prod + sq;
}
