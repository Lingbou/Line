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
        let mut outcome = run_ssh_with_interrupt_guard(runner, &profile, shutdown);
        if let Ok(result) = &outcome
            && is_host_key_changed(result)
            && confirm_host_key_replacement(&profile, result, shutdown)?
        {
            outcome = replace_and_reconnect_with_interrupt_guard(runner, &profile, shutdown);
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
        Ok(result) if matches!(result.signal, Some(libc::SIGINT) | Some(libc::SIGQUIT)) => {
            app.set_status("Disconnected · interrupted")
        }
        Ok(result) if result.signal.is_none() && result.exit_code != Some(255) => {
            match result.exit_code {
                Some(0) => app.set_status("Disconnected"),
                Some(code) => app.set_status(format!("Disconnected · remote exit code {code}")),
                None => app.set_status("Disconnected"),
            }
        }
        Ok(result) => app.set_error(session_failure(&result)),
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

fn run_ssh_with_interrupt_guard(
    runner: &SshRunner,
    profile: &Profile,
    shutdown: &AtomicBool,
) -> Result<SessionResult, line::ssh::SshError> {
    let _guard = InterruptGuard::install();
    runner.connect_with_cancel(profile, shutdown)
}

fn replace_and_reconnect_with_interrupt_guard(
    runner: &SshRunner,
    profile: &Profile,
    shutdown: &AtomicBool,
) -> Result<SessionResult, line::ssh::SshError> {
    let replacement = {
        let _guard = InterruptGuard::install();
        runner.replace_host_key(profile)
    };
    replacement.and_then(|()| run_ssh_with_interrupt_guard(runner, profile, shutdown))
}

#[cfg(unix)]
struct InterruptGuard {
    old_int: libc::sighandler_t,
    old_quit: libc::sighandler_t,
}

#[cfg(unix)]
impl InterruptGuard {
    fn install() -> Self {
        // SAFETY: temporarily changing SIGINT/SIGQUIT dispositions around a
        // blocking child wait is a standard POSIX operation. The old handlers
        // are restored in Drop before control returns to the TUI event loop.
        let old_int = unsafe { libc::signal(libc::SIGINT, libc::SIG_IGN) };
        let old_quit = unsafe { libc::signal(libc::SIGQUIT, libc::SIG_IGN) };
        Self { old_int, old_quit }
    }
}

#[cfg(unix)]
impl Drop for InterruptGuard {
    fn drop(&mut self) {
        unsafe {
            libc::signal(libc::SIGINT, self.old_int);
            libc::signal(libc::SIGQUIT, self.old_quit);
        }
    }
}

#[cfg(not(unix))]
struct InterruptGuard;

#[cfg(not(unix))]
impl InterruptGuard {
    fn install() -> Self {
        Self
    }
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

pub(super) fn session_failure(result: &SessionResult) -> String {
    let status = match (result.exit_code, result.signal) {
        (Some(code), _) => format!("ssh exited with code {code}"),
        (_, Some(signal)) => format!("ssh was terminated by signal {signal}"),
        _ => "ssh ended without an exit status".to_owned(),
    };
    let diagnostics = result.stderr_tail.trim();
    if diagnostics.is_empty() {
        status
    } else {
        format!("{status}\n\n{diagnostics}")
    }
}
