use std::collections::{BTreeMap, HashMap};
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::data::models::UnitDto;
use crate::logic::sensory::SENSES;

/// Learned weights kept between matches, saved as JSON.
#[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
pub struct BrainStore {
    /// Matches these weights have learned from
    pub matches: u32,
    /// Weights per role name. Rows are motors and columns are senses, as in
    /// `FlyBrainConnectome::weights`. A BTreeMap keeps the file in a stable order.
    pub weights: BTreeMap<String, Vec<[f32; SENSES]>>,
}

impl BrainStore {
    /// Reads `path`. A missing or unreadable file starts fresh, so the bot
    /// still plays its hand-tuned brains.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()
    }

    /// Writes to a temporary file and then renames it over `path`, so a crash
    /// mid-write can't leave a half-written file behind.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(tmp, path)
    }
}

/// What a group's units looked like last tick, so this tick's reward can see
/// what changed.
#[derive(Default)]
pub struct Snapshot {
    /// Ore each unit was carrying, keyed by unit id. The keys are also the
    /// group's members as of last tick.
    carrying: HashMap<i32, i32>,
}

impl Snapshot {
    /// Records the units in `ids` that are still alive in `mine` (my units by id).
    pub fn take(ids: &[i32], mine: &HashMap<i32, &UnitDto>) -> Self {
        Self {
            carrying: ids
                .iter()
                .filter_map(|id| mine.get(id).map(|u| (*id, u.carrying)))
                .collect(),
        }
    }

    /// True if the group had no units last tick.
    pub fn is_empty(&self) -> bool {
        self.carrying.is_empty()
    }

    /// Units in the group last tick that are gone now.
    pub fn lost(&self, mine: &HashMap<i32, &UnitDto>) -> usize {
        self.carrying
            .keys()
            .filter(|id| !mine.contains_key(id))
            .count()
    }

    /// Units that carried ore last tick and are alive and empty now: a delivery.
    pub fn delivered(&self, mine: &HashMap<i32, &UnitDto>) -> usize {
        self.carrying
            .iter()
            .filter(|(id, c)| **c > 0 && mine.get(id).is_some_and(|u| u.carrying == 0))
            .count()
    }
}

#[cfg(test)]
mod memory_tests {
    use super::*;

    /// A path in the system temp dir, unique to this test
    fn temp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("flaud-{}-{name}.json", std::process::id()))
    }

    #[test]
    fn a_saved_store_loads_back_the_same() {
        let path = temp("round-trip");
        let mut store = BrainStore {
            matches: 3,
            ..Default::default()
        };
        store
            .weights
            .insert("worker".into(), vec![[0.5, -1.0, 0.0, 0.25]]);
        store.save(&path).unwrap();
        assert_eq!(BrainStore::load(&path), store);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn a_snapshot_of_only_dead_units_is_empty() {
        assert!(Snapshot::default().is_empty());
        assert!(Snapshot::take(&[1, 2], &HashMap::new()).is_empty(), "nobody in `mine` is alive");
    }

    #[test]
    fn a_missing_file_starts_fresh() {
        assert_eq!(BrainStore::load(&temp("missing")), BrainStore::default());
    }

    #[test]
    fn a_corrupt_file_starts_fresh() {
        let path = temp("corrupt");
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(BrainStore::load(&path), BrainStore::default());
        std::fs::remove_file(path).unwrap();
    }
}
