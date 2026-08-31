use std::fs::{self, OpenOptions};
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, ExitStatus};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};

use crate::config::Profile;

use super::{STDERR_TAIL_LIMIT, SessionResult, SshError};

pub(super) fn prepare_line_directory(line_dir: &Path) -> Result<(), SshError> {
    fs::create_dir_all(line_dir).map_err(|source| SshError::CreateLineDirectory {
        path: line_dir.to_path_buf(),
        source,
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(line_dir, fs::Permissions::from_mode(0o700)).map_err(|source| {
            SshError::CreateLineDirectory {
                path: line_dir.to_path_buf(),
                source,
            }
        })?;

        let known_hosts = line_dir.join("known_hosts");
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&known_hosts)
            .and_then(|file| file.set_permissions(fs::Permissions::from_mode(0o600)))
            .map_err(|source| SshError::CreateLineDirectory {
                path: line_dir.to_path_buf(),
                source,
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

pub(super) fn spawn_stderr_reader(mut stderr: ChildStderr) -> JoinHandle<io::Result<Vec<u8>>> {
    thread::spawn(move || {
        let mut tail = Vec::with_capacity(STDERR_TAIL_LIMIT);
        let mut buffer = [0_u8; 8192];

        loop {
            let bytes_read = stderr.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }

            // Forward diagnostics as they arrive. A closed parent stderr
            // (for example, a test harness) must not prevent us from draining
            // the pipe and allowing ssh to exit.
            if io::stderr().is_terminal() {
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
                terminate_child_tree(child);
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

#[cfg(target_os = "linux")]
fn terminate_child_tree(child: &mut Child) {
    let root = child.id() as i32;
    let mut descendants = Vec::new();
    collect_linux_descendants(root, &mut descendants);
    // Stop the root first so it cannot create more descendants, then tear down
    // the already-discovered ProxyCommand/wrapper tree from leaves upward.
    unsafe {
        libc::kill(root, libc::SIGKILL);
        for pid in descendants.into_iter().rev() {
            libc::kill(pid, libc::SIGKILL);
        }
    }
}

#[cfg(target_os = "linux")]
fn collect_linux_descendants(parent: i32, descendants: &mut Vec<i32>) {
    let path = format!("/proc/{parent}/task/{parent}/children");
    let Ok(children) = fs::read_to_string(path) else {
        return;
    };
    for child in children
        .split_whitespace()
        .filter_map(|value| value.parse::<i32>().ok())
    {
        if descendants.contains(&child) {
            continue;
        }
        descendants.push(child);
        collect_linux_descendants(child, descendants);
    }
}

#[cfg(not(target_os = "linux"))]
fn terminate_child_tree(child: &mut Child) {
    let _ = child.kill();
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
