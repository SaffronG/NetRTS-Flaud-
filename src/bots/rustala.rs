use crate::logic::shared::Bot;
use crate::logic::models::*;

pub struct RustalaDaora {

}

impl Bot for RustalaDaora {
    fn decide(&self) {
        todo!()
    }

    fn new(s: GameStateDto, m: MapDto, r: RulesDto) -> Self {
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