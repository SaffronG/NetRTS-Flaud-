use crate::logic::neurons::*;

const VISION_GAIN: f32 = 5.0;
const SMELL_GAIN: f32 = 5.0;

pub struct SensoryLayer {
    pub vision_neurons: Vec<LifNeuron>,
    pub olfactory_neurons: Vec<LifNeuron>,
}

impl SensoryLayer {
    pub fn encode_stimuli(&mut self, threat: f32, sugar: f32) -> (bool, bool) {
        let saw = self.vision_neurons[0].update(threat * VISION_GAIN);
        let smelled = self.olfactory_neurons[0].update(sugar * SMELL_GAIN);
        (saw, smelled)
    }
}