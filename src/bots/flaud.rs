use std::collections::HashMap;

use crate::logic::connectome::{FlyBrainConnectome, MotorAction};
use crate::logic::models::*;
use crate::logic::shared::{Bot, encode_sugar, encode_threat};

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

    #[test]
    fn stub_sends_no_commands() {
        let mut bot = Flaud::new(fixtures::map(), fixtures::rules());
        assert!(bot.decide(&fixtures::state()).is_empty());
    }
}
