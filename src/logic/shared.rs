use crate::logic::models::{BuildingType, *};
use std::collections::HashMap;

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
const R: i32 = (STIMULI_LOOKUP.len() - 1) as i32;
const UNIT_WEIGHT: f64 = 1.0;

pub trait Bot {
    fn decide(&mut self, state: &GameStateDto) -> Vec<CommandRequest>;

    fn building_cost(r: &RulesDto) -> HashMap<BuildingType, i32> {
        r.buildings.iter().fold(HashMap::new(), |mut acc, b| {
            acc.insert(b.kind, b.cost);
            acc
        })
    }

    fn new(m: MapDto, r: RulesDto) -> Self;
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

