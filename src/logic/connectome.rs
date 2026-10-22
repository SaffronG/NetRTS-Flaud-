use crate::logic::{neurons::*, sensory::*};

pub struct FlyBrainConnectome {
    pub sensory: SensoryLayer,
    pub motor_neurons: Vec<LifNeuron>,
    pub weights: Vec<[f32; 2]>,
    // Map bias and actions 1:1 to the motor_neurons
    pub bias: Vec<f32>,
    pub actions: Vec<MotorAction>,
}

impl FlyBrainConnectome {
    pub fn new(
        weights: Vec<[f32; 2]>,
        decay: f32,
        threshold: f32,
        bias: Vec<f32>,
        actions: Vec<MotorAction>,
    ) -> Self {
        assert_eq!(weights.len(), bias.len(), "Need one bias per motor neuron");
        assert_eq!(
            weights.len(),
            actions.len(),
            "Need one action per motor neuron"
        );

        Self {
            sensory: SensoryLayer {
                vision_neurons: vec![LifNeuron::new(decay, threshold)],
                olfactory_neurons: vec![LifNeuron::new(decay, threshold)],
            },
            motor_neurons: vec![LifNeuron::new(decay, threshold); weights.len()],
            weights,
            bias,
            actions,
        }
    }

    pub fn tick(&mut self, threat: f32, sugar: f32) -> MotorAction {
        let (saw, smelled) = self.sensory.encode_stimuli(threat, sugar);
        let spikes = [f32::from(saw), f32::from(smelled)]; // bool → 1.0 / 0.0

        let mut fired: Option<MotorAction> = None;
        let mut count = 0;
        for (i, neuron) in self.motor_neurons.iter_mut().enumerate() {
            let current = self.bias[i]
                + self.weights[i]
                    .iter()
                    .zip(&spikes)
                    .map(|(w, s)| w * s)
                    .sum::<f32>();
            if neuron.update(current) {
                fired = Some(self.actions[i]);
                count += 1;
            }
        }

        match (count, fired) {
            (1, Some(action)) => action,
            _ => MotorAction::HoldPosition,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MotorAction {
    Attack,
    Explore,
    Gather,
    HoldPosition,
    Retreat,
}

#[cfg(test)]
mod connectome_tests {
    use super::*;

    const DECAY: f32 = 0.9;
    const THRESHOLD: f32 = 1.0;

    fn brain(rows: usize) -> FlyBrainConnectome {
        FlyBrainConnectome::new(
            vec![[0.5, -0.5]; rows],
            DECAY,
            THRESHOLD,
            vec![0.0; rows],
            vec![MotorAction::HoldPosition; rows],
        )
    }

    /// One motor neuron per `(weights, bias, action)`.
    fn wired(motors: &[([f32; 2], f32, MotorAction)]) -> FlyBrainConnectome {
        FlyBrainConnectome::new(
            motors.iter().map(|m| m.0).collect(),
            DECAY,
            THRESHOLD,
            motors.iter().map(|m| m.1).collect(),
            motors.iter().map(|m| m.2).collect(),
        )
    }

    fn all_neurons(b: &FlyBrainConnectome) -> impl Iterator<Item = &LifNeuron> {
        b.sensory
            .vision_neurons
            .iter()
            .chain(&b.sensory.olfactory_neurons)
            .chain(&b.motor_neurons)
    }

    // --- new ---

    #[test]
    fn new_builds_one_motor_neuron_per_weight_row() {
        for rows in 0..=4 {
            assert_eq!(brain(rows).motor_neurons.len(), rows, "{rows} rows");
        }
    }

    #[test]
    fn new_builds_one_vision_and_one_smell_neuron() {
        let b = brain(2);
        assert_eq!(b.sensory.vision_neurons.len(), 1);
        assert_eq!(b.sensory.olfactory_neurons.len(), 1);
    }

    #[test]
    fn new_keeps_weights_in_order() {
        let w = vec![[1.0, 2.0], [3.0, 4.0], [5.0, 6.0]];
        let b = FlyBrainConnectome::new(
            w.clone(),
            DECAY,
            THRESHOLD,
            vec![0.1, 0.2, 0.3],
            vec![
                MotorAction::Attack,
                MotorAction::Gather,
                MotorAction::Explore,
            ],
        );
        assert_eq!(b.weights, w);
        assert_eq!(b.bias, vec![0.1, 0.2, 0.3]);
        assert_eq!(
            b.actions,
            vec![
                MotorAction::Attack,
                MotorAction::Gather,
                MotorAction::Explore
            ]
        );
    }

    #[test]
    fn new_neurons_start_at_rest_with_given_params() {
        let b = brain(3);
        for n in all_neurons(&b) {
            assert_eq!(n.voltage, 0.0);
            assert_eq!(n.resting_voltage, 0.0);
            assert_eq!(n.decay, DECAY);
            assert_eq!(n.threshold, THRESHOLD);
        }
    }

    // --- state is owned per neuron and per brain ---

    #[test]
    fn motor_neurons_hold_separate_voltages() {
        let mut b = brain(3);
        b.motor_neurons[0].update(0.5);
        assert!(b.motor_neurons[0].voltage > 0.0);
        assert_eq!(b.motor_neurons[1].voltage, 0.0);
        assert_eq!(b.motor_neurons[2].voltage, 0.0);
    }

    #[test]
    fn brains_do_not_share_state() {
        // Each group gets its own brain; charging one must not charge another
        let (mut a, b) = (brain(2), brain(2));
        // Two ticks stays under threshold, so the voltage isn't reset by a spike
        for _ in 0..2 {
            a.sensory.encode_stimuli(0.1, 0.1);
            a.motor_neurons[0].update(0.5);
        }
        assert!(a.sensory.vision_neurons[0].voltage > 0.0);
        assert!(all_neurons(&b).all(|n| n.voltage == 0.0));
    }

    #[test]
    fn brain_remembers_voltage_between_ticks() {
        // A weak stimulus stays below threshold for one tick but builds up
        // over several, which only works if the brain is kept between ticks.
        // Assumes VISION_GAIN * 0.1 < THRESHOLD.
        let mut b = brain(2);
        let (first, _) = b.sensory.encode_stimuli(0.1, 0.0);
        assert!(!first, "one weak tick should not spike");
        let v1 = b.sensory.vision_neurons[0].voltage;

        b.sensory.encode_stimuli(0.1, 0.0);
        let v2 = b.sensory.vision_neurons[0].voltage;
        assert!(v2 > v1, "voltage carried over: {v1} -> {v2}");

        let spiked = (0..20).any(|_| b.sensory.encode_stimuli(0.1, 0.0).0);
        assert!(spiked, "repeated weak input should eventually spike");
    }

    #[test]
    fn voltage_leaks_without_input() {
        let mut b = brain(1);
        b.motor_neurons[0].update(0.5);
        let before = b.motor_neurons[0].voltage;
        b.motor_neurons[0].update(0.0);
        assert!((b.motor_neurons[0].voltage - before * DECAY).abs() < 1e-6);
    }

    #[test]
    #[should_panic(expected = "bias")]
    fn new_rejects_mismatched_bias() {
        FlyBrainConnectome::new(
            vec![[0.0, 0.0]; 2],
            DECAY,
            THRESHOLD,
            vec![0.0],
            vec![MotorAction::HoldPosition; 2],
        );
    }

    #[test]
    #[should_panic(expected = "action")]
    fn new_rejects_mismatched_actions() {
        FlyBrainConnectome::new(
            vec![[0.0, 0.0]; 2],
            DECAY,
            THRESHOLD,
            vec![0.0; 2],
            vec![MotorAction::HoldPosition],
        );
    }

    // --- tick ---

    #[test]
    fn tick_holds_when_nothing_fires() {
        let mut b = wired(&[([0.0, 0.0], 0.0, MotorAction::Attack)]);
        assert_eq!(b.tick(0.0, 0.0), MotorAction::HoldPosition);
    }

    #[test]
    fn tick_fires_on_bias_alone() {
        // A bias at threshold fires with no senses at all, as a bored scout should
        let mut b = wired(&[([0.0, 0.0], THRESHOLD, MotorAction::Explore)]);
        assert_eq!(b.tick(0.0, 0.0), MotorAction::Explore);
    }

    #[test]
    fn tick_returns_action_of_the_only_neuron_that_fired() {
        let mut b = wired(&[
            ([0.0, 0.0], 0.0, MotorAction::Gather),
            ([0.0, 0.0], THRESHOLD, MotorAction::Attack),
            ([0.0, 0.0], 0.0, MotorAction::Retreat),
        ]);
        assert_eq!(b.tick(0.0, 0.0), MotorAction::Attack);
    }

    #[test]
    fn tick_holds_on_a_tie() {
        let mut b = wired(&[
            ([0.0, 0.0], THRESHOLD, MotorAction::Attack),
            ([0.0, 0.0], THRESHOLD, MotorAction::Gather),
        ]);
        assert_eq!(b.tick(0.0, 0.0), MotorAction::HoldPosition);
    }

    #[test]
    fn tick_reads_weights_as_motor_rows_sensor_columns() {
        // Column 0 is vision, column 1 is smell. A transposed lookup swaps these.
        let motors = [
            ([THRESHOLD, 0.0], 0.0, MotorAction::Retreat), // listens to vision only
            ([0.0, THRESHOLD], 0.0, MotorAction::Gather),  // listens to smell only
        ];
        assert_eq!(wired(&motors).tick(1.0, 0.0), MotorAction::Retreat);
        assert_eq!(wired(&motors).tick(0.0, 1.0), MotorAction::Gather);
    }

    #[test]
    fn tick_negative_weight_suppresses_bias() {
        // Exploring stops once a threat is seen
        let motors = [([-THRESHOLD, 0.0], THRESHOLD, MotorAction::Explore)];
        assert_eq!(wired(&motors).tick(0.0, 0.0), MotorAction::Explore);
        assert_eq!(wired(&motors).tick(1.0, 0.0), MotorAction::HoldPosition);
    }

    #[test]
    fn tick_updates_every_motor_neuron_even_after_one_fires() {
        // Motor 0 fires every tick. If the loop stopped at the first spike,
        // motor 1 would never charge.
        let mut b = wired(&[
            ([0.0, 0.0], THRESHOLD, MotorAction::Attack),
            ([0.0, 0.0], 0.5, MotorAction::Gather),
        ]);
        assert_eq!(b.tick(0.0, 0.0), MotorAction::Attack);
        assert!((b.motor_neurons[1].voltage - 0.5).abs() < 1e-6);

        // 0.5 -> 0.95 -> 1.355 fires on tick 3, tying with motor 0
        assert_eq!(b.tick(0.0, 0.0), MotorAction::Attack);
        assert_eq!(b.tick(0.0, 0.0), MotorAction::HoldPosition);
    }
}