pub mod flaud;
pub mod rustala;
pub mod test_bot;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BotLogic {
    Test,
    #[default]
    Flaud,
    Rustala,
}

impl BotLogic {
    pub const ALL: [BotLogic; 3] = [BotLogic::Test, BotLogic::Flaud, BotLogic::Rustala];

    pub fn name(self) -> &'static str {
        match self {
            BotLogic::Test => "TestBot",
            BotLogic::Flaud => "Flaud",
            BotLogic::Rustala => "RustalaDaora",
        }
    }

    /// Parses a 1-based index into `ALL` or a (case-insensitive) logic name.
    pub fn parse(input: &str) -> Option<Self> {
        if let Ok(n) = input.parse::<usize>() {
            return n.checked_sub(1).and_then(|i| Self::ALL.get(i).copied());
        }
        Self::ALL
            .into_iter()
            .find(|l| l.name().eq_ignore_ascii_case(input))
    }
}

#[cfg(test)]
pub mod fixtures {
    use crate::data::models::*;
    use serde_json::json;

    pub fn map() -> MapDto {
        serde_json::from_value(json!({
            "width": 4,
            "height": 4,
            "terrain": ["....", "....", "....", "...."]
        }))
        .unwrap()
    }

    pub fn rules() -> RulesDto {
        serde_json::from_value(json!({
            "economy": {
                "gatherPerTick": 1,
                "maxProductionQueue": 5,
                "startingResources": 100,
                "startingWorkers": 4,
                "workerCarryCapacity": 5
            }
        }))
        .unwrap()
    }

    pub fn state() -> GameStateDto {
        serde_json::from_value(json!({
            "you": {
                "playerId": "p1",
                "name": "me",
                "slot": 0,
                "resources": 100,
                "storageCapacity": 1000,
                "incomePerMinute": 0,
                "queueCapacity": 10,
                "queuedCommands": 0
            },
            "matchId": "m1",
            "status": "Active",
            "tick": 1,
            "maxTicks": 100,
            "tickIntervalMs": 100,
            "mapWidth": 4,
            "mapHeight": 4,
            "outcome": null
        }))
        .unwrap()
    }
}

#[cfg(test)]
mod bot_logic_tests {
    use super::*;

    #[test]
    fn parses_one_based_index_in_menu_order() {
        for (i, l) in BotLogic::ALL.iter().enumerate() {
            assert_eq!(BotLogic::parse(&(i + 1).to_string()), Some(*l));
        }
    }

    #[test]
    fn rejects_out_of_range_and_unknown_input() {
        for bad in ["0", "4", "-1", "99999999999999999999", "", "nope", "Test", "1.0"] {
            assert_eq!(BotLogic::parse(bad), None, "{bad:?}");
        }
    }
}