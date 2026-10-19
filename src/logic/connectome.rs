use crate::logic::{neurons::*, sensory::*};

pub struct FlyBrainConnectome {
    pub sensory: SensoryLayer,
    pub motor_neurons: Vec<LifNeuron>,
    pub weights: [[f32; 2]; 2],
}

impl FlyBrainConnectome {
    pub fn tick(&mut self, enemy_dist: f32, mineral_dist: f32) -> MotorAction {
        let (vision_spike, smell_spike) = self.sensory.encode_stimuli(enemy_dist, mineral_dist);

        let mut motor_currents = [0.0; 2];

        if vision_spike {
            motor_currents[0] += self.weights[0][0]; // Vision -> Retreat
            motor_currents[1] += self.weights[0][1]; // Vision -> Attack
        }

        if smell_spike {
            motor_currents[0] += self.weights[1][0]; // Smell -> Retreat
            motor_currents[1] += self.weights[1][1]; // Smell -> Attack
        }

        let retreat_spike = self.motor_neurons[0].update(motor_currents[0]);
        let attack_spike = self.motor_neurons[1].update(motor_currents[1]);

        if retreat_spike && !attack_spike {
            MotorAction::Retreat
        } else if attack_spike && !retreat_spike {
            MotorAction::Attack
        } else {
            MotorAction::HoldPosition
        }
    }
}

pub enum MotorAction {
    Retreat,
    Attack,
    HoldPosition,
}
