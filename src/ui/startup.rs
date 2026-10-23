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
    use clap::{Arg, ArgAction, ArgMatches, Command, CommandFactory, FromArgMatches};
    use std::ffi::OsStr;

    const ALL_IDS: [&str; 10] = [
        "server",
        "name",
        "key",
        "vs",
        "create",
        "join",
        "players",
        "tick_ms",
        "max_ticks",
        "seed",
    ];

    fn cmd() -> Command {
        Args::command()
    }

    fn hermetic_cmd() -> Command {
        cmd()
            .mut_arg("server", |a| a.env(None::<&'static str>))
            .mut_arg("key", |a| a.env(None::<&'static str>))
    }

    fn arg(id: &str) -> Arg {
        cmd()
            .get_arguments()
            .find(|a| a.get_id() == id)
            .cloned()
            .unwrap_or_else(|| panic!("no argument with id {id:?}"))
    }

    fn try_matches(args: &[&str]) -> Result<ArgMatches, clap::Error> {
        hermetic_cmd().try_get_matches_from(std::iter::once("bot").chain(args.iter().copied()))
    }

    fn matches(args: &[&str]) -> ArgMatches {
        try_matches(args).unwrap_or_else(|e| panic!("expected {args:?} to parse, got:\n{e}"))
    }

    fn parse(args: &[&str]) -> Args {
        Args::from_arg_matches(&matches(args)).expect("ArgMatches -> Args")
    }

    fn err_kind(args: &[&str]) -> ErrorKind {
        match try_matches(args) {
            Ok(m) => panic!("expected {args:?} to fail, got {m:?}"),
            Err(e) => e.kind(),
        }
    }

    fn defaults(id: &str) -> Vec<String> {
        arg(id)
            .get_default_values()
            .iter()
            .map(|v| v.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn definition_passes_clap_debug_assert() {
        cmd().debug_assert();
    }

    #[test]
    fn defines_exactly_the_python_arguments() {
        let mut ids: Vec<String> = cmd()
            .get_arguments()
            .map(|a| a.get_id().to_string())
            .filter(|id| id != "help" && id != "version")
            .collect();
        ids.sort();
        let mut expected: Vec<String> = ALL_IDS.iter().map(|s| s.to_string()).collect();
        expected.sort();
        assert_eq!(ids, expected);
    }

    #[test]
    fn long_flags_match_python_spelling() {
        let expected = [
            ("server", "server"),
            ("name", "name"),
            ("key", "key"),
            ("vs", "vs"),
            ("create", "create"),
            ("join", "join"),
            ("players", "players"),
            ("tick_ms", "tick-ms"),
            ("max_ticks", "max-ticks"),
            ("seed", "seed"),
        ];
        for (id, long) in expected {
            assert_eq!(arg(id).get_long(), Some(long), "--flag name for {id}");
        }
    }

    #[test]
    fn no_short_flags_like_python() {
        for id in ALL_IDS {
            assert_eq!(arg(id).get_short(), None, "{id} should have no short flag");
        }
    }

    #[test]
    fn nothing_is_required() {
        for id in ALL_IDS {
            assert!(!arg(id).is_required_set(), "{id} should be optional");
        }
    }

    #[test]
    fn default_values_match_python() {
        assert_eq!(defaults("server"), ["http://localhost:5080"]);
        assert_eq!(defaults("vs"), ["sitter"]);
        assert_eq!(defaults("players"), ["2"]);
        assert_eq!(defaults("create"), ["false"]);
        for id in ["name", "key", "join", "tick_ms", "max_ticks", "seed"] {
            assert!(defaults(id).is_empty(), "{id} should default to None");
        }
    }

    #[test]
    fn env_vars_match_python() {
        assert_eq!(arg("server").get_env(), Some(OsStr::new("NETRTS_SERVER")));
        assert_eq!(arg("key").get_env(), Some(OsStr::new("NETRTS_API_KEY")));
        for id in [
            "name",
            "vs",
            "create",
            "join",
            "players",
            "tick_ms",
            "max_ticks",
            "seed",
        ] {
            assert_eq!(arg(id).get_env(), None, "{id} should not read an env var");
        }
    }

    #[test]
    fn api_key_env_value_is_hidden_from_help() {
        assert!(arg("key").is_hide_env_values_set());
    }

    #[test]
    fn create_is_a_store_true_switch() {
        assert!(matches!(arg("create").get_action(), ArgAction::SetTrue));
    }

    #[test]
    fn value_flags_store_a_single_value() {
        for id in ALL_IDS.iter().filter(|id| **id != "create") {
            assert!(
                matches!(arg(id).get_action(), ArgAction::Set),
                "{id} should be ArgAction::Set"
            );
        }
    }

    #[test]
    fn join_uses_match_id_metavar() {
        let names: Vec<String> = arg("join")
            .get_value_names()
            .expect("join should have a value name")
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(names, ["MATCH_ID"]);
    }

    #[test]
    fn seed_allows_negative_numbers() {
        assert!(arg("seed").is_allow_negative_numbers_set());
    }

    #[test]
    fn help_strings_match_python() {
        let expected = [
            (
                "name",
                "bot name to register (needed unless --key is given)",
            ),
            ("key", "existing API key"),
            ("vs", "house bot(s) to play against, comma separated"),
            ("create", "create a match for others to join"),
            ("join", "join an existing match"),
            ("players", "seats when using --create"),
            ("tick_ms", "tick interval for a match you create"),
            ("max_ticks", "tick limit for a match you create"),
            ("seed", "map seed for a match you create"),
        ];
        for (id, text) in expected {
            let help = arg(id).get_help().map(|h| h.to_string());
            assert_eq!(help.as_deref(), Some(text), "help text for {id}");
        }
    }

    #[test]
    fn about_matches_python_description() {
        let about = cmd().get_about().map(|a| a.to_string()).unwrap_or_default();
        assert!(about.contains("NetRts reference bot"), "about = {about:?}");
    }

    #[test]
    fn mode_group_contains_vs_create_join() {
        let cmd = cmd();
        let group = cmd
            .get_groups()
            .find(|g| g.get_id() == "mode")
            .expect("missing 'mode' group");
        let mut members: Vec<&str> = group.get_args().map(|id| id.as_str()).collect();
        members.sort();
        assert_eq!(members, ["create", "join", "vs"]);
        assert!(!group.is_required_set(), "Python's group is not required");
    }

    #[test]
    fn no_args_uses_defaults_and_marks_them_as_defaults() {
        let m = matches(&[]);
        for id in ["server", "vs", "players", "create"] {
            assert_eq!(m.value_source(id), Some(ValueSource::DefaultValue), "{id}");
        }
        for id in ["name", "key", "join", "tick_ms", "max_ticks", "seed"] {
            assert_eq!(
                m.value_source(id),
                None,
                "{id} should be absent (Python None)"
            );
        }

        let a = Args::from_arg_matches(&m).unwrap();
        assert_eq!(a.server, "http://localhost:5080");
        assert_eq!(a.vs, "sitter");
        assert_eq!(a.players, 2);
        assert!(!a.create);
        assert_eq!(a.key, None);
    }

    #[test]
    fn values_are_stored_with_python_types() {
        let m = matches(&[
            "--server",
            "http://x",
            "--name",
            "Flaud",
            "--key",
            "k",
            "--join",
            "abc",
            "--players",
            "4",
            "--tick-ms",
            "250",
            "--max-ticks",
            "5000",
            "--seed",
            "42",
        ]);
        for id in ["server", "name", "key", "join", "vs"] {
            m.try_get_one::<String>(id)
                .unwrap_or_else(|e| panic!("{id}: {e}"));
        }
        for id in ["players", "tick_ms", "max_ticks", "seed"] {
            m.try_get_one::<i32>(id)
                .unwrap_or_else(|e| panic!("{id}: {e}"));
        }
        m.try_get_one::<bool>("create").unwrap();

        assert_eq!(m.get_one::<i32>("players"), Some(&4));
        assert_eq!(m.get_one::<i32>("tick_ms"), Some(&250));
        assert_eq!(m.get_one::<i32>("max_ticks"), Some(&5000));
        assert_eq!(m.get_one::<i32>("seed"), Some(&42));
    }

    #[test]
    fn command_line_values_are_marked_as_command_line() {
        let m = matches(&["--key", "k", "--vs", "rusher", "--players", "3"]);
        for id in ["key", "vs", "players"] {
            assert_eq!(m.value_source(id), Some(ValueSource::CommandLine), "{id}");
        }
    }

    #[test]
    fn equals_syntax_and_negative_seed() {
        let a = parse(&["--players=3", "--key=abc", "--seed=-7"]);
        assert_eq!(
            (a.players, a.key.as_deref(), a.seed),
            (3, Some("abc"), Some(-7))
        );
        assert_eq!(parse(&["--seed", "-5"]).seed, Some(-5));
    }

    #[test]
    fn repeated_flag_keeps_last_value_like_argparse() {
        let a = parse(&[
            "--players",
            "2",
            "--players",
            "3",
            "--key",
            "a",
            "--key",
            "b",
        ]);
        assert_eq!(a.players, 3);
        assert_eq!(a.key.as_deref(), Some("b"));
    }

    #[test]
    fn each_mode_works_alone() {
        assert_eq!(parse(&["--vs", "rusher"]).vs, "rusher");

        let m = matches(&["--create"]);
        assert_eq!(m.get_flag("create"), true);
        assert_eq!(m.value_source("vs"), Some(ValueSource::DefaultValue));

        let m = matches(&["--join", "abc"]);
        assert_eq!(m.get_one::<String>("join").map(String::as_str), Some("abc"));
        assert_eq!(m.value_source("vs"), Some(ValueSource::DefaultValue));
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

    #[test]
    fn integer_flags_reject_non_integers() {
        for flag in ["--players", "--tick-ms", "--max-ticks", "--seed"] {
            for bad in ["abc", "1.5"] {
                assert_eq!(
                    err_kind(&[flag, bad]),
                    ErrorKind::ValueValidation,
                    "{flag} {bad}"
                );
            }
        }
    }

    #[test]
    fn value_flags_require_a_value() {
        for id in ALL_IDS.iter().filter(|id| **id != "create") {
            let flag = format!("--{}", arg(id).get_long().unwrap());
            assert!(
                try_matches(&[&flag]).is_err(),
                "{flag} with no value should fail"
            );
        }
    }

    #[test]
    fn create_takes_no_value_and_unknown_input_is_rejected() {
        assert_eq!(err_kind(&["--create", "true"]), ErrorKind::UnknownArgument);
        assert_eq!(err_kind(&["--bogus"]), ErrorKind::UnknownArgument);
        assert_eq!(err_kind(&["stray"]), ErrorKind::UnknownArgument);
    }
}

