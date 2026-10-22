use crate::logic::neurons::*;

pub const SENSES: usize = 4;

// Index of each sense in an input array or a weight row
pub const THREAT: usize = 0;
pub const SUGAR: usize = 1;
pub const PREY: usize = 2;
pub const WEALTH: usize = 3;

const GAINS: [f32; SENSES] = [5.0, 5.0, 5.0, 5.0];

pub struct SensoryLayer {
    /// One neuron per sense, indexed by `THREAT`, `SUGAR`, `PREY`, `WEALTH`
    pub neurons: Vec<LifNeuron>,
}

impl SensoryLayer {
    pub fn new(decay: f32, threshold: f32) -> Self {
        Self {
            neurons: vec![LifNeuron::new(decay, threshold); SENSES],
        }
    }

    /// Takes heat values from 0 to 1 and returns which senses spiked.
    pub fn encode_stimuli(&mut self, input: [f32; SENSES]) -> [bool; SENSES] {
        std::array::from_fn(|i| self.neurons[i].update(input[i] * GAINS[i]))
    }
}
