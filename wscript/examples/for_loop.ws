@export
fn main() -> i32 {
    let mut sum = 0;
    for i in 0..10 {
        sum = sum + i;
    }
    print(sum);
    return sum;
}
