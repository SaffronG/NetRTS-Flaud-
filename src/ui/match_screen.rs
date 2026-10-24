use crate::data::models::*;
use crate::logic::shared::HeatMaps;
use crossterm::QueueableCommand;
use crossterm::cursor::MoveTo;
use crossterm::style::{Color, ResetColor, SetBackgroundColor, SetForegroundColor, Stylize};
use crossterm::terminal::{self, Clear, ClearType};
use std::collections::VecDeque;
use std::io::{self, Write, stdout};
use std::time::{Duration, Instant};

/// Log lines kept for scrollback; only the ones that fit under the header are shown.
const LOG_LIMIT: usize = 500;
/// Narrowest the stats and log may get before the heat map is hidden to make room
const MIN_LEFT: usize = 50;
/// The darkest colour of the cube: nothing on any map
const BLACK: u8 = 16;

/// Last, average and worst of a repeated measurement.
#[derive(Default)]
pub struct Timing {
    last: Duration,
    total: Duration,
    max: Duration,
    n: u32,
}

impl Timing {
    pub fn record(&mut self, d: Duration) {
        self.last = d;
        self.total += d;
        self.max = self.max.max(d);
        self.n += 1;
    }

    pub fn avg(&self) -> Duration {
        if self.n == 0 { Duration::ZERO } else { self.total / self.n }
    }

    fn row(&self, label: &str) -> String {
        format!(
            "{label:<10} last {:>9}  avg {:>9}  max {:>9}",
            ms(self.last),
            ms(self.avg()),
            ms(self.max)
        )
    }
}

fn ms(d: Duration) -> String {
    format!("{:.2} ms", d.as_secs_f64() * 1000.0)
}

/// Performance counters for one match.
pub struct MatchStats {
    /// Time spent in `Bot::decide`
    pub think: Timing,
    /// Round trip of submitting the tick's commands
    pub submit: Timing,
    /// Time spent drawing this screen
    pub draw: Timing,
    ticks_seen: u32,
    /// Ticks the server played while we were busy, so the bot never saw them
    ticks_missed: u32,
    commands_sent: usize,
    commands_rejected: usize,
    started: Instant,
}

impl MatchStats {
    pub fn new() -> Self {
        Self {
            think: Timing::default(),
            submit: Timing::default(),
            draw: Timing::default(),
            ticks_seen: 0,
            ticks_missed: 0,
            commands_sent: 0,
            commands_rejected: 0,
            started: Instant::now(),
        }
    }

    /// Call after each poll with the tick we had and the tick we got.
    pub fn observe_tick(&mut self, prev: i32, next: i32) {
        self.ticks_seen += 1;
        self.ticks_missed += (next - prev - 1).max(0) as u32;
    }

    pub fn sent(&mut self, commands: usize, rejected: usize) {
        self.commands_sent += commands;
        self.commands_rejected += rejected;
    }

    /// The fixed rows at the top of the screen.
    fn header(&self, bot: &str, s: &GameStateDto) -> Vec<String> {
        let secs = self.started.elapsed().as_secs_f64();
        let played = self.ticks_seen + self.ticks_missed;
        let rate = if secs > 0.0 { played as f64 / secs } else { 0.0 };
        let progress = if s.max_ticks > 0 { 100 * s.tick / s.max_ticks } else { 0 };
        let missed_pct = if played > 0 { 100.0 * self.ticks_missed as f64 / played as f64 } else { 0.0 };

        let count = |k: UnitType| s.my_units().filter(|u| u.kind == k).count();
        let army = count(UnitType::Soldier) + count(UnitType::Archer);

        vec![
            format!(
                "{bot} | tick {}/{} ({progress}%) | {rate:.1} ticks/s | {secs:.0}s elapsed",
                s.tick, s.max_ticks
            ),
            format!("{}   budget {} ms/tick", self.think.row("Think"), s.tick_interval_ms),
            self.submit.row("Submit"),
            self.draw.row("Draw"),
            format!(
                "{:<10} seen {}  missed {} ({missed_pct:.1}%)   commands {} sent, {} rejected   queue {}/{}",
                "Ticks",
                self.ticks_seen,
                self.ticks_missed,
                self.commands_sent,
                self.commands_rejected,
                s.you.queued_commands,
                s.you.queue_capacity
            ),
            format!(
                "{:<10} ore {} (+{}/min)   workers {}  army {}  scouts {}   buildings {}",
                "Economy",
                s.you.resources,
                s.you.income_per_minute,
                count(UnitType::Worker),
                army,
                count(UnitType::Scout),
                s.my_buildings().count()
            ),
        ]
    }
}

/// The three heat maps blended into one picture: threat is red, sugar green,
/// prey blue. Each character is a `▀` whose foreground is the upper block and
/// background the lower, so one row of text shows two rows of blocks.
struct HeatFrame {
    cols: usize,
    rows: usize,
    /// Map tiles per block side; above 1 when the map is shrunk to fit
    scale: usize,
    /// `(upper, lower)` colours, row-major, `cols * rows` long
    cells: Vec<(u8, u8)>,
}

/// Fits `m` into `max_cols` x `max_rows` characters, or None if there's no room
/// or the maps don't match their stated size.
fn heat_frame(m: &HeatMaps, max_cols: usize, max_rows: usize) -> Option<HeatFrame> {
    let (w, h) = (m.w.max(0) as usize, m.h.max(0) as usize);
    let sized = [&m.threat, &m.sugar, &m.prey].iter().all(|v| v.len() == w * h);
    if w == 0 || h == 0 || max_cols == 0 || max_rows == 0 || !sized {
        return None;
    }
    let scale = w.div_ceil(max_cols).max(h.div_ceil(2 * max_rows)).max(1);
    let cols = w.div_ceil(scale);
    let rows = h.div_ceil(scale).div_ceil(2);
    let cells = (0..rows)
        .flat_map(|r| (0..cols).map(move |c| (r, c)))
        .map(|(r, c)| (block(m, c, 2 * r, scale), block(m, c, 2 * r + 1, scale)))
        .collect();
    Some(HeatFrame { cols, rows, scale, cells })
}

/// Colour of the `s` x `s` block at block coordinates (`bx`, `by`). Takes the
/// hottest tile of each map in the block, so shrinking never hides a lone enemy.
fn block(m: &HeatMaps, bx: usize, by: usize, s: usize) -> u8 {
    let (w, h) = (m.w as usize, m.h as usize);
    let (x0, y0) = (bx * s, by * s);
    if y0 >= h {
        return BLACK; // the lower half of the last row on an odd-height map
    }
    let mut rgb = [0.0f64; 3];
    for y in y0..(y0 + s).min(h) {
        for x in x0..(x0 + s).min(w) {
            let i = y * w + x;
            rgb[0] = rgb[0].max(m.threat[i]);
            rgb[1] = rgb[1].max(m.sugar[i]);
            rgb[2] = rgb[2].max(m.prey[i]);
        }
    }
    cube(rgb)
}

/// The xterm 256-colour cube entry for red, green and blue from 0 to 1. Six
/// levels per channel keeps the escape codes short and lets slowly changing
/// heat land on the same colour tick after tick, so the cell needn't be redrawn.
fn cube([r, g, b]: [f64; 3]) -> u8 {
    let q = |v: f64| (v.clamp(0.0, 1.0) * 5.0).round() as u8;
    16 + 36 * q(r) + 6 * q(g) + q(b)
}

/// How to read the heat map: a colour swatch and what it means.
fn legend() -> Vec<(Option<u8>, &'static str)> {
    vec![
        (Some(cube([1.0, 0.0, 0.0])), "threat  enemy fighters & towers"),
        (Some(cube([0.0, 1.0, 0.0])), "sugar   ore deposits"),
        (Some(cube([0.0, 0.0, 1.0])), "prey    enemy workers & buildings"),
        (Some(cube([1.0, 1.0, 0.0])), "threat + sugar"),
        (Some(cube([1.0, 0.0, 1.0])), "threat + prey"),
        (Some(cube([0.0, 1.0, 1.0])), "sugar + prey"),
        (None, "brighter = stronger, black = none"),
    ]
}

/// Where everything went last draw; a change means the whole screen is redrawn.
#[derive(Clone, Copy, PartialEq)]
struct Layout {
    term: (u16, u16),
    /// Heat map columns, rows and scale, if shown
    heat: Option<(usize, usize, usize)>,
}

/// The in-match screen: stats pinned to the top left, the event log scrolling
/// below them, and the bot's heat maps on the right.
pub struct MatchScreen {
    pub stats: MatchStats,
    bot: String,
    /// Where the match can be watched in a browser, shown under the stats
    watch: Option<String>,
    log: VecDeque<(String, bool)>,
    layout: Option<Layout>,
    /// Heat map cells as last drawn, to skip the ones that haven't changed
    drawn: Vec<(u8, u8)>,
}

impl MatchScreen {
    pub fn new(bot: &str) -> Self {
        Self {
            stats: MatchStats::new(),
            bot: bot.to_string(),
            watch: None,
            log: VecDeque::new(),
            layout: None,
            drawn: Vec::new(),
        }
    }

    /// Shows `url` as a link to watch the match on the server.
    pub fn watching(mut self, url: &str) -> Self {
        self.watch = Some(url.to_string());
        self
    }

    pub fn log(&mut self, text: &str, err: bool) {
        if self.log.len() == LOG_LIMIT {
            self.log.pop_front();
        }
        self.log.push_back((text.trim().to_string(), err));
    }

    /// Redraws the screen in place. Every row is cut to fit, because a wrapped
    /// row would push the layout down and scroll the header away.
    pub fn draw(&mut self, s: &GameStateDto, heat: Option<&HeatMaps>) -> io::Result<()> {
        let (w, h) = terminal::size().unwrap_or((120, 40));
        let (tw, th) = (w as usize, h as usize);

        // The map gets at most half the width; the rest of the rows go to the title and legend
        let frame = heat
            .and_then(|m| heat_frame(m, (tw / 2).saturating_sub(4), th.saturating_sub(legend().len() + 3)))
            .filter(|f| tw.saturating_sub(f.cols + 4) >= MIN_LEFT);
        // Keep the last column free so no row ever wraps
        let left = match &frame {
            Some(f) => tw - f.cols - 4,
            None => tw.saturating_sub(1),
        };

        let layout = Layout {
            term: (w, h),
            heat: frame.as_ref().map(|f| (f.cols, f.rows, f.scale)),
        };
        let full = self.layout != Some(layout);
        let mut out = stdout();
        if full {
            out.queue(Clear(ClearType::All))?;
        }

        self.draw_left(&mut out, s, left, th, frame.is_some())?;
        if let (Some(f), Some(m)) = (&frame, heat) {
            self.draw_heat(&mut out, f, m, left + 3, tw, full)?;
        }

        self.layout = Some(layout);
        out.flush()
    }

    /// Stats, a rule and the newest log lines, each padded to `width` instead of
    /// cleared to the end of the line, which would wipe the heat map beside them.
    fn draw_left(
        &self,
        out: &mut impl Write,
        s: &GameStateDto,
        width: usize,
        height: usize,
        divider: bool,
    ) -> io::Result<()> {
        let header = self.stats.header(&self.bot, s);
        // Leave the last row empty so printing it never scrolls the terminal
        let room = height.saturating_sub(header.len() + self.watch.iter().len() + 2);
        let shown = self.log.iter().skip(self.log.len().saturating_sub(room));
        let edge = if divider { " │" } else { "" };

        out.queue(MoveTo(0, 0))?;
        for row in &header {
            write!(out, "{}{edge}\r\n", pad(row, width).bold())?;
        }
        if let Some(url) = &self.watch {
            // Only the url is styled, so the underline stops where it does
            let label = pad("Watch", width.min(11));
            let shown = fit(url, width - label.chars().count());
            let rest = pad("", width - label.chars().count() - shown.chars().count());
            write!(out, "{}{}{rest}{edge}\r\n", label.bold(), link(url, &shown).cyan().underlined())?;
        }
        write!(out, "{}{edge}\r\n", "─".repeat(width).dark_grey())?;
        for (text, err) in shown.clone() {
            let text = pad(text, width);
            if *err {
                write!(out, "{}{edge}\r\n", text.red())?;
            } else {
                write!(out, "{text}{edge}\r\n")?;
            }
        }
        for _ in shown.count()..room {
            write!(out, "{}{edge}\r\n", pad("", width))?;
        }
        Ok(())
    }

    /// The map starting at column `x0`, redrawing only cells whose colour changed.
    /// The title and legend never change for a layout, so they're drawn on `full` only.
    fn draw_heat(
        &mut self,
        out: &mut impl Write,
        f: &HeatFrame,
        m: &HeatMaps,
        x0: usize,
        term_w: usize,
        full: bool,
    ) -> io::Result<()> {
        let text_w = term_w.saturating_sub(x0 + 1);
        if full {
            let title = format!(
                "Heat map {}x{}, 1 char = {}x{} tiles",
                m.w,
                m.h,
                f.scale,
                2 * f.scale
            );
            out.queue(MoveTo(x0 as u16, 0))?;
            write!(out, "{}", fit(&title, text_w).bold())?;

            for (i, (swatch, text)) in legend().into_iter().enumerate() {
                out.queue(MoveTo(x0 as u16, (f.rows + 2 + i) as u16))?;
                match swatch {
                    Some(c) => {
                        out.queue(SetForegroundColor(Color::AnsiValue(c)))?;
                        write!(out, "██")?;
                        out.queue(ResetColor)?;
                        write!(out, " {}", fit(text, text_w.saturating_sub(3)))?;
                    }
                    None => write!(out, "{}", fit(text, text_w).dark_grey())?,
                }
            }
        }

        let mut colours: Option<(u8, u8)> = None;
        for r in 0..f.rows {
            // Set when the cursor already sits where the next cell goes
            let mut in_place = false;
            for c in 0..f.cols {
                let i = r * f.cols + c;
                let cell = f.cells[i];
                if !full && self.drawn.get(i) == Some(&cell) {
                    in_place = false;
                    continue;
                }
                if !in_place {
                    out.queue(MoveTo((x0 + c) as u16, (r + 1) as u16))?;
                    in_place = true;
                }
                if colours != Some(cell) {
                    out.queue(SetForegroundColor(Color::AnsiValue(cell.0)))?;
                    out.queue(SetBackgroundColor(Color::AnsiValue(cell.1)))?;
                    colours = Some(cell);
                }
                write!(out, "▀")?;
            }
        }
        out.queue(ResetColor)?;
        self.drawn.clone_from(&f.cells);
        Ok(())
    }
}

/// `text` cut to at most `width` characters.
fn fit(text: &str, width: usize) -> String {
    text.chars().take(width).collect()
}

/// `text` cut or padded with spaces to exactly `width` characters.
fn pad(text: &str, width: usize) -> String {
    format!("{:<width$}", fit(text, width))
}

/// `text` as an OSC 8 hyperlink to `url`, clickable in terminals that support it.
/// The escapes take no columns, so `text` keeps its width.
fn link(url: &str, text: &str) -> String {
    format!("\x1b]8;;{url}\x1b\\{text}\x1b]8;;\x1b\\")
}

#[cfg(test)]
mod match_screen_tests {
    use super::*;

    fn ms_(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    /// `w` x `h` maps, all cold except the listed `(x, y)` cells of each.
    fn heat(w: i32, h: i32, threat: &[(i32, i32)], sugar: &[(i32, i32)], prey: &[(i32, i32)]) -> HeatMaps {
        let warm = |cells: &[(i32, i32)]| {
            let mut m = vec![0.0; (w * h) as usize];
            for &(x, y) in cells {
                m[(y * w + x) as usize] = 1.0;
            }
            m
        };
        HeatMaps {
            threat: warm(threat),
            sugar: warm(sugar),
            prey: warm(prey),
            w,
            h,
        }
    }

    // --- stats ---

    #[test]
    fn timing_tracks_last_avg_and_max() {
        let mut t = Timing::default();
        assert_eq!(t.avg(), Duration::ZERO, "no samples yet");
        for n in [2, 8, 5] {
            t.record(ms_(n));
        }
        assert_eq!(t.last, ms_(5));
        assert_eq!(t.avg(), ms_(5));
        assert_eq!(t.max, ms_(8));
    }

    #[test]
    fn observe_tick_counts_ticks_the_bot_never_saw() {
        let mut s = MatchStats::new();
        s.observe_tick(10, 11);
        assert_eq!((s.ticks_seen, s.ticks_missed), (1, 0));
        s.observe_tick(11, 14);
        assert_eq!((s.ticks_seen, s.ticks_missed), (2, 2), "12 and 13 were skipped");
        s.observe_tick(14, 14);
        assert_eq!(s.ticks_missed, 2, "a repeat is not negative");
    }

    #[test]
    fn header_shows_progress_and_economy() {
        let mut state = crate::bots::fixtures::state();
        state.tick = 50;
        state.max_ticks = 200;
        state.you.resources = 321;
        let rows = MatchStats::new().header("Flaud", &state);
        assert!(rows[0].starts_with("Flaud | tick 50/200 (25%)"), "{}", rows[0]);
        assert!(rows.iter().any(|r| r.contains("ore 321")), "{rows:?}");
        assert!(rows.iter().any(|r| r.starts_with("Draw")), "{rows:?}");
    }

    #[test]
    fn log_keeps_only_the_newest_lines() {
        let mut m = MatchScreen::new("Flaud");
        for i in 0..LOG_LIMIT + 3 {
            m.log(&format!("\r\nline {i}"), false);
        }
        assert_eq!(m.log.len(), LOG_LIMIT);
        assert_eq!(m.log.front().unwrap().0, "line 3", "oldest dropped, padding trimmed");
    }

    #[test]
    fn fit_and_pad_count_characters_not_bytes() {
        assert_eq!(fit("──abc", 3), "──a");
        assert_eq!(fit("ab", 10), "ab");
        assert_eq!(pad("──", 4), "──  ");
        assert_eq!(pad("abcdef", 3), "abc");
    }

    // --- heat map ---

    #[test]
    fn cube_maps_each_map_to_its_own_channel() {
        assert_eq!(cube([0.0, 0.0, 0.0]), BLACK);
        assert_eq!(cube([1.0, 0.0, 0.0]), 196, "threat is red");
        assert_eq!(cube([0.0, 1.0, 0.0]), 46, "sugar is green");
        assert_eq!(cube([0.0, 0.0, 1.0]), 21, "prey is blue");
        assert_eq!(cube([1.0, 1.0, 1.0]), 231);
        assert_eq!(cube([0.04, 0.0, 0.0]), BLACK, "faint heat rounds to nothing");
        assert_eq!(cube([7.0, -1.0, 0.0]), 196, "out of range is clamped");
    }

    #[test]
    fn heat_frame_puts_two_map_rows_in_each_text_row() {
        let m = heat(4, 4, &[(1, 0)], &[(2, 1)], &[(3, 3)]);
        let f = heat_frame(&m, 10, 10).unwrap();
        assert_eq!((f.cols, f.rows, f.scale), (4, 2, 1));
        assert_eq!(f.cells[1], (196, BLACK), "threat in the upper half");
        assert_eq!(f.cells[2], (BLACK, 46), "sugar in the lower half");
        assert_eq!(f.cells[4 + 3], (BLACK, 21), "prey on map row 3");
        assert_eq!(f.cells[0], (BLACK, BLACK));
    }

    #[test]
    fn heat_frame_shrinks_big_maps_without_losing_a_lone_hot_tile() {
        let m = heat(8, 8, &[(7, 7)], &[], &[]);
        let f = heat_frame(&m, 2, 10).unwrap();
        assert_eq!((f.cols, f.rows, f.scale), (2, 1, 4));
        assert_eq!(f.cells[1], (BLACK, 196), "the corner block keeps its hottest tile");
    }

    #[test]
    fn heat_frame_leaves_the_missing_half_row_black() {
        let m = heat(2, 3, &[(0, 2)], &[], &[]);
        let f = heat_frame(&m, 10, 10).unwrap();
        assert_eq!(f.rows, 2);
        assert_eq!(f.cells[2], (196, BLACK), "map row 2 on top, nothing under it");
    }

    #[test]
    fn heat_frame_is_none_without_room_or_with_bad_maps() {
        let m = heat(4, 4, &[], &[], &[]);
        assert!(heat_frame(&m, 0, 10).is_none());
        assert!(heat_frame(&m, 10, 0).is_none());
        let mut short = heat(4, 4, &[], &[], &[]);
        short.prey.pop();
        assert!(heat_frame(&short, 10, 10).is_none());
    }
}
