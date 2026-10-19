use crate::logic::models::*;
use crate::logic::shared::Bot;

pub struct Flaud {}

impl Bot for Flaud {
    fn decide(&mut self, _s: &GameStateDto) -> Vec<CommandRequest> {
        Vec::new()
    }

    fn new(_m: MapDto, _r: RulesDto) -> Self {
        Self {}
    }
}

impl Flaud {}

#[cfg(test)]
#[allow(unused_imports)]
mod flaud_tests {
    use super::*;
    use crate::bots::fixtures;

    #[test]
    fn stub_sends_no_commands() {
        let mut bot = Flaud::new(fixtures::map(), fixtures::rules());
        assert!(bot.decide(&fixtures::state()).is_empty());
    }
}
