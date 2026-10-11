#[allow(unused)]
pub struct Api {
    pub server: String,
    pub api_key: String,
}

impl Api {
    pub fn new(server_addr: String, key: String) -> Self {
        Self {
            server: server_addr,
            api_key: key,
        }
    }

    pub fn join_match(self) {}

    pub fn connect(self, match_id: String) {}

    pub fn results(self, match_id: String) {}
}

#[cfg(test)]
mod api_tests {
    // use super::*;

    #[test]
    fn initial_setup() {
        assert!(true);
    }
}
