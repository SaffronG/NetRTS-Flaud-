use std::fs::File;
use std::io::{BufReader, Read};

use crate::logic::models::MatchResultDto;

const LOGFILE: &str= "match_history.json";

pub fn store_results(r: &MatchResultDto) -> std::io::Result<()> {
    let mut logfile= File::open( LOGFILE)?;
    let mut contents = String::new();
    logfile.read_to_string(&mut contents)?;

    todo!()
}

pub fn read_results() -> std::io::Result<()> {
    let logfile= File::open( LOGFILE)?;
    let reader = BufReader::new(logfile);
    let matches = serde_json::from_reader(reader)?;
    Ok(matches)
}