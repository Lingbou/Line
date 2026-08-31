use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use fs2::FileExt;
use thiserror::Error;

use super::keys::{KeyError, KeyStore};
use super::model::{Profiles, ValidationError};
use super::unique_suffix;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("HOME is not set; cannot locate ~/.line")]
    HomeNotSet,
    #[error("could not access {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("invalid JSON in {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error(transparent)]
    Validation(#[from] ValidationError),
    #[error("could not lock {path}: {source}")]
    Lock {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("unsupported key operation: {0}")]
    Key(#[from] KeyError),
}

pub type Result<T> = std::result::Result<T, ConfigError>;

#[derive(Debug, Clone)]
pub struct ConfigStore {
    root: PathBuf,
}

impl ConfigStore {
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn in_home() -> Result<Self> {
        if let Some(path) = std::env::var_os("LINE_CONFIG_DIR") {
            return Ok(Self::at(PathBuf::from(path)));
        }
        let home = std::env::var_os("HOME").ok_or(ConfigError::HomeNotSet)?;
        Ok(Self::at(PathBuf::from(home).join(".line")))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn profiles_path(&self) -> PathBuf {
        self.root.join("profiles.json")
    }

    pub fn backup_path(&self) -> PathBuf {
        self.root.join("profiles.json.bak")
    }

    fn lock_path(&self) -> PathBuf {
        self.root.join("profiles.json.lock")
    }

    fn keys_path(&self) -> PathBuf {
        self.root.join("keys")
    }

    pub fn key_store(&self) -> KeyStore {
        KeyStore::at(self.root.clone())
    }

    /// Delete an imported key pair while holding the same lock used for
    /// profile mutations, so another Line instance cannot add a reference
    /// between the usage check and deletion.
    pub fn delete_key_if_unused(&self, private_key: impl AsRef<Path>) -> Result<()> {
        let private_key = private_key.as_ref().to_owned();
        self.with_lock(|store| {
            let profiles = store.read_profiles()?;
            let mut references = profiles.clone();
            if let Ok(backup) = store.load_backup() {
                references.profiles.extend(backup.profiles);
            }
            store
                .key_store()
                .delete_if_unused(&private_key, &references)?;
            Ok(())
        })
    }

    /// Load profiles. A malformed file is returned as an error; it is never
    /// silently replaced with the backup or an empty document.
    pub fn load(&self) -> Result<Profiles> {
        self.with_lock(|store| store.read_profiles())
    }

    fn read_profiles(&self) -> Result<Profiles> {
        self.ensure_layout()?;
        let path = self.profiles_path();
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Profiles::default()),
            Err(source) => return Err(io_error(path, source)),
        };
        // Restrict access before parsing so a malformed file containing a
        // password is not left world-readable.
        set_mode(&path, 0o600)?;
        let profiles: Profiles =
            serde_json::from_slice(&bytes).map_err(|source| ConfigError::Json {
                path: path.clone(),
                source,
            })?;
        profiles.validate()?;
        Ok(profiles)
    }

    /// Load the last successfully saved document from `profiles.json.bak`.
    /// This is intentionally explicit; [`load`](Self::load) never falls back
    /// automatically when the current file is damaged.
    pub fn load_backup(&self) -> Result<Profiles> {
        self.ensure_layout()?;
        let path = self.backup_path();
        let bytes = fs::read(&path).map_err(|source| io_error(path.clone(), source))?;
        set_mode(&path, 0o600)?;
        let profiles: Profiles =
            serde_json::from_slice(&bytes).map_err(|source| ConfigError::Json {
                path: path.clone(),
                source,
            })?;
        profiles.validate()?;
        Ok(profiles)
    }

    /// Replace the current document with the validated backup. The backup is
    /// left in place so pressing restore twice is harmless.
    pub fn restore_backup(&self) -> Result<Profiles> {
        self.with_lock(|store| {
            let profiles = store.load_backup()?;
            let bytes = canonical_json(&profiles)?;
            let path = store.profiles_path();
            // Keep the damaged/unsupported primary available for inspection.
            // Copying happens before replacement, and the source backup is
            // never moved or overwritten by recovery.
            if path.exists() {
                let corrupt = store.root.join(format!(
                    "profiles.json.corrupt-{}-{}",
                    std::process::id(),
                    unique_suffix()
                ));
                fs::copy(&path, &corrupt).map_err(|source| io_error(corrupt.clone(), source))?;
                set_mode(&corrupt, 0o600)?;
                File::open(&corrupt)
                    .and_then(|file| file.sync_all())
                    .map_err(|source| io_error(corrupt, source))?;
            }
            let temporary = store.root.join(format!(
                ".profiles.restore.{}.{}.tmp",
                std::process::id(),
                unique_suffix()
            ));
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(&temporary)
                .map_err(|source| io_error(temporary.clone(), source))?;
            file.write_all(&bytes)
                .and_then(|_| file.sync_all())
                .map_err(|source| io_error(temporary.clone(), source))?;
            drop(file);
            fs::rename(&temporary, &path).map_err(|source| {
                let _ = fs::remove_file(&temporary);
                io_error(path.clone(), source)
            })?;
            set_mode(&path, 0o600)?;
            sync_directory(&store.root)?;
            store.read_profiles()
        })
    }

    /// Save a complete profile set atomically, retaining the prior JSON as
    /// `profiles.json.bak` when one exists.
    pub fn save(&self, profiles: &Profiles) -> Result<()> {
        profiles.validate()?;
        self.with_lock(|store| {
            // Refuse to replace a malformed or unsupported document. Recovery
            // must be an explicit user action so valuable data is not lost.
            if store.profiles_path().exists() {
                store.read_profiles()?;
            }
            store.write_locked(profiles)?;
            store.prune_key_layout_locked(profiles)
        })
    }

    /// Lock, reload, mutate, and save while holding the lock. This prevents
    /// two instances from clobbering one another for ordinary CRUD operations.
    pub fn modify<F>(&self, modify: F) -> Result<Profiles>
    where
        F: FnOnce(&mut Profiles) -> std::result::Result<(), ValidationError>,
    {
        self.with_lock(|store| {
            let mut profiles = store.read_profiles()?;
            modify(&mut profiles)?;
            profiles.validate()?;
            store.write_locked(&profiles)?;
            store.prune_key_layout_locked(&profiles)?;
            Ok(profiles)
        })
    }

    /// Lock, reload, and mutate profiles while allowing the mutation to
    /// materialize key files through this same store. This is used by runtime
    /// saves so stale/duplicate checks happen before a visible key mirror can
    /// be replaced.
    pub fn modify_with<F>(&self, modify: F) -> Result<Profiles>
    where
        F: FnOnce(&Self, &mut Profiles) -> Result<()>,
    {
        self.with_lock(|store| {
            let mut profiles = store.read_profiles()?;
            modify(store, &mut profiles)?;
            profiles.validate()?;
            store.write_locked(&profiles)?;
            store.prune_key_layout_locked(&profiles)?;
            Ok(profiles)
        })
    }

    fn prune_key_layout_locked(&self, profiles: &Profiles) -> Result<()> {
        let backup = self.load_backup().ok();
        self.key_store()
            .prune_profile_mirrors(profiles, backup.as_ref())?;
        Ok(())
    }

    fn ensure_layout(&self) -> Result<()> {
        create_private_dir(&self.root)?;
        create_private_dir(&self.keys_path())
    }

    fn with_lock<T, F>(&self, operation: F) -> Result<T>
    where
        F: FnOnce(&Self) -> Result<T>,
    {
        self.ensure_layout()?;
        let path = self.lock_path();
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .open(&path)
            .map_err(|source| ConfigError::Lock {
                path: path.clone(),
                source,
            })?;
        set_mode(&path, 0o600)?;
        lock.lock_exclusive().map_err(|source| ConfigError::Lock {
            path: path.clone(),
            source,
        })?;
        let result = operation(self);
        let _ = lock.unlock();
        result
    }

    fn write_locked(&self, profiles: &Profiles) -> Result<()> {
        let bytes = canonical_json(profiles)?;
        let path = self.profiles_path();
        let temporary = self.root.join(format!(
            ".profiles.json.{}.{}.tmp",
            std::process::id(),
            unique_suffix()
        ));
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|source| io_error(temporary.clone(), source))?;
        let write_result = (|| {
            file.write_all(&bytes)
                .map_err(|source| io_error(temporary.clone(), source))?;
            file.sync_all()
                .map_err(|source| io_error(temporary.clone(), source))?;
            Ok::<(), ConfigError>(())
        })();
        drop(file);
        if let Err(error) = write_result {
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }

        // Prepare the previous version only after the new JSON is completely
        // durable. A failed new write therefore cannot mutate `.bak`.
        let backup_tmp = if path.exists() {
            let backup_tmp = self.root.join(format!(
                ".profiles.json.bak.{}.{}",
                std::process::id(),
                unique_suffix()
            ));
            if let Err(source) = fs::copy(&path, &backup_tmp) {
                let _ = fs::remove_file(&backup_tmp);
                let _ = fs::remove_file(&temporary);
                return Err(io_error(path.clone(), source));
            }
            if let Err(error) = set_mode(&backup_tmp, 0o600).and_then(|()| {
                File::open(&backup_tmp)
                    .and_then(|file| file.sync_all())
                    .map_err(|source| io_error(backup_tmp.clone(), source))
            }) {
                let _ = fs::remove_file(&backup_tmp);
                let _ = fs::remove_file(&temporary);
                return Err(error);
            }
            Some(backup_tmp)
        } else {
            None
        };

        fs::rename(&temporary, &path).map_err(|source| {
            let _ = fs::remove_file(&temporary);
            if let Some(backup_tmp) = &backup_tmp {
                let _ = fs::remove_file(backup_tmp);
            }
            io_error(path.clone(), source)
        })?;
        if let Some(backup_tmp) = backup_tmp
            && let Err(source) = fs::rename(&backup_tmp, self.backup_path())
        {
            // Roll the primary back to the copied old version so a failed save
            // never reports an error after silently committing new data.
            let rollback = fs::rename(&backup_tmp, &path);
            return match rollback {
                Ok(()) => Err(io_error(self.backup_path(), source)),
                Err(rollback_error) => {
                    let _ = fs::remove_file(&backup_tmp);
                    Err(io_error(path.clone(), rollback_error))
                }
            };
        }
        set_mode(&path, 0o600)?;
        sync_directory(&self.root)?;
        Ok(())
    }
}

fn create_private_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).map_err(|source| io_error(path.to_owned(), source))?;
    set_mode(path, 0o700)
}

fn set_mode(path: &Path, mode: u32) -> Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .map_err(|source| io_error(path.to_owned(), source))
}

fn io_error(path: PathBuf, source: io::Error) -> ConfigError {
    ConfigError::Io { path, source }
}

fn canonical_json(profiles: &Profiles) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(profiles).map_err(|source| ConfigError::Json {
        path: PathBuf::from("profiles.json"),
        source,
    })?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|source| io_error(path.to_owned(), source))
}
