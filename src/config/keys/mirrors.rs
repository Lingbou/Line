use std::fs;
use std::path::{Component, Path};

use crate::config::{AuthMethod, Profiles, profile_name_is_path_safe};

use super::path::relative_to_root;
use super::storage::{
    create_private_dir_key, replace_with_hard_link, same_file, set_mode_key, sync_directory_key,
};
use super::{KeyError, KeyPair, KeyResult, KeyStore};

impl KeyStore {
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
        let mut retained = std::collections::HashSet::new();
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
}
