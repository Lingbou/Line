use std::{
    io::{self, Write},
    sync::atomic::{AtomicBool, Ordering},
};

use line::{
    app::App,
    config::{ConfigError, ConfigStore, Profile},
    ssh::{SessionResult, SshRunner, is_host_key_changed},
};

use super::{
    DynError,
    persistence::{load_with_recovery, refresh_key_choices},
    terminal::{TuiSession, wait_for_terminal_line},
};

pub(super) fn connect(
    store: &ConfigStore,
    runner: &SshRunner,
    terminal: &mut TuiSession,
    app: &mut App,
    id: &str,
    shutdown: &AtomicBool,
) -> Result<(), DynError> {
    let latest = match store.load() {
        Ok(profiles) => profiles,
        Err(ConfigError::Json { .. } | ConfigError::Validation(_)) => {
            terminal.suspend()?;
            let recovery = load_with_recovery(store, Some(shutdown));
            let resume = terminal.resume();
            if let Err(error) = resume {
                return Err(Box::new(error));
            }
            recovery?
        }
        Err(error) => return Err(Box::new(error)),
    };
    let Some(profile) = latest.by_id(id).cloned() else {
        app.set_profiles(latest.profiles);
        app.set_error("Connection was removed by another line process");
        return Ok(());
    };
    app.set_profiles(latest.profiles);
    if let Some(index) = app.profiles().iter().position(|item| item.id == profile.id) {
        app.select(index);
    }

    terminal.suspend()?;
    // Always restore the TUI, even if reading the replacement confirmation or
    // running ssh-keygen fails. Otherwise a transient I/O error would strand
    // the user's terminal in cooked/alternate-screen state.
    let session = (|| -> Result<Result<SessionResult, line::ssh::SshError>, DynError> {
        let mut outcome = runner.connect_with_cancel(&profile, shutdown);
        if let Ok(result) = &outcome
            && is_host_key_changed(result)
            && confirm_host_key_replacement(&profile, result, shutdown)?
        {
            outcome = runner
                .replace_host_key(&profile)
                .and_then(|()| runner.connect_with_cancel(&profile, shutdown));
        }
        Ok(outcome)
    })();
    let resume = terminal.resume();
    if let Err(error) = resume {
        return Err(Box::new(error));
    }
    let outcome = match session {
        Ok(outcome) => outcome,
        Err(_error) if shutdown.load(Ordering::Relaxed) => return Ok(()),
        Err(error) => return Err(error),
    };

    match outcome {
        Ok(result) if result.interrupted() => app.set_status("Disconnected · interrupted"),
        Ok(result) if result.signal.is_none() && result.exit_code != Some(255) => {
            match result.exit_code {
                Some(0) => app.set_status("Disconnected"),
                Some(code) => app.set_status(format!("Disconnected · remote exit code {code}")),
                None => app.set_status("Disconnected"),
            }
        }
        Ok(result) => app.set_error(session_failure(&profile, &result)),
        Err(error) => app.set_error(error.to_string()),
    }

    // Another instance may have edited profiles while SSH owned this terminal.
    if let Ok(latest) = store.load() {
        app.set_profiles(latest.profiles);
        if let Some(index) = app.profiles().iter().position(|item| item.id == id) {
            app.select(index);
        }
        let _ = refresh_key_choices(store, app);
    }
    Ok(())
}

fn confirm_host_key_replacement(
    profile: &Profile,
    result: &SessionResult,
    shutdown: &AtomicBool,
) -> Result<bool, io::Error> {
    eprintln!();
    eprintln!(
        "The saved host key for {}:{} has changed.",
        profile.host, profile.port
    );
    if !result.stderr_tail.trim().is_empty() {
        eprintln!("{}", result.stderr_tail.trim());
    }
    eprint!("Replace it and reconnect? [y/N] ");
    io::stderr().flush()?;
    wait_for_terminal_line(shutdown)?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

pub(super) fn session_failure(profile: &Profile, result: &SessionResult) -> String {
    let status = match (result.exit_code, result.signal) {
        (Some(code), _) => format!("ssh exited with code {code}"),
        (_, Some(signal)) => format!("ssh was terminated by signal {signal}"),
        _ => "ssh ended without an exit status".to_owned(),
    };
    // A chain runs one ssh per hop, so name the hop when OpenSSH blames one.
    let status = match result.failing_jump_hop(&profile.jump_chain) {
        Some(index) => {
            let hop = &profile.jump_chain[index];
            format!(
                "{status} · jump {}/{} {} unreachable",
                index + 1,
                profile.jump_chain.len(),
                hop.authority()
            )
        }
        None => status,
    };
    let diagnostics = result.stderr_tail.trim();
    if diagnostics.is_empty() {
        status
    } else {
        format!("{status}\n\n{diagnostics}")
    }
}
