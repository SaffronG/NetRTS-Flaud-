pub fn flaud_test() {
    println!("Hello from FlaudBot!");
}

#[cfg(test)]
#[allow(unused_imports)]
mod flaud_tests {
    use super::*;

    #[test]
    fn initial_setup() {
        assert!(true);
    }
}