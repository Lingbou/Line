use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

use crate::config::profile_name_is_path_safe;

use super::{KeyError, KeyResult};

pub(super) fn is_safe_relative(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && !path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
}

pub(super) fn is_canonical_private_key(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    !name.ends_with(".pub") && is_canonical_key_name(name)
}

pub(super) fn normalize_key_relative(path: &Path) -> KeyResult<PathBuf> {
    if !is_managed_key_relative(path) {
        return Err(KeyError::InvalidPath(path.to_owned()));
    }
    Ok(path.to_owned())
}

pub(super) fn is_managed_key_relative(path: &Path) -> bool {
    if !is_safe_relative(path) {
        return false;
    }
    let components = path.components().collect::<Vec<_>>();
    let [
        Component::Normal(keys),
        Component::Normal(directory),
        Component::Normal(filename),
    ] = components.as_slice()
    else {
        return false;
    };
    if *keys != "keys" {
        return false;
    }
    let Some(directory) = directory.to_str() else {
        return false;
    };
    let Some(filename) = filename.to_str() else {
        return false;
    };
    if directory == ".shared" {
        return is_canonical_key_name(filename);
    }
    !directory.is_empty()
        && profile_name_is_path_safe(directory)
        && matches!(filename, "key" | "key.pub")
}

fn is_canonical_key_name(name: &str) -> bool {
    let private_name = name.strip_suffix(".pub").unwrap_or(name);
    private_name
        .strip_prefix("key-")
        .is_some_and(|fingerprint| {
            fingerprint.len() == 64 && fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
}

pub(super) fn adjacent_public_key(source: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    let mut appended = OsString::from(source.as_os_str());
    appended.push(".pub");
    candidates.push(PathBuf::from(appended));
    candidates.push(source.with_extension("pub"));
    candidates.into_iter().find(|path| path.is_file())
}

pub(super) fn relative_to_root(root: &Path, path: &Path) -> KeyResult<PathBuf> {
    path.strip_prefix(root)
        .map(Path::to_owned)
        .map_err(|_| KeyError::InvalidPath(path.to_owned()))
}
