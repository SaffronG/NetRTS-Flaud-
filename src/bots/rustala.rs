use crate::bots::shared_logic;

pub struct RustalaDaora {

}

impl shared_logic::Bot for RustalaDaora {
    fn decide(_state: super::models::GameStateDto) {
        todo!()
    }
    
    fn new(self, game_map: super::models::MapDto, rules: super::models::RulesDto) -> Self {
        todo!()
    }
    
    fn enemy_base(state: super::models::GameStateDto) {
        todo!()
    }
    
    fn find_site(state: super::models::GameStateDto, cc: super::models::BuildingType) {
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