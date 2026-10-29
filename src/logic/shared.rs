use crate::data::models::{BuildingType, *};
use std::collections::{HashMap, HashSet};

/// A chunk of touching deposits that still have ore, and the box around them.
pub struct Bound {
    /// Deposit ids in the chunk; its size is `ids.len()`
    pub ids: Vec<i32>,
    pub lx: i32,
    pub ly: i32,
    pub ux: i32,
    pub uy: i32,
}

impl Bound {
    fn at(d: &ResourceDepositDto) -> Self {
        Self {
            ids: vec![d.id],
            lx: d.x,
            ly: d.y,
            ux: d.x,
            uy: d.y,
        }
    }

    /// Adds a deposit, moving each edge out only as far as it needs to go.
    fn add(&mut self, d: &ResourceDepositDto) {
        self.ids.push(d.id);
        self.lx = self.lx.min(d.x);
        self.ly = self.ly.min(d.y);
        self.ux = self.ux.max(d.x);
        self.uy = self.uy.max(d.y);
    }

    /// Chebyshev distance from (x, y) to the nearest tile of the box; 0 inside it.
    pub fn dist_to(&self, x: i32, y: i32) -> i32 {
        let dx = (self.lx - x).max(0).max(x - self.ux);
        let dy = (self.ly - y).max(0).max(y - self.uy);
        dx.max(dy)
    }
}

pub const STIMULI_LOOKUP: [f64; 10] = [
    10.0,
    8.824969025845954,
    6.065306597126334,
    3.2465246735834974,
    1.353352832366127,
    0.4393693362340742,
    0.11108996538242306,
    0.02187491118182885,
    0.0033546262790251184,
    0.0004006529739295107,
];
/// How far one source reaches on a heat map, in tiles
pub const R: i32 = (STIMULI_LOOKUP.len() - 1) as i32;

pub type Loc = (i32, i32);
const UNIT_WEIGHT: f64 = 1.0;
const BARRACKS_WEIGHT: f64 = 0.8;
const CC_WEIGHT: f64 = 0.6;
const TOWER_WEIGHT: f64 = 0.55;
const BUILDING_WEIGHT: f64 = 0.3;

/// One tick's heat maps, each `w * h` long, row-major and normalised to 0..=1.
pub struct HeatMaps {
    pub threat: Vec<f64>,
    pub sugar: Vec<f64>,
    pub prey: Vec<f64>,
    pub w: i32,
    pub h: i32,
}

pub trait Bot {
    fn decide(&mut self, state: &GameStateDto) -> Vec<CommandRequest>;

    /// The maps from the last `decide`, for the match screen. Bots without any return None.
    fn heat_maps(&self) -> Option<&HeatMaps> {
        None
    }

    fn building_cost(r: &RulesDto) -> HashMap<BuildingType, i32> {
        r.buildings.iter().fold(HashMap::new(), |mut acc, b| {
            acc.insert(b.kind, b.cost);
            acc
        })
    }

    fn new(m: MapDto, r: RulesDto) -> Self;

    /// Called once with the final state after the match completes, e.g. to
    /// save what was learned. Bots with nothing to keep do nothing.
    fn finish(&mut self, _state: &GameStateDto) -> std::io::Result<()> {
        Ok(())
    }
}

/// Adds one source's falloff into `map`. Clamps the loop bounds once,
/// so there's no per-cell bounds check and no wasted iterations at edges.
fn region_map(map: &mut [f64], w: i32, h: i32, cx: i32, cy: i32, weight: f64) {
    let (x0, x1) = ((cx - R).max(0), (cx + R).min(w - 1));
    let (y0, y1) = ((cy - R).max(0), (cy + R).min(h - 1));
    for y in y0..=y1 {
        let row = (y * w) as usize;
        let dy = (y - cy).abs();
        for x in x0..=x1 {
            let d = (x - cx).abs().max(dy); // Chebyshev, same as your dist()
            map[row + x as usize] += weight * STIMULI_LOOKUP[d as usize];
        }
    }
}

fn normalize(map: &mut [f64]) {
    let max = map.iter().cloned().fold(0.0, f64::max);
    if max > 0.0 {
        map.iter_mut().for_each(|v| *v /= max);
    }
}

pub fn encode_threat(s: &GameStateDto) -> Vec<f64> {
    let (w, h) = (s.map_width as i32, s.map_height as i32);
    let mut map = vec![0.0; (w * h) as usize];
    // Workers are prey, not threats
    for u in s.enemy_units().filter(|u| u.kind != UnitType::Worker) {
        region_map(&mut map, w, h, u.x, u.y, UNIT_WEIGHT);
    }
    for b in s.enemy_buildings().filter(|b| b.kind == BuildingType::GuardTower) {
        region_map(&mut map, w, h, b.x, b.y, UNIT_WEIGHT);
    }
    normalize(&mut map);
    map
}

pub fn encode_sugar(s: &GameStateDto) -> Vec<f64> {
    let (w, h) = (s.map_width as i32, s.map_height as i32);
    let mut map = vec![0.0; (w * h) as usize];
    for r in &s.resources {
        region_map(&mut map, w, h, r.x, r.y, r.remaining as f64);
    }
    normalize(&mut map);
    map
}

fn prey_weight(kind: BuildingType) -> f64 {
    match kind {
        BuildingType::CommandCenter => CC_WEIGHT,
        BuildingType::Barracks => BARRACKS_WEIGHT,
        BuildingType::GuardTower => TOWER_WEIGHT,
        _ => BUILDING_WEIGHT,
    }
}

pub fn encode_prey(s: &GameStateDto) -> Vec<f64> {
    let (w, h) = (s.map_width as i32, s.map_height as i32);
    let mut map = vec![0.0; (w * h) as usize];
    for u in s.enemy_units().filter(|u| u.kind == UnitType::Worker) {
        region_map(&mut map, w, h, u.x, u.y, UNIT_WEIGHT);
    }
    for b in s.enemy_buildings() {
        region_map(&mut map, w, h, b.x, b.y, prey_weight(b.kind));
    }
    // Enemy buildings seen before but out of sight now; skip any still visible
    let remembered = s
        .remembered_buildings
        .iter()
        .filter(|b| b.owner != s.you.slot)
        .filter(|b| !s.buildings.iter().any(|v| v.id == b.id));
    for b in remembered {
        region_map(&mut map, w, h, b.x, b.y, prey_weight(b.kind));
    }
    normalize(&mut map);
    map
}

/// Heat at (x, y), or 0 off the map.
pub fn sample(map: &[f64], w: i32, h: i32, x: i32, y: i32) -> f32 {
    if x < 0 || y < 0 || x >= w || y >= h {
        return 0.0;
    }
    map[(y * w + x) as usize] as f32
}

/// Unit step (-1..=1, -1..=1) toward the hottest neighbour, or (0, 0) on a peak.
pub fn uphill(map: &[f64], w: i32, h: i32, x: i32, y: i32) -> (i32, i32) {
    let mut best = ((0, 0), sample(map, w, h, x, y));
    for dy in -1..=1 {
        for dx in -1..=1 {
            let (nx, ny) = (x + dx, y + dy);
            // Off-map cells read 0, which would beat a negative cell
            if nx < 0 || ny < 0 || nx >= w || ny >= h {
                continue;
            }
            let v = sample(map, w, h, nx, ny);
            // Strictly greater, so a peak or flat ground stays put
            if v > best.1 {
                best = ((dx, dy), v);
            }
        }
    }
    best.0
}

/// The hottest cell within `radius` of `at`, or None if nothing there is warm.
/// Pass `w.max(h)` as the radius to search the whole map.
pub fn hottest_near(map: &[f64], w: i32, h: i32, at: Loc, radius: i32) -> Option<Loc> {
    let (x0, x1) = ((at.0 - radius).max(0), (at.0 + radius).min(w - 1));
    let (y0, y1) = ((at.1 - radius).max(0), (at.1 + radius).min(h - 1));
    let mut best: Option<(Loc, f64)> = None;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let v = map[(y * w + x) as usize];
            if v > 0.0 && best.is_none_or(|(_, b)| v > b) {
                best = Some(((x, y), v));
            }
        }
    }
    best.map(|(loc, _)| loc)
}

/// An open tile 3-6 tiles from (cx, cy), not on/next to a deposit and not on a building.
pub fn find_site(terrain: &[String], s: &GameStateDto, cx: i32, cy: i32) -> Option<Loc> {
    let taken: HashSet<Loc> = s.buildings.iter().map(|b| (b.x, b.y)).collect();
    let ore: Vec<Loc> = s.resources.iter().map(|d| (d.x, d.y)).collect();
    let height = terrain.len() as i32;
    let width = terrain.first().map_or(0, |r| r.len()) as i32;

    let mut best: Option<(i32, i32, i32)> = None;
    for y in cy - 6..=cy + 6 {
        for x in cx - 6..=cx + 6 {
            let d = dist(x, y, cx, cy);
            if !((3..=6).contains(&d) && (0..width).contains(&x) && (0..height).contains(&y)) {
                continue;
            }
            if terrain[y as usize].as_bytes()[x as usize] != b'.' || taken.contains(&(x, y)) {
                continue;
            }
            if ore.iter().any(|&(ox, oy)| dist(x, y, ox, oy) <= 1) {
                continue;
            }
            // Prefer close to the centre, and away from the ore so we don't block the miners.
            let near_ore = ore
                .iter()
                .map(|&(ox, oy)| dist(x, y, ox, oy))
                .min()
                .unwrap_or(99);
            let score = d - near_ore.min(4);
            if best.is_none_or(|(b, _, _)| score < b) {
                best = Some((score, x, y));
            }
        }
    }
    best.map(|(_, x, y)| (x, y))
}

/// Start position of the first opponent still in the game, or the map centre.
pub fn enemy_base(s: &GameStateDto) -> PositionDto {
    let me = s.you.slot;
    s.players
        .iter()
        .find(|p| p.slot != me && !p.eliminated)
        .map(|p| p.start_position)
        .unwrap_or(PositionDto {
            x: s.map_width / 2,
            y: s.map_height / 2,
        })
}

/// Splits the deposits with ore left into chunks a worker group can mine alone.
/// Two deposits share a chunk if a chain of deposits each touching the next
/// (Chebyshev distance 1) links them, so input order doesn't matter.
pub fn group_resources(deposits: &[ResourceDepositDto]) -> Vec<Bound> {
    let live: Vec<&ResourceDepositDto> = deposits.iter().filter(|d| d.remaining > 0).collect();
    let mut seen = vec![false; live.len()];
    let mut chunks = Vec::new();

    for start in 0..live.len() {
        if seen[start] {
            continue;
        }
        // Flood fill: keep pulling in unseen deposits touching any already in the chunk
        seen[start] = true;
        let mut chunk = Bound::at(live[start]);
        let mut frontier = vec![start];
        while let Some(i) = frontier.pop() {
            for j in 0..live.len() {
                if !seen[j] && dist(live[i].x, live[i].y, live[j].x, live[j].y) <= 1 {
                    seen[j] = true;
                    chunk.add(live[j]);
                    frontier.push(j);
                }
            }
        }
        chunks.push(chunk);
    }
    chunks
}

/// Calculates the chebyshev distance between source and target
pub fn dist(sx: i32, sy: i32, tx: i32, ty: i32) -> i32 {
    (sx - tx).abs().max((sy - ty).abs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bots::fixtures;

    const EPS: f64 = 1e-9;
    const ME: i32 = 0;
    const ENEMY: i32 = 1;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < EPS
    }

    fn unit(id: i32, owner: i32, x: i32, y: i32) -> UnitDto {
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

    fn deposit(id: i32, x: i32, y: i32, remaining: i32) -> ResourceDepositDto {
        ResourceDepositDto { id, x, y, remaining }
    }

    fn state(
        w: i32,
        h: i32,
        units: Vec<UnitDto>,
        resources: Vec<ResourceDepositDto>,
    ) -> GameStateDto {
        let mut s = fixtures::state();
        s.you.slot = ME;
        s.map_width = w;
        s.map_height = h;
        s.units = units;
        s.resources = resources;
        s
    }

    fn at(map: &[f64], w: i32, x: i32, y: i32) -> f64 {
        map[(y * w + x) as usize]
    }

    /// (x, y) of the hottest cell.
    fn peak(map: &[f64], w: i32) -> (i32, i32) {
        let i = map
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .unwrap()
            .0 as i32;
        (i % w, i / w)
    }

    // --- region_map ---

    #[test]
    fn region_map_follows_lookup_by_chebyshev_distance() {
        let (w, h, cx, cy) = (25, 25, 12, 12);
        let mut map = vec![0.0; (w * h) as usize];
        region_map(&mut map, w, h, cx, cy, 1.0);
        for y in 0..h {
            for x in 0..w {
                let d = dist(cx, cy, x, y);
                let expected = if d <= R { STIMULI_LOOKUP[d as usize] } else { 0.0 };
                assert!(approx(at(&map, w, x, y), expected), "cell ({x}, {y}) d={d}");
            }
        }
    }

    #[test]
    fn region_map_scales_by_weight() {
        let (w, h) = (5, 5);
        let mut map = vec![0.0; 25];
        region_map(&mut map, w, h, 2, 2, 3.5);
        assert!(approx(at(&map, w, 2, 2), 3.5 * STIMULI_LOOKUP[0]));
        assert!(approx(at(&map, w, 0, 0), 3.5 * STIMULI_LOOKUP[2]));
    }

    #[test]
    fn region_map_clips_at_edges_and_corners() {
        // Sources on every corner of a map smaller than the falloff radius
        let (w, h) = (4, 3);
        for (cx, cy) in [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)] {
            let mut map = vec![0.0; (w * h) as usize];
            region_map(&mut map, w, h, cx, cy, 1.0);
            assert!(approx(at(&map, w, cx, cy), STIMULI_LOOKUP[0]));
            for y in 0..h {
                for x in 0..w {
                    let d = dist(cx, cy, x, y) as usize;
                    assert!(approx(at(&map, w, x, y), STIMULI_LOOKUP[d]));
                }
            }
        }
    }

    #[test]
    fn region_map_accumulates_overlapping_sources() {
        let (w, h) = (10, 1);
        let mut map = vec![0.0; 10];
        region_map(&mut map, w, h, 4, 0, 1.0);
        region_map(&mut map, w, h, 6, 0, 1.0);
        // Cell 5 is distance 1 from both
        assert!(approx(at(&map, w, 5, 0), 2.0 * STIMULI_LOOKUP[1]));
        assert!(approx(at(&map, w, 4, 0), STIMULI_LOOKUP[0] + STIMULI_LOOKUP[2]));
    }

    #[test]
    fn region_map_uses_row_major_layout() {
        // Non-square map so a swapped x/y would land on a different cell
        let (w, h) = (10, 5);
        let mut map = vec![0.0; 50];
        region_map(&mut map, w, h, 7, 2, 1.0);
        assert!(approx(map[2 * 10 + 7], STIMULI_LOOKUP[0]));
        assert_eq!(peak(&map, w), (7, 2));
    }

    // --- normalize ---

    #[test]
    fn normalize_scales_max_to_one_and_keeps_ratios() {
        let mut map = vec![0.0, 2.0, 5.0, 10.0];
        normalize(&mut map);
        assert_eq!(map, vec![0.0, 0.2, 0.5, 1.0]);
    }

    #[test]
    fn normalize_leaves_all_zero_map_untouched() {
        let mut map = vec![0.0; 6];
        normalize(&mut map);
        assert!(map.iter().all(|v| *v == 0.0), "no NaN from a zero max");
    }

    #[test]
    fn normalize_handles_empty_map() {
        let mut map: Vec<f64> = vec![];
        normalize(&mut map);
        assert!(map.is_empty());
    }

    // --- encode_threat ---

    #[test]
    fn threat_map_is_empty_without_enemies() {
        let s = state(8, 6, vec![unit(1, ME, 3, 3)], vec![]);
        let map = encode_threat(&s);
        assert_eq!(map.len(), 48);
        assert!(map.iter().all(|v| *v == 0.0), "own units are not threats");
    }

    #[test]
    fn threat_map_peaks_on_enemy_and_falls_off() {
        let (w, h) = (20, 12);
        let s = state(w, h, vec![unit(1, ENEMY, 13, 4), unit(2, ME, 2, 2)], vec![]);
        let map = encode_threat(&s);
        assert_eq!(peak(&map, w), (13, 4));
        assert!(approx(at(&map, w, 13, 4), 1.0));
        for d in 1..=R {
            let x = 13 - d.min(13);
            let expected = STIMULI_LOOKUP[dist(13, 4, x, 4) as usize] / STIMULI_LOOKUP[0];
            assert!(approx(at(&map, w, x, 4), expected), "d={d}");
        }
        assert_eq!(at(&map, w, 0, 11), 0.0, "out of range is cold");
    }

    #[test]
    fn threat_map_finds_every_separated_enemy() {
        // More than 2R apart so the falloffs don't touch
        let (w, h) = (40, 10);
        let s = state(w, h, vec![unit(1, ENEMY, 5, 5), unit(2, ENEMY, 34, 5)], vec![]);
        let map = encode_threat(&s);
        assert!(approx(at(&map, w, 5, 5), 1.0));
        assert!(approx(at(&map, w, 34, 5), 1.0));
        assert_eq!(at(&map, w, 20, 5), 0.0, "gap between them is cold");
    }

    #[test]
    fn threat_map_hottest_where_enemies_cluster() {
        let (w, h) = (20, 10);
        let s = state(w, h, vec![unit(1, ENEMY, 10, 5), unit(2, ENEMY, 12, 5)], vec![]);
        let map = encode_threat(&s);
        // Column x=11 is Chebyshev distance 1 from both, so it ties for hottest
        for y in 4..=6 {
            assert!(approx(at(&map, w, 11, y), 1.0), "(11, {y})");
        }
        assert_eq!(peak(&map, w).0, 11);
        assert!(at(&map, w, 10, 5) < 1.0);
        assert!(at(&map, w, 12, 5) < 1.0);
    }

    fn building(id: i32, kind: BuildingType, owner: i32, x: i32, y: i32) -> BuildingDto {
        BuildingDto {
            id,
            kind,
            owner,
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

    #[test]
    fn threat_map_ignores_enemy_workers() {
        // Workers are prey, not threats; counting them would cancel the pull toward them
        let (w, h) = (40, 10);
        let mut worker = unit(1, ENEMY, 30, 5);
        worker.kind = UnitType::Worker;
        let s = state(w, h, vec![unit(2, ENEMY, 5, 5), worker], vec![]);
        let map = encode_threat(&s);
        assert!(approx(at(&map, w, 5, 5), 1.0));
        assert_eq!(at(&map, w, 30, 5), 0.0);
    }

    #[test]
    fn threat_map_counts_every_combat_unit_type() {
        let (w, h) = (80, 10);
        let kinds = [UnitType::Soldier, UnitType::Archer, UnitType::Scout];
        let units = kinds
            .iter()
            .enumerate()
            .map(|(i, k)| {
                let mut u = unit(i as i32, ENEMY, 5 + 25 * i as i32, 5);
                u.kind = *k;
                u
            })
            .collect();
        let map = encode_threat(&state(w, h, units, vec![]));
        for i in 0..kinds.len() as i32 {
            assert!(at(&map, w, 5 + 25 * i, 5) > 0.0, "{:?}", kinds[i as usize]);
        }
    }

    #[test]
    fn threat_map_includes_enemy_guard_towers_only() {
        let (w, h) = (60, 10);
        let mut s = state(w, h, vec![], vec![]);
        s.buildings = vec![
            building(1, BuildingType::GuardTower, ENEMY, 5, 5),
            building(2, BuildingType::Barracks, ENEMY, 30, 5),
            building(3, BuildingType::GuardTower, ME, 54, 5),
        ];
        let map = encode_threat(&s);
        assert!(approx(at(&map, w, 5, 5), 1.0), "enemy tower is a threat");
        assert_eq!(at(&map, w, 30, 5), 0.0, "barracks can't shoot");
        assert_eq!(at(&map, w, 54, 5), 0.0, "own tower is not a threat");
    }

    // --- encode_sugar ---

    #[test]
    fn sugar_map_is_empty_without_resources() {
        let s = state(8, 6, vec![unit(1, ENEMY, 3, 3)], vec![]);
        let map = encode_sugar(&s);
        assert_eq!(map.len(), 48);
        assert!(map.iter().all(|v| *v == 0.0));
    }

    #[test]
    fn sugar_map_weights_deposits_by_remaining() {
        let (w, h) = (40, 10);
        let s = state(w, h, vec![], vec![deposit(1, 4, 4, 100), deposit(2, 33, 6, 50)]);
        let map = encode_sugar(&s);
        assert_eq!(peak(&map, w), (4, 4), "richest deposit is hottest");
        assert!(approx(at(&map, w, 4, 4), 1.0));
        assert!(approx(at(&map, w, 33, 6), 0.5));
    }

    #[test]
    fn sugar_map_ignores_depleted_deposits() {
        let (w, h) = (40, 10);
        let s = state(w, h, vec![], vec![deposit(1, 4, 4, 0), deposit(2, 33, 6, 20)]);
        let map = encode_sugar(&s);
        assert_eq!(at(&map, w, 4, 4), 0.0);
        assert!(approx(at(&map, w, 33, 6), 1.0));

        let empty = state(w, h, vec![], vec![deposit(1, 4, 4, 0)]);
        assert!(encode_sugar(&empty).iter().all(|v| *v == 0.0));
    }

    // --- encode_prey ---

    fn remembered(id: i32, kind: BuildingType, owner: i32, x: i32, y: i32) -> RememberedBuildingDto {
        RememberedBuildingDto {
            id,
            kind,
            owner,
            x,
            y,
            hp: 100,
            max_hp: 100,
            completed: true,
            last_seen_tick: 0,
        }
    }

    #[test]
    fn prey_map_finds_enemy_workers_only() {
        let (w, h) = (80, 10);
        let mut enemy_worker = unit(1, ENEMY, 5, 5);
        enemy_worker.kind = UnitType::Worker;
        let mut own_worker = unit(2, ME, 30, 5);
        own_worker.kind = UnitType::Worker;
        let enemy_soldier = unit(3, ENEMY, 55, 5);
        let s = state(w, h, vec![enemy_worker, own_worker, enemy_soldier], vec![]);
        let map = encode_prey(&s);
        assert!(approx(at(&map, w, 5, 5), 1.0));
        assert_eq!(at(&map, w, 30, 5), 0.0, "own workers are not prey");
        assert_eq!(at(&map, w, 55, 5), 0.0, "soldiers are threat, not prey");
    }

    #[test]
    fn prey_map_ranks_barracks_then_cc_and_tower_then_others() {
        let (w, h) = (100, 10);
        let kinds = [
            BuildingType::Barracks,
            BuildingType::CommandCenter,
            BuildingType::GuardTower,
            BuildingType::ResourceDepot,
        ];
        let mut s = state(w, h, vec![], vec![]);
        s.buildings = kinds
            .iter()
            .enumerate()
            .map(|(i, k)| building(i as i32, *k, ENEMY, 5 + 25 * i as i32, 5))
            .collect();
        let map = encode_prey(&s);
        let heat: Vec<f64> = (0..4).map(|i| at(&map, w, 5 + 25 * i, 5)).collect();
        let (barracks, cc, tower, depot) = (heat[0], heat[1], heat[2], heat[3]);

        assert!(approx(barracks, 1.0), "barracks is the top target");
        assert!(barracks > cc && cc > tower && tower > depot, "{heat:?}");
        assert!(cc - tower < 0.1, "cc and tower are about the same: {heat:?}");
    }

    #[test]
    fn prey_map_ignores_own_buildings() {
        let (w, h) = (20, 10);
        let mut s = state(w, h, vec![], vec![]);
        s.buildings = vec![building(1, BuildingType::Barracks, ME, 5, 5)];
        assert!(encode_prey(&s).iter().all(|v| *v == 0.0));
    }

    #[test]
    fn prey_map_remembers_enemy_buildings_out_of_sight() {
        let (w, h) = (40, 10);
        let mut s = state(w, h, vec![], vec![]);
        s.remembered_buildings = vec![
            remembered(1, BuildingType::CommandCenter, ENEMY, 5, 5),
            remembered(2, BuildingType::CommandCenter, ME, 30, 5),
        ];
        let map = encode_prey(&s);
        assert!(approx(at(&map, w, 5, 5), 1.0), "the army can hunt a base it saw before");
        assert_eq!(at(&map, w, 30, 5), 0.0, "own remembered buildings are not prey");
    }

    #[test]
    fn prey_map_does_not_double_count_visible_remembered_buildings() {
        // Barracks 1 is both visible and remembered; barracks 2 is only visible.
        // Counted once each, they read the same.
        let (w, h) = (40, 10);
        let mut s = state(w, h, vec![], vec![]);
        s.buildings = vec![
            building(1, BuildingType::Barracks, ENEMY, 5, 5),
            building(2, BuildingType::Barracks, ENEMY, 30, 5),
        ];
        s.remembered_buildings = vec![remembered(1, BuildingType::Barracks, ENEMY, 5, 5)];
        let map = encode_prey(&s);
        assert!(approx(at(&map, w, 5, 5), 1.0));
        assert!(approx(at(&map, w, 30, 5), 1.0));
    }

    // --- sample ---

    #[test]
    fn sample_reads_row_major_cell() {
        let (w, h) = (4, 3);
        let map: Vec<f64> = (0..12).map(f64::from).collect();
        assert_eq!(sample(&map, w, h, 0, 0), 0.0);
        assert_eq!(sample(&map, w, h, 3, 0), 3.0);
        assert_eq!(sample(&map, w, h, 1, 2), 9.0);
    }

    #[test]
    fn sample_is_zero_off_the_map() {
        let (w, h) = (4, 3);
        let map = vec![1.0; 12];
        for (x, y) in [(-1, 0), (0, -1), (4, 0), (0, 3), (4, 3)] {
            assert_eq!(sample(&map, w, h, x, y), 0.0, "({x}, {y})");
        }
    }

    // --- uphill ---

    fn peak_map(w: i32, h: i32, cx: i32, cy: i32) -> Vec<f64> {
        let mut map = vec![0.0; (w * h) as usize];
        region_map(&mut map, w, h, cx, cy, 1.0);
        map
    }

    #[test]
    fn uphill_points_toward_the_peak() {
        // Chebyshev rings are squares, so several neighbours can tie; any of
        // them is fine as long as each step gets one tile closer.
        let (w, h, px, py) = (20, 12, 10, 5);
        let map = peak_map(w, h, px, py);
        for (x, y) in [(5, 5), (14, 5), (10, 2), (12, 7), (3, 0), (17, 11)] {
            let (dx, dy) = uphill(&map, w, h, x, y);
            assert_eq!(
                dist(x + dx, y + dy, px, py),
                dist(x, y, px, py) - 1,
                "from ({x}, {y}) stepped ({dx}, {dy})"
            );
        }
        // On a diagonal the closest neighbour is unique
        assert_eq!(uphill(&map, w, h, 12, 7), (-1, -1));
    }

    #[test]
    fn uphill_stays_put_on_a_peak() {
        let (w, h) = (20, 12);
        let map = peak_map(w, h, 10, 5);
        assert_eq!(uphill(&map, w, h, 10, 5), (0, 0));
    }

    #[test]
    fn uphill_stays_put_on_flat_ground() {
        let (w, h) = (20, 12);
        let map = vec![0.0; (w * h) as usize];
        assert_eq!(uphill(&map, w, h, 7, 7), (0, 0));
    }

    #[test]
    fn uphill_never_steps_off_the_map() {
        // On an all-negative map an off-map 0 would look hotter than any cell
        let (w, h) = (5, 5);
        let map = vec![-1.0; 25];
        for (x, y) in [(0, 0), (4, 0), (0, 4), (4, 4), (2, 0)] {
            assert_eq!(uphill(&map, w, h, x, y), (0, 0), "({x}, {y})");
        }
    }

    #[test]
    fn uphill_climbs_from_the_edge() {
        let (w, h) = (20, 12);
        let map = peak_map(w, h, 3, 3);
        assert_eq!(uphill(&map, w, h, 0, 0), (1, 1));
    }

    // --- hottest_near ---

    fn hot_cells(w: i32, h: i32, cells: &[(Loc, f64)]) -> Vec<f64> {
        let mut map = vec![0.0; (w * h) as usize];
        for &((x, y), v) in cells {
            map[(y * w + x) as usize] = v;
        }
        map
    }

    #[test]
    fn hottest_near_finds_the_local_peak_not_the_global_one() {
        let (w, h) = (40, 10);
        let map = hot_cells(w, h, &[((6, 5), 0.4), ((35, 5), 1.0)]);
        assert_eq!(hottest_near(&map, w, h, (5, 5), R), Some((6, 5)));
        assert_eq!(hottest_near(&map, w, h, (5, 5), w.max(h)), Some((35, 5)));
    }

    #[test]
    fn hottest_near_is_none_when_nothing_is_warm() {
        let (w, h) = (20, 10);
        assert_eq!(hottest_near(&vec![0.0; 200], w, h, (5, 5), w), None);
        // Warm, but out of range
        let map = hot_cells(w, h, &[((19, 9), 1.0)]);
        assert_eq!(hottest_near(&map, w, h, (0, 0), 3), None);
    }

    #[test]
    fn hottest_near_clips_its_window_at_the_edges() {
        let (w, h) = (10, 10);
        let map = hot_cells(w, h, &[((0, 0), 0.5), ((9, 9), 0.7)]);
        assert_eq!(hottest_near(&map, w, h, (1, 1), R), Some((9, 9)));
        assert_eq!(hottest_near(&map, w, h, (0, 0), 2), Some((0, 0)));
    }

    // --- find_site ---

    fn open(w: usize, h: usize) -> Vec<String> {
        vec![".".repeat(w); h]
    }

    #[test]
    fn find_site_is_3_to_6_tiles_from_the_centre_on_open_ground() {
        let s = state(20, 20, vec![], vec![]);
        let (x, y) = find_site(&open(20, 20), &s, 10, 10).unwrap();
        assert!((3..=6).contains(&dist(x, y, 10, 10)), "({x}, {y})");
    }

    #[test]
    fn find_site_avoids_buildings_ore_and_walls() {
        let mut terrain = open(20, 20);
        terrain[7] = "#".repeat(20);
        let mut s = state(20, 20, vec![], vec![deposit(1, 13, 10, 50)]);
        s.buildings = vec![building(1, BuildingType::Barracks, ME, 10, 13)];
        for _ in 0..20 {
            let (x, y) = find_site(&terrain, &s, 10, 10).unwrap();
            assert_ne!(y, 7, "not on a wall");
            assert!(dist(x, y, 13, 10) > 1, "not next to ore");
            assert_ne!((x, y), (10, 13), "not on a building");
            // Pretend it got built so the next call has to pick another tile
            s.buildings.push(building(99, BuildingType::GuardTower, ME, x, y));
        }
    }

    #[test]
    fn find_site_is_none_when_boxed_in() {
        let s = state(20, 20, vec![], vec![]);
        assert_eq!(find_site(&vec!["#".repeat(20); 20], &s, 10, 10), None);
    }

    // --- group_resources ---

    fn chunk_ids(chunks: &[Bound]) -> Vec<Vec<i32>> {
        let mut ids: Vec<Vec<i32>> = chunks
            .iter()
            .map(|c| {
                let mut ids = c.ids.clone();
                ids.sort();
                ids
            })
            .collect();
        ids.sort();
        ids
    }

    #[test]
    fn group_resources_joins_touching_deposits_diagonals_included() {
        let deposits = vec![deposit(1, 0, 0, 50), deposit(2, 1, 1, 50), deposit(3, 5, 5, 50)];
        assert_eq!(chunk_ids(&group_resources(&deposits)), vec![vec![1, 2], vec![3]]);
    }

    #[test]
    fn group_resources_ignores_input_order() {
        // 3 sits between 1 and 2 but comes last; a one-pass scan would split them
        let deposits = vec![deposit(1, 0, 0, 50), deposit(2, 2, 0, 50), deposit(3, 1, 0, 50)];
        assert_eq!(chunk_ids(&group_resources(&deposits)), vec![vec![1, 2, 3]]);

        let far_between = vec![deposit(1, 0, 0, 50), deposit(2, 10, 10, 50), deposit(3, 1, 0, 50)];
        assert_eq!(
            chunk_ids(&group_resources(&far_between)),
            vec![vec![1, 3], vec![2]]
        );
    }

    #[test]
    fn group_resources_skips_depleted_deposits() {
        // An empty deposit is neither mined nor a bridge between its neighbours
        let deposits = vec![deposit(1, 0, 0, 50), deposit(2, 1, 0, 0), deposit(3, 2, 0, 50)];
        assert_eq!(chunk_ids(&group_resources(&deposits)), vec![vec![1], vec![3]]);
        assert!(group_resources(&[deposit(1, 0, 0, 0)]).is_empty());
        assert!(group_resources(&[]).is_empty());
    }

    #[test]
    fn group_resources_boxes_each_chunk_tightly() {
        // An L shape: the box grows only along the axes that need it
        let chunks = group_resources(&[
            deposit(1, 4, 6, 50),
            deposit(2, 5, 6, 50),
            deposit(3, 5, 7, 50),
        ]);
        let c = &chunks[0];
        assert_eq!((c.lx, c.ly, c.ux, c.uy), (4, 6, 5, 7));
    }

    #[test]
    fn bound_dist_to_is_zero_inside_and_counts_from_the_nearest_edge() {
        let b = &group_resources(&[deposit(1, 2, 2, 50), deposit(2, 3, 2, 50), deposit(3, 4, 2, 50)])[0];
        assert_eq!(b.dist_to(3, 2), 0, "inside");
        assert_eq!(b.dist_to(3, 3), 1, "below the middle, not near a corner");
        assert_eq!(b.dist_to(5, 3), 1, "diagonal to a corner");
        assert_eq!(b.dist_to(0, 2), 2, "left of the box");
        assert_eq!(b.dist_to(10, 9), 7, "the larger axis wins");
    }

    // --- enemy_base ---

    #[test]
    fn enemy_base_falls_back_to_the_map_centre() {
        let base = enemy_base(&state(40, 20, vec![], vec![]));
        assert_eq!((base.x, base.y), (20, 10));
    }

    // --- both maps together ---

    #[test]
    fn heat_maps_separate_threats_from_resources() {
        let (w, h) = (40, 20);
        let s = state(
            w,
            h,
            vec![unit(1, ENEMY, 30, 15), unit(2, ME, 5, 5)],
            vec![deposit(1, 5, 4, 80)],
        );
        let threat = encode_threat(&s);
        let sugar = encode_sugar(&s);

        assert_eq!(peak(&threat, w), (30, 15));
        assert_eq!(peak(&sugar, w), (5, 4));
        // Each map is blind to the other's sources
        assert_eq!(at(&threat, w, 5, 4), 0.0);
        assert_eq!(at(&sugar, w, 30, 15), 0.0);
    }

    #[test]
    fn calculate_diagonal_dist() {
        // A pure diagonal move costs the same as a straight one in Chebyshev
        assert_eq!(dist(0, 0, 3, 3), 3);
        assert_eq!(dist(0, 0, -3, 3), 3);
        assert_eq!(dist(5, 5, 2, 2), 3);
        // Uneven diagonal: the larger axis wins
        assert_eq!(dist(0, 0, 2, 5), 5);
    }

    #[test]
    fn calculate_adjacent_dist() {
        // All 8 neighbours are distance 1
        for dx in -1..=1 {
            for dy in -1..=1 {
                let expected = if dx == 0 && dy == 0 { 0 } else { 1 };
                assert_eq!(
                    dist(10, 10, 10 + dx, 10 + dy),
                    expected,
                    "offset ({dx}, {dy})"
                );
            }
        }
    }

    #[test]
    fn really_far_dist() {
        assert_eq!(dist(0, 0, 1_000_000, 5), 1_000_000);
        assert_eq!(dist(-500_000, 0, 500_000, 999), 1_000_000);
        // Symmetric
        assert_eq!(dist(1_000_000, 5, 0, 0), dist(0, 0, 1_000_000, 5));
    }
}

