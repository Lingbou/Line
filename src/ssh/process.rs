#[cfg(unix)]
use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, ExitStatus};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};

use crate::config::Profile;
use crate::platform;
use crate::private_fs;

use super::{STDERR_TAIL_LIMIT, SessionResult, SshError};

pub(super) fn prepare_line_directory(line_dir: &Path) -> Result<(), SshError> {
    private_fs::create_private_dir(line_dir).map_err(|source| SshError::CreateLineDirectory {
        path: line_dir.to_path_buf(),
        source,
    })?;

    #[cfg(unix)]
    {
        let known_hosts = line_dir.join("known_hosts");
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&known_hosts)
            .map_err(|source| SshError::CreateLineDirectory {
                path: line_dir.to_path_buf(),
                source,
            })?;
        private_fs::set_private_mode(&known_hosts, 0o600).map_err(|source| {
            SshError::CreateLineDirectory {
                path: line_dir.to_path_buf(),
                source,
            }
        })?;
    }

    Ok(())
}

pub(super) fn resolve_path(line_dir: &Path, path: &Path) -> PathBuf {
    line_dir.join(path)
}

pub(super) fn destination(profile: &Profile) -> String {
    // OpenSSH accepts bare IPv6 literals in destination position. Brackets
    // belong to the convenient `[address]:port` input syntax, not to the
    // actual hostname passed to ssh.
    let host = profile.host.trim();
    let host = host
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(host)
        .to_owned();

    if profile.username.trim().is_empty() {
        host
    } else {
        format!("{}@{host}", profile.username.trim())
    }
}

pub(super) fn spawn_stderr_reader(
    mut stderr: ChildStderr,
    forward_live: bool,
) -> JoinHandle<io::Result<Vec<u8>>> {
    thread::spawn(move || {
        let mut tail = Vec::with_capacity(STDERR_TAIL_LIMIT);
        let mut buffer = [0_u8; 8192];

        loop {
            let bytes_read = stderr.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }

            // Forward diagnostics as they arrive. This must not depend on
            // stderr being a terminal: direct CLI users may redirect it to a
            // file or pipe. A closed parent stderr must not prevent us from
            // draining the pipe and allowing ssh to exit.
            if forward_live {
                let mut output = io::stderr().lock();
                let _ = output.write_all(&buffer[..bytes_read]);
                let _ = output.flush();
            }

            if bytes_read >= STDERR_TAIL_LIMIT {
                tail.clear();
                tail.extend_from_slice(&buffer[bytes_read - STDERR_TAIL_LIMIT..bytes_read]);
            } else {
                let overflow = tail
                    .len()
                    .saturating_add(bytes_read)
                    .saturating_sub(STDERR_TAIL_LIMIT);
                if overflow > 0 {
                    tail.drain(..overflow);
                }
                tail.extend_from_slice(&buffer[..bytes_read]);
            }
        }

        Ok(tail)
    })
}

pub(super) fn wait_for_child(
    child: &mut Child,
    cancel: Option<&AtomicBool>,
) -> Result<(ExitStatus, bool), SshError> {
    if let Some(cancel) = cancel {
        let mut cancelled = false;
        loop {
            if cancel.load(Ordering::Relaxed) && !cancelled {
                platform::current().terminate_child_tree(child);
                cancelled = true;
            }
            match child.try_wait().map_err(SshError::Wait)? {
                Some(status) => return Ok((status, cancelled)),
                None => thread::sleep(std::time::Duration::from_millis(25)),
            }
        }
    }
    child
        .wait()
        .map(|status| (status, false))
        .map_err(SshError::Wait)
}

pub(super) fn collect_stderr(reader: JoinHandle<io::Result<Vec<u8>>>) -> Result<String, SshError> {
    let bytes = reader
        .join()
        .map_err(|_| SshError::StderrReaderPanicked)?
        .map_err(SshError::ReadStderr)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

pub(super) fn session_result(status: ExitStatus, stderr_tail: String) -> SessionResult {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        SessionResult {
            exit_code: status.code(),
            signal: status.signal(),
            stderr_tail,
        }
    }

    #[cfg(not(unix))]
    {
        SessionResult {
            exit_code: status.code(),
            signal: None,
            stderr_tail,
        }
    }
}

pub(super) fn exit_status_parts(status: ExitStatus) -> (Option<i32>, Option<i32>) {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        (status.code(), status.signal())
    }

    #[cfg(not(unix))]
    {
        (status.code(), None)
    }
}

pub(super) fn bounded_lossy(bytes: &[u8]) -> String {
    let start = bytes.len().saturating_sub(STDERR_TAIL_LIMIT);
    String::from_utf8_lossy(&bytes[start..]).into_owned()
}
