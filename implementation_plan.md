# Implementation plan: learning from death, parallel training and a weight-history graph

**Goal:** Train Flaud in rounds of n parallel matches, each played under different but reproducible conditions. Average what the instances learned into `brains.json`, and graph how every weight changes from round to round. Losses, including matches where everything dies, must teach the brain as much as wins do.

The work comes in five phases, and each one depends on the one before it. **Phase 1 fixes a learning bug.** Without it, every parallel round would average n results that are all missing the same signal.

```
Phase 1  learn from death        (punish before pruning, keep dead brains)
Phase 2  split learning from saving  (a match returns a smoothed snapshot; something else writes it)
Phase 3  pseudo-novel conditions (a round seed decides each instance's settings)
Phase 4  parallel training rounds (run n, average, save once)
Phase 5  weight history + graph  (append one row per weight per round)
```

---

## Phase 1: Learn from death

### The problem

Each tick, and again in `finish` (`src/bots/flaud.rs:235`), the order is:

1. `update_groups` → `prune` (`src/bots/flaud.rs:380`) removes dead ids, then **drops any group left with no units**, brain included.
2. `reward_groups` (`src/bots/flaud.rs:330`) gives −1 per lost unit, but only to groups that still exist.

| What happened this tick | Punished? |
|---|---|
| A group lost 2 of its 5 units | Yes, −2 |
| A group lost all 5 units | **No.** It was dropped before it could be punished |

The worst outcome gets no punishment. The eligibility trace $e_{m,s}$, which links the actions to the death, is thrown away with the brain.

`remember` (`src/bots/flaud.rs:314`) then averages only the groups still alive. After a total wipe, `average_weights` returns `None` and the role keeps last match's weights, so the match taught nothing. Even when some groups survive, only the survivors' weights are saved. That's **survivorship bias**: the brains that learned "don't do that" are forgotten.

### Step 1: Punish before discarding

Make sure a group that loses its last unit still gets `learn(r)` with its losses before it's removed. Choose one:

- **(a) Reorder the steps:** reward → prune → add new units → snapshot. `Snapshot::lost` compares last tick's ids with who is alive now, so it doesn't need the group pruned first. Watch out for new units joining *after* the snapshot: they're missing from `last` for a tick, so check that this doesn't count as a loss or a missed delivery.
- **(b) Split `prune`:** drop dead *ids* before the reward (as now), and drop *empty groups* only after `reward_groups` has run. This is the smaller change to the order, so it's easier to reason about. One thing to check: `Snapshot::lost` must still see the dead ids. It reads `g.last`, not `g.ids`, so this should work, but confirm it with a test.

*Recommended: (b).*

**Tests:**
- **A wiped group is punished.** Build a group of 3 units with a `last` snapshot. Give it a state where none of them is alive, run a tick, and assert its brain's weights moved in the punished direction for the eligible synapse (set up the eligibility first, as `threat_fired` does in `connectome.rs`). This fails today.
- **A partly killed group is still punished the same way.** This guards against a regression.

### Step 2: Keep dead brains in a graveyard

When an empty group is dropped (after its final punishment), move its `FlyBrainConnectome` into a per-role list on `Flaud` (for example `fallen: [Vec<…>; 3]`, or one `Vec` per role) instead of dropping it. `remember` then averages living **and** fallen brains.

Rust note: `Vec::retain` can't hand you the removed items. Use `Vec::extract_if` (stable since Rust 1.87), or `std::mem::take` the vector and partition it with `Iterator::partition`. Either way the brain is **moved**, not cloned.

### Step 3: Decide how much each brain counts

**The decision: equal votes or weighted by lifetime?**

$$W^{role}_{m,s} = \frac{\sum_g \ell_g\, W^{(g)}_{m,s}}{\sum_g \ell_g}$$

- $W^{(g)}_{m,s}$: group $g$'s weight from sense $s$ to motor $m$ when it died or when the match ended (`f32`, in $[-W_{max}, W_{max}]$)
- $\ell_g$: how many times group $g$ called `learn`, i.e. ticks lived (`u32`, ≥ 1). Needs a counter on `BrainGroup` or `Plasticity`
- $g$: ranges over living and fallen groups of the role

Each brain counts in proportion to how much it experienced. A group that died on its first tick has barely moved from its starting weights, so with an equal vote it would pull the average back toward the start. With $\ell_g = 1$ for every group, this is the plain average used now (`average_weights`, `src/bots/flaud.rs:582`).

It's still a convex combination (non-negative weights that add up to 1), so if every $W^{(g)}$ obeys Dale's law and lies in $[0, W_{max}]$ or $[-W_{max}, 0]$, the result does too. No clipping is needed.

*Recommended: weight by lifetime.*

**Tests:**
- **A total wipe still produces weights.** Every group dies; `remember` stores weights for that role (it doesn't keep the old ones) and they differ from the starting weights.
- **Lifetime weighting.** One group with $\ell = 1$ and weight 0.0, one with $\ell = 3$ and weight 1.0: the stored weight is 0.75.
- **The existing `remember` tests still pass** (`src/bots/flaud.rs:1998` onward).

**Afterwards:** add the averaging equation to the "Current fly-brain equations" section in `CLAUDE.md`.

---

## Phase 2: Split learning from saving

Today `Flaud::new` reads `brains.json` and `finish` writes it (`BRAINS_FILE`, `src/bots/flaud.rs:35`). With n instances, every one would overwrite the file and the **last one to finish wins**: n−1 matches of learning are lost. The temp-file rename stops torn files, not lost updates.

### Step 4: A match returns what it learned

Give Flaud a way to start from a `BrainStore` passed in and to hand back what it learned, without touching the disk. For example:

- a constructor that takes the starting `BrainStore` (or `&BrainStore`), and
- a method that runs the final `update_groups` / `reward_groups` / `remember` and returns the learned per-role weights **plus each role's total $\ell$**. Phase 4's merge needs that total.

Normal play keeps working as it does now: load, play, `finish` saves. Training uses the new path. Ownership: returning the store **by value** (not `&`) lets the match task hand it back across threads with no lifetimes involved.

What this returns is the **smoothed snapshot** from Step 4a, not the raw final weights.

### Step 4a: Smoothed snapshot (an exponential moving average of the weights)

Each instance keeps a running snapshot of each role's weights **in memory**, updated every $\tau$ ticks. At match end, that snapshot is what Step 4 returns. Nothing touches the disk mid-match, so the single-writer design is unchanged.

**Why:** the final weights depend on the last few ticks, and the end of a match is chaotic. A wipe in the last 20 ticks gives a jolt of punishment the brain never gets to balance. A moving average over the late part of the match smooths that noise out (this is called *Polyak averaging* or weight averaging) without the heavy lag of an equal-weight mean over the whole match.

**The math**

At snapshot $k$, first take the role's current weights, lifetime-weighted over living **and** fallen groups (the same rule as Step 3):

$$W_k = \frac{\sum_g \ell_g\, W^{(g)}}{\sum_g \ell_g}$$

then fold them into the running snapshot:

$$S_1 = W_1, \qquad S_k = \beta\, S_{k-1} + (1-\beta)\, W_k \quad (k \ge 2)$$

- $W_k$: the role's weights at snapshot $k$ (`Vec<[f32; SENSES]>`, each in $[-W_{max}, W_{max}]$)
- $S_k$: the running snapshot after $k$ snapshots, same shape
- $\beta \in [0, 1)$: how much of the old snapshot to keep (suggested name `SNAPSHOT_KEEP`)
- $\tau$: ticks between snapshots (suggested name `SNAPSHOT_EVERY`)
- $\ell_g$, $W^{(g)}$: as in Step 3

In plain words, each snapshot moves the running value a fraction $1-\beta$ of the way toward the current weights. A snapshot taken $j$ steps ago counts with weight $(1-\beta)\beta^{j}$, so older ones fade out geometrically. The memory is about $\frac{1}{1-\beta}$ snapshots, or $\frac{\tau}{1-\beta}$ ticks.

**Choosing $\beta$.** Make the memory cover the last quarter of a match of $T$ ticks:

$$\frac{\tau}{1-\beta} = \frac{T}{4} \quad\Longrightarrow\quad \beta = 1 - \frac{4\tau}{T}$$

With $\tau = 50$ and $T = 2000$, $\beta = 0.9$: the snapshot reflects about the last 500 ticks. If Phase 3 randomizes match length, compute $\beta$ from that instance's `max_ticks` rather than using a fixed constant, so short and long matches both average over their last quarter.

**What it costs.** The snapshot lags the live weights by about $\frac{\tau}{1-\beta}$ ticks. For a weight drifting steadily at $v$ per tick, the snapshot ends about $v\,\frac{\tau\beta}{1-\beta}$ behind the final weight. With $\beta = 0.9$ that's roughly 450 ticks of drift, compared with half the match (1000 ticks) for an equal-weight mean. Learning *inside* the match still runs at full speed; only what's carried into the next match is smoothed.

It is still a convex combination of weights that obey Dale's law and the $\pm W_{max}$ bounds, so $S_k$ does too. No clipping is needed.

**Steps**

1. A small struct per role (for example in `memory.rs`) holding `Option<Vec<[f32; SENSES]>>`: `None` until the first snapshot. Use `None` rather than zeros, because initializing $S_0 = 0$ would drag every weight toward 0 and look like "weakening" on the graph. A method `fold(&mut self, w, beta)` applies the recurrence above.
2. In `Flaud::decide`, every `SNAPSHOT_EVERY` ticks (`state.tick % SNAPSHOT_EVERY == 0`), compute $W_k$ for each role from living groups plus the graveyard, and `fold` it in.
3. In the end-of-match path (Step 4), take **one last snapshot** before returning, so the final stretch after the last multiple of $\tau$ counts too.
4. If a role never got a snapshot (a very short match, or the role never existed), fall back to its lifetime-weighted final weights, or leave it out so the merge keeps the stored value, as `remember` does now.
5. Normal play's `finish` can save the snapshot too, so single matches and training rounds carry the same thing forward.

**Tests:**
- **The first snapshot is taken as-is.** `fold` once with $W$: the snapshot equals $W$ (it doesn't come out as $(1-\beta)W$).
- **The recurrence.** $\beta = 0.5$: fold 0.0, then 1.0, then 1.0. The snapshot is 0.75.
- **Constant weights are a fixed point.** Fold the same $W$ ten times: the snapshot equals $W$.
- **Bounds hold.** Fold random values within $[0, W_{max}]$: the snapshot stays inside.
- **The graveyard is included.** A group that dies between two snapshots still shows up in the next $W_k$ (through Step 2's graveyard).
- **The last snapshot is taken.** A match ending at a tick that isn't a multiple of $\tau$ still folds in its final weights.

**Afterwards:** add the moving-average equation to the "Current fly-brain equations" section in `CLAUDE.md`.

### Step 5: Headless match runner

`play_match` (`src/ui/console.rs:386`) is tied to `self.client.match_id` and to `MatchScreen`. Pull the tick loop (get state → `decide` → `submit_commands` → long-poll) out into a function that takes a match id and an `&Api` (or `Arc<Api>`), draws nothing, and returns the learned store and the match result. `play_match` can then call it and add the screen on top, or keep its own loop if the screen is too tied in.

Check: is `Api` `Clone`, or does it need to go in an `Arc`? Its `match_id` field suggests one `Api` per match, so the runner should take the match id as a parameter instead.

**Tests:**
- Use `wiremock` (already used in `api.rs`) to serve a match that goes from `Running` to `Completed` in two ticks, and assert the runner returns a store with `matches == 1`.

---

## Phase 3: Pseudo-novel conditions

"Pseudo" is the important part. Every instance's conditions come from **one round seed**, so any round can be replayed exactly.

### Step 6: Settings for each instance from the round seed

A function from `(round_seed, instance index i, n)` to that instance's `CreateMatchRequest` settings.

- **Map seed:** from a seeded RNG. Add the `rand` crate and use `StdRng::seed_from_u64(round_seed)`, or write a tiny SplitMix64 if you'd rather avoid a dependency.
- **Opponent:** stratified rather than random. Instance $i$ plays house bot $b_{i \bmod k}$ from a list of $k$ opponents. A random pick could give 6 `sitter`s out of 8 and tilt the round's average. Rotate the starting offset each round so the same instance index doesn't always get the same opponent.
- **Map size, max ticks, etc.:** randomize only inside the range the competition will actually use. Anything outside it trains for situations that will never come up.

$$b(i, \rho) = \text{bots}\big[(i + \rho) \bmod k\big]$$

- $i$: instance index, $0 \le i < n$
- $\rho$: the round number (or a value drawn from the round seed), used as the rotating offset
- $k$: the number of house bots in the list

Every round gets an almost even mix of opponents: each bot appears $\lfloor n/k \rfloor$ or $\lceil n/k \rceil$ times.

**Tests:**
- **Reproducible:** the same round seed gives the same settings for every $i$.
- **Different:** two round seeds give different map seeds.
- **Balanced:** with $n = 8$ and $k = 3$, the opponent counts are 3/3/2 in some order.

---

## Phase 4: Parallel training rounds

### Step 7: The round loop

```
load brains.json ─┬─▶ instance 1 ─▶ (W₁, ℓ₁) ─┐
                  ├─▶ instance 2 ─▶ (W₂, ℓ₂) ─┼─▶ average ─▶ save once ─▶ log the round
                  └─▶ instance n ─▶ (Wₙ, ℓₙ) ─┘
```

For each round: load the store once → create n matches with the Phase 3 settings → spawn one tokio task per instance with the Phase 2 runner, each **owning** a clone of the starting store → `join` them all → merge → save → log (Phase 5).

No `Mutex` is needed. Nothing mutable is shared, and results come back as the tasks' return values (`JoinSet` or a `Vec<JoinHandle<_>>`). A failed instance (network error, rejected match) is left out of the merge and logged, not allowed to crash the round.

`Flaud` must be `Send` to move into `tokio::spawn`. If it isn't, the compiler error will say which field is the problem.

### Step 8: Average across instances

The same rule as Step 3, one level up:

$$W^{new}_{m,s} = \frac{\sum_{i} L_i\, W^{(i)}_{m,s}}{\sum_i L_i}$$

- $W^{(i)}_{m,s}$: instance $i$'s smoothed snapshot for the role ($S$ from Step 4a)
- $L_i$: the total of $\ell_g$ for that role in instance $i$ (returned in Step 4). Use $L_i = 1$ for a plain average
- $i$: only instances that have weights for this role. A role nobody used keeps its stored weights, as `remember` does now

One round moves the weights about as far as **one** match would, but with n times less noise, like a bigger minibatch. That keeps `ETA` meaning what it means now. Summing the changes instead would act like an n× bigger `ETA` and could overshoot.

Put this in `memory.rs` (for example `BrainStore::merge`) so it can be tested without a server.

**Decision: what does `matches` count?** +n per round (the number of games the weights have seen) or +1 (the number of updates)? Consider adding a separate `rounds` field so both are recorded.

**Tests:**
- **Average:** two stores with a weight of 0.2 and 0.6 and equal $L$ merge to 0.4.
- **Weighted:** $L = 1$ and $L = 3$ with 0.0 and 1.0 merge to 0.75.
- **Missing role:** a role present in only one store takes that store's weights. A role in none keeps the base.
- **Dale's law holds:** merging same-sign weights never flips a sign or goes past $W_{max}$.

### Step 9: Starting a training run

A CLI flag (`--train <n> --rounds <r> --round-seed <s>`) and/or a menu entry in `console.rs`. Headless: one progress line per instance (opponent, tick, result) instead of n full match screens.

Before the first big run: check whether the server allows one API key to run n matches at once, and whether it rate-limits. Start with $n = 2$.

---

## Phase 5: Weight history and graph

### Step 10: Append-only history log

After each round's save, **append** one row per weight to `brain_history.csv` (or `.jsonl`). Never rewrite it: a row is a fixed fact, and a line on the graph is just a query over rows.

| run | round | round_seed | role | motor | sense | mean | min | max |
|---|---|---|---|---|---|---|---|---|

- `mean` is the merged weight that was saved. `min` and `max` are across the instances, and they give the band drawn around the line.
- `run` is an id for one training run (a start timestamp, or a hash of `ETA`/`REWARD_WEIGHT`/`W_MAX`), so runs with different tuning don't mix.

Optionally, a second file with one row per round and instance: `round, instance, opponent, map_seed, won, total_reward, ticks`. A weight that climbs while the reward stays flat is drift, not learning. This table shows that.

Open the file in append mode (`OpenOptions::new().append(true).create(true)`). Write the header only when the file is new.

**Tests:**
- Appending two rounds gives header + 2×(number of weights) rows, and the first round's rows are unchanged.

### Step 11: Plot it

Outside the bot first: a spreadsheet or a short Python/matplotlib script reading the CSV. Fix the y-axis to $[-W_{max}, W_{max}] = [-2, 2]$ so a weight that **dies** (pinned at 0) or **saturates** (pinned at ±2) stands out. Both mean `ETA` or the reward needs tuning. A wide min-max band means the instances disagree, and you'd want a bigger n or a smaller `ETA`.

Later, if wanted: sparklines in the terminal visualizer (crossterm only), or `ratatui`'s `Chart` widget.

---

## Decisions to make

| # | Decision | Recommended |
|---|---|---|
| 1 | Punish before pruning: reorder or split `prune` (Step 1) | Split `prune` |
| 2 | Weighting for dead and living brains (Steps 3 and 8) | By lifetime $\ell$ |
| 3 | What to randomize (Step 6) | Map seed + stratified opponent; map size only if the competition varies it |
| 4 | `matches`: +n or +1 per round; add `rounds`? (Step 8) | +n, and add `rounds` |
| 5 | History format (Step 10) | CSV |
| 6 | How training starts (Step 9) | CLI flags first, menu later |
| 7 | Snapshot aggregate (Step 4a) | **Decided:** exponential moving average |
| 8 | `SNAPSHOT_EVERY` $\tau$ and the horizon for $\beta$ (Step 4a) | $\tau = 50$, horizon = last quarter of the match ($\beta = 1 - 4\tau/T$) |

**Next:** Phase 1, Step 1, the test that a wiped group gets punished. Write it first and watch it fail.
