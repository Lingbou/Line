//! OpenSSH execution for a connection profile.
//!
//! The TUI deliberately does not implement an SSH protocol stack. This
//! module is the seam between the rest of the application and the system
//! `ssh` executable. Keeping that seam here gives us terminal compatibility
//! while still making invocation and result handling testable.
//!
//! Password profiles use OpenSSH's `SSH_ASKPASS` protocol. In V1 the saved
//! plaintext password is placed in the environment of the short-lived `ssh`
//! process and its askpass child. It is never placed in argv, diagnostics, or
//! logs, but another process running as the same OS user may be able to inspect
//! it through `/proc`; this is an explicit consequence of V1's plaintext,
//! convenience-first credential model.

use std::io;
use std::path::PathBuf;

use thiserror::Error;

use crate::config::ValidationError;

mod askpass;
mod host_keys;
mod process;
mod runner;

pub use askpass::askpass_entrypoint;
pub use host_keys::is_host_key_changed;
pub use runner::SshRunner;

/// OpenSSH is given ten seconds for DNS/TCP connection establishment.
pub const CONNECT_TIMEOUT_SECONDS: u16 = 10;

/// Maximum amount of stderr retained after a session exits. Stderr is still
/// forwarded live to the user's terminal; this bound only limits the text the
/// TUI may show in its post-session error panel.
pub const STDERR_TAIL_LIMIT: usize = 16 * 1024;

/// Errors raised while preparing or running OpenSSH.
#[derive(Debug, Error)]
pub enum SshError {
    #[error("could not determine the line executable: {0}")]
    CurrentExecutable(#[source] io::Error),

    #[error("could not check the installed OpenSSH version: {0}")]
    VersionProbe(#[source] io::Error),

    #[error("Line password login requires OpenSSH 8.4 or newer; found {0}")]
    UnsupportedVersion(String),

    #[error("could not create line directory {path}: {source}")]
    CreateLineDirectory {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("could not start ssh: {0}")]
    Spawn(#[source] io::Error),

    #[error("could not wait for ssh: {0}")]
    Wait(#[source] io::Error),

    #[error("could not read ssh diagnostics: {0}")]
    ReadStderr(#[source] io::Error),

    #[error("ssh diagnostics reader terminated unexpectedly")]
    StderrReaderPanicked,

    #[error("could not start ssh-keygen: {0}")]
    KeygenSpawn(#[source] io::Error),

    #[error("could not wait for ssh-keygen: {0}")]
    KeygenWait(#[source] io::Error),

    #[error(
        "ssh-keygen could not replace the host key (exit code {exit_code:?}, signal {signal:?}): {stderr}"
    )]
    ReplaceHostKeyFailed {
        exit_code: Option<i32>,
        signal: Option<i32>,
        stderr: String,
    },

    #[error("could not lock Line's known-hosts file: {0}")]
    KnownHostsLock(#[source] io::Error),

    #[error("invalid SSH profile: {0}")]
    InvalidProfile(#[from] ValidationError),
}

/// The observable outcome of an SSH child process.
///
/// A non-zero exit status is not an [`SshError`]: authentication failures,
/// host-key changes, and a remote command's exit status are all normal SSH
/// outcomes which the TUI can present to the user and return from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionResult {
    /// Conventional process exit code, if the process exited normally.
    pub exit_code: Option<i32>,
    /// Signal number when the process was terminated by a signal (Unix).
    pub signal: Option<i32>,
    /// The last [`STDERR_TAIL_LIMIT`] bytes of stderr, decoded lossily as
    /// UTF-8. All stderr is forwarded to the terminal while the session is
    /// running.
    pub stderr_tail: String,
}

impl SessionResult {
    /// Whether OpenSSH exited normally with status zero.
    #[must_use]
    pub const fn success(&self) -> bool {
        matches!(self.exit_code, Some(0)) && self.signal.is_none()
    }
}

#[cfg(test)]
pub(super) mod test_support {
    use std::fs;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    use tempfile::TempDir;

    use crate::config::{AuthMethod, Profile};

    pub(super) fn profile(auth: AuthMethod) -> Profile {
        Profile {
            id: "id".into(),
            name: "Example".into(),
            host: "example.com".into(),
            port: 2222,
            username: "alice".into(),
            auth,
        }
    }

    pub(super) fn fake_ssh(dir: &TempDir, record: &Path, body: &str) -> PathBuf {
        fake_program(
            dir,
            "fake-ssh",
            &body.replace("$RECORD", &record.display().to_string()),
        )
    }

    pub(super) fn fake_program(dir: &TempDir, name: &str, body: &str) -> PathBuf {
        let path = dir.path().join(name);
        let script = format!("#!/bin/sh\n{body}\n");
        let mut file = fs::File::create(&path).expect("create fake executable");
        file.write_all(script.as_bytes())
            .expect("write fake executable");
        file.sync_all().expect("sync fake executable");
        let mut permissions = file.metadata().expect("fake metadata").permissions();
        permissions.set_mode(0o700);
        file.set_permissions(permissions)
            .expect("chmod fake executable");
        drop(file);
        path
    }
}
