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
        self.voltage = self.resting_voltage + (self.voltage - self.resting_voltage) * self.decay;
        self.voltage += current_input;

        if self.voltage >= self.threshold {
            self.voltage = self.resting_voltage;
            true
        } else {
            false
        }
    }
}
