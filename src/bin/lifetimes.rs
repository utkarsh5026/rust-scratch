// Exploring lifetimes
// Run with: cargo run --bin lifetimes

fn longest<'a>(x: &'a str, y: &'a str) -> &'a str {
    if x.len() > y.len() { x } else { y }
}

struct Important<'a> {
    content: &'a str,
}

impl<'a> Important<'a> {
    fn announce(&self) -> &str {
        self.content
    }
}

fn main() {
    let s1 = String::from("long string");
    let result;
    {
        let s2 = String::from("xyz");
        result = longest(s1.as_str(), s2.as_str());
        println!("longest: {result}");
    }

    let novel = String::from("Call me Ishmael. Some years ago...");
    let first_sentence = novel.split('.').next().unwrap();
    let imp = Important { content: first_sentence };
    println!("important: {}", imp.announce());
}
