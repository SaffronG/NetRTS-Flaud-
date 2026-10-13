use crate::logic::shared::Bot;
use crate::logic::models::*;

pub struct RustalaDaora {

}

impl Bot for RustalaDaora {
    fn decide(_state: GameStateDto) {
        todo!()
    }
    
    fn new(self, _game_map: MapDto, _rules: RulesDto) -> Self {
        todo!()
    }
    
    fn enemy_base(_state: GameStateDto) {
        todo!()
    }
    
    fn find_site(_state: GameStateDto, _cc: BuildingType) {
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