use std::fs::{self, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use fs2::FileExt;

use crate::private_fs;

use super::{KeyError, KeyResult, KeyStore, unique_suffix};

pub(super) fn create_private_dir_key(path: &Path) -> KeyResult<()> {
    private_fs::create_private_dir(path).map_err(|source| KeyError::Io {
        path: path.to_owned(),
        source,
    })
}

pub(super) fn set_mode_key(path: &Path, mode: u32) -> KeyResult<()> {
    private_fs::set_private_mode(path, mode).map_err(|source| KeyError::Io {
        path: path.to_owned(),
        source,
    })
}

pub(super) fn write_private_file(path: &Path, bytes: &[u8]) -> KeyResult<()> {
    private_fs::write_new_private_file(path, bytes).map_err(|source| KeyError::Io {
        path: path.to_owned(),
        source,
    })
}

pub(super) fn write_atomic_key_file(path: &Path, bytes: &[u8]) -> KeyResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| KeyError::InvalidPath(path.to_owned()))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| KeyError::InvalidPath(path.to_owned()))?;
    let temporary = parent.join(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        unique_suffix()
    ));
    write_private_file(&temporary, bytes)?;
    let result = (|| {
        fs::rename(&temporary, path).map_err(|source| KeyError::Io {
            path: path.to_owned(),
            source,
        })?;
        set_mode_key(path, 0o600)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub(super) fn replace_with_hard_link(source: &Path, destination: &Path) -> KeyResult<()> {
    if let (Ok(source_metadata), Ok(destination_metadata)) =
        (fs::metadata(source), fs::metadata(destination))
        && same_file(&source_metadata, &destination_metadata)
    {
        return Ok(());
    }
    let parent = destination
        .parent()
        .ok_or_else(|| KeyError::InvalidPath(destination.to_owned()))?;
    let file_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| KeyError::InvalidPath(destination.to_owned()))?;
    let temporary = parent.join(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        unique_suffix()
    ));
    fs::hard_link(source, &temporary).map_err(|source_error| KeyError::Io {
        path: temporary.clone(),
        source: source_error,
    })?;
    let result = fs::rename(&temporary, destination).map_err(|source_error| KeyError::Io {
        path: destination.to_owned(),
        source: source_error,
    });
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(unix)]
pub(super) fn same_file(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    left.dev() == right.dev() && left.ino() == right.ino()
}

pub(super) fn sync_directory_key(path: &Path) -> KeyResult<()> {
    private_fs::sync_directory(path).map_err(|source| KeyError::Io {
        path: path.to_owned(),
        source,
    })
}

impl KeyStore {
    pub(super) fn with_key_lock<T>(
        &self,
        operation: impl FnOnce(&Self) -> KeyResult<T>,
    ) -> KeyResult<T> {
        let path = self.root.join("keys.lock");
        let lock = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .mode(0o600)
            .open(&path)
            .map_err(|source| KeyError::Io {
                path: path.clone(),
                source,
            })?;
        lock.lock_exclusive().map_err(|source| KeyError::Io {
            path: path.clone(),
            source,
        })?;
        let result = operation(self);
        let _ = FileExt::unlock(&lock);
        result
    }
}
