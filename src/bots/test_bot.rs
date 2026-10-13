use crate::logic::shared::Bot;
use crate::logic::models::*;
use std::collections::HashMap;

pub struct TestBot {
    state: GameStateDto,
    terrain: Vec<String>,
    cost: HashMap<UnitType, i32>,
    building_cost: HashMap<BuildingType, i32>,
    barracks_ordered_at: i32,
    attacking: bool,
}

impl Bot for TestBot {
    fn decide(&self) {
        let resources = self.state.you.resources;
        let commands: Vec<CommandRequest>;
    }

    
    fn new(s: GameStateDto, m: MapDto, r: RulesDto) -> Self {
        Self {  
            state: s,
            terrain: m.terrain,
            cost: r.units.iter().fold(HashMap::new(), |mut acc, u| {
                acc.insert(u.kind, u.cost);
                acc
            }),
            barracks_ordered_at: 0,
            attacking: false,
            building_cost: Self::building_cost(r)
        }
    }
}

impl TestBot {
    fn enemy_base(&self) -> Vec<PositionDto> {
        let home = self.state.you.slot;
        self.state.players
            .iter()
            .filter(|p| p.slot != home && !p.eliminated)
            .map(|p| p.start_position)
            .collect()
    }
    
    fn find_site(&self, cc: BuildingType) {
        todo!()
    }

    fn spend(&self, r: &i32, k: BuildingType) -> bool {
        r < self.building_cost.get(&k).expect("Failed to get building price")
    }
}