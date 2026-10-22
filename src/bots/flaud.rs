use std::collections::HashMap;

use crate::logic::connectome::{FlyBrainConnectome, MotorAction};
use crate::logic::models::*;
use crate::logic::shared::{Bot, encode_prey, encode_sugar, encode_threat};

const GROUP_SIZE: usize = 5;
const DECAY: f32 = 0.8; // Determines that the minimum current possible to maybe trigger a response is (1 - DECAY)
const THRESHOLD: f32 = 1.0; // If (v > THRESHOLD) fire every tick

struct BrainGroup {
    ids: Vec<i32>,
    brain: FlyBrainConnectome,
}

// Weight rows are [threat, sugar, prey, wealth]. A -1.0 or stronger threat
// weight means that action can never fire while a threat is in view.

fn worker_brain() -> FlyBrainConnectome {
    FlyBrainConnectome::new(
        vec![
            [0.6, 0.0, 0.0, 0.0],  // Retreat: needs ~2 spikes, so threat >= ~0.11
            [-1.0, 0.6, 0.0, 0.0], // Gather: drawn to sugar, bias covers far from it
            [-1.0, 0.0, 0.0, 0.6], // Build: when rich and safe
        ],
        DECAY,
        THRESHOLD,
        vec![0.0, 0.25, 0.0],
        vec![
            MotorAction::Retreat,
            MotorAction::Gather,
            MotorAction::Build,
        ],
    )
}

fn army_brain() -> FlyBrainConnectome {
    FlyBrainConnectome::new(
        vec![
            [1.2, 0.0, 0.0, 0.0],  // Attack: on the first vision spike
            [-1.5, 0.0, 0.6, 0.0], // Explore: hunt prey, but not while fighting
        ],
        DECAY,
        THRESHOLD,
        vec![0.0, 0.0],
        vec![MotorAction::Attack, MotorAction::Explore],
    )
}

fn scout_brain() -> FlyBrainConnectome {
    FlyBrainConnectome::new(
        vec![
            [0.9, 0.0, 0.0, 0.0],  // Retreat: more fearful than a worker
            [-1.5, 0.0, 0.5, 0.0], // Explore: restless (bias), drawn to prey
        ],
        DECAY,
        THRESHOLD,
        vec![0.0, 0.3],
        vec![MotorAction::Retreat, MotorAction::Explore],
    )
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
        let (threat, sugar, prey) = std::thread::scope(|sc| {
            let threat = sc.spawn(|| encode_threat(s));
            let sugar = sc.spawn(|| encode_sugar(s));
            let prey = sc.spawn(|| encode_prey(s));
            (
                threat.join().unwrap(),
                sugar.join().unwrap(),
                prey.join().unwrap(),
            )
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
    /// Resources as a share of the cheapest building cost, from 0 to 1.
    fn encode_wealth(&self, s: &GameStateDto) -> f32 {
        let cheapest = self
            .building_costs
            .values()
            .copied()
            .filter(|&c| c > 0)
            .min();
        match cheapest {
            Some(cost) => (s.you.resources as f32 / cost as f32).clamp(0.0, 1.0),
            None => 0.0,
        }
    }
}

/// Average position of the group's living units, or None if all are dead.
fn centroid(ids: &[i32], s: &GameStateDto) -> Option<(i32, i32)> {
    let (mut sx, mut sy, mut n) = (0, 0, 0);
    for u in s.my_units().filter(|u| ids.contains(&u.id)) {
        sx += u.x;
        sy += u.y;
        n += 1;
    }
    (n > 0).then(|| (sx / n, sy / n))
}

#[cfg(test)]
#[allow(unused_imports)]
mod flaud_tests {
    use super::*;
    use crate::bots::fixtures;

    use MotorAction::*;

    const TICKS: usize = 12;

    /// Feeds one tick of senses to a brain, in `THREAT, SUGAR, PREY, WEALTH` order.
    fn feel(
        b: &mut FlyBrainConnectome,
        threat: f32,
        sugar: f32,
        prey: f32,
        wealth: f32,
    ) -> MotorAction {
        b.tick([threat, sugar, prey, wealth])
    }

    /// Actions from a fresh brain held at the same senses for `TICKS` ticks.
    fn run(make: fn() -> FlyBrainConnectome, senses: [f32; 4]) -> Vec<MotorAction> {
        let mut b = make();
        let [t, s, p, w] = senses;
        (0..TICKS).map(|_| feel(&mut b, t, s, p, w)).collect()
    }

    fn first(actions: &[MotorAction], a: MotorAction) -> Option<usize> {
        actions.iter().position(|x| *x == a)
    }

    // senses:        [threat, sugar, prey, wealth]
    const CALM: [f32; 4] = [0.0, 0.0, 0.0, 0.0];
    const DANGER: [f32; 4] = [1.0, 1.0, 0.0, 0.0];

    // --- worker ---

    #[test]
    fn worker_gathers_when_calm() {
        assert!(run(worker_brain, CALM).contains(&Gather));
    }

    #[test]
    fn worker_gathers_sooner_near_sugar() {
        let calm = first(&run(worker_brain, CALM), Gather).unwrap_or(TICKS);
        let sweet = first(&run(worker_brain, [0.0, 1.0, 0.0, 0.0]), Gather).unwrap_or(TICKS);
        assert!(sweet < calm, "sugar {sweet} vs calm {calm}");
    }

    #[test]
    fn worker_flees_danger_and_never_gathers_in_it() {
        let a = run(worker_brain, DANGER);
        assert!(a.contains(&Retreat), "{a:?}");
        assert!(!a.contains(&Gather), "{a:?}");
    }

    #[test]
    fn worker_ignores_a_faint_threat() {
        assert!(!run(worker_brain, [0.05, 0.0, 0.0, 0.0]).contains(&Retreat));
    }

    #[test]
    fn worker_builds_when_rich_and_calm() {
        assert!(run(worker_brain, [0.0, 0.0, 0.0, 1.0]).contains(&Build));
    }

    #[test]
    fn worker_never_builds_when_poor_or_threatened() {
        assert!(!run(worker_brain, CALM).contains(&Build));
        assert!(!run(worker_brain, [1.0, 0.0, 0.0, 1.0]).contains(&Build));
    }

    // --- army ---

    #[test]
    fn army_holds_when_calm() {
        assert!(run(army_brain, CALM).iter().all(|a| *a == HoldPosition));
    }

    #[test]
    fn army_attacks_threat() {
        let a = run(army_brain, [0.3, 0.0, 0.0, 0.0]);
        assert!(a.contains(&Attack), "{a:?}");
    }

    #[test]
    fn army_hunts_undefended_prey() {
        let a = run(army_brain, [0.0, 0.0, 1.0, 0.0]);
        assert!(a.contains(&Explore), "{a:?}");
    }

    #[test]
    fn army_does_not_wander_off_mid_fight() {
        // Defended prey: threat cancels the pull toward it
        let a = run(army_brain, [1.0, 0.0, 1.0, 0.0]);
        assert!(!a.contains(&Explore), "{a:?}");
        assert!(a.contains(&Attack), "{a:?}");
    }

    // --- scout ---

    #[test]
    fn scout_explores_when_calm() {
        assert!(run(scout_brain, CALM).contains(&Explore));
    }

    #[test]
    fn scout_flees_danger_and_never_explores_in_it() {
        let a = run(scout_brain, [1.0, 0.0, 0.0, 0.0]);
        assert!(a.contains(&Retreat), "{a:?}");
        assert!(!a.contains(&Explore), "{a:?}");
    }

    #[test]
    fn scout_explores_sooner_toward_prey() {
        let calm = first(&run(scout_brain, CALM), Explore).unwrap_or(TICKS);
        let prey = first(&run(scout_brain, [0.0, 0.0, 1.0, 0.0]), Explore).unwrap_or(TICKS);
        assert!(prey < calm, "prey {prey} vs calm {calm}");
    }

    // --- encode_wealth ---

    fn bot_with_costs(costs: &[(BuildingType, i32)]) -> Flaud {
        let mut bot = Flaud::new(fixtures::map(), fixtures::rules());
        bot.building_costs = costs.iter().copied().collect();
        bot
    }

    fn with_resources(resources: i32) -> GameStateDto {
        let mut s = fixtures::state();
        s.you.resources = resources;
        s
    }

    #[test]
    fn wealth_is_zero_with_no_buildings_to_buy() {
        let bot = bot_with_costs(&[]);
        assert_eq!(bot.encode_wealth(&with_resources(1000)), 0.0);
    }

    #[test]
    fn wealth_is_share_of_cheapest_building() {
        let bot = bot_with_costs(&[
            (BuildingType::Barracks, 150),
            (BuildingType::ResourceDepot, 100),
        ]);
        assert_eq!(bot.encode_wealth(&with_resources(0)), 0.0);
        assert_eq!(bot.encode_wealth(&with_resources(50)), 0.5);
        assert_eq!(
            bot.encode_wealth(&with_resources(100)),
            1.0,
            "exactly affordable"
        );
    }

    #[test]
    fn wealth_caps_at_one() {
        let bot = bot_with_costs(&[(BuildingType::Barracks, 100)]);
        assert_eq!(bot.encode_wealth(&with_resources(1000)), 1.0);
        assert_eq!(bot.encode_wealth(&with_resources(-50)), 0.0);
    }

    #[test]
    fn wealth_skips_free_buildings() {
        // A cost of 0 would divide by zero; the next cheapest is used instead
        let bot = bot_with_costs(&[
            (BuildingType::ResourceDepot, 0),
            (BuildingType::Barracks, 200),
        ]);
        assert_eq!(bot.encode_wealth(&with_resources(100)), 0.5);
    }

    // --- centroid ---

    fn soldier(id: i32, owner: i32, x: i32, y: i32) -> UnitDto {
        UnitDto {
            id,
            kind: UnitType::Soldier,
            owner,
            x,
            y,
            hp: 10,
            max_hp: 10,
            activity: UnitActivity::Idle,
            carrying: 0,
            destination: None,
            target_id: None,
        }
    }

    fn with_units(units: Vec<UnitDto>) -> GameStateDto {
        let mut s = fixtures::state();
        s.units = units;
        s
    }

    #[test]
    fn centroid_averages_group_positions() {
        let me = fixtures::state().you.slot;
        let s = with_units(vec![
            soldier(1, me, 2, 2),
            soldier(2, me, 4, 6),
            soldier(3, me, 40, 40),
        ]);
        assert_eq!(
            centroid(&[1, 2], &s),
            Some((3, 4)),
            "unit 3 isn't in the group"
        );
    }

    #[test]
    fn centroid_skips_dead_units() {
        let me = fixtures::state().you.slot;
        let s = with_units(vec![soldier(1, me, 2, 2)]);
        assert_eq!(centroid(&[1, 99], &s), Some((2, 2)), "unit 99 is dead");
    }

    #[test]
    fn centroid_ignores_enemy_units_with_a_matching_id() {
        let me = fixtures::state().you.slot;
        let s = with_units(vec![soldier(1, me, 2, 2), soldier(2, me + 1, 50, 50)]);
        assert_eq!(centroid(&[1, 2], &s), Some((2, 2)));
    }

    #[test]
    fn centroid_is_none_for_a_wiped_out_or_empty_group() {
        let me = fixtures::state().you.slot;
        let s = with_units(vec![soldier(1, me, 2, 2)]);
        assert_eq!(centroid(&[98, 99], &s), None);
        assert_eq!(centroid(&[], &s), None);
    }

    #[test]
    fn stub_sends_no_commands() {
        let mut bot = Flaud::new(fixtures::map(), fixtures::rules());
        assert!(bot.decide(&fixtures::state()).is_empty());
    }
}
