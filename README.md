# NetRTS(Flaud)

A bot for a simplified version of StarCraft (NetRTS) for the SnowCollege coding competition 2026-2027.

There are three bot implementations: one is algorithm-based, the second is driven by the open-source fly brain, and the third is a test bot based on the provided example bot.

- Flaud -- Fly Brain
- RustalaDaora -- Algorithm-Based
- TestBot -- Rust port of `example_bot.py`, used to check the API works end to end

## Running

Pass a bot name with `--name`. The API key is looked up in this order:

1. `--key`, or the `NETRTS_API_KEY` environment variable
2. An environment variable (or `.env` entry) with the same name as the bot, e.g.:

   ``` .env
   Flaud="TOTALLY_REAL_API_KEY"
   ```

3. If neither exists, the bot is registered on the server under that name. The new key is printed once, so add it to `.env` as shown above to reuse it. If the name is already taken, pass its `--key` or choose another `--name`.

``` shell
cargo run -- --name Flaud
```

The server defaults to `http://localhost:5080`; change it with `--server` or `NETRTS_SERVER`.

### Menu

Use the arrow keys (or `w`/`s`, `k`/`j`) and Enter. `q` or Esc quits.

| Option | What it does |
| --- | --- |
| Create Game | Creates a match (see the options below) and plays it straight away, then prints the result. |
| Join Game | Asks for a match id (blank uses `--join`), joins it, plays it, then prints the result. |
| View Results | Asks for a match id (blank uses the last match) and prints the winner and each player's scores. |
| Change Bot | Asks for a bot name and loads its key from the environment variable of that name. |

While a match runs, the bot logs rejected commands, important events (`CommandFailed`, `BuildingCompleted`, `PlayerEliminated`) and its ore and unit count every 100 ticks.

### Match options

| Flag | Default | Effect |
| --- | --- | --- |
| `--vs <bot>` | `sitter` | House bot that Create Game plays against |
| `--create` | off | Create Game opens a match for other players instead |
| `--players <n>` | `2` | Seats in a match opened with `--create` |
| `--join <MATCH_ID>` | none | Default match id for Join Game |
| `--tick-ms <ms>` | server | Tick interval for a match you create |
| `--max-ticks <n>` | server | Tick limit for a match you create |
| `--seed <n>` | server | Map seed for a match you create |

`--vs`, `--create` and `--join` can't be combined. For example, to play `balanced` on a fixed map:

``` shell
cargo run -- --name Flaud --vs balanced --seed 1234 --max-ticks 2000
```

To get all possible arguments and what they do, run:

``` shell
cargo run -- --help
```

(or)

``` shell
NetRTS-Flaud-.exe --help
```

## Writing a bot

Bots implement the `Bot` trait in `src/logic/shared.rs`:

``` rust
fn new(map: MapDto, rules: RulesDto) -> Self;
fn decide(&mut self, state: &GameStateDto) -> Vec<CommandRequest>;
```

`new` is called once the match has started. `decide` is called every tick with the latest state, and the commands it returns are sent to the server. `src/bots/test_bot.rs` is a complete example. Menu matches currently run `TestBot` (see `play_match::<TestBot>()` in `src/ui/console.rs`).

## Changes

See [changelog.md](changelog.md).
