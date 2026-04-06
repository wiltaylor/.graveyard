@export
fn main() -> i32 {
    let nums = [5, 3, 8, 1, 9, 2, 7, 4, 6, 10];

    let total = nums.sum();
    print(total);

    let first = nums.first();
    print(first);

    let last = nums.last();
    print(last);

    let min_val = nums.min();
    print(min_val);

    let max_val = nums.max();
    print(max_val);

    let count = nums.len();
    print(count);

    return total;
}
