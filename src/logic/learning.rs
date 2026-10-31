use crate::logic::sensory::SENSES;

/// Share of a pre/post trace kept each tick. Slower than the neuron decay (0.8),
/// because rewards arrive several ticks after the action that earned them.
const TRACE_DECAY: f32 = 0.9;
/// Share of eligibility kept each tick: how long a synapse stays open to reward
const ELIG_DECAY: f32 = 0.95;
/// Learning rate. Eligibility peaks near 6 and settles near 3 in the role
/// brains, so a surprise of 1 moves a weight about 0.006-0.01 per tick.
pub const ETA: f32 = 0.002;
/// How fast the reward baseline follows the reward (an EMA rate)
pub const REWARD_WEIGHT: f32 = 0.05;
/// Largest size a learned weight may reach. The hand-tuned weights top out at 1.5.
pub const W_MAX: f32 = 2.0;

/// Reward-modulated STDP state for one brain. Every array has the same shape
/// as the brain's `weights`: rows are motors, columns are senses.
pub struct Plasticity {
    /// Fading memory of each sense's recent spikes
    pub pre: [f32; SENSES],
    /// Fading memory of each motor's recent wins
    pub post: Vec<f32>,
    /// How much each weight helped cause recent actions; reward is applied through it
    pub elig: Vec<[f32; SENSES]>,
    /// Running average reward, so only surprise teaches
    pub r_bar: f32,
    /// Sign each weight started with (1, -1, or 0 for no synapse), kept by `bound`
    pub sign: Vec<[f32; SENSES]>,
}

/// Dale's law: a weight keeps the sign it started with and stays within `W_MAX`.
/// A weight that started at 0 is no synapse at all, so it stays 0.
pub fn bound(w: f32, sign: f32) -> f32 {
    sign * (sign * w).clamp(0.0, W_MAX)
}

impl Plasticity {
    /// Starts with no traces, taking the signs `bound` keeps from `weights`.
    pub fn new(weights: &[[f32; SENSES]]) -> Self {
        let motors = weights.len();
        Self {
            pre: [0.0; SENSES],
            post: vec![0.0; motors],
            elig: vec![[0.0; SENSES]; motors],
            r_bar: 0.0,
            sign: weights
                .iter()
                .map(|row| row.map(|w| if w == 0.0 { 0.0 } else { w.signum() }))
                .collect(),
        }
    }

    /// Returns this tick's surprise, `reward - r_bar`, then moves the baseline
    /// toward `reward`. The surprise is taken first, so a reward is compared
    /// with what was expected before it arrived.
    pub fn surprise(&mut self, reward: f32) -> f32 {
        let d = reward - self.r_bar;
        self.r_bar += REWARD_WEIGHT * d;
        d
    }

    /// Updates the traces after one tick. `spikes` are this tick's sensory spikes
    /// (1.0 or 0.0) and `winner` is the motor that acted, if any.
    pub fn step(&mut self, spikes: &[f32; SENSES], winner: Option<usize>) {
        for (x, s) in self.pre.iter_mut().zip(spikes) {
            *x = TRACE_DECAY * *x + s;
        }
        for (m, row) in self.elig.iter_mut().enumerate() {
            let won = if winner == Some(m) { 1.0 } else { 0.0 };
            for s in 0..SENSES {
                row[s] = ELIG_DECAY * row[s] + self.pre[s] * won - self.post[m] * spikes[s];
            }
        }
        for (m, y) in self.post.iter_mut().enumerate() {
            let won = if winner == Some(m) { 1.0 } else { 0.0 };
            *y = TRACE_DECAY * *y + won;
        }
    }

    /// Forgets recent spikes, wins and eligibility. Keeps `r_bar` and `sign`,
    /// which describe the brain, not what it was just doing.
    pub fn clear_traces(&mut self) {
        self.pre = [0.0; SENSES];
        self.post.fill(0.0);
        self.elig.fill([0.0; SENSES]);
    }
}

#[cfg(test)]
mod learning_tests {
    use super::*;

    #[test]
    fn a_sense_and_motor_spiking_together_count_as_cause() {
        // If `post` were updated before `elig`, this would be +1 - 1 = 0
        let mut p = Plasticity::new(&[[1.0; SENSES]]);
        p.step(&[1.0, 0.0, 0.0, 0.0], Some(0));
        assert_eq!(p.elig[0], [1.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn a_sense_after_the_action_lowers_eligibility() {
        let mut p = Plasticity::new(&[[1.0; SENSES]]);
        p.step(&[0.0; SENSES], Some(0)); // motor wins with no sense spiking
        p.step(&[1.0, 0.0, 0.0, 0.0], None); // then the sense spikes
        assert!(p.elig[0][0] < 0.0, "{:?}", p.elig[0]);
    }

    #[test]
    fn surprise_is_taken_before_the_baseline_moves() {
        let mut p = Plasticity::new(&[]);
        assert_eq!(p.surprise(1.0), 1.0);
        assert!((p.r_bar - REWARD_WEIGHT).abs() < 1e-6);
    }

    #[test]
    fn steady_reward_stops_being_a_surprise() {
        let mut p = Plasticity::new(&[]);
        let last = (0..500).map(|_| p.surprise(1.0)).last().unwrap();
        assert!(last.abs() < 1e-3, "still surprised: {last}");
    }

    #[test]
    fn bound_keeps_the_starting_sign() {
        assert_eq!(bound(-0.3, 1.0), 0.0);
        assert_eq!(bound(0.3, -1.0), 0.0);
        assert_eq!(bound(0.7, 1.0), 0.7);
        assert_eq!(bound(-0.7, -1.0), -0.7);
    }

    #[test]
    fn bound_caps_at_w_max() {
        assert_eq!(bound(5.0, 1.0), W_MAX);
        assert_eq!(bound(-5.0, -1.0), -W_MAX);
    }

    #[test]
    fn a_missing_synapse_stays_missing() {
        let p = Plasticity::new(&[[0.0, 0.5, -0.5, 0.0]]);
        assert_eq!(p.sign[0], [0.0, 1.0, -1.0, 0.0]);
        assert_eq!(bound(0.4, 0.0), 0.0);
    }
}
