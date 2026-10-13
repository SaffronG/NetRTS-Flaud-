use crate::logic::shared::Bot;
use crate::logic::models::*;

pub struct TestBot {

}

impl Bot for TestBot {
    fn decide(state: GameStateDto) {
        todo!()
    }

    fn new(self, game_map: MapDto, rules: RulesDto) -> Self {
        todo!()
    }

    fn enemy_base(state: GameStateDto) {
        todo!()
    }

    fn find_site(state: GameStateDto, cc: BuildingType) {
        todo!()
    }
}

impl Drop for TestBot {
    fn drop(&mut self) {
        todo!()
    }
}