use crate::bots::BotLogic;
use crate::bots::flaud::Flaud;
use crate::bots::rustala::RustalaDaora;
use crate::bots::test_bot::TestBot;
use crate::logic::api::Api;
use crate::logic::models::*;
use crate::logic::shared::Bot;
use crate::ui::match_screen::MatchScreen;
use crate::ui::startup::Args;
use crossterm::ExecutableCommand;
use crossterm::cursor::{Hide, MoveTo, MoveToColumn, MoveToRow, Show};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::style::Stylize;
use crossterm::terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode};
use std::io::{self, Write, stdout};
use std::time::Instant;

const MENU_ITEMS: usize = 8;
/// Main-menu entry that opens the match settings submenu instead of running an action.
const CONFIGURE_ITEM: usize = 5;
/// Rows in the match settings submenu: mode, house bot, seats, tick ms, max ticks, seed, map width, map height.
const SETTING_ITEMS: usize = 8;

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        stdout().execute(Hide)?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = stdout().execute(Show);
        let _ = disable_raw_mode();
    }
}

pub struct MenuContext {
    cursor_pos: usize,
    client: Api,
    /// Command-line options: match mode (`--vs` / `--create` / `--join`) and settings.
    args: Args,
    /// Bot implementation used for the next match.
    logic: BotLogic,
    /// Settings for matches we create; starts from the command line and is editable in "Configure Match".
    settings: MatchSettingsDto,
}

impl MenuContext {
    pub fn new(c: Api, args: Args) -> Self {
        let settings = MatchSettingsDto {
            tick_interval_ms: args.tick_ms,
            max_ticks: args.max_ticks,
            seed: args.seed,
            ..Default::default()
        };
        Self {
            cursor_pos: 0,
            client: c,
            args,
            logic: BotLogic::default(),
            settings,
        }
    }

    /// The house bots to play against: `vs` split on commas, blanks dropped.
    fn house_bots(&self) -> Vec<String> {
        Self::parse_house_bots(&self.args.vs)
    }

    fn parse_house_bots(input: &str) -> Vec<String> {
        input.split(',').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect()
    }

    /// Builds the create-match body, like `get_into_match` in `example_bot.py`:
    /// an open match with `players` seats in create mode, otherwise a match vs the `vs` house bots.
    fn create_request(&self) -> CreateMatchRequest {
        let settings = self.settings.clone();
        if self.args.create {
            CreateMatchRequest {
                max_players: Some(self.args.players),
                settings: Some(settings),
                ..Default::default()
            }
        } else {
            CreateMatchRequest {
                house_bots: Some(self.house_bots()),
                settings: Some(settings),
                ..Default::default()
            }
        }
    }

    pub async fn ui_loop(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let _guard = TerminalGuard::enter()?;

        self.menu()?;

        loop {
            let Event::Key(key_event) = event::read()? else {
                continue;
            };
            if key_event.kind == KeyEventKind::Release {
                continue;
            }

            match key_event.code {
                KeyCode::Char('q') | KeyCode::Esc => {
                    Self::clear_screen()?;
                    return Ok(());
                }
                KeyCode::Char('k') | KeyCode::Char('w') | KeyCode::Up => self.move_cursor(-1),
                KeyCode::Char('j') | KeyCode::Char('s') | KeyCode::Down => self.move_cursor(1),
                KeyCode::Enter if self.cursor_pos == CONFIGURE_ITEM => self.configure_match()?,
                KeyCode::Enter => {
                    Self::clear_screen()?;
                    self.execute(self.cursor_pos).await?;
                    Self::wait_for_enter()?;
                }
                _ => continue,
            }

            self.menu()?;
        }
    }

    async fn execute(&mut self, pos: usize) -> Result<(), Box<dyn std::error::Error>> {
        match pos {
            0 => {
                let request = self.create_request();

                Self::line("\r\nCreating game...", false)?;
                match self.client.create_match(&request).await {
                    Ok(summary) => {
                        self.client.match_id = summary.match_id.clone();
                        let detail = if self.args.create {
                            format!("waiting for {} more player(s) to join it", self.args.players - 1)
                        } else {
                            format!("vs house bot(s) {}", self.house_bots().join(", "))
                        };
                        Self::line(&format!("\r\nCreated match {} - {detail}", summary.match_id), false)?
                    },
                    Err(e) => {
                        Self::line(&format!("\r\nFailed to create match: {e}"), true)?;
                        return Ok(());
                    }
                }

                // The creator already holds a seat, so play straight away (no join).
                self.play_selected().await?;
            }
            1 => {
                let label = match &self.args.join {
                    Some(id) => format!("Enter match name or id to join (blank for {id}): "),
                    None => "Enter match name or id to join: ".to_string(),
                };
                let Some(input) = Self::prompt(&label)? else {
                    Self::line("Cancelled.", true)?;
                    return Ok(());
                };
                let input = match (input.is_empty(), &self.args.join) {
                    (true, Some(id)) => id.clone(),
                    _ => input,
                };
                if input.is_empty() {
                    Self::line("No match name or id given.", true)?;
                    return Ok(());
                }

                // A name is looked up in the server's match list; anything unknown is tried as an id.
                let match_id = match self.client.find_match_by_name(&input).await {
                    Ok(Some(id)) => {
                        Self::line(&format!("Found match '{}' ({})", input, id), false)?;
                        id
                    }
                    Ok(None) => input,
                    Err(err) => {
                        Self::line(&format!("Couldn't list matches ({}), trying '{}' as an id", err, input), true)?;
                        input
                    }
                };

                Self::line("\r\nJoining Game...", false)?;
                match self.client.join_match(&match_id).await {
                    Ok(summary) => {
                        self.client.match_id = summary.match_id.clone();
                        Self::line(&format!("Joined match {}", summary.match_id), false)?
                    }
                    Err(err) => {
                        Self::line(&format!("Failed to join game: {}", err), true)?;
                        return Ok(());
                    }
                }

                self.play_selected().await?;
            },
            2 => {
                let Some(input) = Self::prompt("Enter match id (blank for last match): ")? else {
                    Self::line("Cancelled.", true)?;
                    return Ok(());
                };
                let match_id = if input.is_empty() { self.client.match_id.clone() } else { input };
                if match_id.is_empty() {
                    Self::line("No match id given and no match played yet.", true)?;
                    return Ok(());
                }

                Self::line("\r\nFetching Results...", false)?;
                if let Err(e) = self.show_result(&match_id).await {
                    Self::line(&format!("Failed to fetch results: {e}"), true)?;
                }
            }
            3 => {
                let Some(bot_name) = Self::prompt("Enter desired bot name: ")? else {
                    Self::line("Cancelled.", true)?;
                    return Ok(());
                };

                match std::env::var(&bot_name) {
                    Ok(key) if !bot_name.is_empty() => {
                        self.client.bot_name = bot_name.clone();
                        self.client.api_key = key;
                        Self::line(&format!("Set bot name to: {bot_name}"), false)?;
                    }
                    _ => Self::line(&format!("No API key found in env var {bot_name:?}"), true)?,
                }
            }
            4 => {
                for (i, l) in BotLogic::ALL.iter().enumerate() {
                    Self::line(&format!("\r\n\r\n  {}) {}", i + 1, l.name()), false)?;
                }
                let Some(input) = Self::prompt("Enter logic number or name: ")? else {
                    Self::line("Cancelled.", true)?;
                    return Ok(());
                };
                match BotLogic::parse(&input) {
                    Some(logic) => {
                        self.logic = logic;
                        Self::line(&format!("Set bot logic to: {}", logic.name()), false)?;
                    }
                    None => Self::line(&format!("Unknown logic {input:?}"), true)?,
                }
            }
            _ => (),
        }
        Ok(())
    }

    /// Submenu for editing the settings used by "Create Game"; Enter edits a row, q/Esc goes back.
    fn configure_match(&mut self) -> io::Result<()> {
        let mut cursor = 0;
        let mut status: Option<(String, bool)> = None;

        loop {
            self.settings_menu(cursor, status.take())?;

            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind == KeyEventKind::Release {
                continue;
            }

            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                KeyCode::Char('k') | KeyCode::Char('w') | KeyCode::Up => cursor = cursor.saturating_sub(1),
                KeyCode::Char('j') | KeyCode::Char('s') | KeyCode::Down => {
                    cursor = (cursor + 1).min(SETTING_ITEMS - 1)
                }
                KeyCode::Enter => status = Some(self.edit_setting(cursor)?),
                _ => {}
            }
        }
    }

    /// Edits one settings row, returning a status message and whether it is an error.
    fn edit_setting(&mut self, item: usize) -> io::Result<(String, bool)> {
        // Mode is a toggle, everything else is typed in.
        if item == 0 {
            self.args.create = !self.args.create;
            return Ok((format!("Mode set to {}", self.mode_name()), false));
        }

        let (name, current) = self.settings_rows()[item].clone();
        let label = match item {
            1 => format!("{name}, comma separated (blank to keep {current}): "),
            2 => format!("{name} (blank to keep {current}): "),
            _ => format!("{name} (blank for server default, now {current}): "),
        };
        let Some(input) = Self::prompt(&label)? else {
            return Ok(("Cancelled.".to_string(), true));
        };

        let result = match item {
            1 => {
                let bots = Self::parse_house_bots(&input);
                if !bots.is_empty() {
                    self.args.vs = bots.join(",");
                }
                Ok(())
            }
            2 => Self::parse_setting(&input, 1).map(|v| {
                if let Some(v) = v {
                    self.args.players = v;
                }
            }),
            3 => Self::parse_setting(&input, 1).map(|v| self.settings.tick_interval_ms = v),
            4 => Self::parse_setting(&input, 1).map(|v| self.settings.max_ticks = v),
            5 => Self::parse_setting(&input, i32::MIN).map(|v| self.settings.seed = v),
            6 => Self::parse_setting(&input, 1).map(|v| self.settings.map_width = v),
            7 => Self::parse_setting(&input, 1).map(|v| self.settings.map_height = v),
            _ => Ok(()),
        };

        Ok(match result {
            Ok(()) => (format!("{name} set to {}", self.settings_rows()[item].1), false),
            Err(e) => (format!("{name}: {e}"), true),
        })
    }

    /// Parses an optional whole number: blank means "unset", anything below `min` is rejected.
    fn parse_setting(input: &str, min: i32) -> Result<Option<i32>, String> {
        if input.is_empty() {
            return Ok(None);
        }
        let v: i32 = input.parse().map_err(|_| format!("{input:?} is not a whole number"))?;
        if v < min {
            return Err(format!("must be at least {min}"));
        }
        Ok(Some(v))
    }

    fn mode_name(&self) -> &'static str {
        if self.args.create { "open match" } else { "vs house bot" }
    }

    /// Label and current value of each row in the settings submenu.
    fn settings_rows(&self) -> [(&'static str, String); SETTING_ITEMS] {
        let opt = |v: Option<i32>| v.map_or("server default".to_string(), |v| v.to_string());
        [
            ("Mode", self.mode_name().to_string()),
            ("House bots", self.house_bots().join(", ")),
            ("Player seats", self.args.players.to_string()),
            ("Tick ms", opt(self.settings.tick_interval_ms)),
            ("Max ticks", opt(self.settings.max_ticks)),
            ("Seed", opt(self.settings.seed)),
            ("Map width", opt(self.settings.map_width)),
            ("Map height", opt(self.settings.map_height)),
        ]
    }

    fn settings_menu(&self, cursor: usize, status: Option<(String, bool)>) -> io::Result<()> {
        Self::clear_screen()?;

        print!("\r\n\t[Configure Match]\r\n\r\n");
        for (i, (name, value)) in self.settings_rows().iter().enumerate() {
            let marker = if i == cursor { "->" } else { "  " };
            // House bot only matters vs a house bot, seats only for an open match.
            let unused = (i == 1 && self.args.create) || (i == 2 && !self.args.create);
            let value = if unused { value.clone().dark_grey() } else { value.clone().cyan() };
            print!("[{}] {:<14}{}\r\n", marker.green(), name, value);
        }
        print!("\r\nEnter: edit/toggle   q/Esc: back\r\n\r\n");
        stdout().flush()?;

        if let Some((msg, err)) = status {
            Self::line(&msg, err)?;
        }
        Ok(())
    }

    /// Plays the current match with whichever logic is selected.
    async fn play_selected(&self) -> Result<(), Box<dyn std::error::Error>> {
        Self::line(&format!("Playing with {}", self.logic.name()), false)?;
        match self.logic {
            BotLogic::Test => self.play_match::<TestBot>().await,
            BotLogic::Flaud => self.play_match::<Flaud>().await,
            BotLogic::Rustala => self.play_match::<RustalaDaora>().await,
        }
    }

    /// Waits for the current match to start, then runs bot `B` every tick until it ends.
    async fn play_match<B: Bot>(&self) -> Result<(), Box<dyn std::error::Error>> {
        let id = &self.client.match_id;

        // waitForTick=0 blocks (up to 30 s per request) until the match starts.
        let mut state = loop {
            match self.client.get_state(id, 0, None).await {
                Err(e) if e.code() == Some("MATCH_NOT_STARTED") => {
                    Self::line("Waiting for the match to fill up...", false)?
                }
                other => break other?,
            }
        };

        // The map only exists once the match has started.
        let rules = self.client.get_rules().await?;
        let map = self.client.get_map(id).await?;

        // From here the stats stay pinned to the top and the log scrolls beneath them
        Self::clear_screen()?;
        let mut screen = MatchScreen::new(self.logic.name()).watching(self.client.watch_url(id).as_str());
        screen.log(
            &format!("Match started: slot {} on a {}x{} map", state.you.slot, map.width, map.height),
            false,
        );
        let mut bot = B::new(map, rules);
        screen.draw(&state, bot.heat_maps())?;

        while state.status != MatchStatus::Completed {
            let started = Instant::now();
            let commands = bot.decide(&state);
            screen.stats.think.record(started.elapsed());

            if !commands.is_empty() {
                let started = Instant::now();
                let reply = self.client.submit_commands(id, &commands).await?;
                screen.stats.submit.record(started.elapsed());

                let rejected: Vec<_> = reply.results.iter().filter(|r| !r.accepted).collect();
                screen.stats.sent(commands.len(), rejected.len());
                for r in rejected {
                    let kind = commands.get(r.index as usize).map(|c| c.kind);
                    let msg = r.error.as_ref().map_or("unknown error", |e| e.message.as_str());
                    screen.log(&format!("tick {}: rejected {kind:?}: {msg}", state.tick), true);
                }
            }

            // Long-poll for the next tick; sinceTick limits events to the ones we haven't seen.
            let tick = state.tick;
            state = self.client.get_state(id, tick + 1, Some(tick)).await?;
            screen.stats.observe_tick(tick, state.tick);
            for e in &state.events {
                if matches!(e.kind.as_str(), "CommandFailed" | "BuildingCompleted" | "PlayerEliminated") {
                    screen.log(&format!("tick {}: {}: {}", e.tick, e.kind, e.message), false);
                }
            }
            let started = Instant::now();
            screen.draw(&state, bot.heat_maps())?;
            screen.stats.draw.record(started.elapsed());
        }

        Self::line("\r\nMatch completed.", false)?;
        self.show_result(id).await
    }

    /// Prints the outcome and per-player scores of a finished match.
    async fn show_result(&self, match_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        let result = self.client.get_result(match_id).await?;
        let outcome = &result.outcome;
        let winner = result
            .players
            .iter()
            .find(|p| p.winner)
            .map_or("draw", |p| p.name.as_str());

        Self::line(
            &format!(
                "\r\n=== Finished after {} ticks ({:?}). Winner: {} ===",
                outcome.ticks, outcome.reason, winner
            ),
            false,
        )?;
        for p in &result.players {
            let sc = &p.score;
            let total = sc.destruction + sc.economy + sc.survival;
            Self::line(
                &format!(
                    "  {:<20} total {:>6}  (destruction {}, economy {}, survival {})  units made {}, lost {}, killed {}",
                    p.name, total, sc.destruction, sc.economy, sc.survival,
                    p.units_produced, p.units_lost, p.units_killed
                ),
                false,
            )?;
        }
        Ok(())
    }

    fn prompt(label: &str) -> io::Result<Option<String>> {
        let mut out = stdout();
        let mut input = String::new();
        out.execute(Show)?;

        let result = loop {
            out.execute(MoveToColumn(0))?;
            out.execute(MoveToRow(2))?;
            out.execute(Clear(ClearType::CurrentLine))?;
            print!("{label}{input}");
            out.flush()?;

            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind == KeyEventKind::Release {
                continue;
            }

            match key.code {
                KeyCode::Enter => break Some(input.trim().to_string()),
                KeyCode::Esc => break None,
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break None,
                KeyCode::Backspace => {
                    input.pop();
                }
                KeyCode::Char(c) => input.push(c),
                _ => {}
            }
        };

        out.execute(Hide)?;
        print!("\r\n");
        out.flush()?;
        Ok(result)
    }

    fn line(text: &str, err: bool) -> io::Result<()> {
        if !err {
            print!("{}\r\n", text);
        }
        else {
            print!("{}\r\n", text.red());
        }
        stdout().flush()
    }

    fn clear_screen() -> io::Result<()> {
        let mut out = stdout();
        out.execute(Clear(ClearType::All))?;
        out.execute(MoveTo(0, 1))?;
        Ok(())
    }

    fn wait_for_enter() -> io::Result<()> {
        Self::line("\r\nPress [Enter] to continue...", false)?;

        loop {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Release && key.code == KeyCode::Enter {
                    return Ok(());
                }
            }
        }
    }

    pub fn move_cursor(&mut self, dir: i16) {
        self.cursor_pos = ((self.cursor_pos as i16) + dir).clamp(0, MENU_ITEMS as i16 - 1) as usize;
    }

    fn is_selected(&self, target: usize) -> &'static str {
        if target == self.cursor_pos { "->" } else { "  " }
    }

    /// Hides all but the last 4 characters of the API key so it isn't readable on screen.
    fn mask_key(key: &str) -> String {
        let chars: Vec<char> = key.chars().collect();
        if chars.len() <= 4 {
            return "*".repeat(chars.len());
        }
        let tail: String = chars[chars.len() - 4..].iter().collect();
        format!("{}{tail}", "*".repeat(8))
    }

    fn menu(&self) -> io::Result<()> {
        Self::clear_screen()?;

        print!(
            "\r\n\t[NetRTS Flaud]\r\nBotName: {}\r\nServerAddr: {}\r\nApiKey: {}\r\nLogic: {}\r\n\r\n[{}] Create Game\r\n[{}] Join Game\r\n[{}] View Results\r\n[{}] Change Bot\r\n[{}] Change Logic\r\n[{}] Configure Match\r\n\r\n",
            self.client.bot_name.to_string().yellow(),
            self.client.server.to_string().blue(),
            Self::mask_key(&self.client.api_key).green(),
            self.logic.name().magenta(),
            self.is_selected(0).green(),
            self.is_selected(1).green(),
            self.is_selected(2).green(),
            self.is_selected(3).green(),
            self.is_selected(4).green(),
            self.is_selected(5).green(),
        );

        stdout().flush()
    }
}
#[cfg(test)]
mod console_tests {
    use super::*;

    fn menu() -> MenuContext {
        let args = <Args as clap::Parser>::parse_from(["bot"]);
        MenuContext::new(Api::new("bot".to_string(), "http://localhost:0", "key"), args)
    }

    #[test]
    fn starts_with_default_logic() {
        assert_eq!(menu().logic, BotLogic::default());
    }

    #[test]
    fn parse_setting_handles_blank_numbers_and_bounds() {
        assert_eq!(MenuContext::parse_setting("", 1), Ok(None));
        assert_eq!(MenuContext::parse_setting("250", 1), Ok(Some(250)));
        assert_eq!(MenuContext::parse_setting("-7", i32::MIN), Ok(Some(-7)));
        assert!(MenuContext::parse_setting("0", 1).is_err());
        assert!(MenuContext::parse_setting("fast", 1).is_err());
    }

    #[test]
    fn create_request_uses_configured_settings() {
        let mut m = menu();
        m.args.create = true;
        m.args.players = 4;
        m.settings.tick_interval_ms = Some(50);
        m.settings.map_width = Some(64);

        let req = m.create_request();
        assert_eq!(req.max_players, Some(4));
        assert!(req.house_bots.is_none());
        let s = req.settings.unwrap();
        assert_eq!(s.tick_interval_ms, Some(50));
        assert_eq!(s.map_width, Some(64));
    }

    #[test]
    fn create_request_sends_every_house_bot() {
        let mut m = menu();
        m.args.vs = " sitter, rusher,,sitter ".to_string();
        let req = m.create_request();
        assert_eq!(req.house_bots, Some(vec!["sitter".into(), "rusher".into(), "sitter".into()]));
        assert!(req.max_players.is_none());
    }

    #[test]
    fn cursor_clamps_at_both_ends() {
        let mut m = menu();
        m.move_cursor(-1);
        assert_eq!(m.cursor_pos, 0);
        for _ in 0..10 {
            m.move_cursor(1);
        }
        assert_eq!(m.cursor_pos, MENU_ITEMS - 1);
    }
}