use crate::data::models::*;
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
