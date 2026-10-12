use crate::bots::models::GameStateDto;

pub trait Bot {
    fn decide(state: GameStateDto);
}