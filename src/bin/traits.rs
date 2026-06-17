// Exploring traits, default impls, trait objects
// Run with: cargo run --bin traits

trait Animal {
    fn name(&self) -> &str;
    fn sound(&self) -> &str;
    fn describe(&self) -> String {
        format!("{} says {}", self.name(), self.sound())
    }
}

struct Dog;
struct Cat;

impl Animal for Dog {
    fn name(&self) -> &str { "dog" }
    fn sound(&self) -> &str { "woof" }
}

impl Animal for Cat {
    fn name(&self) -> &str { "cat" }
    fn sound(&self) -> &str { "meow" }
}

fn print_animal(a: &dyn Animal) {
    println!("{}", a.describe());
}

fn main() {
    let animals: Vec<Box<dyn Animal>> = vec![Box::new(Dog), Box::new(Cat)];
    for a in &animals {
        print_animal(a.as_ref());
    }
}
