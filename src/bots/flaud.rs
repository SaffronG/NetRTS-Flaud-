use std::collections::HashMap;

use crate::logic::models::*;
use crate::logic::shared::Bot;

const GROUP_SIZE: usize = 5;

pub struct Flaud {
    regiments: Vec<[i32;5]>,
    armies: Vec<[i32;5]>,
    workers: Vec<[i32;5]>,
    building_costs: HashMap<BuildingType, i32>,
}

impl Bot for Flaud {
    fn decide(&mut self, _s: &GameStateDto) -> Vec<CommandRequest> {
        Vec::new()
    }

    fn new(m: MapDto, r: RulesDto) -> Self {
        Self {
            regiments: vec!(),
            armies: vec!(),
            workers: vec!(),
            building_costs: Self::building_cost(&r),
        }
    }
}

impl Flaud {
    
}

#[cfg(test)]
#[allow(unused_imports)]
mod flaud_tests {
    use super::*;
    use crate::bots::fixtures;

    #[test]
    fn stub_sends_no_commands() {
        let mut bot = Flaud::new(fixtures::map(), fixtures::rules());
        assert!(bot.decide(&fixtures::state()).is_empty());
    }
}
