const MAX_SIZE: i32 = 1024;
const FACTOR: i32 = 3;

trait Describable {
    fn value(&self) -> i32;
}

struct Counter {
    count: i32,
}

impl Describable for Counter {
    fn value(&self) -> i32 {
        return self.count;
    }
}

@export
fn main() -> i32 {
    let c = Counter { count: 42 };
    let v = c.value();
    print(v);
    return v * FACTOR;
}
