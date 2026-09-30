use std::ffi::{OsStr, OsString};
use std::process::ExitCode;

use unicode_width::UnicodeWidthStr;

use line::config::{AuthMethod, ConfigStore, Profile, profile_names_equal};
use line::ssh::SshRunner;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, PartialEq, Eq)]
enum CliCommand {
    Help,
    Version,
    List,
    Connect(String),
}

pub(crate) fn handle_cli_args(args: &[OsString]) -> Option<ExitCode> {
    let command = match parse_args(args) {
        Ok(Some(command)) => command,
        Ok(None) => return None,
        Err(error) => {
            eprintln!("line: {error}");
            eprintln!("Try 'line --help' for more information.");
            return Some(ExitCode::FAILURE);
        }
    };

    match command {
        CliCommand::Help => {
            print_help();
            Some(ExitCode::SUCCESS)
        }
        CliCommand::Version => {
            println!("line {VERSION}");
            Some(ExitCode::SUCCESS)
        }
        CliCommand::List => match print_list() {
            Ok(()) => Some(ExitCode::SUCCESS),
            Err(error) => {
                eprintln!("line: {error}");
                Some(ExitCode::FAILURE)
            }
        },
        CliCommand::Connect(name) => Some(direct_connect(&name)),
    }
}

fn parse_args(args: &[OsString]) -> Result<Option<CliCommand>, String> {
    let Some(first) = args.get(1) else {
        return Ok(None);
    };

    if first == "--" {
        return match args {
            [_, _, name] => Ok(Some(CliCommand::Connect(os_to_string(name)?))),
            [_, _] => Err("missing connection name after '--'".to_owned()),
            _ => Err("too many arguments".to_owned()),
        };
    }

    if is_option(first, "-h", "--help") {
        return exactly_one_argument(args, CliCommand::Help);
    }
    if is_option(first, "-v", "--version") {
        return exactly_one_argument(args, CliCommand::Version);
    }
    if is_option(first, "-l", "--list") {
        return exactly_one_argument(args, CliCommand::List);
    }
    if first.to_string_lossy().starts_with('-') {
        return Err(format!("unrecognized option '{}'", first.to_string_lossy()));
    }
    if args.len() != 2 {
        return Err("too many arguments".to_owned());
    }
    Ok(Some(CliCommand::Connect(os_to_string(first)?)))
}

fn exactly_one_argument(
    args: &[OsString],
    command: CliCommand,
) -> Result<Option<CliCommand>, String> {
    if args.len() == 2 {
        Ok(Some(command))
    } else {
        Err("too many arguments".to_owned())
    }
}

fn is_option(value: &OsStr, short: &str, long: &str) -> bool {
    value == short || value == long
}

fn os_to_string(value: &OsStr) -> Result<String, String> {
    value
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| "connection name is not valid UTF-8".to_owned())
}

fn open_store() -> Result<ConfigStore, String> {
    ConfigStore::in_home().map_err(|error| format!("failed to access config: {error}"))
}

fn load_profiles(store: &ConfigStore) -> Result<Vec<Profile>, String> {
    store
        .load()
        .map(|data| data.profiles)
        .map_err(|error| format!("failed to load profiles: {error}"))
}

fn print_help() {
    println!(
        "Line: Lightweight Linux TUI SSH connection manager

USAGE:
    line                          Launch interactive TUI
    line <NAME>                   Connect directly to a saved server
    line -- <NAME>                Connect to a name beginning with '-'
    line -l, --list               List saved connection profiles
    line -v, --version            Print version information
    line -h, --help               Print this help message

TUI SHORTCUTS:
    ↑/↓ or ←/→                    Select connection
    Enter                         Connect to selected server
    Ctrl+T                        Add new connection
    Ctrl+E                        Edit selected connection
    Ctrl+D                        Delete selected connection
    Ctrl+C                        Quit
    Esc                           Clear filter / dismiss modal"
    );
}

fn pad_right(s: &str, target_width: usize) -> String {
    let width = UnicodeWidthStr::width(s);
    if width >= target_width {
        s.to_string()
    } else {
        format!("{s}{}", " ".repeat(target_width - width))
    }
}

fn print_list() -> Result<(), String> {
    let store = open_store()?;
    let profiles = load_profiles(&store)?;

    if profiles.is_empty() {
        println!("No connections saved. Run 'line' to add a connection.");
        return Ok(());
    }

    let max_name = profiles
        .iter()
        .map(|p| UnicodeWidthStr::width(p.name.as_str()))
        .max()
        .unwrap_or(10)
        .clamp(10, 36);

    println!(
        "{}  {}  AUTH",
        pad_right("NAME", max_name),
        pad_right("DESTINATION", 32)
    );
    for p in &profiles {
        let auth = match p.auth {
            AuthMethod::Key { .. } => "KEY",
            AuthMethod::Password { .. } => "PWD",
        };
        let dest = format!("{}@{}:{}", p.username, p.host, p.port);
        println!(
            "{}  {}  {}",
            pad_right(&p.name, max_name),
            pad_right(&dest, 32),
            auth
        );
    }
    Ok(())
}

pub(crate) fn find_profile<'a>(
    profiles: &'a [Profile],
    query: &str,
) -> Result<&'a Profile, String> {
    if profiles.is_empty() {
        return Err("No saved connections found. Run 'line' to add one.".into());
    }

    // 1. Exact match (case-insensitive)
    if let Some(p) = profiles
        .iter()
        .find(|p| profile_names_equal(&p.name, query))
    {
        return Ok(p);
    }

    // 2. Prefix match
    let query_lower = query.to_lowercase();
    let prefix_matches: Vec<&Profile> = profiles
        .iter()
        .filter(|p| p.name.to_lowercase().starts_with(&query_lower))
        .collect();

    if prefix_matches.len() == 1 {
        return Ok(prefix_matches[0]);
    }
    if prefix_matches.len() > 1 {
        let names = prefix_matches
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "ambiguous connection '{query}' matches multiple servers: {names}"
        ));
    }

    // 3. Substring match
    let sub_matches: Vec<&Profile> = profiles
        .iter()
        .filter(|p| p.name.to_lowercase().contains(&query_lower))
        .collect();

    if sub_matches.len() == 1 {
        return Ok(sub_matches[0]);
    }
    if sub_matches.len() > 1 {
        let names = sub_matches
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "ambiguous connection '{query}' matches multiple servers: {names}"
        ));
    }

    Err(format!(
        "connection '{query}' not found. Run 'line --list' to see saved connections."
    ))
}

fn direct_connect(name: &str) -> ExitCode {
    let store = match open_store() {
        Ok(store) => store,
        Err(error) => {
            eprintln!("line: {error}");
            return ExitCode::FAILURE;
        }
    };

    let profiles = match load_profiles(&store) {
        Ok(profiles) => profiles,
        Err(error) => {
            eprintln!("line: {error}");
            return ExitCode::FAILURE;
        }
    };

    let profile = match find_profile(&profiles, name) {
        Ok(p) => p,
        Err(err) => {
            eprintln!("line: {err}");
            return ExitCode::FAILURE;
        }
    };

    let runner = match SshRunner::new(store.root()) {
        Ok(runner) => runner,
        Err(err) => {
            eprintln!("line: {err}");
            return ExitCode::FAILURE;
        }
    };

    match runner.connect(profile) {
        Ok(result) => {
            if let Some(code) = result.exit_code {
                ExitCode::from(code as u8)
            } else if let Some(signal) = result.signal {
                ExitCode::from((128 + signal) as u8)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(err) => {
            eprintln!("line: {err}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_profile(name: &str) -> Profile {
        Profile {
            id: name.into(),
            name: name.into(),
            host: "127.0.0.1".into(),
            port: 22,
            username: "root".into(),
            jump_chain: Vec::new(),
            auth: AuthMethod::Password {
                password: "pw".into(),
            },
        }
    }

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn find_profile_matches_exact_case_insensitively() {
        let profiles = vec![test_profile("Production"), test_profile("Staging")];
        assert_eq!(
            find_profile(&profiles, "production").unwrap().name,
            "Production"
        );
        assert_eq!(
            find_profile(&profiles, "PRODUCTION").unwrap().name,
            "Production"
        );
        assert_eq!(find_profile(&profiles, "Staging").unwrap().name, "Staging");
    }

    #[test]
    fn find_profile_matches_unique_prefix_and_substring() {
        let profiles = vec![
            test_profile("HongKong-API-01"),
            test_profile("Tokyo-API-02"),
        ];
        assert_eq!(
            find_profile(&profiles, "hong").unwrap().name,
            "HongKong-API-01"
        );
        assert_eq!(
            find_profile(&profiles, "tokyo").unwrap().name,
            "Tokyo-API-02"
        );
        assert_eq!(find_profile(&profiles, "02").unwrap().name, "Tokyo-API-02");
    }

    #[test]
    fn find_profile_reports_ambiguous_and_missing() {
        let profiles = vec![test_profile("Novix-01"), test_profile("Novix-02")];
        assert!(
            find_profile(&profiles, "Novix")
                .unwrap_err()
                .contains("ambiguous")
        );
        assert!(
            find_profile(&profiles, "Missing")
                .unwrap_err()
                .contains("not found")
        );
    }

    #[test]
    fn cli_parser_recognizes_commands_and_rejects_extra_arguments() {
        assert_eq!(parse_args(&args(&["line"])), Ok(None));
        assert_eq!(
            parse_args(&args(&["line", "-v"])),
            Ok(Some(CliCommand::Version))
        );
        assert_eq!(
            parse_args(&args(&["line", "--help"])),
            Ok(Some(CliCommand::Help))
        );
        assert_eq!(
            parse_args(&args(&["line", "-l"])),
            Ok(Some(CliCommand::List))
        );
        assert_eq!(
            parse_args(&args(&["line", "production"])),
            Ok(Some(CliCommand::Connect("production".into())))
        );
        assert_eq!(
            parse_args(&args(&["line", "--", "-production"])),
            Ok(Some(CliCommand::Connect("-production".into())))
        );
        assert!(parse_args(&args(&["line", "--list", "extra"])).is_err());
        assert!(parse_args(&args(&["line", "production", "extra"])).is_err());
        assert!(parse_args(&args(&["line", "--"])).is_err());
        assert!(parse_args(&args(&["line", "--unknown-flag"])).is_err());
    }

    #[test]
    fn cli_handler_returns_none_for_the_interactive_entrypoint() {
        assert_eq!(handle_cli_args(&args(&["line"])), None);
        assert_eq!(
            handle_cli_args(&args(&["line", "--unknown-flag"])),
            Some(ExitCode::FAILURE)
        );
    }
}
