use std::ffi::OsString;
use std::io::{self, Write};

use uuid::Uuid;

// These names are intentionally internal-looking. A fresh UUID is used for
// every invocation and is also incorporated into the password variable name,
// so a normal `line` process can never accidentally enter askpass mode.
pub(super) const MARKER_ENV: &str = "LINE_INTERNAL_ASKPASS";
pub(super) const PASSWORD_PREFIX: &str = "LINE_INTERNAL_ASKPASS_PASSWORD_";

/// Handle the hidden askpass invocation used by [`super::SshRunner`].
///
/// The application should call this before entering the TUI:
///
/// ```ignore
/// if let Some(code) = line::ssh::askpass_entrypoint() {
///     std::process::exit(code);
/// }
/// ```
///
/// `None` means this is an ordinary invocation. When the internal marker is
/// present, a malformed invocation returns exit code 1 rather than falling
/// through into the TUI. OpenSSH supplies exactly one argument (the prompt)
/// after `argv[0]`, which is checked to keep accidental activation impossible.
pub fn askpass_entrypoint() -> Option<i32> {
    let marker = std::env::var_os(MARKER_ENV)?;
    let args: Vec<OsString> = std::env::args_os().collect();

    if args.len() != 2 {
        return Some(1);
    }

    // The runner uses UUIDs as one-shot markers. Besides documenting the
    // protocol, parsing here rejects a hand-written environment variable with
    // a merely similar value.
    let token = marker
        .to_str()
        .filter(|value| Uuid::parse_str(value).is_ok());
    let Some(token) = token else {
        return Some(1);
    };

    let password_var = format!("{PASSWORD_PREFIX}{token}");
    let Some(password) = std::env::var_os(password_var) else {
        return Some(1);
    };

    let mut stdout = io::stdout().lock();
    if stdout.write_all(password.as_encoded_bytes()).is_err()
        || stdout.write_all(b"\n").is_err()
        || stdout.flush().is_err()
    {
        Some(1)
    } else {
        Some(0)
    }
}
