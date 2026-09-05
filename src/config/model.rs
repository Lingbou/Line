use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

/// A saved SSH connection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    /// Stable identity used by the UI when a list is reordered.
    pub id: String,
    /// Human-readable connection label. Names are unique case-insensitively.
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthMethod,
}

impl Profile {
    /// Construct a profile using an explicit id (useful when importing data).
    pub fn with_id(
        id: impl Into<String>,
        name: impl Into<String>,
        host: impl Into<String>,
        port: u16,
        username: impl Into<String>,
        auth: AuthMethod,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            host: host.into(),
            port,
            username: username.into(),
            auth,
        }
    }

    pub fn validate(&self) -> std::result::Result<(), ValidationError> {
        if self.id.trim().is_empty() {
            return Err(ValidationError::EmptyId);
        }
        if self.name.trim().is_empty() {
            return Err(ValidationError::EmptyName);
        }
        if !profile_name_is_path_safe(&self.name) {
            return Err(ValidationError::InvalidName);
        }
        if self.host.trim().is_empty() {
            return Err(ValidationError::EmptyHost);
        }
        if self
            .host
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
        {
            return Err(ValidationError::InvalidHost);
        }
        if self.username.trim().is_empty() {
            return Err(ValidationError::EmptyUsername);
        }
        if self
            .username
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
        {
            return Err(ValidationError::InvalidUsername);
        }
        if self.port == 0 {
            return Err(ValidationError::InvalidPort);
        }
        if let AuthMethod::Password { password } = &self.auth
            && password.contains(['\0', '\r', '\n'])
        {
            return Err(ValidationError::InvalidPassword);
        }
        if let AuthMethod::Key {
            private_key,
            public_key,
        } = &self.auth
        {
            validate_key_relative_path(private_key)?;
            validate_key_relative_path(public_key)?;
            let profile_directory = PathBuf::from("keys").join(&self.name);
            if private_key != &profile_directory.join("key")
                || public_key != &profile_directory.join("key.pub")
            {
                return Err(ValidationError::InvalidKeyLayout(self.name.clone()));
            }
        }
        Ok(())
    }
}

/// Authentication data for a profile.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AuthMethod {
    Password {
        password: String,
    },
    Key {
        /// Relative to `~/.line` as `keys/<profile name>/key`.
        private_key: PathBuf,
        /// Relative to `~/.line` as `keys/<profile name>/key.pub`.
        public_key: PathBuf,
    },
}

impl std::fmt::Debug for AuthMethod {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Password { .. } => formatter
                .debug_struct("Password")
                .field("password", &"<redacted>")
                .finish(),
            Self::Key {
                private_key,
                public_key,
            } => formatter
                .debug_struct("Key")
                .field("private_key", private_key)
                .field("public_key", public_key)
                .finish(),
        }
    }
}

impl AuthMethod {
    pub fn password(password: impl Into<String>) -> Self {
        Self::Password {
            password: password.into(),
        }
    }
}

/// The JSON envelope stored in `profiles.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profiles {
    pub schema_version: u32,
    pub profiles: Vec<Profile>,
}

impl Default for Profiles {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            profiles: Vec::new(),
        }
    }
}

impl Profiles {
    pub fn validate(&self) -> std::result::Result<(), ValidationError> {
        if self.schema_version != CURRENT_SCHEMA_VERSION {
            return Err(ValidationError::UnsupportedSchema(self.schema_version));
        }
        let mut ids = HashSet::with_capacity(self.profiles.len());
        let mut names = HashSet::with_capacity(self.profiles.len());
        for profile in &self.profiles {
            profile.validate()?;
            if !ids.insert(profile.id.as_str()) {
                return Err(ValidationError::DuplicateId(profile.id.clone()));
            }
            let normalized = normalize_name(&profile.name);
            if !names.insert(normalized) {
                return Err(ValidationError::DuplicateName(profile.name.clone()));
            }
        }
        Ok(())
    }

    /// Add a profile, enforcing id and case-insensitive name uniqueness.
    pub fn insert(&mut self, profile: Profile) -> std::result::Result<(), ValidationError> {
        profile.validate()?;
        if self.profiles.iter().any(|p| p.id == profile.id) {
            return Err(ValidationError::DuplicateId(profile.id));
        }
        if self
            .profiles
            .iter()
            .any(|p| profile_names_equal(&p.name, &profile.name))
        {
            return Err(ValidationError::DuplicateName(profile.name));
        }
        self.profiles.push(profile);
        Ok(())
    }

    /// Replace a profile with the same id.
    pub fn update(&mut self, profile: Profile) -> std::result::Result<(), ValidationError> {
        profile.validate()?;
        let index = self
            .profiles
            .iter()
            .position(|p| p.id == profile.id)
            .ok_or_else(|| ValidationError::UnknownProfile(profile.id.clone()))?;
        if self
            .profiles
            .iter()
            .enumerate()
            .any(|(i, p)| i != index && profile_names_equal(&p.name, &profile.name))
        {
            return Err(ValidationError::DuplicateName(profile.name));
        }
        self.profiles[index] = profile;
        Ok(())
    }

    pub fn remove(&mut self, id: &str) -> std::result::Result<Profile, ValidationError> {
        let index = self
            .profiles
            .iter()
            .position(|p| p.id == id)
            .ok_or_else(|| ValidationError::UnknownProfile(id.to_owned()))?;
        Ok(self.profiles.remove(index))
    }

    pub fn by_id(&self, id: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.id == id)
    }
}

fn normalize_name(name: &str) -> String {
    name.trim().to_lowercase()
}

/// Whether a profile name can safely be used verbatim as one directory name.
pub fn profile_name_is_path_safe(name: &str) -> bool {
    !name.chars().any(char::is_control)
        && !name.contains(std::path::MAIN_SEPARATOR)
        && !matches!(name, "." | "..")
        && !name.eq_ignore_ascii_case(".shared")
}

pub fn profile_names_equal(left: &str, right: &str) -> bool {
    normalize_name(left) == normalize_name(right)
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ValidationError {
    #[error("profile id cannot be empty")]
    EmptyId,
    #[error("profile name cannot be empty")]
    EmptyName,
    #[error("profile name must be a safe directory name and cannot be .shared")]
    InvalidName,
    #[error("host cannot be empty")]
    EmptyHost,
    #[error("host cannot contain whitespace or control characters")]
    InvalidHost,
    #[error("username cannot be empty")]
    EmptyUsername,
    #[error("username cannot contain whitespace or control characters")]
    InvalidUsername,
    #[error("port must be between 1 and 65535")]
    InvalidPort,
    #[error("password cannot contain NUL or line breaks")]
    InvalidPassword,
    #[error("duplicate profile id: {0}")]
    DuplicateId(String),
    #[error("duplicate profile name: {0}")]
    DuplicateName(String),
    #[error("profile does not exist: {0}")]
    UnknownProfile(String),
    #[error("profile changed in another line process; reload and try again: {0}")]
    StaleProfile(String),
    #[error("imported key file is missing: {0}")]
    MissingKey(PathBuf),
    #[error("unsupported profile schema version: {0}")]
    UnsupportedSchema(u32),
    #[error("key path must be relative: {0}")]
    AbsoluteKeyPath(PathBuf),
    #[error("key path contains a parent traversal: {0}")]
    TraversalKeyPath(PathBuf),
    #[error("key path cannot be empty")]
    EmptyKeyPath,
    #[error("key path must be inside the keys directory: {0}")]
    KeyOutsideDirectory(PathBuf),
    #[error("key paths for profile {0:?} must be keys/<profile name>/key and key.pub")]
    InvalidKeyLayout(String),
}

fn validate_key_relative_path(path: &Path) -> std::result::Result<(), ValidationError> {
    if path.as_os_str().is_empty() {
        return Err(ValidationError::EmptyKeyPath);
    }
    if path.is_absolute() {
        return Err(ValidationError::AbsoluteKeyPath(path.to_owned()));
    }
    if path
        .components()
        .any(|component| component == Component::ParentDir)
    {
        return Err(ValidationError::TraversalKeyPath(path.to_owned()));
    }
    let mut components = path.components();
    if components.next() != Some(Component::Normal("keys".as_ref())) || components.next().is_none()
    {
        return Err(ValidationError::KeyOutsideDirectory(path.to_owned()));
    }
    Ok(())
}
