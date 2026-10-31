use crate::logic::history::{self, Row, Series};
use crate::logic::learning::W_MAX;
use crossterm::QueueableCommand;
use crossterm::cursor::MoveTo;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::style::{Color, Stylize};
use crossterm::terminal::{self, Clear, ClearType};
use std::io::{self, Write, stdout};
use std::path::Path;

/// Plot rows. Odd, so 0 gets its own row; 21 rows is 0.2 per row over [-2, 2].
const PLOT_H: usize = 21;
/// Width of the y-axis labels plus the axis line
const AXIS_W: usize = 7;
/// A symbol and a colour per line, so overlapping lines can still be told apart
const MARKS: [(char, Color); 10] = [
    ('*', Color::Red),
    ('+', Color::Green),
    ('o', Color::Yellow),
    ('x', Color::Blue),
    ('#', Color::Magenta),
    ('@', Color::Cyan),
    ('%', Color::White),
    ('&', Color::DarkYellow),
    ('=', Color::DarkCyan),
    ('~', Color::DarkMagenta),
];

/// Shows `path` as one graph per role until q, Esc or Enter. Left/right (or
/// a/d) switch role. A missing file shows how to make one.
pub fn show(path: &Path) -> io::Result<()> {
    let rows = match history::load(path) {
        Ok(rows) => rows,
        Err(e) if e.kind() == io::ErrorKind::NotFound => vec![],
        Err(e) => return Err(e),
    };
    let roles = history::roles(&rows);
    let mut role = 0;
    loop {
        draw(&rows, &roles, role, path)?;
        let Event::Key(key) = event::read()? else {
            continue; // a resize: draw again at the new size
        };
        if key.kind == KeyEventKind::Release {
            continue;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Enter => return Ok(()),
            KeyCode::Left | KeyCode::Char('a') | KeyCode::Char('h') if !roles.is_empty() => {
                role = (role + roles.len() - 1) % roles.len();
            }
            KeyCode::Right | KeyCode::Char('d') | KeyCode::Char('l') | KeyCode::Tab if !roles.is_empty() => {
                role = (role + 1) % roles.len();
            }
            _ => {}
        }
    }
}

fn draw(rows: &[Row], roles: &[String], role: usize, path: &Path) -> io::Result<()> {
    let mut out = stdout();
    out.queue(Clear(ClearType::All))?.queue(MoveTo(0, 0))?;
    let Some(name) = roles.get(role) else {
        print!(
            "\r\n  No weight history yet in {}.\r\n  Every match Flaud finishes adds one point per weight.\r\n\r\n  [q] back\r\n",
            path.display()
        );
        return out.flush();
    };

    let series = history::series(rows, name);
    let (term_w, _) = terminal::size().unwrap_or((100, 40));
    let width = (term_w as usize).saturating_sub(AXIS_W + 2).clamp(10, 120);
    let (first, last) = match_range(&series).unwrap_or((0, 0));

    let tabs: Vec<String> = roles
        .iter()
        .enumerate()
        .map(|(i, r)| if i == role { format!("[{r}]").yellow().to_string() } else { format!(" {r} ") })
        .collect();
    print!("\r\n  Weight history  {}\r\n", tabs.join(" "));
    print!(
        "  {} weights, matches {first} to {last}   {}\r\n\r\n",
        series.len(),
        "[<-/->] role  [q] back".dark_grey()
    );

    let grid = plot(&series, width, PLOT_H);
    for (r, cells) in grid.iter().enumerate() {
        let v = row_value(r, PLOT_H);
        // Label every whole number: +2, +1, 0, -1, -2
        let label = if (v - v.round()).abs() < 1e-4 { format!("{v:+5.1}") } else { " ".repeat(5) };
        print!("{label} |");
        for cell in cells {
            match cell {
                Some(i) => {
                    let (c, color) = MARKS[i % MARKS.len()];
                    print!("{}", c.to_string().with(color));
                }
                None if v.abs() < 1e-4 => print!("{}", "-".dark_grey()),
                None => print!(" "),
            }
        }
        print!("\r\n");
    }
    let end = last.to_string();
    print!("{} +{}\r\n", " ".repeat(AXIS_W - 2), "-".repeat(width));
    print!(
        "{}{first:<w$}{end}\r\n\r\n",
        " ".repeat(AXIS_W),
        w = width.saturating_sub(end.len())
    );

    for (i, s) in series.iter().enumerate() {
        let (c, color) = MARKS[i % MARKS.len()];
        print!("  {} {}\r\n", c.to_string().with(color), legend(s));
    }
    out.flush()
}

/// One legend line: the weight's name, where it started and ended, and a
/// warning if it's pinned at 0 or at `W_MAX`.
fn legend(s: &Series) -> String {
    let (Some(&(_, start)), Some(&(_, end))) = (s.points.first(), s.points.last()) else {
        return s.label();
    };
    let flag = if end == 0.0 {
        "  dead (pinned at 0)"
    } else if end.abs() >= W_MAX - 1e-3 {
        "  saturated (at the limit)"
    } else {
        ""
    };
    format!("{:<18} {start:+.3} -> {end:+.3}  ({:+.3}){flag}", s.label(), end - start)
}

/// The first and last match across `series`, or None if there are no points.
fn match_range(series: &[Series]) -> Option<(u32, u32)> {
    let ms = series.iter().flat_map(|s| s.points.iter().map(|p| p.0));
    Some((ms.clone().min()?, ms.max()?))
}

/// The weight at the middle of plot row `r`: `W_MAX` at the top, `-W_MAX` at the bottom.
fn row_value(r: usize, height: usize) -> f32 {
    W_MAX - 2.0 * W_MAX * r as f32 / (height - 1) as f32
}

/// The plot row nearest `w`, clipped to the plot.
fn row_of(w: f32, height: usize) -> usize {
    let r = ((W_MAX - w) / (2.0 * W_MAX) * (height - 1) as f32).round();
    r.clamp(0.0, (height - 1) as f32) as usize
}

/// A `height` x `width` grid holding, in each cell, the index of the series
/// drawn there. Matches map to columns by number, so a gap in the matches shows
/// as a gap on the x axis, and each series is drawn as straight segments
/// between its points. Where lines cross, the later series wins.
fn plot(series: &[Series], width: usize, height: usize) -> Vec<Vec<Option<usize>>> {
    let mut grid = vec![vec![None; width]; height];
    let Some((first, last)) = match_range(series) else {
        return grid;
    };
    let span = (last - first).max(1) as f32;
    let col = |m: u32| (((m - first) as f32 / span) * (width - 1) as f32).round() as usize;

    for (i, s) in series.iter().enumerate() {
        if let [(m, w)] = s.points[..] {
            grid[row_of(w, height)][col(m)] = Some(i);
        }
        for pair in s.points.windows(2) {
            let ((m0, w0), (m1, w1)) = (pair[0], pair[1]);
            let (c0, c1) = (col(m0), col(m1));
            for c in c0..=c1 {
                // Share of the way from the left point to the right one
                let t = if c1 == c0 { 1.0 } else { (c - c0) as f32 / (c1 - c0) as f32 };
                grid[row_of(w0 + t * (w1 - w0), height)][c] = Some(i);
            }
        }
    }
    grid
}

#[cfg(test)]
mod history_screen_tests {
    use super::*;

    fn line(points: &[(u32, f32)]) -> Series {
        Series { motor: 0, action: "Gather".into(), sense: 1, points: points.to_vec() }
    }

    /// The (row, column) of every cell series `i` was drawn in
    fn cells(grid: &[Vec<Option<usize>>], i: usize) -> Vec<(usize, usize)> {
        let mut found = vec![];
        for (r, row) in grid.iter().enumerate() {
            for (c, cell) in row.iter().enumerate() {
                if *cell == Some(i) {
                    found.push((r, c));
                }
            }
        }
        found
    }

    #[test]
    fn rows_run_from_w_max_at_the_top_to_minus_w_max_at_the_bottom() {
        assert_eq!(row_of(W_MAX, PLOT_H), 0);
        assert_eq!(row_of(0.0, PLOT_H), PLOT_H / 2);
        assert_eq!(row_of(-W_MAX, PLOT_H), PLOT_H - 1);
        assert_eq!(row_of(9.0, PLOT_H), 0, "clipped");
        assert_eq!(row_value(PLOT_H / 2, PLOT_H), 0.0);
    }

    #[test]
    fn a_flat_line_fills_its_row_from_first_to_last_match() {
        let grid = plot(&[line(&[(1, 1.0), (5, 1.0)])], 9, PLOT_H);
        let r = row_of(1.0, PLOT_H);
        assert_eq!(cells(&grid, 0), (0..9).map(|c| (r, c)).collect::<Vec<_>>());
    }

    #[test]
    fn a_rising_line_is_interpolated_between_its_points() {
        let grid = plot(&[line(&[(0, -W_MAX), (10, W_MAX)])], 11, PLOT_H);
        let drawn = cells(&grid, 0);
        assert!(drawn.contains(&(PLOT_H - 1, 0)));
        assert_eq!(drawn.iter().find(|(_, c)| *c == 5).map(|p| p.0), Some(PLOT_H / 2), "halfway is 0");
        assert!(drawn.contains(&(0, 10)));
    }

    #[test]
    fn matches_are_spaced_by_number_not_by_count() {
        // Matches 0, 1 and 10: match 1 belongs near the left, not in the middle
        let grid = plot(&[line(&[(0, 0.0), (1, 0.0), (10, 2.0)])], 11, PLOT_H);
        let col_of_top = cells(&grid, 0).iter().find(|(r, _)| *r == 0).map(|p| p.1);
        assert_eq!(col_of_top, Some(10));
        assert_eq!(grid[row_of(0.0, PLOT_H)][1], Some(0), "match 1 is in column 1");
    }

    #[test]
    fn a_single_point_is_still_drawn() {
        let grid = plot(&[line(&[(4, 0.5)])], 10, PLOT_H);
        assert_eq!(cells(&grid, 0), vec![(row_of(0.5, PLOT_H), 0)]);
    }

    #[test]
    fn no_points_draws_nothing() {
        assert!(plot(&[line(&[])], 10, PLOT_H).iter().flatten().all(Option::is_none));
    }

    #[test]
    fn the_legend_flags_dead_and_saturated_weights() {
        assert!(legend(&line(&[(1, 0.4), (2, 0.0)])).contains("dead"));
        assert!(legend(&line(&[(1, 1.5), (2, W_MAX)])).contains("saturated"));
        assert!(!legend(&line(&[(1, 0.4), (2, 0.5)])).contains("  dead"));
    }
}
