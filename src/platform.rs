use std::{
    env,
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
    process::{Child, Command},
};

use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum PlatformError {
    #[error("HOME is not set; cannot locate ~/.line")]
    HomeNotSet,
}

pub(crate) trait Platform: Send + Sync {
    fn config_root(&self) -> Result<PathBuf, PlatformError>;
    fn ssh_program(&self) -> PathBuf;
    fn ssh_keygen_program(&self) -> PathBuf;
    fn system_ssh_config(&self) -> PathBuf;
    fn null_device(&self) -> PathBuf;
    fn set_private_mode(&self, path: &Path, mode: u32) -> io::Result<()>;
    fn reset_child_signals(&self, command: &mut Command);
    fn terminate_child_tree(&self, child: &mut Child);
}

pub(crate) fn current() -> &'static dyn Platform {
    &LINUX
}

static LINUX: LinuxPlatform = LinuxPlatform;

struct LinuxPlatform;

impl Platform for LinuxPlatform {
    fn config_root(&self) -> Result<PathBuf, PlatformError> {
        config_root_from(|key| env::var_os(key))
    }

    fn ssh_program(&self) -> PathBuf {
        PathBuf::from("ssh")
    }

    fn ssh_keygen_program(&self) -> PathBuf {
        PathBuf::from("ssh-keygen")
    }

    fn system_ssh_config(&self) -> PathBuf {
        let path = Path::new("/etc/ssh/ssh_config");
        if path.is_file() {
            path.to_owned()
        } else {
            PathBuf::from("/dev/null")
        }
    }

    fn null_device(&self) -> PathBuf {
        PathBuf::from("/dev/null")
    }

    fn set_private_mode(&self, path: &Path, mode: u32) -> io::Result<()> {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
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
    }

    fn terminate_child_tree(&self, child: &mut Child) {
        terminate_linux_child_tree(child);
    }
}

fn config_root_from(get_env: impl Fn(&str) -> Option<OsString>) -> Result<PathBuf, PlatformError> {
    if let Some(path) = get_env("LINE_CONFIG_DIR") {
        return Ok(PathBuf::from(path));
    }
    let home = get_env("HOME").ok_or(PlatformError::HomeNotSet)?;
    Ok(PathBuf::from(home).join(".line"))
}

fn terminate_linux_child_tree(child: &mut Child) {
    let root = child.id() as i32;
    let mut descendants = Vec::new();
    collect_linux_descendants(root, &mut descendants);
    unsafe {
        libc::kill(root, libc::SIGKILL);
        for pid in descendants.into_iter().rev() {
            libc::kill(pid, libc::SIGKILL);
        }
    }
}

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
