use crate::bots::models::{BuildingType, *};

pub trait Bot {
    fn decide(state: GameStateDto);

    fn new(self, game_map: MapDto, rules: RulesDto) -> Self;
    
    fn enemy_base(state: GameStateDto);

    fn find_site(state: GameStateDto, cc: BuildingType);
}