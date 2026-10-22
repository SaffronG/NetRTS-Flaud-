use std::collections::HashMap;

use crate::logic::connectome::FlyBrainConnectome;
use crate::logic::models::*;
use crate::logic::shared::{Bot, encode_sugar, encode_threat};

const GROUP_SIZE: usize = 5;

struct BrainGroup {
    ids: Vec<i32>,
    brain: FlyBrainConnectome,
}

pub struct Flaud {
    regiments: Vec<BrainGroup>,
    workers: Vec<BrainGroup>,
    armies: Vec<BrainGroup>,
    building_costs: HashMap<BuildingType, i32>,
}

impl Bot for Flaud {
    fn decide(&mut self, s: &GameStateDto) -> Vec<CommandRequest> {
        // CPU-bound, so run on scoped OS threads; they borrow `s` and join before returning
        let (threat, sugar) = std::thread::scope(|sc| {
            let threat = sc.spawn(|| encode_threat(s));
            let sugar = sc.spawn(|| encode_sugar(s));
            (threat.join().unwrap(), sugar.join().unwrap())
        });

        Vec::new()
    }

    fn new(m: MapDto, r: RulesDto) -> Self {
        Self {
            regiments: vec![],
            armies: vec![],
            workers: vec![],
            building_costs: Self::building_cost(&r),
        }
    }
}

impl Flaud {
    fn workers() {}

    fn regiments() {}

    fn army() {}
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
