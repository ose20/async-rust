trait Greeting {
    fn greet(&self) -> String;
}

struct HelloWorld;
impl Greeting for HelloWorld {
    fn greet(&self) -> String {
        "Hello, World!".to_string()
    }
}

struct ExcitedGreeting<T> {
    inner: T,
}

impl<T> ExcitedGreeting<T> {
    fn greet(&self) -> String
    where
        T: Greeting,
    {
        let mut greeting = self.inner.greet();
        greeting.push_str(" I'm so excited to be in Rust!");
        greeting
    }
}

fn main() {
    let raw_one = HelloWorld;
    let raw_two = HelloWorld;
    let docorated = ExcitedGreeting { inner: raw_two };
    println!("{}", raw_one.greet());
    println!("{}", docorated.greet());

    // -------------------
    #[cfg(feature = "logging_decorator")]
    let hello = ExcitedGreeting { inner: HelloWorld };

    #[cfg(not(feature = "logging_decorator"))]
    let hello = HelloWorld;

    println!("{}", hello.greet());
}
