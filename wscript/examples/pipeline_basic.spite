/// Pipeline operations: filter, map, collect, for_each.

@export
fn main() -> i32 {
    let nums = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

    // Filter even numbers using .filter() with lambda
    let evens = nums.filter(|x| x % 2 == 0);
    let even_sum = evens.sum();
    print(even_sum);  // 2+4+6+8+10 = 30

    // Map: double each element
    let doubled = nums.map(|x| x * 2);
    let doubled_sum = doubled.sum();
    print(doubled_sum);  // 2+4+6+8+10+12+14+16+18+20 = 110

    // Chained: filter then map then collect
    let result = nums.filter(|x| x > 5).map(|x| x * 10).collect();
    let result_sum = result.sum();
    print(result_sum);  // 60+70+80+90+100 = 400

    // Return a known value for verification
    return even_sum + doubled_sum + result_sum;
}
