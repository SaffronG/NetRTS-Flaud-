pub mod bots;

use crate::bots::{api::Api, startup::Args};

#[tokio::main]
async fn main() {
    let args = <Args as clap::Parser>::parse();
    let key = args.key.clone();
    match key {
        Some(val) => {
            let mut api = Api::new(&args.server, val);
            match api.register_bot("Flaud").await {
                Ok(response) => {
                    api.set_token(response.api_key)
                },
                Err(error) => eprint!("Failed to fetch token: {error}"),
            }
        }
        None => {
            return eprintln!("Failed to initialize: No key or name argument provided");
        }
    }
}