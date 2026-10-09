pub fn rnetr_test() {
    println!("Hello from RnetrBot!");
}

#[cfg(test)]
#[allow(unused_imports)]
mod rnetr_tests {
    use super::*;

    #[test]
    fn initial_setup() {
        assert!(true);
    }
}