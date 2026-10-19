use crate::logic::neurons::*;

pub struct SensoryLayer {
    pub vision_neurons: Vec<LifNeuron>,
    pub olfactory_neurons: Vec<LifNeuron>,
}

impl SensoryLayer {
    pub fn encode_stimuli(&mut self, enemy_distance: f32, mineral_distance: f32) -> (bool, bool) {
        pub const MAX_DIST: f32 = 100.00;
        let vision_current = (MAX_DIST - enemy_distance).max(0.0) * 0.1;
        let vision_spiked = self.olfactory_neurons[0].update(vision_current);

        let mineral_current = (MAX_DIST - mineral_distance).max(0.0) * 0.05;
        let mineral_spiked = self.olfactory_neurons[0].update(mineral_current);
        (vision_spiked, mineral_spiked)
    }
}
