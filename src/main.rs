pub mod bots;
pub mod logic;
pub mod ui;

use crate::logic::api::Api;
use crate::ui::{console::MenuContext, startup::Args};
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

            bot_keys
                .get(&bot_name)
                .or_else(|| bot_keys.get(&bot_name))
                .cloned()
        }
    };

    match key {
        Some(val) => {
            let api = Api::new(name, &args.server, val);

            let mut menu = MenuContext::new(api);

            match menu.ui_loop().await {
                Ok(_) => {
                    std::process::exit(0);
                }
                Err(code) => {
                    eprint!("Error: {}", code);
                    std::process::exit(1);
                }
            }
        }
        None => {
            eprintln!(
                "Failed to initialize: No explicit key passed, and no matching bot name found in environment."
            );
            std::process::exit(1);
        }
    }
}

