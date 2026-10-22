# Changelog

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
