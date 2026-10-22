#[derive(Clone)]
pub struct LifNeuron {
    pub voltage: f32,
    pub threshold: f32,
    pub resting_voltage: f32,
    pub decay: f32,
}

impl LifNeuron {
    pub fn new(decay: f32, threshold: f32) -> Self {
        Self {
            voltage: 0.0,
            threshold,
            resting_voltage: 0.0,
            decay,
        }
    }

    pub fn update(&mut self, current_input: f32) -> bool {
        if self.charge(current_input) >= self.threshold {
            self.reset();
            true
        } else {
            false
        }
    }

    /// Leaks toward rest, adds `input` and returns the new voltage. Never resets,
    /// so the caller decides whether a neuron over threshold gets to fire.
    pub fn charge(&mut self, input: f32) -> f32 {
        self.voltage = self.resting_voltage + (self.voltage - self.resting_voltage) * self.decay;
        self.voltage += input;
        self.voltage
    }

    pub fn reset(&mut self) {
        self.voltage = self.resting_voltage;
    }
}
