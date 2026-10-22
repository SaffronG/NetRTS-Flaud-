pub mod bots;
pub mod logic;
pub mod ui;

use crate::logic::api::Api;
use crate::ui::{console::MenuContext, startup::Args};
use clap::parser::ValueSource;
use clap::{CommandFactory, FromArgMatches};
use std::fs::OpenOptions;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

#[tokio::main]
async fn main() {
    // Remember where .env was found so a newly registered key can be saved back to it.
    let env_path = dotenvy::dotenv().unwrap_or_else(|_| PathBuf::from(".env"));

    let matches = Args::command().get_matches();
    let key_on_cli = matches.value_source("key") == Some(ValueSource::CommandLine);
    let args = Args::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());

    let name = args.name.clone().unwrap_or_default();

    // Key lookup order: --key, then the .env entry named after --name, then
    // NETRTS_API_KEY, then register --name and save its key to .env.
    let key = if key_on_cli {
        args.key.clone().unwrap_or_default()
    } else if !name.is_empty() {
        match bot_key(&env_path, &name).or_else(|| args.key.clone()) {
            Some(key_val) => key_val,
            None => register(&name, &args.server, &env_path).await,
        }
    } else if let Some(key_val) = args.key.clone() {
        key_val
    } else {
        eprintln!("pass --name to register, or --key / NETRTS_API_KEY");
        std::process::exit(1);
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

/// Looks up `name`'s key in the .env file, falling back to an env var of the
/// same name (e.g. from `docker run --env-file .env`). Blank values count as missing.
fn bot_key(env_path: &Path, name: &str) -> Option<String> {
    let from_file = dotenvy::from_path_iter(env_path).ok().and_then(|iter| {
        iter.filter_map(Result::ok)
            .find(|(k, _)| k == name)
            .map(|(_, v)| v)
    });

    from_file
        .or_else(|| std::env::var(name).ok())
        .filter(|v| !v.trim().is_empty())
}

/// Appends `name=key` to the .env file, creating it if needed.
fn save_key(env_path: &Path, name: &str, key: &str) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .read(true)
        .append(true)
        .create(true)
        .open(env_path)?;

    // Start on a fresh line if the file doesn't end with one.
    let mut needs_newline = false;
    if file.seek(SeekFrom::End(0))? > 0 {
        file.seek(SeekFrom::End(-1))?;
        let mut last = [0u8; 1];
        file.read_exact(&mut last)?;
        needs_newline = last[0] != b'\n';
    }

    let prefix = if needs_newline { "\n" } else { "" };
    writeln!(file, "{prefix}{name}={key}")
}

/// Registers `name` as a new player, saves its key to .env and returns it, exiting on failure.
async fn register(name: &str, server: &str, env_path: &Path) -> String {
    // No key yet: the Authorization header is skipped while the key is empty.
    let api = Api::new(name.to_string(), server, "");

    match api.register_bot(name).await {
        Ok(reg) => {
            println!("\nRegistered '{}'. API key (shown once):\n  {}\n", reg.name, reg.api_key);
            // Saved under the name we look up by, in case the server normalises it.
            match save_key(env_path, name, &reg.api_key) {
                Ok(()) => println!("Saved it to {} as {name}=...", env_path.display()),
                Err(e) => {
                    println!("Couldn't write {}: {e}", env_path.display());
                    println!("Add it to .env as {name}={} to reuse it next time.", reg.api_key);
                }
            }
            println!("Press [Enter] to continue...");
            let _ = io::stdin().read_line(&mut String::new());
            reg.api_key
        }
        Err(e) if e.code() == Some("NAME_TAKEN") => {
            eprintln!(
                "'{name}' is already registered but has no key in {} - add {name}=<key> there, pass its --key, or choose another --name.",
                env_path.display()
            );
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("Failed to register '{name}': {e}");
            std::process::exit(1);
        }
    }
}
