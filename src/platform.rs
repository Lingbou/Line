use std::{
    env,
    ffi::OsString,
    io,
    path::{Path, PathBuf},
    process::{Child, Command},
};

#[cfg(unix)]
use std::fs;

use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum PlatformError {
    #[error("HOME is not set; cannot locate ~/.line")]
    HomeNotSet,
}

#[cfg(unix)]
pub(crate) struct InterruptGuard {
    old_int: libc::sighandler_t,
    old_quit: libc::sighandler_t,
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

/// Windows delivers console control events rather than POSIX signals, and
/// `ssh.exe` owns the console while it runs, so there is nothing to guard.
#[cfg(not(unix))]
pub(crate) struct InterruptGuard;

/// Platform decisions that are not simply POSIX.
///
/// Everything the supported Unix platforms share lives in this trait as a
/// default implementation, so a new platform only states what actually
/// differs. Linux and macOS differ in exactly one place today: how the
/// descendants of a running child are discovered.
pub(crate) trait Platform: Send + Sync {
    fn config_root(&self) -> Result<PathBuf, PlatformError> {
        config_root_from(|key| env::var_os(key))
    }

    fn ssh_program(&self) -> PathBuf {
        PathBuf::from("ssh")
    }

    fn ssh_keygen_program(&self) -> PathBuf {
        PathBuf::from("ssh-keygen")
    }

    /// The administrator's OpenSSH client policy.
    ///
    /// Both platforms install it at the same path; when it is missing Line
    /// points OpenSSH at the null device so the user's own `~/.ssh/config`
    /// still cannot change what a saved profile means.
    fn system_ssh_config(&self) -> PathBuf {
        #[cfg(unix)]
        {
            let path = Path::new("/etc/ssh/ssh_config");
            if path.is_file() {
                path.to_owned()
            } else {
                PathBuf::from("/dev/null")
            }
        }
        #[cfg(windows)]
        {
            let path = program_data().join("ssh").join("ssh_config");
            if path.is_file() {
                path
            } else {
                PathBuf::from("NUL")
            }
        }
    }

    fn null_device(&self) -> PathBuf {
        #[cfg(unix)]
        {
            PathBuf::from("/dev/null")
        }
        #[cfg(windows)]
        {
            PathBuf::from("NUL")
        }
    }

    /// Windows has no POSIX mode bits. The ADR records that v0.2 relies on the
    /// user-profile ACL there instead of pretending to tighten anything.
    fn set_private_mode(&self, path: &Path, mode: u32) -> io::Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(mode))
        }
        #[cfg(windows)]
        {
            let _ = (path, mode);
            Ok(())
        }
    }

    fn reset_child_signals(&self, command: &mut Command) {
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
        #[cfg(windows)]
        {
            let _ = command;
        }
    }

    fn install_interrupt_guard(&self) -> InterruptGuard {
        #[cfg(unix)]
        {
            let old_int = unsafe { libc::signal(libc::SIGINT, libc::SIG_IGN) };
            let old_quit = unsafe { libc::signal(libc::SIGQUIT, libc::SIG_IGN) };
            InterruptGuard { old_int, old_quit }
        }
        #[cfg(windows)]
        {
            InterruptGuard
        }
    }

    fn is_interrupt_signal(&self, signal: Option<i32>) -> bool {
        #[cfg(unix)]
        {
            matches!(signal, Some(libc::SIGINT) | Some(libc::SIGQUIT))
        }
        #[cfg(windows)]
        {
            // Windows child processes report an exit code, never a signal.
            let _ = signal;
            false
        }
    }

    /// Whether a saved password can be handed to OpenSSH on this platform.
    ///
    /// The plaintext password travels through `SSH_ASKPASS`, which Windows
    /// OpenSSH does not implement, so a password profile is refused there
    /// rather than left to hang on an interactive prompt.
    fn supports_saved_passwords(&self) -> bool {
        cfg!(unix)
    }

    fn terminate_child_tree(&self, child: &mut Child);
}

#[cfg(target_os = "linux")]
pub(crate) fn current() -> &'static dyn Platform {
    &LINUX
}

#[cfg(target_os = "macos")]
pub(crate) fn current() -> &'static dyn Platform {
    &MACOS
}

#[cfg(windows)]
pub(crate) fn current() -> &'static dyn Platform {
    &WINDOWS
}

#[cfg(target_os = "linux")]
static LINUX: LinuxPlatform = LinuxPlatform;

#[cfg(target_os = "macos")]
static MACOS: MacPlatform = MacPlatform;

#[cfg(target_os = "linux")]
struct LinuxPlatform;

#[cfg(target_os = "linux")]
impl Platform for LinuxPlatform {
    fn terminate_child_tree(&self, child: &mut Child) {
        terminate_child_tree(child, collect_linux_descendants);
    }
}

#[cfg(windows)]
static WINDOWS: WindowsPlatform = WindowsPlatform;

#[cfg(target_os = "macos")]
struct MacPlatform;

#[cfg(target_os = "macos")]
impl Platform for MacPlatform {
    fn terminate_child_tree(&self, child: &mut Child) {
        terminate_child_tree(child, collect_macos_descendants);
    }
}

#[cfg(windows)]
struct WindowsPlatform;

#[cfg(windows)]
impl Platform for WindowsPlatform {
    fn terminate_child_tree(&self, child: &mut Child) {
        // `taskkill /T` walks the tree the same way the Unix collectors do,
        // and `/F` matches the SIGKILL the other platforms send.
        let _ = Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

fn config_root_from(get_env: impl Fn(&str) -> Option<OsString>) -> Result<PathBuf, PlatformError> {
    if let Some(path) = get_env("LINE_CONFIG_DIR") {
        return Ok(PathBuf::from(path));
    }
    // Windows sets USERPROFILE; the Unix shells and Git Bash set HOME. Both
    // name the same user directory, so either is accepted on either platform.
    let home = get_env("HOME")
        .or_else(|| get_env("USERPROFILE"))
        .ok_or(PlatformError::HomeNotSet)?;
    Ok(PathBuf::from(home).join(".line"))
}

#[cfg(windows)]
fn program_data() -> PathBuf {
    env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:\\ProgramData"))
}

/// Kill a child and everything it started.
///
/// A cancelled `ssh` can leave `ProxyCommand` descendants holding a duplicate
/// of stderr, which would block terminal restoration, so the tree is collected
/// first and killed leaves-first.
#[cfg(unix)]
fn terminate_child_tree(child: &mut Child, collect: fn(i32, &mut Vec<i32>)) {
    let root = child.id() as i32;
    let mut descendants = Vec::new();
    collect(root, &mut descendants);
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

/// macOS has no `/proc`; libproc answers the same question.
#[cfg(target_os = "macos")]
fn collect_macos_descendants(parent: i32, descendants: &mut Vec<i32>) {
    let mut buffer = [0i32; 256];
    let capacity = std::mem::size_of_val(&buffer) as i32;
    // SAFETY: the buffer is a live, correctly sized array of pid_t values and
    // the size passed matches it, which is what libproc requires.
    let written = unsafe { libc::proc_listchildpids(parent, buffer.as_mut_ptr().cast(), capacity) };
    if written <= 0 {
        return;
    }
    let count = (written as usize / std::mem::size_of::<i32>()).min(buffer.len());
    for pid in buffer[..count].iter().copied() {
        if pid <= 0 || descendants.contains(&pid) {
            continue;
        }
        descendants.push(pid);
        collect_macos_descendants(pid, descendants);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_root_honors_explicit_override() {
        let root = config_root_from(|key| match key {
            "LINE_CONFIG_DIR" => Some(OsString::from("/tmp/custom-line")),
            "HOME" => Some(OsString::from("/tmp/home")),
            _ => None,
        })
        .expect("config root");

        assert_eq!(root, PathBuf::from("/tmp/custom-line"));
    }

    #[cfg(unix)]
    #[test]
    fn recognizes_terminal_interrupt_signals() {
        assert!(current().is_interrupt_signal(Some(libc::SIGINT)));
        assert!(current().is_interrupt_signal(Some(libc::SIGQUIT)));
        assert!(!current().is_interrupt_signal(Some(libc::SIGTERM)));
        assert!(!current().is_interrupt_signal(None));
    }

    #[cfg(windows)]
    #[test]
    fn windows_reports_no_posix_interrupts_and_no_saved_passwords() {
        assert!(!current().is_interrupt_signal(Some(libc::SIGINT)));
        assert!(!current().is_interrupt_signal(None));
        assert!(!current().supports_saved_passwords());
    }

    #[test]
    fn user_profile_is_used_when_home_is_absent() {
        let root = config_root_from(|key| match key {
            "HOME" => None,
            "USERPROFILE" => Some(OsString::from(r"C:\Users\tester")),
            _ => None,
        })
        .expect("config root");

        assert_eq!(root, PathBuf::from(r"C:\Users\tester").join(".line"));
    }

    #[test]
    fn config_root_falls_back_to_home() {
        let root = config_root_from(|key| match key {
            "HOME" => Some(OsString::from("/tmp/home")),
            _ => None,
        })
        .expect("config root");

        assert_eq!(root, PathBuf::from("/tmp/home/.line"));
    }
}
