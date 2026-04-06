@export
fn main() -> i32 {
    let base = 100;
    let add_base = |x: i32| -> i32 { return x + base; };
    let result = add_base(42);
    print(result);
    return result;
}
