@export
fn main() -> i32 {
    let mut a = 0;
    let mut b = 1;
    let mut i = 0;
    while i < 10 {
        let temp = b;
        b = a + b;
        a = temp;
        i = i + 1;
    }
    return a;
}
