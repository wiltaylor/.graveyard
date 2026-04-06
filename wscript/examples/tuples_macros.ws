@export
fn main() -> i32 {
    let pair = (10, 20);
    let sum = pair.0 + pair.1;
    print(sum);

    assert!(sum == 30);

    return sum;
}
