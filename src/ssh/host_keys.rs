use std::fs::{self, OpenOptions};
use std::process::{Command, Stdio};

use fs2::FileExt;

use crate::config::Profile;

use super::process::{bounded_lossy, exit_status_parts, prepare_line_directory};
use super::{SessionResult, SshError, SshRunner};

impl SshRunner {
    /// Remove the saved host key for a profile after the user has explicitly
    /// confirmed that the server was reinstalled or otherwise changed.
    ///
    /// The caller should only offer this action when
    /// [`is_host_key_changed`] returns true. This method does not reconnect;
    /// the next call to [`Self::connect`] will perform the normal `accept-new`
    /// handshake and write the replacement key.
    pub fn replace_host_key(&self, profile: &Profile) -> Result<(), SshError> {
        profile.validate()?;
        prepare_line_directory(&self.line_dir)?;

        let lock_path = self.line_dir.join("known_hosts.lock");
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)
            .map_err(SshError::KnownHostsLock)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            lock.set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(SshError::KnownHostsLock)?;
        }
        lock.lock_exclusive().map_err(SshError::KnownHostsLock)?;

        let target = known_host_target(profile);
        let known_hosts = self.line_dir.join("known_hosts");
        let mut keygen = Command::new(&self.ssh_keygen_program);
        keygen
            .arg("-R")
            .arg(target)
            .arg("-f")
            .arg(known_hosts)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            unsafe {
                keygen.pre_exec(|| {
                    libc::signal(libc::SIGINT, libc::SIG_DFL);
                    libc::signal(libc::SIGQUIT, libc::SIG_DFL);
                    Ok(())
                });
            }
        }
        let child = keygen.spawn().map_err(SshError::KeygenSpawn)?;
        let output = child.wait_with_output().map_err(SshError::KeygenWait)?;
        let _ = lock.unlock();

        if output.status.success() {
            return Ok(());
        }

        let (exit_code, signal) = exit_status_parts(output.status);
        Err(SshError::ReplaceHostKeyFailed {
            exit_code,
            signal,
            stderr: bounded_lossy(&output.stderr),
        })
    }
}

/// Return true when OpenSSH's diagnostics indicate a changed host key.
///
/// OpenSSH has kept the first wording stable for many releases, but localized
/// builds and newer versions also use the shorter "host key ... has changed"
/// form. Matching these narrow phrases avoids treating ordinary
/// authentication or network failures as a replacement request.
#[must_use]
pub fn is_host_key_changed(result: &SessionResult) -> bool {
    if result.signal.is_some() || result.exit_code == Some(0) {
        return false;
    }
    let diagnostics = result.stderr_tail.to_ascii_lowercase();
    diagnostics.contains("remote host identification has changed")
        || (diagnostics.contains("host key for") && diagnostics.contains("has changed"))
        || (diagnostics.contains("offending") && diagnostics.contains("known_hosts"))
}

fn known_host_target(profile: &Profile) -> String {
    let host = profile
        .host
        .trim()
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or_else(|| profile.host.trim());
    if profile.port == 22 {
        host.to_owned()
    } else {
        format!("[{host}]:{}", profile.port)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use crate::config::AuthMethod;

    use super::super::test_support::{fake_program, fake_ssh, profile};
    use super::*;

    #[test]
    fn changed_host_key_diagnostic_is_identified_for_confirmation_ui() {
        let result = SessionResult {
            exit_code: Some(255),
            signal: None,
            stderr_tail: concat!(
                "@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@\n",
                "@    WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED!     @\n",
                "@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@\n",
                "Host key verification failed.\n",
            )
            .into(),
        };

        assert!(is_host_key_changed(&result));
        assert!(!is_host_key_changed(&SessionResult {
            exit_code: Some(255),
            signal: None,
            stderr_tail: "Permission denied (publickey).\n".into(),
        }));
    }

    #[test]
    fn replacing_nondefault_port_host_key_uses_openssh_known_hosts_target() {
        let dir = TempDir::new().expect("temp dir");
        let record = dir.path().join("keygen-record");
        let fake_ssh = fake_ssh(&dir, &dir.path().join("unused"), "exit 0");
        let fake_keygen = fake_program(
            &dir,
            "fake-keygen",
            &format!("printf '%s\\n' \"$@\" > '{}'", record.display()),
        );
        let line_dir = dir.path().join("line");
        let runner = SshRunner::for_test(&line_dir, fake_ssh, fake_keygen, "/bin/false");
        let mut target = profile(AuthMethod::Password {
            password: "secret".into(),
        });
        target.host = "2001:db8::10".into();
        target.port = 2222;

        runner
            .replace_host_key(&target)
            .expect("host key replacement");
        let args = fs::read_to_string(record).expect("recorded args");
        assert_eq!(
            args.lines().collect::<Vec<_>>(),
            vec![
                "-R",
                "[2001:db8::10]:2222",
                "-f",
                line_dir.join("known_hosts").to_string_lossy().as_ref(),
            ]
        );
    }

    #[test]
    fn replacing_default_port_ipv6_host_key_uses_bare_address() {
        let dir = TempDir::new().expect("temp dir");
        let record = dir.path().join("keygen-record");
        let fake_ssh = fake_ssh(&dir, &dir.path().join("unused"), "exit 0");
        let fake_keygen = fake_program(
            &dir,
            "fake-keygen",
            &format!("printf '%s\\n' \"$@\" > '{}'", record.display()),
        );
        let line_dir = dir.path().join("line");
        let runner = SshRunner::for_test(&line_dir, fake_ssh, fake_keygen, "/bin/false");
        let mut target = profile(AuthMethod::Password {
            password: "secret".into(),
        });
        target.host = "[2001:db8::10]".into();
        target.port = 22;

        runner
            .replace_host_key(&target)
            .expect("host key replacement");
        let args = fs::read_to_string(record).expect("recorded args");
        assert_eq!(args.lines().next(), Some("-R"));
        assert_eq!(args.lines().nth(1), Some("2001:db8::10"));
    }

    #[test]
    fn failed_host_key_replacement_returns_the_tool_diagnostic() {
        let dir = TempDir::new().expect("temp dir");
        let fake_ssh = fake_ssh(&dir, &dir.path().join("unused"), "exit 0");
        let fake_keygen = fake_program(
            &dir,
            "fake-keygen",
            "printf 'known_hosts is malformed\\n' >&2; exit 9",
        );
        let runner =
            SshRunner::for_test(dir.path().join("line"), fake_ssh, fake_keygen, "/bin/false");
        let error = runner
            .replace_host_key(&profile(AuthMethod::Password {
                password: "secret".into(),
            }))
            .expect_err("replacement must fail");

        assert!(matches!(
            error,
            SshError::ReplaceHostKeyFailed {
                exit_code: Some(9),
                ref stderr,
                ..
            } if stderr == "known_hosts is malformed\n"
        ));
    }
}
