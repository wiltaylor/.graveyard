@export
fn main() -> i32 {
    let a = [10, 20, 30, 40, 50];
    let second = a[1];
    print(second);

    let x = 0 - 42;
    let abs_x = if x < 0 { 0 - x } else { x };
    print(abs_x);

    let t = true;
    let f = false;
    let both = t && f;
    let either = t || f;

    return second + abs_x;
}
