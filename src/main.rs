pub mod bots;

use crate::bots::{api::Api, startup::Args};

#[tokio::main]
async fn main() {
    let args = <Args as clap::Parser>::parse();

    let api = Api::new(args.server, args.key.expect("Key was not provided!"));
}

