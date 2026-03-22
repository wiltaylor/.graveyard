fn safe_div(a: i32, b: i32) -> i32 {
    if b == 0 {
        return 0;
    }
    return a / b;
}

fn find_positive(items: i32, fallback: i32) -> i32 {
    if items > 0 {
        return items;
    }
    return fallback;
}

@export
fn main() -> i32 {
    let a = safe_div(100, 5);
    let b = safe_div(100, 0);
    print(a);
    print(b);
    return a + b;
}
