use clap::{ArgGroup, Parser};

#[derive(Parser, Debug)]
#[command(version, about = "NetRts reference bot", long_about = None)]
#[command(args_override_self = true)]
#[command(group(ArgGroup::new("mode").args(["vs", "create", "join"])))]
pub struct Args {
    #[arg(long, env = "NETRTS_SERVER", default_value = "http://localhost:5080")]
    pub server: String,

    #[arg(long, help = "bot name to register (needed unless --key is given)")]
    pub name: Option<String>,

    #[arg(
        long,
        help = "existing API key",
        env = "NETRTS_API_KEY",
        hide_env_values = true
    )]
    pub key: Option<String>,

    #[arg(long, help = "house bot(s) to play against, comma separated", default_value = "sitter")]
    pub vs: String,

    #[arg(
        long,
        help = "create a match for others to join",
        default_value_t = false
    )]
    pub create: bool,

    #[arg(long, help = "join an existing match", value_name = "MATCH_ID")]
    pub join: Option<String>,

    #[arg(long, help = "seats when using --create", default_value_t = 2)]
    pub players: i32,

    #[arg(long, help = "tick interval for a match you create")]
    pub tick_ms: Option<i32>,

    #[arg(long, help = "tick limit for a match you create")]
    pub max_ticks: Option<i32>,

    #[arg(
        long,
        help = "map seed for a match you create",
        allow_negative_numbers = true
    )]
    pub seed: Option<i32>,
}

#[cfg(test)]
mod startup_test {
    use super::*;
    use clap::error::ErrorKind;
    use clap::parser::ValueSource;
    use clap::{ArgMatches, Command, CommandFactory};

    fn cmd() -> Command {
        Args::command()
    }

    fn hermetic_cmd() -> Command {
        cmd()
            .mut_arg("server", |a| a.env(None::<&'static str>))
            .mut_arg("key", |a| a.env(None::<&'static str>))
    }

    fn try_matches(args: &[&str]) -> Result<ArgMatches, clap::Error> {
        hermetic_cmd().try_get_matches_from(std::iter::once("bot").chain(args.iter().copied()))
    }

    fn err_kind(args: &[&str]) -> ErrorKind {
        match try_matches(args) {
            Ok(m) => panic!("expected {args:?} to fail, got {m:?}"),
            Err(e) => e.kind(),
        }
    }

    #[test]
    fn definition_passes_clap_debug_assert() {
        cmd().debug_assert();
    }

    #[test]
    fn modes_conflict_in_every_combination() {
        for args in [
            &["--vs", "x", "--create"][..],
            &["--vs", "x", "--join", "abc"][..],
            &["--create", "--join", "abc"][..],
            &["--vs", "x", "--create", "--join", "abc"][..],
        ] {
            assert_eq!(err_kind(args), ErrorKind::ArgumentConflict, "{args:?}");
        }
    }

    #[test]
    #[allow(unused_unsafe)]
    fn env_fallback_then_command_line_override() {
        const VAR: &str = "NETRTS_STARTUP_TEST_ONLY_KEY";
        unsafe { std::env::set_var(VAR, "env-key") };
        let test_cmd = || hermetic_cmd().mut_arg("key", |a| a.env(VAR));

        let from_env = test_cmd().try_get_matches_from(["bot"]).unwrap();
        let from_cli = test_cmd()
            .try_get_matches_from(["bot", "--key", "cli-key"])
            .unwrap();

        unsafe { std::env::remove_var(VAR) };

        assert_eq!(from_env.value_source("key"), Some(ValueSource::EnvVariable));
        assert_eq!(
            from_env.get_one::<String>("key").map(String::as_str),
            Some("env-key")
        );
        assert_eq!(from_cli.value_source("key"), Some(ValueSource::CommandLine));
        assert_eq!(
            from_cli.get_one::<String>("key").map(String::as_str),
            Some("cli-key")
        );
    }
}
