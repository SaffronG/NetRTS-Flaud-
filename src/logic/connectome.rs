use crate::logic::{learning::{ETA, Plasticity, bound}, neurons::*, sensory::*};

/// Voltage a motor spike takes from every other motor (lateral inhibition).
/// Keep it small: a frequent winner drains the others on every spike, so a
/// slow, bias-only motor such as Gather far from sugar starves above ~0.11
/// and is already slowed from tick 9 to 17 at 0.1.
const INHIBITION: f32 = 0.05;

pub struct FlyBrainConnectome {
    pub sensory: SensoryLayer,
    pub motor_neurons: Vec<LifNeuron>,
    pub weights: Vec<[f32; SENSES]>,
    // Map bias and actions 1:1 to the motor_neurons
    pub bias: Vec<f32>,
    pub actions: Vec<MotorAction>,
    /// Traces that let `learn` credit the weights behind recent actions
    pub plasticity: Plasticity,
}

impl FlyBrainConnectome {
    pub fn new(
        weights: Vec<[f32; SENSES]>,
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
            sensory: SensoryLayer::new(decay, threshold),
            motor_neurons: vec![LifNeuron::new(decay, threshold); weights.len()],
            plasticity: Plasticity::new(&weights),
            weights,
            bias,
            actions,
        }
    }

    /// `senses` is indexed by `THREAT`, `SUGAR`, `PREY`, `WEALTH`.
    pub fn tick(&mut self, senses: [f32; SENSES]) -> MotorAction {
        let spikes = self.sensory.encode_stimuli(senses).map(f32::from); // bool → 1.0 / 0.0

        // Charge every motor first; nobody resets until a winner is picked
        let mut winner: Option<(usize, f32)> = None;
        for (i, neuron) in self.motor_neurons.iter_mut().enumerate() {
            let current = self.bias[i]
                + self.weights[i]
                    .iter()
                    .zip(&spikes)
                    .map(|(w, s)| w * s)
                    .sum::<f32>();
            let v = neuron.charge(current);
            // Strict > keeps ties with the earlier row
            if v >= neuron.threshold && winner.is_none_or(|(_, best)| v > best) {
                winner = Some((i, v));
            }
        }

        let winner = winner.map(|(w, _)| w);
        // Before the early return, so the traces keep fading on ticks nothing fires.
        // Only the winner counts as a post spike: it's the only action the game saw.
        self.plasticity.step(&spikes, winner);

        let Some(w) = winner else {
            return MotorAction::HoldPosition;
        };
        // Only the winner resets. A loser over threshold stays charged and wins
        // next tick, so motors firing in lockstep take turns instead of deadlocking.
        for (i, neuron) in self.motor_neurons.iter_mut().enumerate() {
            if i == w {
                neuron.reset();
            } else {
                neuron.voltage -= INHIBITION;
            }
        }
        self.actions[w]
    }

    /// Replaces the weights with saved ones, held to this brain's own starting
    /// signs, so a sign changed in code since the save still wins. Returns false
    /// and changes nothing if `saved` has a different number of motors.
    pub fn load_weights(&mut self, saved: &[[f32; SENSES]]) -> bool {
        if saved.len() != self.weights.len() {
            return false;
        }
        for ((row, saved), sign) in self.weights.iter_mut().zip(saved).zip(&self.plasticity.sign) {
            *row = std::array::from_fn(|s| bound(saved[s], sign[s]));
        }
        true
    }

    /// Puts every neuron back at rest and clears the learning traces, keeping
    /// the weights and the reward baseline. For a brain handed to new units.
    pub fn reset_state(&mut self) {
        for n in self.sensory.neurons.iter_mut().chain(&mut self.motor_neurons) {
            n.reset();
        }
        self.plasticity.clear_traces();
    }

    /// Nudges every weight by its eligibility times how much better (or worse)
    /// `reward` was than usual, then holds it to its starting sign (`bound`). Lives here rather than on `Plasticity` because
    /// it borrows `weights` and `plasticity` mutably at once, which the borrow
    /// checker allows for two separate fields.
    pub fn learn(&mut self, reward: f32) {
        let d = self.plasticity.surprise(reward);
        let p = &self.plasticity;
        for ((row, elig), sign) in self.weights.iter_mut().zip(&p.elig).zip(&p.sign) {
            for s in 0..SENSES {
                row[s] = bound(row[s] + ETA * d * elig[s], sign[s]);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MotorAction {
    Attack,
    Build,
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
            vec![[0.5, -0.5, 0.0, 0.0]; rows],
            DECAY,
            THRESHOLD,
            vec![0.0; rows],
            vec![MotorAction::HoldPosition; rows],
        )
    }

    /// One motor neuron per `(weights, bias, action)`.
    fn wired(motors: &[([f32; SENSES], f32, MotorAction)]) -> FlyBrainConnectome {
        FlyBrainConnectome::new(
            motors.iter().map(|m| m.0).collect(),
            DECAY,
            THRESHOLD,
            motors.iter().map(|m| m.1).collect(),
            motors.iter().map(|m| m.2).collect(),
        )
    }

    fn all_neurons(b: &FlyBrainConnectome) -> impl Iterator<Item = &LifNeuron> {
        b.sensory.neurons.iter().chain(&b.motor_neurons)
    }

    // --- new ---

    #[test]
    fn tick_reads_each_sense_from_its_own_column() {
        // Each motor listens to exactly one sense; only the sense that is fed should act
        let actions = [
            MotorAction::Retreat,
            MotorAction::Gather,
            MotorAction::Explore,
            MotorAction::Build,
        ];
        for sense in [THREAT, SUGAR, PREY, WEALTH] {
            let motors: Vec<_> = (0..SENSES)
                .map(|m| {
                    let mut w = [0.0; SENSES];
                    w[m] = THRESHOLD;
                    (w, 0.0, actions[m])
                })
                .collect();
            let mut input = [0.0; SENSES];
            input[sense] = 1.0;
            assert_eq!(wired(&motors).tick(input), actions[sense], "sense {sense}");
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
            a.sensory.encode_stimuli([0.1, 0.1, 0.0, 0.0]);
            a.motor_neurons[0].update(0.5);
        }
        assert!(a.sensory.neurons[THREAT].voltage > 0.0);
        assert!(all_neurons(&b).all(|n| n.voltage == 0.0));
    }

    #[test]
    fn brain_remembers_voltage_between_ticks() {
        // A weak stimulus stays below threshold for one tick but builds up
        // over several, which only works if the brain is kept between ticks.
        // Assumes GAINS[THREAT] * 0.1 < THRESHOLD.
        let mut b = brain(2);
        let first = b.sensory.encode_stimuli([0.1, 0.0, 0.0, 0.0])[THREAT];
        assert!(!first, "one weak tick should not spike");
        let v1 = b.sensory.neurons[THREAT].voltage;

        b.sensory.encode_stimuli([0.1, 0.0, 0.0, 0.0]);
        let v2 = b.sensory.neurons[THREAT].voltage;
        assert!(v2 > v1, "voltage carried over: {v1} -> {v2}");

        let spiked = (0..20).any(|_| b.sensory.encode_stimuli([0.1, 0.0, 0.0, 0.0])[THREAT]);
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

    // --- tick ---

    #[test]
    fn tick_holds_when_nothing_fires() {
        let mut b = wired(&[([0.0; SENSES], 0.0, MotorAction::Attack)]);
        assert_eq!(b.tick([0.0, 0.0, 0.0, 0.0]), MotorAction::HoldPosition);
    }

    #[test]
    fn tick_fires_on_bias_alone() {
        // A bias at threshold fires with no senses at all, as a bored scout should
        let mut b = wired(&[([0.0; SENSES], THRESHOLD, MotorAction::Explore)]);
        assert_eq!(b.tick([0.0, 0.0, 0.0, 0.0]), MotorAction::Explore);
    }

    #[test]
    fn tick_returns_action_of_the_only_neuron_that_fired() {
        let mut b = wired(&[
            ([0.0; SENSES], 0.0, MotorAction::Gather),
            ([0.0; SENSES], THRESHOLD, MotorAction::Attack),
            ([0.0; SENSES], 0.0, MotorAction::Retreat),
        ]);
        assert_eq!(b.tick([0.0, 0.0, 0.0, 0.0]), MotorAction::Attack);
    }

    #[test]
    fn tick_picks_one_action_on_a_tie() {
        let mut b = wired(&[
            ([0.0; SENSES], THRESHOLD, MotorAction::Attack),
            ([0.0; SENSES], THRESHOLD, MotorAction::Gather),
        ]);
        let a = b.tick([0.0, 0.0, 0.0, 0.0]);
        assert!(
            matches!(a, MotorAction::Attack | MotorAction::Gather),
            "a tie must still act, got {a:?}"
        );
    }

    #[test]
    fn tick_tie_goes_to_the_most_charged_neuron() {
        // Both cross threshold, but Gather is driven harder
        let mut b = wired(&[
            ([0.0; SENSES], THRESHOLD, MotorAction::Attack),
            ([0.0; SENSES], 1.5 * THRESHOLD, MotorAction::Gather),
        ]);
        assert_eq!(b.tick([0.0, 0.0, 0.0, 0.0]), MotorAction::Gather);
    }

    #[test]
    fn tick_loser_of_a_tie_fires_next_tick() {
        // The loser stays charged instead of resetting, so it gets its turn next
        let mut b = wired(&[
            ([0.0; SENSES], 1.5 * THRESHOLD, MotorAction::Attack),
            ([0.0; SENSES], THRESHOLD, MotorAction::Gather),
        ]);
        assert_eq!(b.tick([0.0, 0.0, 0.0, 0.0]), MotorAction::Attack);
        assert_eq!(b.tick([0.0, 0.0, 0.0, 0.0]), MotorAction::Gather);
    }

    #[test]
    fn tick_neurons_in_lockstep_take_turns() {
        // Equal drive fires both on the same ticks forever. Neither may starve.
        let mut b = wired(&[
            ([0.0; SENSES], 0.6, MotorAction::Build),
            ([0.0; SENSES], 0.6, MotorAction::Gather),
        ]);
        let a: Vec<_> = (0..12).map(|_| b.tick([0.0, 0.0, 0.0, 0.0])).collect();
        assert!(a.contains(&MotorAction::Build), "{a:?}");
        assert!(a.contains(&MotorAction::Gather), "{a:?}");
    }

    #[test]
    fn tick_winner_inhibits_the_others() {
        // Lateral inhibition: a spike pushes the neurons that lost further from threshold
        let mut b = wired(&[
            ([0.0; SENSES], THRESHOLD, MotorAction::Attack),
            ([0.0; SENSES], 0.5, MotorAction::Gather),
        ]);
        assert_eq!(b.tick([0.0, 0.0, 0.0, 0.0]), MotorAction::Attack);
        let v = b.motor_neurons[1].voltage;
        assert!(v < 0.5, "loser kept its full charge: {v}");
    }

    #[test]
    fn tick_does_not_inhibit_when_nothing_fires() {
        let mut b = wired(&[([0.0; SENSES], 0.5, MotorAction::Gather)]);
        b.tick([0.0, 0.0, 0.0, 0.0]);
        assert!((b.motor_neurons[0].voltage - 0.5).abs() < 1e-6);
    }

    #[test]
    fn tick_reads_weights_as_motor_rows_sensor_columns() {
        // Column THREAT is vision, column SUGAR is smell. A transposed lookup swaps these.
        let motors = [
            ([THRESHOLD, 0.0, 0.0, 0.0], 0.0, MotorAction::Retreat), // listens to threat only
            ([0.0, THRESHOLD, 0.0, 0.0], 0.0, MotorAction::Gather),  // listens to sugar only
        ];
        assert_eq!(
            wired(&motors).tick([1.0, 0.0, 0.0, 0.0]),
            MotorAction::Retreat
        );
        assert_eq!(
            wired(&motors).tick([0.0, 1.0, 0.0, 0.0]),
            MotorAction::Gather
        );
    }

    #[test]
    fn tick_negative_weight_suppresses_bias() {
        // Exploring stops once a threat is seen
        let motors = [([-THRESHOLD, 0.0, 0.0, 0.0], THRESHOLD, MotorAction::Explore)];
        assert_eq!(
            wired(&motors).tick([0.0, 0.0, 0.0, 0.0]),
            MotorAction::Explore
        );
        assert_eq!(
            wired(&motors).tick([1.0, 0.0, 0.0, 0.0]),
            MotorAction::HoldPosition
        );
    }

    #[test]
    fn tick_updates_every_motor_neuron_even_after_one_fires() {
        // Motor 0 fires every tick. If the loop stopped at the first spike,
        // motor 1 would never charge.
        // motor 1 may be inhibited afterwards, but it must have moved off rest
        let mut b = wired(&[
            ([0.0; SENSES], THRESHOLD, MotorAction::Attack),
            ([0.0; SENSES], 0.5, MotorAction::Gather),
        ]);
        assert_eq!(b.tick([0.0, 0.0, 0.0, 0.0]), MotorAction::Attack);
        assert!(b.motor_neurons[1].voltage != 0.0, "never charged");
    }

    // --- learning ---

    /// Retreat listens to threat only, Gather to sugar only. One tick of full
    /// threat spikes the threat neuron and fires Retreat (motor 0).
    fn threat_fired() -> FlyBrainConnectome {
        let mut b = wired(&[
            ([THRESHOLD, 0.0, 0.0, 0.0], 0.0, MotorAction::Retreat),
            ([0.0, THRESHOLD, 0.0, 0.0], 0.0, MotorAction::Gather),
        ]);
        assert_eq!(b.tick([1.0, 0.0, 0.0, 0.0]), MotorAction::Retreat);
        b
    }

    #[test]
    fn eligibility_marks_only_the_winner_and_the_senses_that_spiked() {
        // A swapped row/column lookup would mark [THREAT][0] or the wrong motor
        let b = threat_fired();
        for (m, row) in b.plasticity.elig.iter().enumerate() {
            for (s, e) in row.iter().enumerate() {
                if (m, s) == (0, THREAT) {
                    assert!(*e > 0.0, "winner's threat synapse not eligible");
                } else {
                    assert_eq!(*e, 0.0, "elig[{m}][{s}]");
                }
            }
        }
    }

    #[test]
    fn reward_strengthens_and_punishment_weakens_the_eligible_weight() {
        let mut b = threat_fired();
        b.learn(1.0);
        assert!(b.weights[0][THREAT] > THRESHOLD);
        let mut b = threat_fired();
        b.learn(-1.0);
        assert!(b.weights[0][THREAT] < THRESHOLD);
    }

    #[test]
    fn learn_leaves_ineligible_weights_alone() {
        let mut b = threat_fired();
        b.learn(1.0);
        assert_eq!(b.weights[1], [0.0, THRESHOLD, 0.0, 0.0]);
    }

    #[test]
    fn reset_state_clears_voltages_and_traces_but_keeps_what_was_learned() {
        let mut b = threat_fired();
        b.plasticity.r_bar = 0.5;
        b.motor_neurons[1].voltage = 0.3;
        let weights = b.weights.clone();
        b.reset_state();
        assert!(b.motor_neurons.iter().chain(&b.sensory.neurons).all(|n| n.voltage == 0.0));
        assert!(b.plasticity.pre.iter().chain(&b.plasticity.post).all(|&x| x == 0.0));
        assert!(b.plasticity.elig.iter().flatten().all(|&e| e == 0.0));
        assert_eq!(b.weights, weights);
        assert_eq!(b.plasticity.r_bar, 0.5);
    }

    #[test]
    fn no_surprise_means_no_change() {
        let mut b = threat_fired();
        b.plasticity.r_bar = 0.5;
        let before = b.weights.clone();
        b.learn(0.5);
        assert_eq!(b.weights, before);
    }

    #[test]
    fn weights_stay_inside_their_bounds() {
        let mut b = wired(&[
            ([0.5, -0.5, 0.0, 0.0], 0.6, MotorAction::Attack),
            ([-0.5, 0.5, 0.0, 0.0], 0.6, MotorAction::Gather),
        ]);
        let start = b.weights.clone();
        for t in 0..200 {
            b.tick([1.0, 1.0, 0.0, 0.0]);
            b.learn(if t < 100 { 1000.0 } else { -1000.0 });
            for (row, row0) in b.weights.iter().zip(&start) {
                for (w, w0) in row.iter().zip(row0) {
                    assert!(w * w0.signum() >= 0.0, "{w} flipped sign from {w0}");
                    assert!(w.abs() <= crate::logic::learning::W_MAX);
                    if *w0 == 0.0 {
                        assert_eq!(*w, 0.0, "a missing synapse grew");
                    }
                }
            }
        }
    }

    #[test]
    fn eligibility_fades_while_nothing_fires() {
        // Nothing fires on these ticks, so this also checks `tick` still steps
        // the traces before its HoldPosition early return
        let mut b = threat_fired();
        for _ in 0..100 {
            assert_eq!(b.tick([0.0; SENSES]), MotorAction::HoldPosition);
        }
        let max = b.plasticity.elig.iter().flatten().fold(0.0f32, |a, e| a.max(e.abs()));
        assert!(max < 0.01, "eligibility left: {max}");
    }

    #[test]
    fn load_weights_replaces_the_weights() {
        let mut b = brain(2);
        let saved = vec![[0.7, -0.2, 0.0, 0.0], [0.3, -0.9, 0.0, 0.0]];
        assert!(b.load_weights(&saved));
        assert_eq!(b.weights, saved);
    }

    #[test]
    fn load_weights_keeps_the_brains_own_signs() {
        // brain() starts at [0.5, -0.5, 0, 0]: flipped signs clip to 0, a new synapse stays 0
        let mut b = brain(1);
        assert!(b.load_weights(&[[-0.7, 0.4, 0.9, 0.0]]));
        assert_eq!(b.weights, vec![[0.0, 0.0, 0.0, 0.0]]);
    }

    #[test]
    fn load_weights_rejects_a_different_shape() {
        let mut b = brain(2);
        assert!(!b.load_weights(&[[1.0; SENSES]]));
        assert_eq!(b.weights, brain(2).weights);
    }
}
