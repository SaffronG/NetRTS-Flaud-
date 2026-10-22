//! Rust port of `example_bot.py`, used to exercise the API end to end.
//!
//! Strategy: mine with ~10 workers, build one Barracks, train soldiers non-stop and
//! attack-move to the enemy's start position once 6 soldiers are ready.

use crate::logic::models::*;
use crate::logic::shared::*;
use std::collections::HashMap;

const TARGET_WORKERS: usize = 10;
/// Soldiers needed before the first attack
const ATTACK_AT: usize = 6;
/// Workers mining before we save up for the Barracks
const BARRACKS_AT_WORKERS: usize = 6;
/// Ticks to wait before re-ordering a Barracks that never showed up
const BARRACKS_RETRY_TICKS: i32 = 40;
/// Max units a building may have queued before we stop adding more
const QUEUE_LIMIT: usize = 2;

pub struct TestBot {
    terrain: Vec<String>,
    rules: RulesDto,
    /// Tick we last told a worker to build a Barracks
    barracks_ordered_at: i32,
    attacking: bool,
}

impl Bot for TestBot {
    fn decide(&mut self, s: &GameStateDto) -> Vec<CommandRequest> {
        let mut ore = s.you.resources;

        let mut workers: Vec<&UnitDto> = Vec::new();
        let mut soldiers: Vec<&UnitDto> = Vec::new();
        for u in s.my_units() {
            match u.kind {
                UnitType::Worker => workers.push(u),
                UnitType::Soldier => soldiers.push(u),
                _ => {}
            }
        }

        let cc = s
            .my_buildings()
            .find(|b| b.kind == BuildingType::CommandCenter);
        let barracks: Vec<&BuildingDto> = s
            .my_buildings()
            .filter(|b| b.kind == BuildingType::Barracks)
            .collect();

        // (id, x, y) of every visible enemy unit and building
        let enemies: Vec<(i32, i32, i32)> = s
            .enemy_units()
            .map(|u| (u.id, u.x, u.y))
            .chain(s.enemy_buildings().map(|b| (b.id, b.x, b.y)))
            .collect();

        let mut commands = Vec::new();

        // 1. Build a Barracks once we have a few workers mining and can afford it.
        if let Some(cc) = cc
            && barracks.is_empty()
            && workers.len() >= BARRACKS_AT_WORKERS
            && s.tick - self.barracks_ordered_at > BARRACKS_RETRY_TICKS
        {
            let site = find_site(&self.terrain, s, cc.x, cc.y);
            let builder = workers
                .iter()
                .copied()
                .min_by_key(|w| (w.carrying, dist(w.x, w.y, cc.x, cc.y)));

            if let (Some((x, y)), Some(builder)) = (site, builder)
                && spend(&mut ore, self.building_price(BuildingType::Barracks))
            {
                commands.push(CommandRequest::build(
                    vec![builder.id],
                    BuildingType::Barracks,
                    x,
                    y,
                ));
                self.barracks_ordered_at = s.tick;
                workers.retain(|w| w.id != builder.id);
            }
        }

        // 2. Idle workers mine, spread over the visible deposits (near ones first).
        if !s.resources.is_empty() {
            let mut load: HashMap<i32, i32> = s.resources.iter().map(|d| (d.id, 0)).collect();
            for w in &workers {
                if let Some(n) = w.target_id.and_then(|t| load.get_mut(&t)) {
                    *n += 1;
                }
            }
            for w in workers.iter().filter(|w| w.activity == UnitActivity::Idle) {
                let best = s
                    .resources
                    .iter()
                    .min_by_key(|d| dist(w.x, w.y, d.x, d.y) + 4 * load[&d.id])
                    .expect("resources is non-empty");
                *load.get_mut(&best.id).unwrap() += 1;
                commands.push(CommandRequest::gather(vec![w.id], best.id));
            }
        }

        // 3. Keep the Command Center training workers up to the target (but save up for the Barracks).
        if let Some(cc) = cc
            && cc.completed
        {
            let planned = workers.len() + cc.production.len();
            let saving = barracks.is_empty() && workers.len() >= BARRACKS_AT_WORKERS;
            if planned < TARGET_WORKERS
                && cc.production.len() < QUEUE_LIMIT
                && !saving
                && spend(&mut ore, self.unit_price(UnitType::Worker))
            {
                commands.push(CommandRequest::produce(cc.id, UnitType::Worker, 1));
            }
        }

        // 4. Every finished Barracks trains soldiers continuously.
        for b in &barracks {
            if b.completed
                && b.production.len() < QUEUE_LIMIT
                && spend(&mut ore, self.unit_price(UnitType::Soldier))
            {
                commands.push(CommandRequest::produce(b.id, UnitType::Soldier, 1));
            }
        }

        // 5. Attack: once ATTACK_AT soldiers exist, send every idle soldier in.
        if soldiers.len() >= ATTACK_AT {
            self.attacking = true;
        }
        if self.attacking {
            let idle: Vec<i32> = soldiers
                .iter()
                .filter(|u| u.activity == UnitActivity::Idle)
                .map(|u| u.id)
                .collect();
            if !idle.is_empty() {
                let (sx, sy) = (soldiers[0].x, soldiers[0].y);
                match enemies
                    .iter()
                    .min_by_key(|(_, ex, ey)| dist(sx, sy, *ex, *ey))
                {
                    // Something visible: go hit the closest thing
                    Some(&(target, _, _)) => commands.push(CommandRequest::attack(idle, target)),
                    // Attack-move toward the enemy base (engages anything seen on the way)
                    None => {
                        let target = enemy_base(s);
                        commands.push(CommandRequest::attack_move(idle, target.x, target.y));
                    }
                }
            }
        }

        commands
    }

    fn new(m: MapDto, r: RulesDto) -> Self {
        Self {
            terrain: m.terrain,
            rules: r,
            barracks_ordered_at: -1000,
            attacking: false,
        }
    }
}

impl TestBot {
    /// Cost of a unit from the rules; unknown types are never affordable.
    fn unit_price(&self, t: UnitType) -> i32 {
        self.rules.unit(t).map_or(i32::MAX, |u| u.cost)
    }

    /// Cost of a building from the rules; unknown types are never affordable.
    fn building_price(&self, t: BuildingType) -> i32 {
        self.rules.building(t).map_or(i32::MAX, |b| b.cost)
    }
}

/// Deducts `price` from `ore` if affordable; returns whether it was.
fn spend(ore: &mut i32, price: i32) -> bool {
    if *ore < price {
        return false;
    }
    *ore -= price;
    true
}
