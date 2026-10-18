pub mod bots;
pub mod logic;
pub mod ui;

use crate::logic::api::Api;
use crate::ui::{console::MenuContext, startup::Args};
use std::collections::HashMap;
use std::io;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let args = <Args as clap::Parser>::parse();

    let bot_keys: HashMap<String, String> = std::env::vars().collect();

    let name = args.name.clone().unwrap_or_default();

    // Key lookup order: --key / NETRTS_API_KEY, then an env var named after the bot, then register.
    let key = match args.key.clone() {
        Some(key_val) => key_val,
        None => {
            if name.is_empty() {
                eprintln!("pass --name to register, or --key / NETRTS_API_KEY");
                std::process::exit(1);
            }

            match bot_keys.get(&name) {
                Some(key_val) => key_val.clone(),
                None => register(&name, &args.server).await,
            }
        }
    };

    let api = Api::new(name, &args.server, key);

    let mut menu = MenuContext::new(api, args);

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

/// Registers `name` as a new player and returns its API key, exiting on failure.
async fn register(name: &str, server: &str) -> String {
    // No key yet: the Authorization header is skipped while the key is empty.
    let api = Api::new(name.to_string(), server, "");

    match api.register_bot(name).await {
        Ok(reg) => {
            println!("\nRegistered '{}'. API key (shown once - save it!):\n  {}\n", reg.name, reg.api_key);
            println!("Add it to .env as {}={} to reuse it next time.", reg.name, reg.api_key);
            println!("Press [Enter] to continue...");
            let _ = io::stdin().read_line(&mut String::new());
            reg.api_key
        }
        Err(e) if e.code() == Some("NAME_TAKEN") => {
            eprintln!("'{name}' is already registered - pass its --key, or choose another --name.");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("Failed to register '{name}': {e}");
            std::process::exit(1);
        }
    }
}
