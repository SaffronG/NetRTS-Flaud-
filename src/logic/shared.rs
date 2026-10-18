use crate::logic::models::{BuildingType, *};
use std::collections::HashMap;

pub trait Bot {
    fn decide(&mut self) -> Vec<CommandRequest>;

    fn building_cost(r: RulesDto) -> HashMap<BuildingType, i32> {
        r.buildings.iter().fold(HashMap::new(), |mut acc, b| {
            acc.insert(b.kind, b.cost);
            acc
        })
    }

    fn new(s: GameStateDto, m: MapDto, r: RulesDto) -> Self;
}

/// Chebyshev distance: source (x, y), target (x, y)
pub fn dist(sx: i32, sy: i32, tx: i32, ty: i32) -> i32 {
    (sx - tx).abs().max((sy - ty).abs())
}

#[cfg(test)]
mod tests {
    use super::*;

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

