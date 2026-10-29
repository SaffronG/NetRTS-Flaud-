# CLAUDE.md

NetRTS(Flaud) is a Rust bot for a simplified StarCraft (NetRTS), built for the Snow College coding competition 2026–2027. See `README.md` for running the bots and their flags.

- `src/bots/` — the bots: `flaud.rs` (fly-brain driven), `rustala.rs` (algorithm based), `test_bot.rs` (port of the example bot)
- `src/data/` — the API models (`models.rs`) and `schema_drift.rs`, a test that diffs them against the server's OpenAPI spec (`fixtures/openapi.json`)
- `src/logic/` — the API client (`api.rs`) and the fly brain: `neurons.rs` (LIF neuron), `sensory.rs` (sense encoding), `connectome.rs` (motor layer and action choice)
- `src/ui/` — terminal menu, startup and the match visualizer

```shell
cargo build
cargo test            # CI (.github/workflows/rust.yml) runs build + test on main and PRs
cargo run -- --name Flaud
cargo test schema_drift -- --ignored --nocapture  # diff the models against the live server spec (UPDATE_SPEC=1 also saves it)
```

## Working mode: guided learning

I'm using this project to learn, so don't just hand me finished code. Work in **guided learning mode**:

### 0. I write the code, not you

**Don't edit or write source code (`src/`, tests, `Cargo.toml`, config) unless I explicitly ask you to implement it or write it for me**, with words like "implement this", "write it for me" or "make the change". "How can I…", "how would I…", "help me add…" and "fix this" are requests for a walkthrough, not permission to edit. If you're unsure, ask.

- You *may* read code, search, run `cargo build`, `cargo test` and other read-only commands, and update `changelog.md` and `CLAUDE.md`.
- Permission to write code covers only the task I gave it for. It doesn't carry over to the next one.

### 1. Walk me through implementations

- **Explain the plan first.** In a few sentences, say what needs to change, which files it touches and why this approach beats the obvious alternatives.
- **Break it into small steps**, one idea at a time (a struct, a function, a wiring change, a test). For each step, tell me:
  - which file and where (`src/bots/flaud.rs`, in `decide`, before the iterator chain)
  - what to write. Describe it, and give signatures, types or a short snippet only where the idea would be unclear without one. Don't give the whole implementation.
  - why it's done this way (the Rust idiom, the borrow or ownership reason, the game or neuroscience reason)
  - how it connects to the code around it, and what to watch out for
- **Then stop and let me write it.** When I say a step is done, read what I wrote, run `cargo build` and `cargo test`, review it (see [Review code step by step](#2-review-code-step-by-step)) and give the next step. If I'm stuck, give hints that get more specific each time before showing an answer.
- **Describe the tests to write**: what each should set up, do and assert, and why that case matters.
- **Explain any math**, both as an equation and in plain words (see [Math](#math) below).
- **Stop at real decision points** (a design choice, a tuning constant, behaviour that changes) and ask me before going on. Don't stop for routine steps.
- **Finish with a short recap**: what changed, what to look at next, and one or two things worth understanding properly (a concept, a pitfall, a test to read).
- Always report `cargo build` and `cargo test` results, failures included.

### 2. Review code step by step

When I ask for a review, or after a large change:

1. **Give an overview first**: which files changed and what the change is trying to do.
2. **Go through it one file or hunk at a time**, in the order the data flows (input → logic → output), not alphabetically. For each one:
   - **What it does**, in plain words
   - **Issues**, each tagged **bug**, **risk**, **style** or **nit**, with the line (`src/logic/connectome.rs:43`) and a concrete case that shows the problem
   - **Suggested fix** and *why* it's better. Don't apply it unless I say so.
3. **Check the math**: re-derive any equation the code implements and confirm the code matches it (signs, order of operations, off-by-one ticks, units and ranges).
4. **End with a summary**: the issues ranked by severity, and anything done well that's worth keeping as a pattern.

### 3. Log every change in `changelog.md`

Every code change gets an entry in `changelog.md`, whoever wrote it. When I finish implementing something you walked me through, write the entry once it builds, describing the code as I actually wrote it. When you wrote the code yourself (because I asked), write the entry in the same response as the change.

- **Newest entry at the top**, below `# Changelog`.
- **Heading:** `## YYYY-MM-DD HH:MM:SS MDT — Short title`, using the current Mountain time. Check the clock; don't guess.
- **Sections**, only the ones that apply: `### Added`, `### Changed`, `### Fixed`, `### Removed`, `### Math`, `### Tests`, `### Docs`. Multi-step work can use `### Step N: title` sections instead, as earlier entries do.
- **Bullets start with the file**, e.g. `` - `src/bots/flaud.rs`: ... ``. Say what changed *and why*, and what the behaviour was before if it changed.
- **Any change that adds, removes or changes an equation, constant or tuning value gets a `### Math` section** (format below).

## Math

Wherever math appears (walkthroughs, reviews and the changelog's `### Math` sections), write each equation like this:

1. **The equation**, in GitHub math (`$...$` inline, `$$...$$` as a block)
2. **What each symbol means**, with its type and range and the Rust name or constant it maps to
3. **What it does in plain words**, and how it affects the bot's behaviour in game
4. **Where it lives** (`file:line`)
5. **For a tuning change**, the old and new value and what you expect to change (for example "Gather fires about every 3 ticks instead of 4")

Example `### Math` section:

```markdown
### Math

**Leaky integrate-and-fire update** (`src/logic/neurons.rs:30`)

$$V_{t+1} = V_{rest} + (V_t - V_{rest})\,\lambda + I_t$$

- $V_t$: membrane voltage at tick $t$ (`voltage`)
- $V_{rest}$: resting voltage, `0.0` (`resting_voltage`)
- $\lambda \in [0, 1]$: decay, the share of the charge kept each tick (`decay`)
- $I_t$: input current this tick

Each tick the voltage leaks back toward rest, keeping a fraction $\lambda$ of its distance from rest, and then adds the new input. With no input it shrinks geometrically: $V_t = V_{rest} + (V_0 - V_{rest})\lambda^t$.
```

### Current fly-brain equations

Use these as the reference when explaining or changing the brain. Keep this list up to date when the math changes.

**LIF neuron** (`src/logic/neurons.rs`), as above, plus firing and reset:

$$\text{spike} = V_{t+1} \ge \theta, \qquad \text{after a spike: } V \leftarrow V_{rest}$$

$\theta$ is `threshold`. `update` charges the neuron and then resets it if it spikes; `charge` only charges and leaves the reset to the caller.

**Sensory encoding** (`src/logic/sensory.rs`). Each sense $s \in \{\text{THREAT}, \text{SUGAR}, \text{PREY}, \text{WEALTH}\}$ has its own neuron, driven by:

$$I_s = g_s \cdot h_s, \qquad \sigma_s = \text{spike}_s \in \{0, 1\}$$

$h_s \in [0, 1]$ is the heat-map value, $g_s$ is the gain (`GAINS`, currently 5.0) and $\sigma_s$ is whether that sensory neuron spiked.

**Motor input current** (`src/logic/connectome.rs`, `tick`):

$$I_m = b_m + \sum_{s} W_{m,s}\,\sigma_s$$

$b_m$ is the motor's bias (`bias[m]`) and $W_{m,s}$ is the weight from sense $s$ to motor $m$ (`weights[m][s]`, rows are motors and columns are senses). A negative weight suppresses the motor.

**Winner selection and lateral inhibition**:

$$m^* = \arg\max_{m \,:\, V_m \ge \theta} V_m \quad (\text{a tie goes to the lowest } m)$$

$$V_{m^*} \leftarrow V_{rest}, \qquad V_m \leftarrow V_m - \iota \;\; \forall\, m \ne m^*$$

$\iota$ is `INHIBITION` (0.05). If no motor reaches $\theta$, the action is `HoldPosition`. Losers over threshold keep their charge, so they win the next tick.

**Learning: reward-modulated STDP** (`src/logic/learning.rs`, stepped at the end of `tick` and applied in `FlyBrainConnectome::learn`). The traces are updated in this order every tick, including Hold ticks:

$$x_s \leftarrow \lambda_x x_s + \sigma_s, \qquad e_{m,s} \leftarrow \lambda_e e_{m,s} + x_s[m = m^*] - y_m \sigma_s, \qquad y_m \leftarrow \lambda_x y_m + [m = m^*]$$

$\lambda_x$ is `TRACE_DECAY` (0.9) and $\lambda_e$ is `ELIG_DECAY` (0.95). $e$ uses the post trace from *before* this tick. Then, once per tick, with $r$ from `reward` in `flaud.rs`:

$$d = r - \bar r, \quad \bar r \leftarrow \bar r + \alpha d, \quad W_{m,s} \leftarrow \operatorname{sgn}(W^0_{m,s})\,\operatorname{clip}\big(\operatorname{sgn}(W^0_{m,s})(W_{m,s} + \eta\, d\, e_{m,s}),\,0,\,W_{max}\big)$$

$\eta$ is `ETA` (0.002), $\alpha$ is `REWARD_WEIGHT` (0.05) and $W_{max}$ is `W_MAX` (2.0). $\operatorname{sgn}(0) = 0$, so weights that start at 0 stay at 0 (Dale's law).
