use std::collections::{HashMap, HashSet};

use crate::logic::connectome::{FlyBrainConnectome, MotorAction};
use crate::logic::models::*;
use crate::logic::sensory::{PREY, SENSES, SUGAR, THREAT, WEALTH};
use crate::logic::shared::{
    Bot, Loc, R, dist, encode_prey, encode_sugar, encode_threat, enemy_base, find_site,
    hottest_near, sample,
};

const GROUP_SIZE: usize = 5;
const DECAY: f32 = 0.8; // Determines that the minimum current possible to maybe trigger a response is (1 - DECAY)
const THRESHOLD: f32 = 1.0; // If (v > THRESHOLD) fire every tick

// Production
const TARGET_WORKERS: usize = 12;
const TARGET_SCOUTS: usize = 2;
/// Units a building may have queued before we stop adding more
const QUEUE_LIMIT: usize = 2;
/// Workers needed before we stop training them to save for the first Barracks
const BARRACKS_AT_WORKERS: usize = 6;
/// Same as Build in `urgency`. Training is added after the unit commands and
/// the sort is stable, so a Build that tick still gets the ore first.
const SPAWN_URGENCY: u8 = 2;

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

struct HeatMaps {
    threat: Vec<f64>,
    sugar: Vec<f64>,
    prey: Vec<f64>,
    w: i32,
    h: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Role {
    Worker,
    Army,
    Scout,
}

pub struct Flaud {
    regiments: Vec<BrainGroup>,
    workers: Vec<BrainGroup>,
    armies: Vec<BrainGroup>,
    building_costs: HashMap<BuildingType, i32>,
    map: MapDto,
    rules: RulesDto,
}

impl Bot for Flaud {
    fn decide(&mut self, s: &GameStateDto) -> Vec<CommandRequest> {
        self.update_groups(s);

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
        let maps = HeatMaps {
            threat,
            sugar,
            prey,
            w: s.map_width,
            h: s.map_height,
        };
        let wealth = self.encode_wealth(s);

        let mut decisions: Vec<(Role, usize, MotorAction, Loc)> = Vec::new();
        for (role, groups) in [
            (Role::Worker, &mut self.workers),
            (Role::Army, &mut self.armies),
            (Role::Scout, &mut self.regiments),
        ] {
            for (i, g) in groups.iter_mut().enumerate() {
                let Some(at) = centroid(&g.ids, s) else {
                    continue;
                };
                let action = g.brain.tick(senses(&maps, at, wealth));
                decisions.push((role, i, action, at));
            }
        }

        // Training slots in after Build, so a new Barracks gets the ore first
        let mut ranked: Vec<(u8, CommandRequest)> = decisions
            .into_iter()
            .filter_map(|(role, i, action, at)| {
                let c = self.command(action, role, &self.groups(role)[i], at, &maps, s)?;
                Some((urgency(action), c))
            })
            .chain(self.spawn_units(s).into_iter().map(|c| (SPAWN_URGENCY, c)))
            .collect();
        ranked.sort_by_key(|(u, _)| *u);

        let room = (s.you.queue_capacity - s.you.queued_commands).max(0) as usize;
        ranked.into_iter().take(room).map(|(_, c)| c).collect()
    }

    fn new(m: MapDto, r: RulesDto) -> Self {
        Self {
            regiments: vec![],
            armies: vec![],
            workers: vec![],
            building_costs: Self::building_cost(&r),
            map: m,
            rules: r,
        }
    }
}

impl Flaud {
    fn update_groups(&mut self, s: &GameStateDto) {
        let alive: HashSet<i32> = s.my_units().fold(HashSet::new(), |mut acc, u| {
            acc.insert(u.id);
            acc
        });

        for groups in [&mut self.workers, &mut self.armies, &mut self.regiments] {
            Self::prune(groups, &alive);
        }

        let assigned: HashSet<i32> = self
            .workers
            .iter()
            .chain(&self.armies)
            .chain(&self.regiments)
            .flat_map(|g| &g.ids)
            .copied()
            .collect();

        for u in s.my_units().filter(|u| !assigned.contains(&u.id)) {
            match u.kind {
                UnitType::Worker => Self::join(&mut self.workers, u.id, worker_brain),
                UnitType::Scout => Self::join(&mut self.regiments, u.id, scout_brain),
                UnitType::Soldier | UnitType::Archer => {
                    Self::join(&mut self.armies, u.id, army_brain)
                }
            }
        }
    }

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

    fn prune(groups: &mut Vec<BrainGroup>, alive: &HashSet<i32>) {
        for g in groups.iter_mut() {
            g.ids.retain(|uid| alive.contains(uid));
        }
        groups.retain(|g| !g.ids.is_empty());
    }

    fn join(groups: &mut Vec<BrainGroup>, id: i32, make: fn() -> FlyBrainConnectome) {
        match groups
            .iter_mut()
            .filter(|g| g.ids.len() < GROUP_SIZE)
            .next()
        {
            Some(free_group) => free_group.ids.push(id),
            None => groups.push(BrainGroup {
                ids: vec![id],
                brain: make(),
            }),
        }
    }

    fn groups(&self, role: Role) -> &[BrainGroup] {
        match role {
            Role::Worker => &self.workers,
            Role::Army => &self.armies,
            Role::Scout => &self.regiments,
        }
    }

    /// Units to train this tick: workers from the CC, then scouts and an even
    /// mix of soldiers and archers from wherever the rules say they're made.
    fn spawn_units(&self, s: &GameStateDto) -> Vec<CommandRequest> {
        // What we have plus what's already queued, so we don't over-order
        let mut count: HashMap<UnitType, usize> = HashMap::new();
        for u in s.my_units() {
            *count.entry(u.kind).or_default() += 1;
        }
        for p in s.my_buildings().flat_map(|b| &b.production) {
            *count.entry(p.unit_type).or_default() += 1;
        }

        // Without a Barracks there's no army, so stop making workers and save for one
        let has_barracks = s.my_buildings().any(|b| b.kind == BuildingType::Barracks);
        let workers = count.get(&UnitType::Worker).copied().unwrap_or(0);
        let saving = !has_barracks && workers >= BARRACKS_AT_WORKERS;

        // Spend from a running total so one tick can't order more than we can pay for
        let mut ore = s.you.resources;
        let mut orders = Vec::new();
        for b in s
            .my_buildings()
            .filter(|b| b.completed && b.production.len() < QUEUE_LIMIT)
        {
            let Some(kind) = self.next_unit(b.kind, &count, saving) else {
                continue;
            };
            let Some(cost) = self.rules.unit(kind).map(|u| u.cost) else {
                continue;
            };
            if ore < cost {
                continue;
            }
            ore -= cost;
            *count.entry(kind).or_default() += 1;
            orders.push(CommandRequest::produce(b.id, kind, 1));
        }
        orders
    }

    /// What a building of type `at` should train next, if anything.
    fn next_unit(
        &self,
        at: BuildingType,
        count: &HashMap<UnitType, usize>,
        saving: bool,
    ) -> Option<UnitType> {
        let n = |k: UnitType| count.get(&k).copied().unwrap_or(0);
        let made_here = |k: UnitType| self.rules.unit(k).is_some_and(|u| u.produced_at == at);

        if made_here(UnitType::Worker) && !saving && n(UnitType::Worker) < TARGET_WORKERS {
            return Some(UnitType::Worker);
        }
        if made_here(UnitType::Scout) && n(UnitType::Scout) < TARGET_SCOUTS {
            return Some(UnitType::Scout);
        }
        // Whichever fighter we have fewer of; ties go to Soldier
        [UnitType::Soldier, UnitType::Archer]
            .into_iter()
            .filter(|&k| made_here(k))
            .min_by_key(|&k| n(k))
    }

    /// The command for a group's action, or None to send nothing.
    /// `at` is the group's centroid. The server does the pathfinding, so each
    /// action only needs a destination.
    fn command(
        &self,
        action: MotorAction,
        role: Role,
        g: &BrainGroup,
        at: Loc,
        maps: &HeatMaps,
        s: &GameStateDto,
    ) -> Option<CommandRequest> {
        let ids = g.ids.clone();
        let (w, h) = (maps.w, maps.h);
        match action {
            MotorAction::HoldPosition => None,

            MotorAction::Attack => {
                let (x, y) = hottest_near(&maps.threat, w, h, at, R).unwrap_or(at);
                Some(CommandRequest::attack_move(ids, x, y))
            }

            MotorAction::Retreat => {
                let (x, y) = match my_cc(s) {
                    Some(cc) => (cc.x, cc.y),
                    None => {
                        let peak = hottest_near(&maps.threat, w, h, at, R)?;
                        (
                            (2 * at.0 - peak.0).clamp(0, w - 1),
                            (2 * at.1 - peak.1).clamp(0, h - 1),
                        )
                    }
                };
                Some(CommandRequest::move_to(ids, x, y))
            }

            MotorAction::Gather => {
                let idle: Vec<i32> = s
                    .my_units()
                    .filter(|u| g.ids.contains(&u.id))
                    .filter(|u| {
                        !matches!(
                            u.activity,
                            UnitActivity::Gathering | UnitActivity::Returning
                        )
                    })
                    .map(|u| u.id)
                    .collect();
                if idle.is_empty() {
                    return None;
                }
                let deposit = s
                    .resources
                    .iter()
                    .filter(|d| d.remaining > 0)
                    .min_by_key(|d| dist(at.0, at.1, d.x, d.y))?;
                Some(CommandRequest::gather(idle, deposit.id))
            }

            MotorAction::Explore => {
                let (x, y) = hottest_near(&maps.prey, w, h, at, w.max(h)).unwrap_or_else(|| {
                    let base = enemy_base(s);
                    (base.x, base.y)
                });
                Some(match role {
                    Role::Army => CommandRequest::attack_move(ids, x, y),
                    Role::Worker | Role::Scout => CommandRequest::move_to(ids, x, y),
                })
            }

            MotorAction::Build => {
                let kind = next_building(s);
                let cost = *self.building_costs.get(&kind)?;
                if s.you.resources < cost {
                    return None;
                }
                let cc = my_cc(s)?;
                let (x, y) = find_site(&self.map.terrain, s, cc.x, cc.y)?;
                Some(CommandRequest::build(vec![*ids.first()?], kind, x, y))
            }
        }
    }
}

fn senses(maps: &HeatMaps, at: Loc, wealth: f32) -> [f32; SENSES] {
    let (w, h, (x, y)) = (maps.w, maps.h, at);
    let mut out = [0.0; SENSES];
    out[THREAT] = sample(&maps.threat, w, h, x, y);
    out[SUGAR] = sample(&maps.sugar, w, h, x, y);
    out[PREY] = sample(&maps.prey, w, h, x, y);
    out[WEALTH] = wealth;
    out
}

fn urgency(a: MotorAction) -> u8 {
    match a {
        MotorAction::Retreat => 0,
        MotorAction::Attack => 1,
        MotorAction::Build => 2,
        MotorAction::Gather => 3,
        MotorAction::Explore => 4,
        MotorAction::HoldPosition => 5,
    }
}

fn my_cc(s: &GameStateDto) -> Option<&BuildingDto> {
    s.my_buildings()
        .find(|b| b.kind == BuildingType::CommandCenter)
}

fn next_building(s: &GameStateDto) -> BuildingType {
    if s.my_buildings().any(|b| b.kind == BuildingType::Barracks) {
        BuildingType::GuardTower
    } else {
        BuildingType::Barracks
    }
}

fn centroid(ids: &[i32], s: &GameStateDto) -> Option<Loc> {
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

    // --- update_groups ---

    fn mine(id: i32, kind: UnitType) -> UnitDto {
        let mut u = soldier(id, fixtures::state().you.slot, 0, 0);
        u.kind = kind;
        u
    }

    fn bot() -> Flaud {
        Flaud::new(fixtures::map(), fixtures::rules())
    }

    fn ids(groups: &[BrainGroup]) -> Vec<Vec<i32>> {
        groups.iter().map(|g| g.ids.clone()).collect()
    }

    #[test]
    fn new_units_are_grouped_by_role() {
        let mut b = bot();
        b.update_groups(&with_units(vec![
            mine(1, UnitType::Worker),
            mine(2, UnitType::Scout),
            mine(3, UnitType::Soldier),
            mine(4, UnitType::Archer),
        ]));
        assert_eq!(ids(&b.workers), vec![vec![1]]);
        assert_eq!(ids(&b.regiments), vec![vec![2]]);
        assert_eq!(
            ids(&b.armies),
            vec![vec![3, 4]],
            "soldiers and archers share armies"
        );
    }

    #[test]
    fn groups_fill_to_group_size_then_a_new_one_starts() {
        let mut b = bot();
        let n = GROUP_SIZE as i32 + 1;
        b.update_groups(&with_units(
            (1..=n).map(|i| mine(i, UnitType::Worker)).collect(),
        ));
        assert_eq!(b.workers.len(), 2);
        assert_eq!(b.workers[0].ids.len(), GROUP_SIZE);
        assert_eq!(b.workers[1].ids, vec![n]);
    }

    #[test]
    fn enemy_units_are_not_grouped() {
        let mut b = bot();
        let enemy = fixtures::state().you.slot + 1;
        b.update_groups(&with_units(vec![soldier(1, enemy, 0, 0)]));
        assert!(b.armies.is_empty());
    }

    #[test]
    fn units_are_not_added_twice_across_ticks() {
        let mut b = bot();
        let s = with_units(vec![mine(1, UnitType::Worker), mine(2, UnitType::Worker)]);
        for _ in 0..3 {
            b.update_groups(&s);
        }
        assert_eq!(ids(&b.workers), vec![vec![1, 2]]);
    }

    #[test]
    fn dead_units_leave_their_group() {
        let mut b = bot();
        b.update_groups(&with_units(vec![
            mine(1, UnitType::Worker),
            mine(2, UnitType::Worker),
        ]));
        b.update_groups(&with_units(vec![mine(2, UnitType::Worker)]));
        assert_eq!(ids(&b.workers), vec![vec![2]]);
    }

    #[test]
    fn wiped_out_groups_are_dropped() {
        let mut b = bot();
        b.update_groups(&with_units(vec![mine(1, UnitType::Scout)]));
        b.update_groups(&with_units(vec![]));
        assert!(b.regiments.is_empty());
    }

    #[test]
    fn surviving_groups_keep_their_brain() {
        // Rebuilding groups would reset every voltage; the brain must persist
        let mut b = bot();
        b.update_groups(&with_units(vec![
            mine(1, UnitType::Worker),
            mine(2, UnitType::Worker),
        ]));
        b.workers[0].brain.motor_neurons[0].voltage = 0.42;

        b.update_groups(&with_units(vec![
            mine(1, UnitType::Worker),
            mine(3, UnitType::Worker),
        ]));
        assert_eq!(ids(&b.workers), vec![vec![1, 3]], "2 died, 3 joined");
        assert_eq!(b.workers[0].brain.motor_neurons[0].voltage, 0.42);
    }

    #[test]
    fn new_groups_get_their_roles_brain() {
        let mut b = bot();
        b.update_groups(&with_units(vec![
            mine(1, UnitType::Worker),
            mine(2, UnitType::Scout),
            mine(3, UnitType::Soldier),
        ]));
        assert_eq!(b.workers[0].brain.actions, worker_brain().actions);
        assert_eq!(b.regiments[0].brain.actions, scout_brain().actions);
        assert_eq!(b.armies[0].brain.actions, army_brain().actions);
    }

    // --- command ---

    const W: i32 = 40;
    const H: i32 = 20;

    /// Heat maps with the listed cells warm and everything else cold.
    fn maps(threat: &[Loc], prey: &[Loc]) -> HeatMaps {
        let warm = |cells: &[Loc]| {
            let mut m = vec![0.0; (W * H) as usize];
            for &(x, y) in cells {
                m[(y * W + x) as usize] = 1.0;
            }
            m
        };
        HeatMaps {
            threat: warm(threat),
            sugar: vec![0.0; (W * H) as usize],
            prey: warm(prey),
            w: W,
            h: H,
        }
    }

    fn group(ids: &[i32]) -> BrainGroup {
        BrainGroup {
            ids: ids.to_vec(),
            brain: worker_brain(),
        }
    }

    fn my_building(id: i32, kind: BuildingType, x: i32, y: i32) -> BuildingDto {
        BuildingDto {
            id,
            kind,
            owner: fixtures::state().you.slot,
            x,
            y,
            hp: 100,
            max_hp: 100,
            completed: true,
            construction_percent: 100,
            production: vec![],
            rally: None,
            research: None,
        }
    }

    fn world() -> GameStateDto {
        let mut s = fixtures::state();
        s.map_width = W;
        s.map_height = H;
        s
    }

    fn with_cc(mut s: GameStateDto, x: i32, y: i32) -> GameStateDto {
        s.buildings
            .push(my_building(500, BuildingType::CommandCenter, x, y));
        s
    }

    /// Open terrain, and costs so a Barracks or GuardTower is affordable at 100.
    fn builder_bot() -> Flaud {
        let mut b = bot_with_costs(&[
            (BuildingType::Barracks, 100),
            (BuildingType::GuardTower, 80),
        ]);
        b.map.terrain = vec![".".repeat(W as usize); H as usize];
        b
    }

    fn cmd(
        b: &Flaud,
        a: MotorAction,
        role: Role,
        at: Loc,
        m: &HeatMaps,
        s: &GameStateDto,
    ) -> Option<CommandRequest> {
        b.command(a, role, &group(&[1, 2, 3]), at, m, s)
    }

    fn target(c: &CommandRequest) -> Loc {
        (c.x.unwrap(), c.y.unwrap())
    }

    #[test]
    fn hold_sends_nothing() {
        let c = cmd(
            &bot(),
            HoldPosition,
            Role::Army,
            (5, 5),
            &maps(&[], &[]),
            &world(),
        );
        assert!(c.is_none());
    }

    #[test]
    fn attack_moves_to_the_nearby_threat() {
        let c = cmd(
            &bot(),
            Attack,
            Role::Army,
            (5, 5),
            &maps(&[(8, 6)], &[]),
            &world(),
        )
        .unwrap();
        assert_eq!(c.kind, CommandType::Attack);
        assert_eq!(c.unit_ids, Some(vec![1, 2, 3]));
        assert_eq!(target(&c), (8, 6));
    }

    #[test]
    fn attack_ignores_a_distant_fight_and_holds_ground() {
        // The threat felt here came from within R; a fight across the map isn't it
        let c = cmd(
            &bot(),
            Attack,
            Role::Army,
            (5, 5),
            &maps(&[(35, 5)], &[]),
            &world(),
        )
        .unwrap();
        assert_eq!(target(&c), (5, 5));
    }

    #[test]
    fn retreat_runs_home_to_the_cc() {
        let s = with_cc(world(), 2, 3);
        let c = cmd(
            &bot(),
            Retreat,
            Role::Worker,
            (10, 10),
            &maps(&[(12, 10)], &[]),
            &s,
        )
        .unwrap();
        assert_eq!(c.kind, CommandType::Move);
        assert_eq!(target(&c), (2, 3));
    }

    #[test]
    fn retreat_without_a_cc_runs_away_from_the_threat() {
        let c = cmd(
            &bot(),
            Retreat,
            Role::Scout,
            (10, 10),
            &maps(&[(13, 8)], &[]),
            &world(),
        )
        .unwrap();
        assert_eq!(
            target(&c),
            (7, 12),
            "mirror of the threat through the group"
        );
    }

    #[test]
    fn retreat_away_stays_on_the_map() {
        let c = cmd(
            &bot(),
            Retreat,
            Role::Scout,
            (1, 1),
            &maps(&[(6, 6)], &[]),
            &world(),
        )
        .unwrap();
        assert_eq!(target(&c), (0, 0));
    }

    #[test]
    fn retreat_without_a_cc_or_a_threat_sends_nothing() {
        let c = cmd(
            &bot(),
            Retreat,
            Role::Scout,
            (10, 10),
            &maps(&[], &[]),
            &world(),
        );
        assert!(c.is_none());
    }

    #[test]
    fn gather_picks_the_nearest_deposit_with_ore_left() {
        let mut s = world();
        s.resources = vec![
            ResourceDepositDto {
                id: 1,
                x: 6,
                y: 5,
                remaining: 0,
            },
            ResourceDepositDto {
                id: 2,
                x: 9,
                y: 5,
                remaining: 50,
            },
            ResourceDepositDto {
                id: 3,
                x: 30,
                y: 5,
                remaining: 500,
            },
        ];
        s.units = (1..=3).map(|i| mine(i, UnitType::Worker)).collect();
        let c = cmd(&bot(), Gather, Role::Worker, (5, 5), &maps(&[], &[]), &s).unwrap();
        assert_eq!(c.kind, CommandType::Gather);
        assert_eq!(c.target_id, Some(2), "1 is empty, 3 is farther");
    }

    #[test]
    fn gather_only_orders_workers_that_are_not_already_mining() {
        let mut s = world();
        s.resources = vec![ResourceDepositDto {
            id: 7,
            x: 9,
            y: 5,
            remaining: 50,
        }];
        let mut mining = mine(1, UnitType::Worker);
        mining.activity = UnitActivity::Gathering;
        let mut carrying = mine(2, UnitType::Worker);
        carrying.activity = UnitActivity::Returning;
        s.units = vec![mining, carrying, mine(3, UnitType::Worker)];

        let c = cmd(&bot(), Gather, Role::Worker, (5, 5), &maps(&[], &[]), &s).unwrap();
        assert_eq!(c.unit_ids, Some(vec![3]), "1 and 2 are busy");

        s.units.truncate(2);
        let c = cmd(&bot(), Gather, Role::Worker, (5, 5), &maps(&[], &[]), &s);
        assert!(c.is_none(), "everyone is busy");
    }

    // --- senses ---

    #[test]
    fn senses_puts_each_map_in_its_weight_row_slot() {
        // One map warm at a time, so a swapped slot shows up as the wrong index
        let at = (5, 5);
        let mut m = maps(&[at], &[]);
        assert_eq!(senses(&m, at, 0.0), [1.0, 0.0, 0.0, 0.0], "threat");

        m.threat[(at.1 * W + at.0) as usize] = 0.0;
        m.sugar[(at.1 * W + at.0) as usize] = 1.0;
        assert_eq!(senses(&m, at, 0.0), [0.0, 1.0, 0.0, 0.0], "sugar");

        assert_eq!(
            senses(&maps(&[], &[at]), at, 0.0),
            [0.0, 0.0, 1.0, 0.0],
            "prey"
        );
        assert_eq!(
            senses(&maps(&[], &[]), at, 0.7),
            [0.0, 0.0, 0.0, 0.7],
            "wealth"
        );
    }

    #[test]
    fn urgent_actions_sort_first() {
        let mut a = [Explore, Gather, HoldPosition, Build, Attack, Retreat];
        a.sort_by_key(|&x| urgency(x));
        assert_eq!(a, [Retreat, Attack, Build, Gather, Explore, HoldPosition]);
    }

    // --- decide: a whole tick ---

    fn placed(id: i32, kind: UnitType, owner: i32, x: i32, y: i32) -> UnitDto {
        let mut u = soldier(id, owner, x, y);
        u.kind = kind;
        u
    }

    /// Commands from `TICKS` ticks of the same state.
    fn play(s: &GameStateDto) -> Vec<Vec<CommandRequest>> {
        let mut b = bot();
        b.map.terrain = vec![".".repeat(W as usize); H as usize];
        (0..TICKS).map(|_| b.decide(s)).collect()
    }

    #[test]
    fn idle_workers_near_ore_get_sent_to_gather() {
        let me = fixtures::state().you.slot;
        let mut s = world();
        s.units = vec![
            placed(1, UnitType::Worker, me, 10, 10),
            placed(2, UnitType::Worker, me, 11, 10),
        ];
        s.resources = vec![ResourceDepositDto {
            id: 9,
            x: 12,
            y: 10,
            remaining: 100,
        }];
        let sent = play(&s).concat();
        let gather = sent.iter().find(|c| c.kind == CommandType::Gather);
        assert_eq!(gather.and_then(|c| c.target_id), Some(9), "{sent:?}");
    }

    #[test]
    fn busy_workers_are_left_alone() {
        let me = fixtures::state().you.slot;
        let mut s = world();
        let mut w = placed(1, UnitType::Worker, me, 10, 10);
        w.activity = UnitActivity::Gathering;
        s.units = vec![w];
        s.resources = vec![ResourceDepositDto {
            id: 9,
            x: 12,
            y: 10,
            remaining: 100,
        }];
        assert!(
            play(&s)
                .concat()
                .iter()
                .all(|c| c.kind != CommandType::Gather)
        );
    }

    #[test]
    fn army_attacks_an_enemy_next_to_it() {
        let me = fixtures::state().you.slot;
        let mut s = world();
        s.units = vec![
            placed(1, UnitType::Soldier, me, 10, 10),
            placed(2, UnitType::Soldier, me + 1, 12, 10),
        ];
        let sent = play(&s).concat();
        let attack = sent
            .iter()
            .find(|c| c.kind == CommandType::Attack)
            .unwrap_or_else(|| panic!("no attack in {sent:?}"));
        assert_eq!(target(attack), (12, 10), "at the enemy");
        assert_eq!(attack.unit_ids, Some(vec![1]));
    }

    #[test]
    fn calm_scouts_go_exploring() {
        let me = fixtures::state().you.slot;
        let mut s = world();
        s.units = vec![placed(1, UnitType::Scout, me, 3, 3)];
        let sent = play(&s).concat();
        assert!(sent.iter().any(|c| c.kind == CommandType::Move), "{sent:?}");
    }

    #[test]
    fn decide_never_overfills_the_command_queue() {
        // 30 calm scouts make 6 groups whose identical brains all fire on the same tick
        let me = fixtures::state().you.slot;
        let mut s = world();
        s.units = (1..=30)
            .map(|i| placed(i, UnitType::Scout, me, i % W, 3))
            .collect();
        s.you.queue_capacity = 3;
        s.you.queued_commands = 1;
        let ticks = play(&s);
        assert!(ticks.iter().all(|t| t.len() <= 2), "room was 2");
        assert!(ticks.iter().any(|t| t.len() == 2), "and it was used");
    }

    #[test]
    fn gather_with_no_deposits_sends_nothing() {
        let c = cmd(
            &bot(),
            Gather,
            Role::Worker,
            (5, 5),
            &maps(&[], &[]),
            &world(),
        );
        assert!(c.is_none());
    }

    #[test]
    fn explore_hunts_the_best_prey_anywhere() {
        let m = maps(&[], &[(35, 15)]);
        let army = cmd(&bot(), Explore, Role::Army, (2, 2), &m, &world()).unwrap();
        assert_eq!(target(&army), (35, 15));
        assert_eq!(army.kind, CommandType::Attack, "armies fight on the way");

        let scout = cmd(&bot(), Explore, Role::Scout, (2, 2), &m, &world()).unwrap();
        assert_eq!(target(&scout), (35, 15));
        assert_eq!(
            scout.kind,
            CommandType::Move,
            "scouts look, they don't fight"
        );
    }

    #[test]
    fn explore_with_no_prey_heads_for_the_enemy_base() {
        // No players listed, so enemy_base falls back to the map centre
        let c = cmd(
            &bot(),
            Explore,
            Role::Scout,
            (2, 2),
            &maps(&[], &[]),
            &world(),
        )
        .unwrap();
        assert_eq!(target(&c), (W / 2, H / 2));
    }

    #[test]
    fn build_sends_one_worker_to_build_a_barracks_first() {
        let mut s = with_cc(world(), 20, 10);
        s.you.resources = 100;
        let c = cmd(
            &builder_bot(),
            Build,
            Role::Worker,
            (20, 10),
            &maps(&[], &[]),
            &s,
        )
        .unwrap();
        assert_eq!(c.kind, CommandType::Build);
        assert_eq!(
            c.unit_ids,
            Some(vec![1]),
            "one builder, not the whole group"
        );
        assert_eq!(c.building_type, Some(BuildingType::Barracks));
        assert!((3..=6).contains(&dist(c.x.unwrap(), c.y.unwrap(), 20, 10)));
    }

    #[test]
    fn build_picks_a_tower_once_a_barracks_exists() {
        let mut s = with_cc(world(), 20, 10);
        s.buildings
            .push(my_building(501, BuildingType::Barracks, 25, 10));
        s.you.resources = 100;
        let c = cmd(
            &builder_bot(),
            Build,
            Role::Worker,
            (20, 10),
            &maps(&[], &[]),
            &s,
        )
        .unwrap();
        assert_eq!(c.building_type, Some(BuildingType::GuardTower));
    }

    #[test]
    fn build_waits_until_the_chosen_building_is_affordable() {
        let mut s = with_cc(world(), 20, 10);
        s.you.resources = 90; // a tower (80) would do, but a Barracks (100) is next
        let c = cmd(
            &builder_bot(),
            Build,
            Role::Worker,
            (20, 10),
            &maps(&[], &[]),
            &s,
        );
        assert!(c.is_none());
    }

    #[test]
    fn build_without_a_cc_sends_nothing() {
        let mut s = world();
        s.you.resources = 1000;
        let c = cmd(
            &builder_bot(),
            Build,
            Role::Worker,
            (20, 10),
            &maps(&[], &[]),
            &s,
        );
        assert!(c.is_none());
    }

    // --- spawn_units ---

    fn stats(kind: UnitType, cost: i32, produced_at: BuildingType) -> UnitStatsDto {
        UnitStatsDto {
            kind,
            armor: 0,
            attack_cooldown: 1,
            build_ticks: 10,
            cost,
            damage: 1,
            max_hp: 10,
            produced_at,
            range: 1,
            speed: 1,
            vision: 5,
        }
    }

    /// Workers from the CC at 50; scouts, soldiers and archers from the Barracks.
    fn trainer_bot() -> Flaud {
        let mut b = bot();
        b.rules.units = vec![
            stats(UnitType::Worker, 50, BuildingType::CommandCenter),
            stats(UnitType::Scout, 40, BuildingType::Barracks),
            stats(UnitType::Soldier, 60, BuildingType::Barracks),
            stats(UnitType::Archer, 70, BuildingType::Barracks),
        ];
        b
    }

    fn base(resources: i32, buildings: Vec<BuildingDto>, units: Vec<UnitDto>) -> GameStateDto {
        let mut s = world();
        s.you.resources = resources;
        s.buildings = buildings;
        s.units = units;
        s
    }

    fn cc() -> BuildingDto {
        my_building(500, BuildingType::CommandCenter, 20, 10)
    }

    fn barracks(id: i32) -> BuildingDto {
        my_building(id, BuildingType::Barracks, 25, 10)
    }

    fn workers(n: i32) -> Vec<UnitDto> {
        (1..=n).map(|i| mine(i, UnitType::Worker)).collect()
    }

    fn trained(orders: &[CommandRequest]) -> Vec<(i32, UnitType)> {
        orders
            .iter()
            .map(|c| (c.building_id.unwrap(), c.unit_type.unwrap()))
            .collect()
    }

    #[test]
    fn cc_trains_a_worker_when_affordable() {
        let s = base(50, vec![cc()], workers(3));
        let orders = trainer_bot().spawn_units(&s);
        assert_eq!(trained(&orders), vec![(500, UnitType::Worker)]);
        assert_eq!(orders[0].kind, CommandType::Produce);
    }

    #[test]
    fn nothing_is_trained_without_the_ore() {
        let s = base(49, vec![cc()], workers(3));
        assert!(trainer_bot().spawn_units(&s).is_empty());
    }

    #[test]
    fn cc_stops_at_the_worker_target_counting_the_queue() {
        let mut c = cc();
        c.production = vec![ProductionDto {
            unit_type: UnitType::Worker,
            percent: 50,
        }];
        let s = base(
            1000,
            vec![c, barracks(501)],
            workers(TARGET_WORKERS as i32 - 1),
        );
        let orders = trainer_bot().spawn_units(&s);
        assert!(
            !trained(&orders).contains(&(500, UnitType::Worker)),
            "11 alive + 1 queued is already 12"
        );
    }

    #[test]
    fn workers_pause_to_save_for_the_first_barracks() {
        let n = BARRACKS_AT_WORKERS as i32;
        let saving = base(1000, vec![cc()], workers(n));
        assert!(trainer_bot().spawn_units(&saving).is_empty());

        let has_barracks = base(1000, vec![cc(), barracks(501)], workers(n));
        assert!(
            trained(&trainer_bot().spawn_units(&has_barracks)).contains(&(500, UnitType::Worker))
        );
    }

    #[test]
    fn barracks_trains_scouts_first_then_a_soldier_archer_mix() {
        let me = fixtures::state().you.slot;
        let bot = trainer_bot();
        let next = |units: Vec<UnitDto>| {
            trained(&bot.spawn_units(&base(1000, vec![barracks(501)], units)))
        };

        assert_eq!(next(vec![]), vec![(501, UnitType::Scout)]);

        let two_scouts = vec![
            placed(1, UnitType::Scout, me, 0, 0),
            placed(2, UnitType::Scout, me, 0, 0),
        ];
        assert_eq!(
            next(two_scouts.clone()),
            vec![(501, UnitType::Soldier)],
            "tie goes to Soldier"
        );

        let mut with_soldier = two_scouts;
        with_soldier.push(placed(3, UnitType::Soldier, me, 0, 0));
        assert_eq!(
            next(with_soldier),
            vec![(501, UnitType::Archer)],
            "fewer archers"
        );
    }

    #[test]
    fn unfinished_or_busy_buildings_train_nothing() {
        let mut building = barracks(501);
        building.completed = false;
        let mut busy = barracks(502);
        busy.production = vec![
            ProductionDto {
                unit_type: UnitType::Soldier,
                percent: 10
            };
            QUEUE_LIMIT
        ];
        let s = base(1000, vec![building, busy], vec![]);
        assert!(trainer_bot().spawn_units(&s).is_empty());
    }

    #[test]
    fn one_tick_never_orders_more_than_the_ore_covers() {
        // Two Barracks want a scout (40) each, but there's only 70
        let s = base(70, vec![barracks(501), barracks(502)], vec![]);
        assert_eq!(
            trained(&trainer_bot().spawn_units(&s)),
            vec![(501, UnitType::Scout)]
        );
    }

    #[test]
    fn units_missing_from_the_rules_are_never_trained() {
        let s = base(1000, vec![cc(), barracks(501)], vec![]);
        assert!(
            bot().spawn_units(&s).is_empty(),
            "fixture rules list no units"
        );
    }

    #[test]
    fn decide_sends_training_orders() {
        let mut b = trainer_bot();
        let orders = b.decide(&base(50, vec![cc()], vec![]));
        assert!(
            orders.iter().any(|c| c.kind == CommandType::Produce),
            "{orders:?}"
        );
    }

    #[test]
    fn a_full_queue_keeps_a_retreat_over_training() {
        // Room for one command: a threatened worker's Retreat must win over the CC's order
        let me = fixtures::state().you.slot;
        let mut s = base(
            1000,
            vec![cc(), barracks(501)],
            vec![placed(1, UnitType::Worker, me, 10, 10)],
        );
        s.units.push(placed(2, UnitType::Soldier, me + 1, 11, 10));
        s.you.queue_capacity = 1;
        let mut b = trainer_bot();
        let ticks: Vec<_> = (0..TICKS).map(|_| b.decide(&s)).collect();
        let retreat_tick = ticks
            .iter()
            .find(|t| t.iter().any(|c| c.kind == CommandType::Move))
            .unwrap_or_else(|| panic!("worker never retreated: {ticks:?}"));
        assert_eq!(retreat_tick.len(), 1);
    }

    #[test]
    fn no_units_sends_no_commands() {
        let mut bot = Flaud::new(fixtures::map(), fixtures::rules());
        assert!(bot.decide(&fixtures::state()).is_empty());
    }
}
