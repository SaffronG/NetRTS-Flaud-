# Implementation plan: reward-modulated learning for the fly brain

**Question:** Is there a good way to implement a feedback loop for my LIF system so I can incrementally, programmatically change the weights with my current framework?

Yes. What's started in `src/logic/learning.rs` is **reward-modulated STDP** (R-STDP, a "three-factor" rule), and it suits this setup well. It only needs things the brain already computes each tick: which senses spiked (`spikes`) and which motor won. The third factor is a reward the bot works out from game state.

## Review of the current sketch (`src/logic/learning.rs`)

Keep the overall shape: an eligibility trace per synapse, with the reward applied as $(r - \bar r)$ against a running baseline. A few things need changing:

- **bug** `src/logic/learning.rs:28`: `_ = syn.iter_mut().map(...)` never runs. Rust iterators are lazy, and `map` does nothing until something consumes it. Use a `for` loop or `.for_each`.
- **bug** `learning.rs:4`: `clamp(0.0, W_MAX)` with `W_MAX = 0.1` would squash the hand-tuned weights. For example, the army's Attack weight of `1.2` drops to `0.1`, and every inhibitory `-1.0` becomes `0.0`, so workers would gather under fire.
- **risk** `learning.rs:1`: the import from `bots::flaud::DECAY` makes `logic` depend on `bots`, which is the wrong direction. Also, the trace decay shouldn't be the neuron decay. Rewards arrive several ticks after the action, so traces need to fade more slowly (≈0.9–0.95, versus 0.8).
- **style**: `Synapse { pre, post }` repeats information `weights` already holds. The network is dense (every motor sees every sense), so an eligibility matrix with the same shape as `weights` (`Vec<[f32; SENSES]>`) is simpler. No indices are needed.
- **gap**: `pre_tr`/`post_tr` are passed in, but nothing stores or updates them between ticks. They need to live on the brain.

## The plan

Plasticity state goes **inside `FlyBrainConnectome`**. `tick` updates the traces using values it already has. A new `learn(reward)` method changes the weights. `flaud.rs` computes one reward per group from what changed since the last tick. Per-group rewards are better than one global reward for credit assignment: a worker group shouldn't be rewarded because an army somewhere won a fight.

### The math

**Pre trace** (one per sense) and **post trace** (one per motor):

$$x_s \leftarrow \lambda_x x_s + \sigma_s, \qquad y_m \leftarrow \lambda_y y_m + [m = m^*]$$

- $\sigma_s \in \{0,1\}$: sense $s$ spiked this tick (`spikes`).
- $m^*$: the winning motor. $[m = m^*]$ is 1 for the winner and 0 otherwise.
- $\lambda_x, \lambda_y \in [0,1)$: how quickly each trace fades.

In plain words, each trace is a fading memory of recent spikes.

**Eligibility** (one per weight, same shape as `weights`):

$$e_{m,s} \leftarrow \lambda_e\, e_{m,s} + x_s\,[m = m^*] - y_m\,\sigma_s$$

When a motor wins, every sense that spiked recently gets credit for it (sense before action means it caused the action, so the trace goes up). When a sense spikes after that motor recently fired, the sense couldn't have caused it, so the trace goes down. $\lambda_e$ sets how long a synapse stays open to reward.

**Weight update and baseline** (in `learn`):

$$W_{m,s} \leftarrow \text{clip}\big(W_{m,s} + \eta\,(r - \bar r)\,e_{m,s}\big), \qquad \bar r \leftarrow \bar r + \alpha (r - \bar r)$$

- $r$: this tick's reward.
- $\bar r$: the running average reward (`r_bar`).
- $\eta$: learning rate.
- $\alpha$: how fast the baseline follows the reward (`REWARD_WEIGHT`).

The brain learns from *surprise*: a group that is always rewarded about the same amount stops changing. Compute $d = r - \bar r$ *before* moving $\bar r$; the sketch already does this correctly.

Scale check: the weights are around 0.5–1.2, so aim for $\eta \cdot |d| \cdot e \approx 0.01$ per tick. That way learning nudges the weights over a match instead of overwriting the tuning in a few ticks.

### Steps

1. **Plasticity struct** (`learning.rs`): replace `Synapse` with a struct holding `pre: [f32; SENSES]`, `post: Vec<f32>`, `elig: Vec<[f32; SENSES]>`, `r_bar: f32`. Give it its own constants (`TRACE_DECAY`, `ELIG_DECAY`, `ETA`). Add `fn new(motors: usize)`.
2. **`Plasticity::step(&mut self, spikes: &[f32; SENSES], winner: Option<usize>)`**, which implements the eligibility equation. Order matters: add this tick's spikes to `pre` first, then update `elig` using the post trace *from before this tick*, and update `post` last. If both traces include this tick, a sense and motor spiking together cancel out ($+1 - 1 = 0$) and nothing is learned.
3. **Wire it into `tick`** (`connectome.rs`): add a `plasticity: Plasticity` field built in `new`. Call `step` at the end of `tick` with `spikes` and the winner index `w`. Remember the early `return HoldPosition`: `step` must still run with `None` there, or the traces stop fading. "Post spike" means the *winner*, not every motor over threshold, because only the winner's action reached the game.
4. **`FlyBrainConnectome::learn(&mut self, reward: f32)`**: applies the weight update. Write it here rather than in `Plasticity`, because it needs `&mut self.weights` and `&mut self.plasticity` at the same time. As two separate fields of `self`, the borrow checker allows that.
5. **Weight bounds** (see decisions below).
6. **Reward per group** (`memory.rs` + `flaud.rs`): add a small snapshot struct per `BrainGroup` (unit count, resources, plus whatever the role needs). Each tick in `decide`, compare the current state with the snapshot, call `g.brain.learn(r)`, then save a new snapshot. Losing units counts as a negative reward for every role.
7. **Tests** (`connectome.rs` and `learning.rs`):
   - **Eligibility only marks the winner's row and the senses that spiked.** Feed one sense hard enough to fire a motor once. Assert that eligibility is positive only at `[winner][sense]` and zero everywhere else. This catches rows and columns being swapped.
   - **The sign of the reward sets the direction of the change.** Do the same setup, then call `learn(+1.0)` and check the weight went up. On a fresh brain, `learn(-1.0)` should make it go down.
   - **No surprise means no change.** Once `r_bar` equals `r`, `learn(r)` leaves the weights unchanged.
   - **Bounds hold.** Call `learn(1000.0)` many times and assert every weight stays inside its bounds.
   - **Eligibility fades.** After about 50 ticks with no spikes, all eligibility traces should be close to 0.
   - **Behaviour test** (in `flaud.rs`): reward a worker brain every time it gathers near sugar. Afterwards it should reach Gather in fewer ticks than a fresh brain, reusing `first(...)`.
8. *(Optional, later)* **Save weights between matches** with serde, so learning builds up across games. Also *optional*: learn biases too, by treating each bias as a weight from an input that's always 1.

## Decisions before starting

1. **Weight bounds (step 5).** Which is preferred?
   - **(a)** Keep the sign: excitatory weights stay in $[0, W_{max}]$ and inhibitory ones in $[-W_{max}, 0]$. This is like Dale's law in biology: learning can't turn "flee threats" into "approach threats".
   - **(b)** Stay near the hand-tuned value: $W_0 \pm \Delta$. This is the safest for a competition bot.

   Recommended: **(b)** to start with. The tuned behaviour can't drift far, and $\Delta$ can be widened once learning looks healthy.
2. **What counts as reward for each role (step 6).** Some starting ideas:
   - Workers: the change in `you.resources`.
   - Army: enemy units that disappeared near the group, minus its own losses.
   - Scouts: new tiles seen, minus losses.

   Pick the signals, then check `events`/`GameStateDto` for what's actually available.

**Next:** start with **step 1**: rewrite `learning.rs` with the `Plasticity` struct and `new`.
