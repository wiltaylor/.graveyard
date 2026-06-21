/// A simple hello world program.
@export
fn main() -> String {
    let name = "World";
    let greeting = `Hello, ${name}!`;
    print(greeting);
    return greeting;
}
