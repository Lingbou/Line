use std::collections::HashSet;
use std::fs;
use std::path::Path;

use super::openssh::{
    canonical_pair_matches, canonical_public_identity, derive_public_key, fingerprint_for_identity,
    fingerprint_for_public, validate_private_bytes,
};
use super::path::{
    adjacent_public_key, is_canonical_private_key, normalize_key_relative, relative_to_root,
};
use super::storage::{same_file, set_mode_key, sync_directory_key, write_atomic_key_file};
use super::{KeyError, KeyPair, KeyResult, KeyStore, ensure_newline};

impl KeyStore {
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

    /// Compare managed private-key paths by inode, falling back to their
    /// canonical public identity.
    pub fn same_private_key(
        &self,
        left: impl AsRef<Path>,
        right: impl AsRef<Path>,
    ) -> KeyResult<bool> {
        let left = self.resolve(left.as_ref())?;
        let right = self.resolve(right.as_ref())?;
        if same_file(&left, &right) {
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

    pub(super) fn canonicalize_pair_locked(
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
}
