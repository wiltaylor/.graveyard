@export
fn main() -> i32 {
    let a = [10, 20, 30, 40, 50];
    let total = a.sum();
    let count = a.len();
    print(total);
    print(count);
    return total;
}
