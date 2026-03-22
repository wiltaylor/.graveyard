enum Color {
    Red,
    Green,
    Blue,
}

fn color_code(c: Color) -> i32 {
    return match c {
        Color::Red => 1,
        Color::Green => 2,
        Color::Blue => 3,
    };
}

@export
fn main() -> i32 {
    let r = color_code(Color::Red);
    let g = color_code(Color::Green);
    let b = color_code(Color::Blue);
    print(r);
    print(g);
    print(b);
    return r + g + b;
}
