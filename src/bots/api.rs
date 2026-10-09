#[allow(unused)]
pub struct Api {
    server: String,
    api_key: String,
}

impl Api {
    fn new(server_addr: String, key: String) -> Self {
        Self {
            server: server_addr,
            api_key: key,
        }
    }

    fn send(self, method: String, path: String, body: String) {
        
    }
}

#[cfg(test)]
mod api_tests {
    // use super::*;

    #[test]
    fn initial_setup() {
        assert!(true);
    }
}
