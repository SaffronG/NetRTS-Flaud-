use crate::logic::models::*;
use crate::logic::shared::*;
use std::collections::HashMap;

const WORKER_TARGET: usize = 10;
const SCOUT_TAREGET: usize = 6;
const ATTACK_THRESHOLD: usize = 6;
const BARRACKS_TARGET: usize = 2;

pub struct TestBot {
    state: GameStateDto,
    terrain: Vec<String>,
    cost: HashMap<UnitType, i32>,
    building_cost: HashMap<BuildingType, i32>,
    barracks_ordered_at: i32,
    attacking: bool,
}

impl Bot for TestBot {
    fn decide(&mut self) -> Vec<CommandRequest> {
        let me = self.state.you.slot;
        let mut resources = self.state.you.resources;

        // Split once into mine / enemy
        let (my_units, enemy_units): (Vec<&UnitDto>, Vec<&UnitDto>) =
            self.state.units.iter().partition(|u| u.owner == me);
        let (my_buildings, enemy_buildings): (Vec<&BuildingDto>, Vec<&BuildingDto>) =
            self.state.buildings.iter().partition(|b| b.owner == me);

        // My units by type, in one pass
        let mut workers = Vec::new();
        let mut soldiers = Vec::new();
        let mut archers = Vec::new();
        let mut scouts = Vec::new();
        for &u in &my_units {
            match u.kind {
                UnitType::Worker => workers.push(u),
                UnitType::Soldier => soldiers.push(u),
                UnitType::Archer => archers.push(u),
                UnitType::Scout => scouts.push(u),
            }
        }

        let barracks_count: usize = my_buildings
            .iter()
            .filter(|b| b.kind == BuildingType::Barracks)
            .count();
        let mut commands = Vec::new();

        let barracks_cost = *self
            .building_cost
            .get(&BuildingType::Barracks)
            .expect("Building price not found");

        if barracks_count < BARRACKS_TARGET
            && workers.len() >= WORKER_TARGET / 2
            && self.barracks_ordered_at.clone() > 40
            && barracks_cost < resources
            && self.spend(&resources, BuildingType::Barracks)
        {
            if let Ok(pos) = self.find_site(BuildingType::Barracks) {
                let builder = workers.iter().min_by_key(|w| dist(pos.x, pos.y, w.x, w.y));

                if let Some(builder) = builder {
                    resources -= barracks_cost;
                    self.barracks_ordered_at = self.state.tick;

                    commands.push(CommandRequest {
                        kind: CommandType::Build,
                        unit_ids: Some(vec![builder.id]),
                        building_type: Some(BuildingType::Barracks),
                        x: Some(pos.x),
                        y: Some(pos.y),
                        ..CommandRequest::new(CommandType::Build)
                    });
                }
            }
        }

        commands
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
            building_cost: Self::building_cost(r),
        }
    }
}

impl TestBot {
    fn enemy_base(&self) -> Vec<PositionDto> {
        let home = self.state.you.slot;
        self.state
            .players
            .iter()
            .filter(|p| p.slot != home && !p.eliminated)
            .map(|p| p.start_position)
            .collect()
    }

    fn find_site(&self, cc: BuildingType) -> Result<PositionDto, ()> {
        Err(())
    }

    fn spend(&self, r: &i32, k: BuildingType) -> bool {
        r < self
            .building_cost
            .get(&k)
            .expect("Failed to get building price")
    }
}
