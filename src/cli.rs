use std::process::ExitCode;

use unicode_width::UnicodeWidthStr;

use line::config::{AuthMethod, ConfigStore, Profile, profile_names_equal};
use line::ssh::SshRunner;

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub(crate) fn handle_cli_args(args: &[String]) -> Option<ExitCode> {
    if args.len() <= 1 {
        return None;
    }

    let first = args[1].as_str();
    match first {
        "-v" | "--version" => {
            println!("line {VERSION}");
            Some(ExitCode::SUCCESS)
        }
        "-h" | "--help" => {
            print_help();
            Some(ExitCode::SUCCESS)
        }
        "-l" | "--list" => {
            print_list();
            Some(ExitCode::SUCCESS)
        }
        arg if arg.starts_with('-') => {
            eprintln!("line: unrecognized option '{arg}'");
            eprintln!("Try 'line --help' for more information.");
            Some(ExitCode::FAILURE)
        }
        name => Some(direct_connect(name)),
    }
}

fn print_help() {
    println!(
        "Line: Lightweight Linux TUI SSH connection manager

USAGE:
    line                          Launch interactive TUI
    line <NAME>                   Connect directly to a saved server
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

fn print_list() {
    let store = match ConfigStore::in_home() {
        Ok(store) => store,
        Err(err) => {
            eprintln!("line: failed to access config: {err}");
            return;
        }
    };

    let profiles = match store.load() {
        Ok(data) => data.profiles,
        Err(err) => {
            eprintln!("line: failed to load profiles: {err}");
            return;
        }
    };

    if profiles.is_empty() {
        println!("No connections saved. Run 'line' to add a connection.");
        return;
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
    let store = match ConfigStore::in_home() {
        Ok(store) => store,
        Err(err) => {
            eprintln!("line: {err}");
            return ExitCode::FAILURE;
        }
    };

    let data = match store.load() {
        Ok(data) => data,
        Err(err) => {
            eprintln!("line: failed to load profiles: {err}");
            return ExitCode::FAILURE;
        }
    };

    let profile = match find_profile(&data.profiles, name) {
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
            auth: AuthMethod::Password {
                password: "pw".into(),
            },
        }
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
    fn cli_flags_are_recognized() {
        assert_eq!(handle_cli_args(&["line".into()]), None);
        assert_eq!(
            handle_cli_args(&["line".into(), "-v".into()]),
            Some(ExitCode::SUCCESS)
        );
        assert_eq!(
            handle_cli_args(&["line".into(), "--version".into()]),
            Some(ExitCode::SUCCESS)
        );
        assert_eq!(
            handle_cli_args(&["line".into(), "-h".into()]),
            Some(ExitCode::SUCCESS)
        );
        assert_eq!(
            handle_cli_args(&["line".into(), "--help".into()]),
            Some(ExitCode::SUCCESS)
        );
        assert_eq!(
            handle_cli_args(&["line".into(), "--unknown-flag".into()]),
            Some(ExitCode::FAILURE)
        );
    }
}
