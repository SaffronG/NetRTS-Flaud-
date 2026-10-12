pub mod bots;

use crate::bots::{api::Api, models::*, startup::Args};
use std::collections::HashMap;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let args = <Args as clap::Parser>::parse();

    let bot_keys: HashMap<String, String> = std::env::vars().collect();

    let mut name: String = "".to_string();

    let key = match args.key.clone() {
        Some(key_val) => Some(key_val),
        None => {
            let bot_name = match args.name {
                Some(name) => name,
                None => {
                    eprint!("Not bot name or key provided");
                    std::process::exit(1);
                }
            };

            name = bot_name.clone();

            bot_keys.get(&bot_name)
                .or_else(|| bot_keys.get(&bot_name))
                .cloned()
        }
    };

    match key {
        Some(val) => {
            let api = Api::new(name, &args.server, val);
            let request = CreateMatchRequest {
                house_bots: Some(vec!["balanced".into(), "rusher".into()]),
                settings: Some(MatchSettingsDto {
                    seed: Some(1234),
                    max_ticks: Some(2000),
                    ..Default::default()
                }),
                ..Default::default()
            };
            let _summary = api.create_match(&request).await;
            println!("{}:api__key: {}", &api.bot_name, &api.api_key);
        }
        None => {
            eprintln!("Failed to initialize: No explicit key passed, and no matching bot name found in environment.");
            std::process::exit(1);
        }
    }
}