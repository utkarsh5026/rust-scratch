// Scratch pad — quick experiments go here.
// Run with: cargo run

pub struct Point {
    x: i32,
    y: i32,
}

impl PartialEq for Point {
    fn eq(&self, other: &Self) -> bool {
        self.x == other.x && self.y == other.y
    }
}

impl Eq for Point {}
fn main() {
    println!("scratch");
}
