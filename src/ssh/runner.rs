use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;

use uuid::Uuid;

use crate::config::{AuthMethod, Profile};

use super::askpass::{MARKER_ENV, PASSWORD_PREFIX};
use super::process::{
    collect_stderr, destination, prepare_line_directory, resolve_path, session_result,
    spawn_stderr_reader, wait_for_child,
};
use super::{CONNECT_TIMEOUT_SECONDS, SessionResult, SshError};

/// A configured OpenSSH launcher.
///
/// `line_dir` is normally `~/.line`; relative key paths in a [`Profile`] are
/// resolved beneath it. The launcher disables the user's `~/.ssh/config` but
/// retains system policy, and uses a Line-owned known-hosts file.
#[derive(Clone, Debug)]
pub struct SshRunner {
    pub(super) line_dir: PathBuf,
    ssh_program: PathBuf,
    pub(super) ssh_keygen_program: PathBuf,
    askpass_program: PathBuf,
    probe_version: bool,
}

impl SshRunner {
    /// Construct a launcher using the system `ssh` and this process's binary
    /// as the askpass helper.
    pub fn new(line_dir: impl Into<PathBuf>) -> Result<Self, SshError> {
        let askpass_program = std::env::current_exe().map_err(SshError::CurrentExecutable)?;
        Ok(Self {
            line_dir: line_dir.into(),
            ssh_program: PathBuf::from("ssh"),
            ssh_keygen_program: PathBuf::from("ssh-keygen"),
            askpass_program,
            probe_version: true,
        })
    }

    /// Inject process paths without exposing test plumbing as production API.
    #[cfg(test)]
    #[must_use]
    pub(super) fn for_test(
        line_dir: impl Into<PathBuf>,
        ssh_program: impl Into<PathBuf>,
        ssh_keygen_program: impl Into<PathBuf>,
        askpass_program: impl Into<PathBuf>,
    ) -> Self {
        Self {
            line_dir: line_dir.into(),
            ssh_program: ssh_program.into(),
            ssh_keygen_program: ssh_keygen_program.into(),
            askpass_program: askpass_program.into(),
            probe_version: false,
        }
    }

    /// Execute OpenSSH for a profile, handing the terminal to it until the
    /// child exits.
    pub fn connect(&self, profile: &Profile) -> Result<SessionResult, SshError> {
        self.connect_inner(profile, None)
    }

    /// Execute OpenSSH while observing a process-wide shutdown flag. This is
    /// used by the binary for SIGTERM/SIGHUP cleanup; regular library callers
    /// can keep using [`Self::connect`] without setting up signal handling.
    pub fn connect_with_cancel(
        &self,
        profile: &Profile,
        cancel: &AtomicBool,
    ) -> Result<SessionResult, SshError> {
        self.connect_inner(profile, Some(cancel))
    }

    fn connect_inner(
        &self,
        profile: &Profile,
        cancel: Option<&AtomicBool>,
    ) -> Result<SessionResult, SshError> {
        profile.validate()?;
        if self.probe_version && matches!(profile.auth, AuthMethod::Password { .. }) {
            ensure_supported_openssh(&self.ssh_program)?;
        }

        // OpenSSH creates known_hosts itself, but only when its parent exists.
        // Creating the already-configured ~/.line directory here also makes a
        // first-run connection work when the user has not yet saved a profile.
        prepare_line_directory(&self.line_dir)?;

        let mut command = Command::new(&self.ssh_program);
        self.configure_command(&mut command, profile);

        // stdout and stdin remain attached to the terminal. `-tt` below
        // forces a remote pty, which is what lets full-screen shells/programs
        // behave exactly as they do under a direct `ssh` invocation. Stderr
        // is piped only so we can retain a bounded diagnostic tail; a reader
        // thread forwards every byte immediately to the real stderr stream.
        command.stdin(Stdio::inherit());
        command.stdout(Stdio::inherit());
        command.stderr(Stdio::piped());

        // The binary ignores terminal interrupts in the parent only while it
        // waits for SSH. Restore normal dispositions in the child after fork
        // so Ctrl-C still reaches and terminates OpenSSH as expected.
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            unsafe {
                command.pre_exec(|| {
                    libc::signal(libc::SIGINT, libc::SIG_DFL);
                    libc::signal(libc::SIGQUIT, libc::SIG_DFL);
                    Ok(())
                });
            }
        }

        let mut child = command.spawn().map_err(SshError::Spawn)?;
        let stderr = child
            .stderr
            .take()
            .expect("stderr was configured as piped before spawning ssh");
        let reader = spawn_stderr_reader(stderr);

        let (status, cancelled) = wait_for_child(&mut child, cancel)?;
        let stderr_tail = if cancelled {
            // A wrapper/ProxyCommand descendant may still hold a duplicate of
            // stderr after the ssh process is killed. Do not let that orphaned
            // descriptor block TUI restoration indefinitely.
            drop(reader);
            String::new()
        } else {
            collect_stderr(reader)?
        };

        Ok(session_result(status, stderr_tail))
    }

    fn configure_command(&self, command: &mut Command, profile: &Profile) {
        let known_hosts = self.line_dir.join("known_hosts");
        let destination = destination(profile);

        // An explicit system config prevents ~/.ssh/config from changing what
        // a profile means while retaining Linux distribution/administrator
        // policy. The known-hosts overrides below remain Line-owned.
        let system_config = if Path::new("/etc/ssh/ssh_config").is_file() {
            "/etc/ssh/ssh_config"
        } else {
            "/dev/null"
        };
        command
            // Keep local OpenSSH diagnostics stable for post-session error
            // classification. This does not set the remote shell's locale.
            .env("LC_ALL", "C")
            .arg("-F")
            .arg(system_config)
            .arg("-o")
            .arg(format!("ConnectTimeout={CONNECT_TIMEOUT_SECONDS}"))
            .arg("-o")
            .arg(format!("UserKnownHostsFile={}", known_hosts.display()))
            .arg("-o")
            .arg("GlobalKnownHostsFile=/dev/null")
            .arg("-o")
            .arg("StrictHostKeyChecking=accept-new")
            .arg("-o")
            // OpenSSH reports a successfully accepted first-use host key at
            // INFO level. Line already chose accept-new, so that routine
            // notice only adds noise between the TUI and remote shell. ERROR
            // keeps authentication, network, and changed-host-key failures.
            .arg("LogLevel=ERROR")
            .arg("-o")
            .arg("ConnectionAttempts=1")
            .arg("-tt")
            .arg("-p")
            .arg(profile.port.to_string());

        match &profile.auth {
            AuthMethod::Key { private_key, .. } => {
                let key_path = resolve_path(&self.line_dir, private_key);
                command
                    .arg("-i")
                    .arg(key_path)
                    .arg("-o")
                    .arg("IdentitiesOnly=yes")
                    .arg("-o")
                    .arg("IdentityAgent=none")
                    .arg("-o")
                    .arg("PreferredAuthentications=publickey")
                    .arg("-o")
                    .arg("PubkeyAuthentication=yes")
                    .arg("-o")
                    .arg("PasswordAuthentication=no")
                    .arg("-o")
                    .arg("KbdInteractiveAuthentication=no")
                    .arg("-o")
                    .arg("BatchMode=yes");

                // Do not allow askpass settings inherited from a parent shell
                // to turn an encrypted/invalid key into an unexpected prompt.
                command
                    .env_remove(MARKER_ENV)
                    .env_remove("SSH_ASKPASS_REQUIRE")
                    .env_remove("SSH_ASKPASS");
            }
            AuthMethod::Password { password } => {
                let token = Uuid::new_v4().to_string();
                let password_var = format!("{PASSWORD_PREFIX}{token}");

                command
                    .arg("-o")
                    .arg("BatchMode=no")
                    .arg("-o")
                    .arg("PreferredAuthentications=password,keyboard-interactive")
                    .arg("-o")
                    .arg("PubkeyAuthentication=no")
                    .arg("-o")
                    .arg("PasswordAuthentication=yes")
                    .arg("-o")
                    .arg("KbdInteractiveAuthentication=yes")
                    .arg("-o")
                    // One try for password plus one for keyboard-interactive:
                    // the saved value is submitted at most twice overall on
                    // ordinary single-prompt servers.
                    .arg("NumberOfPasswordPrompts=1")
                    // OpenSSH 8.4+ honours `force` even when stdin is a tty;
                    // this keeps stdin available for the remote session while
                    // obtaining the saved password without sshpass.
                    .env(MARKER_ENV, &token)
                    .env(&password_var, password)
                    .env("SSH_ASKPASS", &self.askpass_program)
                    .env("SSH_ASKPASS_REQUIRE", "force")
                    // A DISPLAY value is required by older OpenSSH versions
                    // before they consider invoking SSH_ASKPASS. The helper
                    // does not contact an X server, so a harmless fallback is
                    // enough when the user is on a plain console.
                    .env(
                        "DISPLAY",
                        std::env::var_os("DISPLAY").unwrap_or_else(|| OsString::from(":0")),
                    );
            }
        }

        // End option parsing before accepting a host read from a JSON profile.
        command.arg("--").arg(destination);
    }
}

fn ensure_supported_openssh(program: &Path) -> Result<(), SshError> {
    let mut probe = Command::new(program);
    probe.arg("-V");
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            probe.pre_exec(|| {
                libc::signal(libc::SIGINT, libc::SIG_DFL);
                libc::signal(libc::SIGQUIT, libc::SIG_DFL);
                Ok(())
            });
        }
    }
    let output = probe.output().map_err(SshError::VersionProbe)?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let Some((major, minor)) = parse_openssh_version(&text) else {
        return Err(SshError::UnsupportedVersion(text.trim().to_owned()));
    };
    if (major, minor) < (8, 4) {
        return Err(SshError::UnsupportedVersion(format!("{major}.{minor}")));
    }
    Ok(())
}

fn parse_openssh_version(text: &str) -> Option<(u32, u32)> {
    let version = text.split("OpenSSH_").nth(1)?;
    let mut parts = version.splitn(3, ['.', 'p', ' ']);
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    Some((major, minor))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    use tempfile::TempDir;

    use crate::config::AuthMethod;

    use super::super::askpass::PASSWORD_PREFIX;
    use super::super::test_support::{fake_ssh, profile};
    use super::super::{STDERR_TAIL_LIMIT, SshError};
    use super::*;

    #[test]
    fn parses_supported_openssh_versions() {
        assert_eq!(
            parse_openssh_version("OpenSSH_10.0p2, OpenSSL 3.5.7"),
            Some((10, 0))
        );
        assert_eq!(parse_openssh_version("OpenSSH_8.4p1 Debian"), Some((8, 4)));
        assert_eq!(parse_openssh_version("not openssh"), None);
    }

    #[test]
    fn old_openssh_is_allowed_for_keys_but_rejected_for_saved_passwords() {
        let dir = TempDir::new().expect("temp dir");
        let record = dir.path().join("record");
        let fake = fake_ssh(
            &dir,
            &record,
            r#"
            if [ "$1" = "-V" ]; then
                printf 'OpenSSH_8.3p1 test\n' >&2
                exit 0
            fi
            printf 'connected\n' > "$RECORD"
            exit 0
            "#,
        );
        let mut runner =
            SshRunner::for_test(dir.path().join("line"), fake, "ssh-keygen", "/bin/false");
        runner.probe_version = true;

        assert!(
            runner
                .connect(&profile(AuthMethod::Key {
                    private_key: PathBuf::from("keys/Example/key"),
                    public_key: PathBuf::from("keys/Example/key.pub"),
                }))
                .unwrap()
                .success()
        );
        assert_eq!(fs::read_to_string(&record).unwrap(), "connected\n");
        let error = runner
            .connect(&profile(AuthMethod::Password {
                password: "secret".into(),
            }))
            .unwrap_err();
        assert!(matches!(error, SshError::UnsupportedVersion(version) if version == "8.3"));
    }

    #[test]
    fn key_profile_uses_isolated_openssh_options_and_returns_status() {
        let dir = TempDir::new().expect("temp dir");
        let record = dir.path().join("record");
        let fake = fake_ssh(
            &dir,
            &record,
            r#"
            {
                printf '%s\n' "$@" > "$RECORD"
                printf 'warning from ssh\n' >&2
                exit 7
            }
            "#,
        );
        let line_dir = dir.path().join("line");
        let runner = SshRunner::for_test(&line_dir, &fake, "ssh-keygen", "/bin/false");
        let result = runner
            .connect(&profile(AuthMethod::Key {
                private_key: PathBuf::from("keys/Example/key"),
                public_key: PathBuf::from("keys/Example/key.pub"),
            }))
            .expect("ssh invocation");

        assert_eq!(result.exit_code, Some(7));
        assert_eq!(result.signal, None);
        assert_eq!(result.stderr_tail, "warning from ssh\n");

        let args = fs::read_to_string(record).expect("recorded args");
        let args: Vec<&str> = args.lines().collect();
        let expected_config = if Path::new("/etc/ssh/ssh_config").is_file() {
            "/etc/ssh/ssh_config"
        } else {
            "/dev/null"
        };
        assert!(args.windows(2).any(|pair| pair == ["-F", expected_config]));
        assert!(args.contains(&"ConnectTimeout=10"));
        let expected_known_hosts = format!(
            "UserKnownHostsFile={}",
            line_dir.join("known_hosts").display()
        );
        assert!(args.iter().any(|arg| *arg == expected_known_hosts));
        assert!(args.contains(&"GlobalKnownHostsFile=/dev/null"));
        assert!(args.contains(&"StrictHostKeyChecking=accept-new"));
        assert!(args.contains(&"LogLevel=ERROR"));
        assert!(args.contains(&"-tt"));
        assert!(args.windows(2).any(|pair| pair == ["-p", "2222"]));
        let expected_key = line_dir
            .join("keys/Example/key")
            .to_string_lossy()
            .into_owned();
        assert!(
            args.windows(2)
                .any(|pair| pair == ["-i", expected_key.as_str()])
        );
        assert!(args.contains(&"IdentityAgent=none"));
        assert_eq!(args.last().copied(), Some("alice@example.com"));
    }

    #[test]
    fn routine_first_connect_notice_is_suppressed_without_losing_errors() {
        let dir = TempDir::new().expect("temp dir");
        let record = dir.path().join("unused");
        let fake = fake_ssh(
            &dir,
            &record,
            r#"
            log_level=INFO
            for argument in "$@"; do
                case "$argument" in
                    LogLevel=*) log_level=${argument#LogLevel=} ;;
                esac
            done
            if [ "$log_level" != "ERROR" ]; then
                printf "Warning: Permanently added 'example.com' (ED25519) to the list of known hosts.\n" >&2
            fi
            printf 'Permission denied (publickey).\n' >&2
            printf 'ssh: connect to host example.com port 22: Connection refused\n' >&2
            printf 'WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED!\n' >&2
            exit 255
            "#,
        );
        let runner = SshRunner::for_test(dir.path().join("line"), fake, "ssh-keygen", "/bin/false");

        let result = runner
            .connect(&profile(AuthMethod::Key {
                private_key: PathBuf::from("keys/Example/key"),
                public_key: PathBuf::from("keys/Example/key.pub"),
            }))
            .expect("ssh invocation");

        assert_eq!(result.exit_code, Some(255));
        assert_eq!(
            result.stderr_tail,
            concat!(
                "Permission denied (publickey).\n",
                "ssh: connect to host example.com port 22: Connection refused\n",
                "WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED!\n",
            )
        );
        assert!(super::super::is_host_key_changed(&result));
    }

    #[test]
    fn password_profile_uses_forced_askpass_without_secret_in_arguments() {
        let dir = TempDir::new().expect("temp dir");
        let record = dir.path().join("record");
        let fake = fake_ssh(
            &dir,
            &record,
            r#"
            {
                printf '%s\n' "$@" > "$RECORD"
                {
                    printf 'ASKPASS=%s\n' "$SSH_ASKPASS"
                    printf 'ASKPASS_REQUIRE=%s\n' "$SSH_ASKPASS_REQUIRE"
                    env | grep '^LINE_INTERNAL_ASKPASS' || true
                } >> "$RECORD"
                exit 0
            }
            "#,
        );
        let line_dir = dir.path().join("line");
        let runner = SshRunner::for_test(&line_dir, &fake, "ssh-keygen", "/tmp/line-test-binary");
        let secret = "correct horse battery staple";
        let result = runner
            .connect(&profile(AuthMethod::Password {
                password: secret.into(),
            }))
            .expect("ssh invocation");

        assert!(result.success());
        let record = fs::read_to_string(record).expect("recorded invocation");
        let (arg_text, env_text) = record.split_once("ASKPASS=").expect("askpass marker");
        assert!(!arg_text.contains(secret));
        assert!(env_text.contains("ASKPASS_REQUIRE=force"));
        assert!(env_text.contains("/tmp/line-test-binary"));
        let password_line = env_text
            .lines()
            .find(|line| line.starts_with(PASSWORD_PREFIX))
            .expect("dynamic password variable");
        assert!(password_line.ends_with(secret));
        let token = password_line
            .split_once('=')
            .and_then(|(name, _)| name.strip_prefix(PASSWORD_PREFIX))
            .expect("password token");
        assert!(Uuid::parse_str(token).is_ok());
    }

    #[test]
    fn session_result_retains_only_the_final_sixteen_kibibytes_of_diagnostics() {
        let dir = TempDir::new().expect("temp dir");
        let record = dir.path().join("unused");
        let fake = fake_ssh(
            &dir,
            &record,
            r#"
            {
                head -c 17000 /dev/zero | tr '\000' x >&2
                printf 'THE-END\n' >&2
                exit 255
            }
            "#,
        );
        let runner = SshRunner::for_test(dir.path().join("line"), fake, "ssh-keygen", "/bin/false");
        let result = runner
            .connect(&profile(AuthMethod::Key {
                private_key: PathBuf::from("keys/Example/key"),
                public_key: PathBuf::from("keys/Example/key.pub"),
            }))
            .expect("ssh invocation");

        assert_eq!(result.stderr_tail.len(), STDERR_TAIL_LIMIT);
        assert!(result.stderr_tail.ends_with("THE-END\n"));
        assert_eq!(result.exit_code, Some(255));
    }

    #[test]
    fn signal_termination_is_returned_instead_of_becoming_a_runner_error() {
        let dir = TempDir::new().expect("temp dir");
        let record = dir.path().join("unused");
        let fake = fake_ssh(&dir, &record, "kill -TERM $$");
        let runner = SshRunner::for_test(dir.path().join("line"), fake, "ssh-keygen", "/bin/false");
        let result = runner
            .connect(&profile(AuthMethod::Key {
                private_key: PathBuf::from("keys/Example/key"),
                public_key: PathBuf::from("keys/Example/key.pub"),
            }))
            .expect("ssh invocation");

        assert_eq!(result.exit_code, None);
        assert_eq!(result.signal, Some(15));
        assert!(!result.success());
    }

    #[test]
    fn connection_prepares_private_line_and_known_hosts_permissions() {
        let dir = TempDir::new().expect("temp dir");
        let record = dir.path().join("unused");
        let fake = fake_ssh(&dir, &record, "exit 0");
        let line_dir = dir.path().join("line");
        let runner = SshRunner::for_test(&line_dir, fake, "ssh-keygen", "/bin/false");
        runner
            .connect(&profile(AuthMethod::Key {
                private_key: PathBuf::from("keys/Example/key"),
                public_key: PathBuf::from("keys/Example/key.pub"),
            }))
            .expect("ssh invocation");

        let directory_mode = fs::metadata(&line_dir)
            .expect("line directory")
            .permissions()
            .mode()
            & 0o777;
        let known_hosts_mode = fs::metadata(line_dir.join("known_hosts"))
            .expect("known_hosts")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(directory_mode, 0o700);
        assert_eq!(known_hosts_mode, 0o600);
    }

    #[test]
    fn bracketed_ipv6_input_is_passed_to_openssh_as_a_bare_literal() {
        let dir = TempDir::new().expect("temp dir");
        let record = dir.path().join("record");
        let fake = fake_ssh(&dir, &record, "printf '%s\\n' \"$@\" > \"$RECORD\"");
        let line_dir = dir.path().join("line");
        let runner = SshRunner::for_test(&line_dir, fake, "ssh-keygen", "/bin/false");
        let mut ipv6 = profile(AuthMethod::Key {
            private_key: PathBuf::from("keys/Example/key"),
            public_key: PathBuf::from("keys/Example/key.pub"),
        });
        ipv6.host = "[2001:db8::10]".into();

        runner.connect(&ipv6).expect("ssh invocation");
        let args = fs::read_to_string(record).expect("recorded args");
        assert_eq!(args.lines().last(), Some("alice@2001:db8::10"));
    }
}
