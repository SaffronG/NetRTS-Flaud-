use crate::logic::sensory::SENSES;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::Path;

/// Where each match's learned weights are appended, next to `brains.json`
pub const HISTORY_FILE: &str = "brain_history.csv";
const HEADER: &str = "match,role,motor,action,sense,weight";
/// Column names for the senses, indexed by `THREAT`, `SUGAR`, `PREY`, `WEALTH`
pub const SENSE_NAMES: [&str; SENSES] = ["threat", "sugar", "prey", "wealth"];

/// One saved weight as of the end of one match: a fixed fact, never rewritten.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// `BrainStore::matches` after the save, so the first match is 1
    pub matches: u32,
    pub role: String,
    pub motor: usize,
    /// The motor's action, so the graph can label it without the brain
    pub action: String,
    /// Index into `SENSE_NAMES`
    pub sense: usize,
    pub weight: f32,
}

impl Row {
    fn to_csv(&self) -> String {
        format!(
            "{},{},{},{},{},{}",
            self.matches, self.role, self.motor, self.action, SENSE_NAMES[self.sense], self.weight
        )
    }

    /// None for a line that isn't a valid row, such as the header.
    fn parse(line: &str) -> Option<Self> {
        let mut f = line.trim().split(',');
        let (matches, role, motor, action) = (f.next()?, f.next()?, f.next()?, f.next()?);
        let sense = f.next()?;
        let row = Self {
            matches: matches.parse().ok()?,
            role: role.to_string(),
            motor: motor.parse().ok()?,
            action: action.to_string(),
            sense: SENSE_NAMES.iter().position(|&s| s == sense)?,
            weight: f.next()?.parse().ok()?,
        };
        f.next().is_none().then_some(row)
    }
}

/// Adds `rows` to the end of `path`, writing the header first if the file is
/// new or empty. Earlier rows are never touched.
pub fn append(path: &Path, rows: &[Row]) -> io::Result<()> {
    let mut file = OpenOptions::new().append(true).create(true).open(path)?;
    let mut text = String::new();
    if file.metadata()?.len() == 0 {
        text.push_str(HEADER);
        text.push('\n');
    }
    for r in rows {
        text.push_str(&r.to_csv());
        text.push('\n');
    }
    file.write_all(text.as_bytes())
}

/// Every valid row in `path`, in file order. Lines that don't parse are skipped.
pub fn load(path: &Path) -> io::Result<Vec<Row>> {
    Ok(std::fs::read_to_string(path)?.lines().filter_map(Row::parse).collect())
}

/// One weight's value over time, for one line on the graph.
#[derive(Debug, PartialEq)]
pub struct Series {
    pub motor: usize,
    pub action: String,
    pub sense: usize,
    /// `(matches, weight)`, oldest first
    pub points: Vec<(u32, f32)>,
}

impl Series {
    pub fn label(&self) -> String {
        format!("{} <- {}", self.action, SENSE_NAMES[self.sense])
    }
}

/// The roles in `rows`, in the order they first appear.
pub fn roles(rows: &[Row]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for r in rows {
        if !seen.contains(&r.role) {
            seen.push(r.role.clone());
        }
    }
    seen
}

/// One series per (motor, sense) of `role`, ordered by motor then sense, each
/// sorted by match. If a match was logged twice, the later row wins.
pub fn series(rows: &[Row], role: &str) -> Vec<Series> {
    let mut out: Vec<Series> = Vec::new();
    for r in rows.iter().filter(|r| r.role == role) {
        let i = match out.iter().position(|s| s.motor == r.motor && s.sense == r.sense) {
            Some(i) => i,
            None => {
                out.push(Series { motor: r.motor, action: r.action.clone(), sense: r.sense, points: vec![] });
                out.len() - 1
            }
        };
        let points = &mut out[i].points;
        match points.iter_mut().find(|(m, _)| *m == r.matches) {
            Some(p) => p.1 = r.weight,
            None => points.push((r.matches, r.weight)),
        }
    }
    for s in &mut out {
        s.points.sort_by_key(|(m, _)| *m);
    }
    out.sort_by_key(|s| (s.motor, s.sense));
    out
}

#[cfg(test)]
mod history_tests {
    use super::*;

    fn temp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("flaud-{}-{name}.csv", std::process::id()))
    }

    fn row(matches: u32, role: &str, motor: usize, sense: usize, weight: f32) -> Row {
        Row { matches, role: role.into(), motor, action: format!("M{motor}"), sense, weight }
    }

    #[test]
    fn a_row_survives_a_round_trip_through_csv() {
        let r = row(7, "worker", 2, 3, -0.125);
        assert_eq!(Row::parse(&r.to_csv()), Some(r));
    }

    #[test]
    fn bad_lines_and_the_header_are_skipped() {
        assert_eq!(Row::parse(HEADER), None);
        assert_eq!(Row::parse("1,worker,0,Retreat,smell,0.5"), None, "unknown sense");
        assert_eq!(Row::parse("1,worker,0,Retreat,threat"), None, "missing weight");
        assert_eq!(Row::parse("1,worker,0,Retreat,threat,0.5,9"), None, "extra column");
    }

    #[test]
    fn appending_twice_writes_one_header_and_keeps_earlier_rows() {
        let path = temp("append");
        let _ = std::fs::remove_file(&path);
        let first = vec![row(1, "worker", 0, 0, 0.6), row(1, "army", 1, 2, 0.9)];
        let second = vec![row(2, "worker", 0, 0, 0.5)];
        append(&path, &first).unwrap();
        append(&path, &second).unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().filter(|l| *l == HEADER).count(), 1);
        assert_eq!(text.lines().count(), 1 + 3);
        assert_eq!(load(&path).unwrap(), [first, second].concat());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn series_groups_each_weight_and_sorts_it_by_match() {
        let rows = vec![
            row(3, "worker", 1, 0, 0.3),
            row(1, "worker", 1, 0, 0.1),
            row(1, "worker", 0, 0, 0.9),
            row(1, "army", 1, 0, 5.0),
        ];
        let s = series(&rows, "worker");
        assert_eq!(s.len(), 2);
        assert_eq!((s[0].motor, s[0].points.clone()), (0, vec![(1, 0.9)]));
        assert_eq!((s[1].motor, s[1].points.clone()), (1, vec![(1, 0.1), (3, 0.3)]));
    }

    #[test]
    fn a_match_logged_twice_keeps_the_later_value() {
        let rows = vec![row(1, "worker", 0, 0, 0.1), row(1, "worker", 0, 0, 0.2)];
        assert_eq!(series(&rows, "worker")[0].points, vec![(1, 0.2)]);
    }

    #[test]
    fn roles_keep_their_first_seen_order() {
        let rows = vec![row(1, "worker", 0, 0, 0.0), row(1, "army", 0, 0, 0.0), row(2, "worker", 0, 0, 0.0)];
        assert_eq!(roles(&rows), vec!["worker", "army"]);
    }
}
