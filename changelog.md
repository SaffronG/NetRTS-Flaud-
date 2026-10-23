# Changelog

## 2026-10-23 00:00:03 MDT — Worker groups mine their own ore chunk

### Added

- `src/logic/shared.rs`: `group_resources` rewritten as a flood fill. Deposits with ore left are split into chunks: two deposits share a chunk if a chain of touching deposits (Chebyshev distance 1) links them. The previous version didn't compile (`acc.last()` is read-only). Its containment check could never be true, it grew boxes by +1 on the wrong axes, it only checked the box's two corners, and it only compared each deposit with the last box, so the result depended on input order.
- `src/logic/shared.rs`: `Bound` now holds the chunk's deposit `ids` (replacing the `w` count) and public box edges. It gains `Bound::at`, `Bound::add` (moves each edge out with min/max) and `Bound::dist_to` (point-to-box distance).
- `src/bots/flaud.rs`: `BrainGroup.mine`, the deposit ids a worker group mines exclusively. `assign_mines` runs each tick after `update_groups`. It drops dry ids from every claim, then gives each worker group with an empty claim the unclaimed chunk nearest its centroid. A chunk later split by a dry deposit stays with its owner, because both halves contain claimed ids.

### Changed

- `src/bots/flaud.rs`: Gather only targets deposits in the group's claim. A group with no claim (every chunk is taken) still gathers from the nearest deposit; worker exploration will go there later.

### Math

**Point-to-box Chebyshev distance** (`src/logic/shared.rs`, `Bound::dist_to`)

$$d_x = \max(l_x - x,\; 0,\; x - u_x), \qquad d_y = \max(l_y - y,\; 0,\; y - u_y), \qquad d = \max(d_x, d_y)$$

- $(l_x, l_y)$, $(u_x, u_y)$: the box's lower and upper corners (`lx`, `ly`, `ux`, `uy`)
- $d_x$, $d_y$: how far $(x, y)$ is outside the box on each axis, 0 when inside on that axis
- $d$: tiles from $(x, y)$ to the nearest tile of the box, with diagonal steps counting 1, as in `dist`

`assign_mines` uses it to pick the chunk closest to a group's centroid. Measuring to the nearest edge rather than the centre means a long chunk that passes right by the workers counts as close.

**Box growth** (`Bound::add`)

$$l_x \leftarrow \min(l_x, x), \quad u_x \leftarrow \max(u_x, x), \quad l_y \leftarrow \min(l_y, y), \quad u_y \leftarrow \max(u_y, y)$$

Each edge moves only as far as needed to cover the new deposit, so the box stays the smallest one around the chunk.

**Chunks as connected components** (`group_resources`)

$$i \sim j \iff \text{dist}(i, j) \le 1$$

A chunk is a set of deposits all linked by chains of $\sim$ steps (a connected component of the "touching" graph). The flood fill checks every pair once a deposit is pulled in, $O(n^2)$ for $n$ deposits, which is fine for a few dozen.

### Tests

- `src/logic/shared.rs`: touching deposits join (diagonals included); input order doesn't matter; depleted deposits are skipped and don't bridge; boxes are tight around an L shape; `dist_to` is 0 inside and measured from the nearest edge.
- `src/bots/flaud.rs`: a group claims the nearest chunk; groups never share one; a group with no chunk left has no claim; a claim is kept while any of it has ore; a dry chunk is swapped for the next; a chunk split by a dry deposit stays with its owner; Gather ignores a closer deposit outside the claim.
- Still failing, not from this change: `army_holds_when_calm` (army bias 0.3, see the 2026-10-22 22:49:41 entry).

## 2026-10-22 23:01:49 MDT — FlaudBot splash logo

### Docs

- `README.md`: added `src/resources/FlaudLogo.png` as a centred splash logo above the title, 600px wide. It uses an HTML `<p align="center">`, since Markdown image syntax can't centre on GitHub.

## 2026-10-22 22:50:19 MDT — CLAUDE.md: I write the code

### Changed

- `CLAUDE.md`: new section 0. Claude doesn't edit source code unless explicitly asked to implement or write it. "How can I…" or "fix this" means a walkthrough. Claude may still read, search, run `cargo build`/`cargo test` and update `changelog.md` and `CLAUDE.md`. Permission to write code covers only the task it was given for.
- `CLAUDE.md`: walkthroughs now describe each step (where, what, why, pitfalls) and stop so I can write it. Snippets appear only where an idea needs one. Claude reviews each finished step, runs the build and tests, gives hints that get more specific when I'm stuck, and describes tests instead of writing them.
- `CLAUDE.md`: changelog entries cover code changes whoever wrote them. For my own work, the entry is written once it builds and describes the code as I actually wrote it.

## 2026-10-22 22:49:41 MDT — Flaud waits for a building every 8 units

### Added

- `src/bots/flaud.rs`: `UNITS_PER_BUILDING` (8). After ordering that many non-worker units, Flaud stops training them until a new building completes. Ore saved during the pause raises the wealth sense, which pushes the worker brain toward Build.
- `src/bots/flaud.rs`: `units_since_build` and `built` fields, and `note_new_buildings`, called at the top of `decide`. It resets the counter whenever a completed building id appears that hasn't been seen before. Tracking ids rather than a count means a building lost and another finished on the same tick still resets.
- `src/bots/flaud.rs`: `spawn_units` keeps a running `allowed` total, like its ore total, so one tick with several Barracks can't go past the limit.

### Changed

- `src/bots/flaud.rs`: `decide` adds the non-worker `Produce` orders that survive the `take(room)` cut to `units_since_build`. Orders dropped for lack of queue room aren't counted.
- `src/bots/flaud.rs`: Workers are exempt. They neither count toward the limit nor pause, so losing every worker mid-pause can't leave nobody to build the building that ends it.

### Removed

- `src/bots/flaud.rs`: `build_flag` and `BUILDING_UNIT_RATIO`. Mutating `self.build_flag` inside the `decide` iterator chain didn't compile (E0500: the `filter_map` closure already borrowed `self`). It also never held units back, only changed their urgency. And it relied on an exact `!= 1.0` float comparison that only works for ratios that are powers of two.

### Math

**Training allowance** (`src/bots/flaud.rs`, `spawn_units` and `decide`)

$$a_t = \max(0,\; N - c_t), \qquad c_{t+1} = \begin{cases} 0 & \text{a new building completed at tick } t \\ c_t + k_t & \text{otherwise} \end{cases}$$

- $N$: `UNITS_PER_BUILDING`, 8
- $c_t$: non-worker units ordered since the last new building (`units_since_build`)
- $a_t$: non-worker orders still allowed this tick (`allowed`, where `saturating_sub` computes $\max(0, \cdot)$ without underflow)
- $k_t \le a_t$: non-worker `Produce` orders actually sent after the queue-room cut

Once $c_t$ reaches $N$, $a_t = 0$, so only workers train until a building finishes and $c$ resets to 0.

### Tests

- `src/bots/flaud.rs`: training stops after `UNITS_PER_BUILDING` ticks and resumes when a new Barracks completes. An unfinished building doesn't end the pause. One tick with two Barracks stops at the limit. Workers keep training and aren't counted. Orders dropped for lack of room aren't counted.
- Known failure, not from this change: `army_holds_when_calm` fails because of the uncommitted army bias change `[0.0, 0.0]` → `[0.3, 0.3]`. Bias alone levels off at $V^* = b/(1-\lambda) = 0.3/0.2 = 1.5 \ge \theta$, so a calm army fires about every 5 ticks.

## 2026-10-22 22:30:58 MDT — CLAUDE.md with guided learning mode

### Added

- `CLAUDE.md`: project guide for Claude Code. It sets up guided learning mode: implementations are explained, built in small steps and walked through; code reviews go one file or hunk at a time with issues tagged by severity; every change gets a `changelog.md` entry.
- `CLAUDE.md`: equations must be written as GitHub math, with each symbol defined, a plain-words explanation and the file location. Entries that change math or tuning get a `### Math` section.
- `CLAUDE.md`: reference list of the current fly-brain equations (LIF update and reset, sensory gain encoding, motor input current, winner selection with lateral inhibition).

## 2026-10-22 11:49:39 MDT — `--name` uses the `.env` key, or registers and saves one

### Changed

- `src/main.rs`: the API key is now found in this order: `--key` on the command line, then the `.env` entry named after `--name` (or an env var of that name, for Docker's `--env-file`), then `NETRTS_API_KEY`, then registering `--name`. Previously `NETRTS_API_KEY` came first, so a global key overrode `--name`. Clap's `value_source` tells a typed `--key` apart from one read from `NETRTS_API_KEY`.
- `src/main.rs`: added `bot_key(env_path, name)`. It reads the `.env` file directly (`dotenvy::from_path_iter`) instead of searching every process env var, so a bot named like a system var (`Path`, `USERNAME`) no longer picks up its value. Blank values count as missing.
- `src/main.rs`: `register(name, server, env_path)` now saves the new key to `.env` with `save_key`, which appends `name=key` to the `.env` that `dotenvy::dotenv()` found (or `./.env` if none), starting a new line if the file doesn't end with one. The key is still printed once either way (inside Docker the saved `.env` is lost with the container), and if the write fails it also prints the `.env` line to add. The key is saved under `--name`, the name it's looked up by.
- `src/main.rs`: the `NAME_TAKEN` error now says the name has no key in `.env`, and suggests adding `name=<key>` there, passing `--key`, or choosing another `--name`.

### Docs

- `README.md`: updated the key lookup order and noted that a newly registered key is saved to `.env`.

## 2026-10-22 01:25:03 MDT — Step 10: Flaud trains units

### Added

- `src/bots/flaud.rs`: `spawn_units` trains units each tick, and `next_unit` picks what each building trains. Each unit's `produced_at` in the rules decides which building trains it, so nothing is hard-coded.
  - CommandCenter: Workers, up to `TARGET_WORKERS` (12). Units already queued count toward the target.
  - Barracks: Scouts first, up to `TARGET_SCOUTS` (2). Then Soldiers and Archers, whichever there are fewer of (ties go to Soldier), with no limit.
  - Buildings that are unfinished or already have `QUEUE_LIMIT` (2) units queued are skipped.
  - Once there are `BARRACKS_AT_WORKERS` (6) workers and no Barracks, workers stop being trained so ore can build up for the first Barracks, as `TestBot` does.
  - Orders are paid from one running ore total, so one tick never orders more than it can afford. Units missing from the rules are never trained.
- `src/bots/flaud.rs`: `Flaud` keeps the `RulesDto`, for unit costs and where each unit is trained.

### Changed

- `src/bots/flaud.rs`: `decide` adds training orders to the group commands with `SPAWN_URGENCY` (the same as Build), then sorts and cuts everything to the queue's free room. Training is added after the group commands and the sort is stable, so a Build that tick gets the ore first, and Retreat and Attack always come before training.

### Tests

- `src/bots/flaud.rs`: the CC trains a worker when it can afford one and nothing without the ore, stops at the worker target counting the queue, and pauses to save for the first Barracks. The Barracks trains scouts first, then a soldier/archer mix. Unfinished or busy buildings train nothing. One tick never orders more than the ore covers. Units missing from the rules are never trained. `decide` sends training orders, and with room for one command, a Retreat beats training.

## 2026-10-22 01:18:36 MDT — Steps 6–9: Flaud plays its own moves

`Flaud::decide` now goes from game state to commands: it groups units, builds heat maps, ticks each group's brain on what it senses, and turns the action into a server command. It still doesn't train units (step 10).

### Step 6: senses (committed in `fb25ac3`)

- `src/logic/shared.rs`: `encode_prey` covers enemy workers and enemy buildings, including enemy buildings remembered from earlier sightings. Remembered buildings that are still visible aren't counted twice. Prey weights are Barracks `0.8`, CommandCenter `0.6`, GuardTower `0.55`, other buildings `0.3`, and workers `1.0`.
- `src/logic/shared.rs`: `sample(map, w, h, x, y)` reads one cell as `f32`, or `0` off the map. `uphill` returns the step toward the hottest neighbour; Flaud no longer uses it since the server does the pathfinding.
- `src/bots/flaud.rs`: `encode_wealth` is resources divided by the cheapest building cost (free buildings are skipped), capped between 0 and 1. `centroid` is the average position of a group's living units, or `None` if all of them are dead. `decide` builds the prey map on a third scoped thread.

### Step 7: groups

- `src/bots/flaud.rs`: `update_groups` runs at the start of every tick. `prune` removes dead unit ids and drops empty groups. New units then `join` a group of their role with fewer than `GROUP_SIZE` units, or start a new group with that role's brain: Workers go to `workers`, Scouts to `regiments`, and Soldiers and Archers to `armies`. Surviving groups keep their brain and its voltages.

### Step 8: actions to commands

- `src/logic/shared.rs`: added `pub type Loc`, made `R` public, and added `hottest_near(map, w, h, at, radius)`, which returns the hottest warm cell near `at` (pass `w.max(h)` to search the whole map).
- `src/logic/shared.rs`: moved `find_site` (now `find_site(terrain, s, cx, cy)`) and `enemy_base` here from `TestBot`, which now calls the shared versions. TestBot's behaviour is unchanged.
- `src/bots/flaud.rs`: `Flaud` keeps the `MapDto`. Added `Role`, `my_cc`, `next_building` (a Barracks first, then GuardTowers), and `command`, which turns an action into a destination; the server does the pathfinding:
  - Hold: no command.
  - Attack: attack-move to the hottest threat within `R`, or where the group stands.
  - Retreat: move to the CC. Without a CC, move directly away from the nearby threat, kept on the map.
  - Gather: the nearest deposit with ore left.
  - Explore: the hottest prey on the whole map, or the enemy base. Armies attack-move and scouts move.
  - Build: one worker builds `next_building`, but only if that building is affordable and there is a CC and a site.
- `src/bots/flaud.rs`: removed the local `type Loc` and the unfinished `step` and `STEP`.

### Step 9: `decide`

- `src/bots/flaud.rs`: `decide` packs the heat maps into a `HeatMaps` and works out `wealth` once per tick. It then runs in two passes. The first is mutable: it ticks every group's brain with `senses` at the group's `centroid` and records each decision. The second is read-only: it turns those decisions into commands with `command`. Two passes are needed because `command` borrows `self` while the groups are borrowed mutably in the first pass.
- `src/bots/flaud.rs`: decisions are sorted by `urgency` (Retreat, Attack, Build, Gather, Explore, Hold), and commands are cut to the queue's free room (`queue_capacity - queued_commands`). When the queue is full, Gather and Explore are dropped before Retreat or Attack.
- `src/bots/flaud.rs`: Gather only orders workers that aren't already `Gathering` or `Returning`, so a worker carrying ore home isn't sent back to the deposit. If every worker is busy, nothing is sent.
- `src/bots/flaud.rs`: added `senses` (the maps read at a location, in `THREAT, SUGAR, PREY, WEALTH` order) and `Flaud::groups(role)`.
- `README.md`: added a "How Flaud works" section covering the heat maps, senses, role brains, groups, actions to commands, and status.

### Tests

- `src/logic/shared.rs`: `encode_prey` (enemy workers only, the building order, own buildings ignored, remembered buildings, no double counting); `sample`; `uphill`; `hottest_near` (local vs whole map, cold or out of range, clipped at edges); `find_site` (3–6 tiles out, avoids walls, ore and buildings, `None` when boxed in); `enemy_base` falls back to the map centre.
- `src/bots/flaud.rs`: `encode_wealth`; `centroid`; `update_groups` (grouped by role, the `GROUP_SIZE` cap, enemies ignored, no duplicates across ticks, dead units removed, empty groups dropped, brains kept, the right brain per role); every arm of `command`; `senses` slot order; `urgency` order.
- `src/bots/flaud.rs`: whole-tick `decide` tests. Idle workers near ore are sent to gather, busy workers are left alone, the army attacks an adjacent enemy, calm scouts explore, and commands never exceed the queue's free room. `stub_sends_no_commands` is now `no_units_sends_no_commands`.

## 2026-10-21 22:45:52 MDT — Step 5: role brains with four senses

### Added

- `src/bots/flaud.rs`: role brains `worker_brain()`, `army_brain()` and `scout_brain()`, with `DECAY = 0.8` and `THRESHOLD = 1.0`. Weight rows are `[threat, sugar, prey, wealth]`. Any action with a threat weight of -1.0 or stronger can't fire while a threat is in view.
  - Worker: Retreat once threat stays above about 0.11, Gather drawn to sugar (its bias keeps workers gathering even far from deposits), and Build when rich and safe.
  - Army: Attack on the first vision spike, and Explore toward prey (enemy workers and buildings), but never while fighting.
  - Scout: Retreat, more fearful than a worker, and Explore when calm (from its bias) and sooner toward prey, but never into danger.
- `src/logic/connectome.rs`: `MotorAction::Build`.
- `src/logic/sensory.rs`: `SENSES = 4`, the index names `THREAT`, `SUGAR`, `PREY` and `WEALTH`, a gain per sense in `GAINS`, and `SensoryLayer::new(decay, threshold)`.

### Changed

- `src/logic/sensory.rs`: `SensoryLayer` has one `neurons` list, one neuron per sense, in place of `vision_neurons` and `olfactory_neurons`. `encode_stimuli` takes `[f32; SENSES]` and returns `[bool; SENSES]`.
- `src/logic/connectome.rs`: `weights` is `Vec<[f32; SENSES]>`, and `tick(senses: [f32; SENSES])` takes all four senses as one array.
- `src/logic/shared.rs`: `encode_threat` ignores enemy workers, because they are prey and would cancel out the pull toward them, and adds enemy GuardTowers. Other buildings and your own towers are not threats.
- `src/bots/flaud.rs`, `src/logic/connectome.rs`, `src/logic/sensory.rs`: formatted with rustfmt.

### Tests

- `src/bots/flaud.rs`: role behaviour tests that start a new brain and hold the same senses for 12 ticks.
  - Worker: gathers when calm and sooner near sugar, flees danger without gathering in it, ignores a faint threat, builds when rich and calm, and never builds when poor or threatened.
  - Army: holds when calm, attacks threat, hunts undefended prey, and doesn't wander off in the middle of a fight.
  - Scout: explores when calm and sooner toward prey, and flees danger without exploring into it.
- `src/logic/shared.rs`: tests that `encode_threat` ignores enemy workers, counts every enemy combat unit type, and includes only enemy GuardTowers.
- `src/logic/connectome.rs`: updated for 4 senses, and added `tick_reads_each_sense_from_its_own_column`.

## 2026-10-21 21:28:50 MDT — Brain per group and variable motor neurons

### Added

- `src/bots/flaud.rs`: `BrainGroup` pairs a group's unit `ids` with its own `FlyBrainConnectome`. `regiments`, `workers` and `armies` are now `Vec<BrainGroup>`, so each group keeps its neuron voltages between ticks and a group can't end up with another group's brain when one is removed.
- `src/logic/connectome.rs`: `FlyBrainConnectome` has new `bias` and `actions` fields, one per motor neuron. `bias` is a constant current added every tick, so a neuron can fire with no input, for example a scout exploring when nothing is around.

### Changed

- `src/logic/connectome.rs`: `new(weights, decay, threshold, bias, actions)` now also takes `bias` and `actions`. It panics if their lengths don't match `weights`.
- `src/logic/connectome.rs`: `tick(threat, sugar)` works with any number of motor neurons. Each neuron's input is its bias plus its weight row times the vision and smell spikes, with `weights` read as `[motor][sensor]`. Every motor neuron is updated each tick. If exactly one fires, `tick` returns its action; if none or several fire, it returns `HoldPosition`. The parameters were renamed from `enemy_dist`/`mineral_dist`, since they are heat values now.
- `src/logic/connectome.rs`: `MotorAction` derives `Clone`, `Copy`, `Debug` and `PartialEq`.
- `src/bots/flaud.rs`: formatted with `cargo fmt`.

### Removed

- `src/logic/connectome.rs`: the old 2-neuron `tick`, which had Retreat and Attack hard-coded and read `weights` as `[sensor][motor]`.

### Tests

- `src/logic/connectome.rs`: updated the `brain()` helper and `new_keeps_weights_in_order` for the new `new`, and added a `wired()` helper. New tests check that a `bias` or `actions` list of the wrong length panics, and that `tick` holds when nothing fires, fires on bias alone, returns the only neuron that fired, holds on a tie, reads weights as `[motor][sensor]`, lets a negative weight cancel the bias, and updates every motor neuron even after one fires.

## 2026-10-21 20:19:01 MDT — Heat map inputs and connectome constructor

### Changed

- `src/logic/sensory.rs`: `encode_stimuli(threat, sugar)` now takes heat map values from 0 to 1 instead of distances. It scales them by `VISION_GAIN` and `SMELL_GAIN` (both `5.0`) to get the input currents. The `MAX_DIST` conversion is gone.
- `src/logic/connectome.rs`: `weights` is now `Vec<[f32; 2]>`, one row per motor neuron. `tick` still reads it as `[sensor][motor]`; that is fixed in step 4.
- `src/logic/connectome.rs`: added `MotorAction::Gather` and `MotorAction::Explore`.
- `src/bots/flaud.rs`: unit groups are now `Vec<Vec<i32>>` instead of `[i32; 5]`, so a group can shrink when units die.

### Added

- `src/logic/connectome.rs`: `FlyBrainConnectome::new(weights, decay, threshold)` builds one vision neuron, one smell neuron, and one motor neuron per row of `weights`. All neurons start at rest.
- `src/logic/neurons.rs`: `LifNeuron` now derives `Clone`, so `vec![neuron; n]` creates `n` independent neurons.

### Fixed

- `src/logic/sensory.rs`: vision input was sent to `olfactory_neurons[0]`, so vision and smell were adding up in one neuron. It now uses `vision_neurons[0]`.

### Tests

- `src/logic/connectome.rs`: replaced the placeholder `ConnectomeTesting::setup` test with `connectome_tests`. The new tests check that `new` creates one motor neuron per weight row, one vision and one smell neuron, keeps the weights in order, and starts every neuron at rest with the given `decay` and `threshold`. They also check that motor neurons and separate brains don't share voltage, that weak input builds up over ticks until a neuron fires, and that voltage leaks by `decay` when there is no input.

## 2026-10-21 18:01:38 MDT — Build Flaud's heat maps concurrently

### Changed

- `src/bots/flaud.rs`: `Flaud::decide` builds the threat and sugar maps at the same time, running `encode_threat` and `encode_sugar` on two `std::thread::scope` threads. Both threads borrow the game state, so it isn't cloned, and both finish before `decide` returns. The results are in `_threat` and `_sugar` until they're used.
- `encode_threat` and `encode_sugar` remain plain sync functions, and the `Bot` trait is unchanged. Running them through `tokio::spawn` would have needed owned `'static` state, an async `decide`, and a tokio runtime in the tests, and these CPU-bound loops would have tied up tokio's worker threads.

### Fixed

- `src/bots/flaud.rs`: `decide` named its parameter `_s` but used `s`, and passed the encoders' `Vec<f64>` results to `tokio::spawn`, which needs a future. Both compile errors are gone.

## 2026-10-21 17:48:24 MDT — Heat map tests

### Tests

- `src/logic/shared.rs`: tests that `encode_threat` and `encode_sugar` find threats and resources separately. With an enemy and a deposit in different places, the threat map is hottest on the enemy and the sugar map on the deposit, and each map reads 0 at the other's source.
- `src/logic/shared.rs`: `encode_threat` tests. Own units are ignored and a map with no enemies is all zeros. A single enemy peaks at 1.0 on its cell and fades with distance. Enemies far apart both show up at 1.0 with a cold gap between them. Enemies close together are hottest in the cells between them (three cells tie, since each is Chebyshev distance 1 from both).
- `src/logic/shared.rs`: `encode_sugar` tests. A map with no deposits is all zeros even with units on it. Deposits are weighted by `remaining` (100 reads 1.0 and 50 reads 0.5), and empty deposits add nothing.
- `src/logic/shared.rs`: `region_map` tests. Each cell gets the `STIMULI_LOOKUP` value for its Chebyshev distance and cells beyond `R` stay at zero. Values scale with the weight. Sources on every corner of a map smaller than `R` stay in bounds. Overlapping sources add together. The map is laid out row by row (`y * w + x`).
- `src/logic/shared.rs`: `normalize` tests. The highest value becomes 1.0 and the others keep their ratios. An all-zero map stays at zero (no NaN), and an empty map is handled.
- `src/logic/shared.rs`: added test helpers `unit`, `deposit`, `state` (built on `bots::fixtures::state()`), `at`, and `peak`.

## 2026-10-18 23:58:47 MDT — Switchable bot logic

### Added

- `src/bots/mod.rs`: added `BotLogic` (`Test`, `Flaud`, `Rustala`; default `Test`) with `ALL`, `name()`, and `parse()`, which accepts a 1-based menu number or a case-insensitive name.
- `src/ui/console.rs`: new **Change Logic** menu item (option 4). It lists the logics and asks for a number or name; unknown input prints an error and keeps the current logic. The menu header shows the current choice as `Logic: <name>`.
- `src/ui/console.rs`: added `MenuContext::play_selected()`, which runs `play_match` with the selected logic. Create Game and Join Game now use it instead of always playing `TestBot`.
- `src/bots/flaud.rs`, `src/bots/rustala.rs`: `Flaud` and `RustalaDaora` now implement `Bot` as placeholders that send no commands (marked `TODO`), so they can be selected before their strategies are written.

### Tests

- `src/bots/mod.rs`: `BotLogic` tests for the default, `ALL` listing every variant once, names, parsing by number and by name in any case, and rejecting out-of-range or unknown input.
- `src/bots/mod.rs`: added a test-only `fixtures` module (`map()`, `rules()`, `state()`) for building bots without a server.
- `src/bots/flaud.rs`, `src/bots/rustala.rs`: tests that the placeholder bots send no commands.
- `src/ui/console.rs`: tests that the menu starts on the default logic, the cursor can reach Change Logic, and the cursor stays within the menu at both ends.

## 2026-10-18 16:09:29 MDT — Menu header labels and README

### Fixed

- `src/ui/console.rs`: the menu header printed its values in the wrong order ("BotName" showed the API key, "ServerAddr" the bot name, "ApiKey" the server). Each label now shows its own value.

### Changed

- `src/ui/console.rs`: added `MenuContext::mask_key`; the menu shows the API key as `********` plus its last 4 characters instead of the full key.
- `README.md`: documented the key lookup order and registration, the menu options, the match flags, the `Bot` trait, and `TestBot`. Fixed the help command (`cargo run -- --help`, `NetRTS-Flaud-.exe`) and typos.

## 2026-10-18 16:07:59 MDT — Step 7: cost lookup cleanups

### Changed

- `src/logic/shared.rs`: `Bot::building_cost` now takes `&RulesDto` instead of `RulesDto`, so calling it no longer uses up the rules.
- `src/bots/test_bot.rs`: replaced the `cost` and `building_cost` maps with a stored `rules: RulesDto`, and added `unit_price` / `building_price`, which use the existing `RulesDto::unit()` / `RulesDto::building()` lookups.

### Fixed

- A unit or building type missing from the rules no longer panics on a `HashMap` index; it is treated as unaffordable (`i32::MAX`) and simply never bought.

## 2026-10-18 16:06:26 MDT — Step 6: register a new bot when there is no key

### Changed

- `src/main.rs`: the API key is now found in this order: `--key` / `NETRTS_API_KEY`, then an env var (or `.env` entry) named after `--name`, then registering `--name` with `register_bot`. Previously the app exited if neither of the first two was found.
- `src/main.rs`: added `register(name, server)`. On success it prints the new key (shown once), suggests adding `<name>=<key>` to `.env`, and waits for Enter so the key isn't cleared by the menu screen. On `NAME_TAKEN` it exits with "'name' is already registered - pass its --key, or choose another --name."; other errors exit with the error message.
- `src/main.rs`: with no `--name` and no key, the error now reads "pass --name to register, or --key / NETRTS_API_KEY", as in `example_bot.py`.
- `src/main.rs`: `--name` is now kept as the display name even when `--key` is given (it was previously blank).
- `src/main.rs`: removed the duplicate `bot_keys.get(&bot_name).or_else(|| bot_keys.get(&bot_name))` lookup and the nested `match` on the key.

## 2026-10-18 16:04:39 MDT — Step 5: wire command-line options into the menu

### Changed

- `src/ui/console.rs`: `MenuContext::new(api, args)` now takes the parsed `Args`.
- `src/ui/console.rs`: added `MenuContext::create_request()`, mirroring `get_into_match` in `example_bot.py`:
  - `--create` → open match with `maxPlayers = --players`;
  - otherwise → match vs the `--vs` house bot (default `sitter`);
  - settings come from `--tick-ms`, `--max-ticks` and `--seed`; unset ones are left out so the server uses its defaults.
- `src/ui/console.rs`: Create Game uses `create_request()` and prints either "waiting for N more player(s) to join it" or `vs house bot '<name>'`.
- `src/ui/console.rs`: Join Game shows the `--join` id in the prompt; leaving it blank joins that match.
- `src/main.rs`: passes `args` to `MenuContext::new` (clones `args.name` so `args` isn't partially moved).

### Removed

- The hardcoded Create Game request (house bots `balanced` + `rusher`, seed `1234`, max ticks `2000`). The default opponent is now `sitter` with server-default settings; pass `--vs`, `--seed` and `--max-ticks` to choose.

## 2026-10-18 16:02:57 MDT — Step 4: View Results

### Changed

- `src/ui/console.rs`: added `MenuContext::show_result(match_id)`, which calls `get_result` and prints the tick count, end reason and winner (or "draw"), then one line per player with total score (destruction + economy + survival, as in `example_bot.py`), the score breakdown, and units made / lost / killed.
- `src/ui/console.rs`: View Results (menu option 2) now asks for a match id; leaving it blank uses the last match created or joined. Errors (e.g. match not finished yet) are printed instead of ending the menu.
- `src/ui/console.rs`: `play_match` now prints the result automatically when the match completes.

## 2026-10-18 16:01:37 MDT — Step 3: Create Game plays immediately

### Changed

- `src/ui/console.rs`: Create Game (menu option 0) now goes straight into `play_match::<TestBot>()` after creating the match, matching `example_bot.py`. The creator is seated when the match is created, so no join call is made. If creation fails it returns to the menu.
- `src/ui/console.rs`: Join Game (menu option 1) now asks for a match id, so you can join a match someone else created. On success it stores the id and plays; if the join fails (match not found, full, or already started) it returns to the menu instead of polling anyway.
- `src/ui/console.rs`: the create message now prints just the match id instead of the full summary debug dump.

### Fixed

- Calling Join after Create on a house-bot match failed with "already started", because house-bot matches start as soon as they are created.
- Removed the unused `summary` variable warning in the join handler.

## 2026-10-18 15:59:38 MDT — Step 2: run the bot in the Join Game loop

### Changed

- `src/ui/console.rs`: added `MenuContext::play_match::<B: Bot>()`, which:
  - long-polls `get_state(id, 0)` until the match starts, printing "Waiting for the match to fill up..." on `MATCH_NOT_STARTED`;
  - loads rules and map only after the first state arrives, then creates the bot with `B::new(map, rules)`;
  - each tick calls `bot.decide(&state)`, submits the commands, and prints any rejected ones with their error message;
  - prints ore and unit count every 100 ticks, and `CommandFailed` / `BuildingCompleted` / `PlayerEliminated` events;
  - long-polls the next tick with `sinceTick` so only new events come back.
- `src/ui/console.rs`: Join Game (menu option 1) now calls `play_match::<TestBot>()` instead of only waiting for ticks.

## 2026-10-18 15:58:03 MDT — Step 1: pass state to `Bot::decide`

### Changed

- `src/logic/shared.rs`: `Bot::decide(&mut self)` is now `Bot::decide(&mut self, state: &GameStateDto)`, so the bot gets each tick's state when it is called.
- `src/logic/shared.rs`: `Bot::new(s, m, r)` is now `Bot::new(m, r)`, since bots no longer keep a copy of the state.
- `src/bots/test_bot.rs`: removed the stored `state` field and `update_state`; `decide`, `find_site` and `enemy_base` take the state as a parameter.

## 2026-10-18 15:55:02 MDT

### Changed

- `src/bots/test_bot.rs`: rewrote `TestBot` as a Rust port of `example_bot.py` for testing the API end to end.
  - Builds one Barracks once 6+ workers exist; retries after 40 ticks if it never appears. Placement via `find_site` (open tile 3-6 from the Command Center, not on/next to ore or another building).
  - Idle workers gather, spread across visible deposits by distance + current load.
  - Command Center trains workers up to 10 (max 2 queued), pausing while saving for the Barracks.
  - Each completed Barracks trains Soldiers continuously (max 2 queued).
  - Once 6 Soldiers exist, idle Soldiers attack the closest visible enemy, or attack-move to the first living opponent's start position (map center as fallback).
  - Added `TestBot::update_state(state)` to feed each tick's state, since `Bot::decide` takes no state.
  - `enemy_base` now returns a single `PositionDto` instead of a `Vec`.
  - Uses the existing `CommandRequest` helpers (`build`, `gather`, `produce`, `attack`, `attack_move`) and `GameStateDto` filters (`my_units`, `my_buildings`, `enemy_units`, `enemy_buildings`).

### Fixed

- `spend` returned the opposite of affordability (`resources < cost`); it now returns true only when affordable and deducts the cost.
- Barracks retry check compared `barracks_ordered_at > 40` instead of ticks elapsed since the last order.
- Barracks cost was deducted twice when building.
- `barracks_ordered_at` started at `0`, delaying the first Barracks by 40 ticks; it now starts at `-1000`, as in the Python bot.

### Known gaps (not yet changed)

- ~~`Bot::decide` should take `&GameStateDto` instead of reading stored state (`src/logic/shared.rs`).~~ Done in Step 1.
- ~~The Join Game loop in `src/ui/console.rs` never creates a bot, calls `decide`, or submits commands.~~ Done in Step 2.
- ~~Creating a match doesn't seat/play the bot directly; check whether a separate join is needed.~~ Done in Step 3.
- ~~View Results is a placeholder; should call `get_result` and print scores.~~ Done in Step 4.
- ~~CLI flags (`--vs`, `--create`, `--join`, `--players`, `--tick-ms`, `--max-ticks`, `--seed`) aren't wired into the menu; match settings are hardcoded.~~ Done in Step 5.
- ~~`main.rs` exits without a key instead of registering via `register_bot`.~~ Done in Step 6.
- ~~`Bot::building_cost` takes `RulesDto` by value; `&RulesDto` would be easier to reuse.~~ Done in Step 7.
