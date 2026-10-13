use crate::bots::shared_logic;

pub struct RustalaDaora {

}

impl shared_logic::Bot for RustalaDaora {
    fn decide(_state: super::models::GameStateDto) {
        todo!()
    }
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