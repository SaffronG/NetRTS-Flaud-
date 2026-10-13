use crate::bots::api::Api;
use crate::bots::models::*;
use crossterm::ExecutableCommand;
use crossterm::cursor::{Hide, MoveTo, MoveToColumn, MoveToRow, Show};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::style::Stylize;
use crossterm::terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode};
use std::io::{self, Write, stdout};

const MENU_ITEMS: usize = 4;

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
}

impl MenuContext {
    pub fn new(c: Api) -> Self {
        Self {
            cursor_pos: 0,
            client: c,
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
                let request = CreateMatchRequest {
                    house_bots: Some(vec!["balanced".into(), "rusher".into()]),
                    settings: Some(MatchSettingsDto {
                        seed: Some(1234),
                        max_ticks: Some(2000),
                        ..Default::default()
                    }),
                    ..Default::default()
                };

                Self::line("\r\nCreating game...", false)?;
                match self.client.create_match(&request).await {
                    Ok(summary) => Self::line(&format!("\r\nCreated match: {summary:?}"), false)?,
                    Err(e) => Self::line(&format!("\r\nFailed to create match: {e}"), true)?,
                }
            }
            1 => Self::line("\r\nJoining Game...", false)?,
            2 => Self::line("\r\nFetching Results...", false)?,
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
            _ => (),
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

    fn menu(&self) -> io::Result<()> {
        Self::clear_screen()?;

        print!(
            "\r\n\t[NetRTS Flaud]\r\nBotName: {}\r\nServerAddr: {}\r\nApiKey: {}\r\n\r\n[{}] Create Game\r\n[{}] Join Game\r\n[{}] View Results\r\n[{}] Change Bot\r\n\r\n",
            self.client.api_key.to_string().green(),
            self.client.bot_name.to_string().yellow(),
            self.client.server.to_string().blue(),
            self.is_selected(0).green(),
            self.is_selected(1).green(),
            self.is_selected(2).green(),
            self.is_selected(3).green(),
        );

        stdout().flush()
    }
}