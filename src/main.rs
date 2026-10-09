pub mod bots;

use crate::bots::startup::Args;

#[tokio::main]
async fn main() {
    let args = <Args as clap::Parser>::parse();
}