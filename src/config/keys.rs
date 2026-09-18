use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use super::model::AuthMethod;
use super::unique_suffix;

mod import;
mod mirrors;
mod openssh;
mod path;
mod storage;

use path::is_managed_key_relative;
use storage::create_private_dir_key;

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

    fn ensure_layout(&self) -> KeyResult<()> {
        create_private_dir_key(&self.root)?;
        create_private_dir_key(&self.keys_dir())?;
        create_private_dir_key(&self.shared_dir())
    }
}

fn ensure_newline(value: &str) -> String {
    if value.ends_with('\n') {
        value.to_owned()
    } else {
        format!("{value}\n")
    }
}
