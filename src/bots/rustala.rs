use crate::logic::models::*;
use crate::logic::shared::Bot;

pub struct RustalaDaora {}

impl Bot for RustalaDaora {
    // TODO: real strategy; idles for now
    fn decide(&mut self, _s: &GameStateDto) -> Vec<CommandRequest> {
        Vec::new()
    }

    fn new(_m: MapDto, _r: RulesDto) -> Self {
        Self {}
    }
}

#[cfg(test)]
#[allow(unused_imports)]
mod rnetr_tests {
    use super::*;
    use crate::bots::fixtures;

    #[test]
    fn initial_setup() {
        assert!(true);
    }

    #[test]
    fn stub_sends_no_commands() {
        let mut bot = RustalaDaora::new(fixtures::map(), fixtures::rules());
        assert!(bot.decide(&fixtures::state()).is_empty());
    }
}
