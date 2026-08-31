use std::collections::HashSet;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

use base64::Engine;
use fs2::FileExt;
use sha2::{Digest, Sha256};
use thiserror::Error;

use super::model::{AuthMethod, Profiles, profile_name_is_path_safe};
use super::unique_suffix;

/// A key pair stored below a [`KeyStore`] root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyPair {
    pub private_key: PathBuf,
    pub public_key: PathBuf,
    /// Lower-case SHA-256 of the canonical public-key identity.
    pub fingerprint: String,
}

impl KeyPair {
    pub fn auth_method(&self) -> AuthMethod {
        AuthMethod::Key {
            private_key: self.private_key.clone(),
            public_key: self.public_key.clone(),
        }
    }
}

#[derive(Debug, Error)]
pub enum KeyError {
    #[error("could not access {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("private key is empty")]
    EmptyPrivateKey,
    #[error("private key is encrypted; V1 requires an unencrypted private key")]
    EncryptedPrivateKey,
    #[error("private key format is invalid")]
    InvalidPrivateKey,
    #[error("public key format is invalid")]
    InvalidPublicKey,
    #[error("public key does not match the private key")]
    PublicKeyMismatch,
    #[error("unsupported managed key path: {0}")]
    InvalidPath(PathBuf),
    #[error("key is still used by {0} profile(s)")]
    InUse(usize),
    #[error("key does not exist: {0}")]
    NotFound(PathBuf),
    #[error("profile name cannot be used as a key directory: {0}")]
    InvalidProfileName(String),
}

pub type KeyResult<T> = std::result::Result<T, KeyError>;

#[derive(Clone, Debug)]
pub struct KeyStore {
    root: PathBuf,
}

impl KeyStore {
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn keys_dir(&self) -> PathBuf {
        self.root.join("keys")
    }

    /// Hidden canonical storage. Visible per-profile key files are hard links
    /// into this directory, so shared keys still occupy only one set of data
    /// blocks while the user-facing layout remains understandable.
    fn shared_dir(&self) -> PathBuf {
        self.keys_dir().join(".shared")
    }

    /// Import a private key file and, when supplied, a specific public-key
    /// file. Without an explicit public path, a sibling `.pub` is used when
    /// present; otherwise OpenSSH derives the public half. Identical public
    /// keys are deduplicated regardless of source filename.
    pub fn import_files(
        &self,
        private_source: impl AsRef<Path>,
        public_source: Option<&Path>,
    ) -> KeyResult<KeyPair> {
        self.ensure_layout()?;
        let private_source = private_source.as_ref();
        let private = fs::read(private_source).map_err(|source_error| KeyError::Io {
            path: private_source.to_owned(),
            source: source_error,
        })?;
        let public_hint = if let Some(public_source) = public_source {
            Some(
                fs::read_to_string(public_source).map_err(|source_error| KeyError::Io {
                    path: public_source.to_owned(),
                    source: source_error,
                })?,
            )
        } else {
            adjacent_public_key(private_source).and_then(|path| fs::read_to_string(path).ok())
        };
        self.import_bytes(&private, public_hint.as_deref())
    }

    /// Import pasted private-key text and an optional pasted public key.
    pub fn import_pasted(&self, private: &str, public: Option<&str>) -> KeyResult<KeyPair> {
        self.ensure_layout()?;
        self.import_bytes(
            private.as_bytes(),
            public.filter(|value| !value.trim().is_empty()),
        )
    }

    /// List valid, paired key files. Stray editor backups are skipped.
    pub fn list(&self) -> KeyResult<Vec<KeyPair>> {
        self.ensure_layout()?;
        self.with_key_lock(|store| store.list_locked())
    }

    fn list_locked(&self) -> KeyResult<Vec<KeyPair>> {
        let shared_dir = self.shared_dir();
        let entries = fs::read_dir(&shared_dir).map_err(|source| KeyError::Io {
            path: shared_dir.clone(),
            source,
        })?;
        let mut paths = entries
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| path.is_file())
            .filter(|path| is_canonical_private_key(path))
            .collect::<Vec<_>>();
        paths.sort();
        let mut candidates = Vec::new();
        for path in paths {
            let private = match fs::read(&path) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let derived = match derive_public_key(&self.root, &private) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let fingerprint = fingerprint_for_public(&derived)?;
            let public_path = path.with_file_name(format!(
                "{}{}",
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default(),
                ".pub"
            ));
            if public_path.exists() {
                let supplied = match fs::read_to_string(&public_path) {
                    Ok(value) => value,
                    Err(_) => continue,
                };
                if canonical_public_identity(&supplied).ok()
                    != canonical_public_identity(&derived).ok()
                {
                    continue;
                }
            } else {
                write_atomic_key_file(&public_path, ensure_newline(&derived).as_bytes())?;
            }
            set_mode_key(&path, 0o600)?;
            set_mode_key(&public_path, 0o600)?;
            let canonical = self.write_key_pair(
                &private,
                fs::read_to_string(&public_path).ok().as_deref(),
                &derived,
                fingerprint,
            )?;
            candidates.push(canonical);
        }
        let mut fingerprints = HashSet::new();
        let mut result = candidates
            .into_iter()
            .filter(|pair| fingerprints.insert(pair.fingerprint.clone()))
            .collect::<Vec<_>>();
        result.sort_by(|left, right| left.private_key.cmp(&right.private_key));
        sync_directory_key(&shared_dir)?;
        Ok(result)
    }

    /// Create the visible `keys/<profile name>/key(.pub)` mirror for a
    /// canonical or already materialized pair.
    pub fn materialize_for_profile(
        &self,
        profile_name: &str,
        pair: &KeyPair,
    ) -> KeyResult<KeyPair> {
        self.ensure_layout()?;
        if !profile_name_is_path_safe(profile_name) || profile_name.trim().is_empty() {
            return Err(KeyError::InvalidProfileName(profile_name.to_owned()));
        }
        self.with_key_lock(|store| {
            let canonical =
                store.canonicalize_pair_locked(&pair.private_key, Some(&pair.public_key))?;
            store.materialize_locked(profile_name, &canonical)
        })
    }

    /// Resolve a visible profile mirror or hidden canonical pair to the
    /// canonical shared entry.
    pub fn canonical_for_paths(
        &self,
        private_key: impl AsRef<Path>,
        public_key: Option<&Path>,
    ) -> KeyResult<KeyPair> {
        self.ensure_layout()?;
        self.with_key_lock(|store| store.canonicalize_pair_locked(private_key.as_ref(), public_key))
    }

    /// Remove visible per-profile directories that are referenced by neither
    /// the current document nor its valid recovery backup.
    pub fn prune_profile_mirrors(
        &self,
        current: &Profiles,
        backup: Option<&Profiles>,
    ) -> KeyResult<()> {
        self.ensure_layout()?;
        self.with_key_lock(|store| store.prune_profile_mirrors_locked(current, backup))
    }

    /// Compare managed private-key paths by inode, falling back to their
    /// canonical public identity.
    pub fn same_private_key(
        &self,
        left: impl AsRef<Path>,
        right: impl AsRef<Path>,
    ) -> KeyResult<bool> {
        let left = self.resolve(left.as_ref())?;
        let right = self.resolve(right.as_ref())?;
        if let (Ok(left_metadata), Ok(right_metadata)) = (fs::metadata(&left), fs::metadata(&right))
            && same_file(&left_metadata, &right_metadata)
        {
            return Ok(true);
        }
        let left_private = fs::read(&left).map_err(|source| KeyError::Io {
            path: left.clone(),
            source,
        })?;
        let right_private = fs::read(&right).map_err(|source| KeyError::Io {
            path: right.clone(),
            source,
        })?;
        let left_public = derive_public_key(&self.root, &left_private)?;
        let right_public = derive_public_key(&self.root, &right_private)?;
        Ok(canonical_public_identity(&left_public)? == canonical_public_identity(&right_public)?)
    }

    /// Resolve a managed canonical or per-profile key path.
    pub fn resolve(&self, relative: impl AsRef<Path>) -> KeyResult<PathBuf> {
        let relative = relative.as_ref();
        if !is_managed_key_relative(relative) {
            return Err(KeyError::InvalidPath(relative.to_owned()));
        }
        let path = self.root.join(relative);
        if !path.starts_with(self.keys_dir()) {
            return Err(KeyError::InvalidPath(relative.to_owned()));
        }
        Ok(path)
    }

    /// Delete a key pair only when no profile references either half.
    pub fn delete_if_unused(
        &self,
        private_key: impl AsRef<Path>,
        profiles: &Profiles,
    ) -> KeyResult<()> {
        self.ensure_layout()?;
        self.with_key_lock(|store| store.delete_if_unused_locked(private_key.as_ref(), profiles))
    }

    fn delete_if_unused_locked(&self, private_key: &Path, profiles: &Profiles) -> KeyResult<()> {
        let canonical = self.canonicalize_pair_locked(private_key, None)?;
        let pair = self.resolve(&canonical.private_key)?;
        let public = self.resolve(&canonical.public_key)?;
        let private_metadata = fs::metadata(&pair).map_err(|source| KeyError::Io {
            path: pair.clone(),
            source,
        })?;
        let public_metadata = fs::metadata(&public).map_err(|source| KeyError::Io {
            path: public.clone(),
            source,
        })?;
        let uses = profiles
            .profiles
            .iter()
            .filter(|profile| match &profile.auth {
                AuthMethod::Password { .. } => false,
                AuthMethod::Key {
                    private_key,
                    public_key,
                } => [private_key, public_key].into_iter().any(|relative| {
                    self.resolve(relative)
                        .ok()
                        .and_then(|path| fs::metadata(path).ok())
                        .is_some_and(|metadata| {
                            same_file(&metadata, &private_metadata)
                                || same_file(&metadata, &public_metadata)
                        })
                }),
            })
            .count();
        if uses > 0 {
            return Err(KeyError::InUse(uses));
        }
        fs::remove_file(&pair).map_err(|source| KeyError::Io {
            path: pair.clone(),
            source,
        })?;
        if public.exists() {
            fs::remove_file(&public).map_err(|source| KeyError::Io {
                path: public.clone(),
                source,
            })?;
        }
        self.remove_hard_link_mirrors_locked(&private_metadata, &public_metadata)?;
        sync_directory_key(&self.keys_dir())?;
        Ok(())
    }

    fn import_bytes(&self, private: &[u8], public_hint: Option<&str>) -> KeyResult<KeyPair> {
        validate_private_bytes(private)?;
        let derived = derive_public_key(&self.root, private)?;
        let derived_identity = canonical_public_identity(&derived)?;
        if let Some(public) = public_hint {
            let supplied_identity = canonical_public_identity(public)?;
            if supplied_identity != derived_identity {
                return Err(KeyError::PublicKeyMismatch);
            }
        }
        let fingerprint = fingerprint_for_identity(&derived_identity);
        self.with_key_lock(|store| {
            store.write_key_pair(private, public_hint, &derived, fingerprint)
        })
    }

    fn write_key_pair(
        &self,
        private: &[u8],
        public_hint: Option<&str>,
        derived: &str,
        fingerprint: String,
    ) -> KeyResult<KeyPair> {
        let private_name = format!("key-{fingerprint}");
        let private_path = self.shared_dir().join(&private_name);
        let public_path = self.shared_dir().join(format!("{private_name}.pub"));
        let public_contents = public_hint
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(derived.trim());

        // Do not replace a healthy canonical file: visible profile mirrors
        // are hard links to it, so preserving the inode is what makes
        // deduplication durable across later imports of the same key.
        if canonical_pair_matches(
            &self.root,
            &private_path,
            &public_path,
            &canonical_public_identity(derived)?,
        ) {
            set_mode_key(&private_path, 0o600)?;
            set_mode_key(&public_path, 0o600)?;
            return Ok(KeyPair {
                private_key: relative_to_root(&self.root, &private_path)?,
                public_key: relative_to_root(&self.root, &public_path)?,
                fingerprint,
            });
        }

        // Always replace through same-directory temporary files. This repairs
        // a previous interrupted write and makes concurrent imports of the
        // same fingerprint converge on a complete pair.
        write_atomic_key_file(&public_path, ensure_newline(public_contents).as_bytes())?;
        write_atomic_key_file(&private_path, private)?;
        sync_directory_key(&self.shared_dir())?;
        Ok(KeyPair {
            private_key: relative_to_root(&self.root, &private_path)?,
            public_key: relative_to_root(&self.root, &public_path)?,
            fingerprint,
        })
    }

    fn canonicalize_pair_locked(
        &self,
        private_key: &Path,
        public_key: Option<&Path>,
    ) -> KeyResult<KeyPair> {
        let private_relative = normalize_key_relative(private_key)?;
        let private_path = self.resolve(&private_relative)?;
        let private = fs::read(&private_path).map_err(|source| KeyError::Io {
            path: private_path.clone(),
            source,
        })?;
        validate_private_bytes(&private)?;
        let derived = derive_public_key(&self.root, &private)?;
        let public_hint = public_key
            .filter(|path| !path.as_os_str().is_empty())
            .map(|path| {
                let relative = normalize_key_relative(path)?;
                let resolved = self.resolve(&relative)?;
                fs::read_to_string(&resolved).map_err(|source| KeyError::Io {
                    path: resolved,
                    source,
                })
            })
            .transpose()?;
        if let Some(public) = public_hint.as_deref()
            && canonical_public_identity(public)? != canonical_public_identity(&derived)?
        {
            return Err(KeyError::PublicKeyMismatch);
        }
        let fingerprint = fingerprint_for_public(&derived)?;
        self.write_key_pair(&private, public_hint.as_deref(), &derived, fingerprint)
    }

    fn materialize_locked(&self, profile_name: &str, canonical: &KeyPair) -> KeyResult<KeyPair> {
        let private_source = self.resolve(&canonical.private_key)?;
        let public_source = self.resolve(&canonical.public_key)?;
        if !private_source.is_file() {
            return Err(KeyError::NotFound(canonical.private_key.clone()));
        }
        if !public_source.is_file() {
            return Err(KeyError::NotFound(canonical.public_key.clone()));
        }

        let directory = self.keys_dir().join(profile_name);
        create_private_dir_key(&directory)?;
        let private_path = directory.join("key");
        let public_path = directory.join("key.pub");
        replace_with_hard_link(&private_source, &private_path)?;
        replace_with_hard_link(&public_source, &public_path)?;
        set_mode_key(&private_path, 0o600)?;
        set_mode_key(&public_path, 0o600)?;
        sync_directory_key(&directory)?;
        sync_directory_key(&self.keys_dir())?;
        Ok(KeyPair {
            private_key: relative_to_root(&self.root, &private_path)?,
            public_key: relative_to_root(&self.root, &public_path)?,
            fingerprint: canonical.fingerprint.clone(),
        })
    }

    fn prune_profile_mirrors_locked(
        &self,
        current: &Profiles,
        backup: Option<&Profiles>,
    ) -> KeyResult<()> {
        let mut retained = HashSet::new();
        for profiles in std::iter::once(current).chain(backup) {
            for profile in &profiles.profiles {
                let AuthMethod::Key { private_key, .. } = &profile.auth else {
                    continue;
                };
                let mut components = private_key.components();
                if components.next() == Some(Component::Normal("keys".as_ref()))
                    && let Some(Component::Normal(directory)) = components.next()
                    && directory != ".shared"
                    && components.next().is_some()
                {
                    retained.insert(directory.to_os_string());
                }
            }
        }
        let keys_dir = self.keys_dir();
        for entry in fs::read_dir(&keys_dir).map_err(|source| KeyError::Io {
            path: keys_dir.clone(),
            source,
        })? {
            let entry = entry.map_err(|source| KeyError::Io {
                path: keys_dir.clone(),
                source,
            })?;
            let path = entry.path();
            if path.is_dir()
                && entry.file_name() != ".shared"
                && !retained.contains(&entry.file_name())
            {
                fs::remove_dir_all(&path).map_err(|source| KeyError::Io {
                    path: path.clone(),
                    source,
                })?;
            }
        }
        sync_directory_key(&keys_dir)
    }

    fn remove_hard_link_mirrors_locked(
        &self,
        private_metadata: &fs::Metadata,
        public_metadata: &fs::Metadata,
    ) -> KeyResult<()> {
        let keys_dir = self.keys_dir();
        for entry in fs::read_dir(&keys_dir).map_err(|source| KeyError::Io {
            path: keys_dir.clone(),
            source,
        })? {
            let entry = entry.map_err(|source| KeyError::Io {
                path: keys_dir.clone(),
                source,
            })?;
            let directory = entry.path();
            if !directory.is_dir() || entry.file_name() == ".shared" {
                continue;
            }
            for path in [directory.join("key"), directory.join("key.pub")] {
                let Ok(metadata) = fs::metadata(&path) else {
                    continue;
                };
                if same_file(&metadata, private_metadata) || same_file(&metadata, public_metadata) {
                    fs::remove_file(&path).map_err(|source| KeyError::Io {
                        path: path.clone(),
                        source,
                    })?;
                }
            }
            if fs::read_dir(&directory)
                .map(|mut entries| entries.next().is_none())
                .unwrap_or(false)
            {
                fs::remove_dir(&directory).map_err(|source| KeyError::Io {
                    path: directory,
                    source,
                })?;
            }
        }
        Ok(())
    }

    fn ensure_layout(&self) -> KeyResult<()> {
        create_private_dir_key(&self.root)?;
        create_private_dir_key(&self.keys_dir())?;
        create_private_dir_key(&self.shared_dir())
    }

    fn with_key_lock<T>(&self, operation: impl FnOnce(&Self) -> KeyResult<T>) -> KeyResult<T> {
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
        let _ = lock.unlock();
        result
    }
}

fn is_safe_relative(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && !path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
}

fn is_canonical_private_key(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    !name.ends_with(".pub") && is_canonical_key_name(name)
}

fn normalize_key_relative(path: &Path) -> KeyResult<PathBuf> {
    if !is_managed_key_relative(path) {
        return Err(KeyError::InvalidPath(path.to_owned()));
    }
    Ok(path.to_owned())
}

fn is_managed_key_relative(path: &Path) -> bool {
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

fn create_private_dir_key(path: &Path) -> KeyResult<()> {
    fs::create_dir_all(path).map_err(|source| KeyError::Io {
        path: path.to_owned(),
        source,
    })?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|source| KeyError::Io {
        path: path.to_owned(),
        source,
    })
}

fn set_mode_key(path: &Path, mode: u32) -> KeyResult<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|source| KeyError::Io {
        path: path.to_owned(),
        source,
    })
}

fn adjacent_public_key(source: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    let mut appended = OsString::from(source.as_os_str());
    appended.push(".pub");
    candidates.push(PathBuf::from(appended));
    candidates.push(source.with_extension("pub"));
    candidates.into_iter().find(|path| path.is_file())
}

fn relative_to_root(root: &Path, path: &Path) -> KeyResult<PathBuf> {
    path.strip_prefix(root)
        .map(Path::to_owned)
        .map_err(|_| KeyError::InvalidPath(path.to_owned()))
}

fn write_private_file(path: &Path, bytes: &[u8]) -> KeyResult<()> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true).mode(0o600);
    let mut file = options.open(path).map_err(|source| KeyError::Io {
        path: path.to_owned(),
        source,
    })?;
    file.write_all(bytes).map_err(|source| KeyError::Io {
        path: path.to_owned(),
        source,
    })?;
    file.sync_all().map_err(|source| KeyError::Io {
        path: path.to_owned(),
        source,
    })?;
    set_mode_key(path, 0o600)
}

fn write_atomic_key_file(path: &Path, bytes: &[u8]) -> KeyResult<()> {
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

fn replace_with_hard_link(source: &Path, destination: &Path) -> KeyResult<()> {
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
fn same_file(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    left.dev() == right.dev() && left.ino() == right.ino()
}

fn sync_directory_key(path: &Path) -> KeyResult<()> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|source| KeyError::Io {
            path: path.to_owned(),
            source,
        })
}

fn canonical_pair_matches(
    root: &Path,
    private_path: &Path,
    public_path: &Path,
    expected_identity: &str,
) -> bool {
    let Ok(private) = fs::read(private_path) else {
        return false;
    };
    let Ok(derived) = derive_public_key(root, &private) else {
        return false;
    };
    if canonical_public_identity(&derived).ok().as_deref() != Some(expected_identity) {
        return false;
    }
    let Ok(public) = fs::read_to_string(public_path) else {
        return false;
    };
    canonical_public_identity(&public).ok().as_deref() == Some(expected_identity)
}

fn ensure_newline(value: &str) -> String {
    if value.ends_with('\n') {
        value.to_owned()
    } else {
        format!("{value}\n")
    }
}

fn validate_private_bytes(bytes: &[u8]) -> KeyResult<()> {
    if bytes.is_empty() || bytes.iter().all(u8::is_ascii_whitespace) {
        return Err(KeyError::EmptyPrivateKey);
    }
    let text = String::from_utf8_lossy(bytes);
    if !text.contains("PRIVATE KEY") {
        return Err(KeyError::InvalidPrivateKey);
    }
    if text.contains("ENCRYPTED") || text.contains("Proc-Type: 4,ENCRYPTED") {
        return Err(KeyError::EncryptedPrivateKey);
    }
    if let Some(cipher) = openssh_cipher_name(&text)
        && cipher != "none"
    {
        return Err(KeyError::EncryptedPrivateKey);
    }
    Ok(())
}

fn openssh_cipher_name(text: &str) -> Option<String> {
    if !text.contains("BEGIN OPENSSH PRIVATE KEY") {
        return None;
    }
    let body: String = text
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .collect();
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(body)
        .ok()?;
    let magic = b"openssh-key-v1\0";
    if !decoded.starts_with(magic) {
        return None;
    }
    let mut cursor = magic.len();
    read_ssh_string(&decoded, &mut cursor).map(|bytes| String::from_utf8_lossy(bytes).into_owned())
}

fn read_ssh_string<'a>(bytes: &'a [u8], cursor: &mut usize) -> Option<&'a [u8]> {
    if bytes.len().saturating_sub(*cursor) < 4 {
        return None;
    }
    let length = u32::from_be_bytes(bytes[*cursor..*cursor + 4].try_into().ok()?) as usize;
    *cursor += 4;
    let end = (*cursor).checked_add(length)?;
    if end > bytes.len() {
        return None;
    }
    let value = &bytes[*cursor..end];
    *cursor = end;
    Some(value)
}

fn derive_public_key(root: &Path, private: &[u8]) -> KeyResult<String> {
    let temporary = root.join(format!(".keygen-{}", unique_suffix()));
    write_private_file(&temporary, private)?;
    let result = (|| {
        let output = Command::new("ssh-keygen")
            .args(["-y", "-P", "", "-f"])
            .arg(&temporary)
            .stdin(Stdio::null())
            .output()
            .map_err(|source| KeyError::Io {
                path: PathBuf::from("ssh-keygen"),
                source,
            })?;
        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr)
                .trim()
                .to_lowercase();
            if detail.contains("passphrase") || detail.contains("encrypted") {
                return Err(KeyError::EncryptedPrivateKey);
            }
            return Err(KeyError::InvalidPrivateKey);
        }
        let public = String::from_utf8(output.stdout).map_err(|_| KeyError::InvalidPublicKey)?;
        canonical_public_identity(&public)?;
        Ok(public.trim().to_owned())
    })();
    let _ = fs::remove_file(&temporary);
    result
}

fn canonical_public_identity(public: &str) -> KeyResult<String> {
    let known_types = [
        "ssh-rsa",
        "ssh-dss",
        "ssh-ed25519",
        "ecdsa-sha2-nistp256",
        "ecdsa-sha2-nistp384",
        "ecdsa-sha2-nistp521",
        "sk-ssh-ed25519@openssh.com",
        "sk-ecdsa-sha2-nistp256@openssh.com",
        "rsa-sha2-256",
        "rsa-sha2-512",
    ];
    for line in public.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        for index in 0..fields.len().saturating_sub(1) {
            if known_types.contains(&fields[index])
                && base64::engine::general_purpose::STANDARD
                    .decode(fields[index + 1])
                    .is_ok()
            {
                return Ok(format!("{} {}", fields[index], fields[index + 1]));
            }
        }
    }
    Err(KeyError::InvalidPublicKey)
}

fn fingerprint_for_identity(identity: &str) -> String {
    Sha256::digest(identity.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Return a stable lower-case SHA-256 fingerprint for an authorized-key line.
fn fingerprint_for_public(public: &str) -> KeyResult<String> {
    Ok(fingerprint_for_identity(&canonical_public_identity(
        public,
    )?))
}
