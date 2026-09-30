use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

/// One intermediate SSH endpoint used before the target.
///
/// A hop either spells out an endpoint or points at another saved profile.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JumpHop {
    Endpoint(EndpointHop),
    Profile(ProfileHop),
}

impl JumpHop {
    /// Build an endpoint hop.
    #[must_use]
    pub fn endpoint(username: Option<String>, host: impl Into<String>, port: u16) -> Self {
        Self::Endpoint(EndpointHop {
            username,
            host: host.into(),
            port,
        })
    }

    #[must_use]
    pub fn as_endpoint(&self) -> Option<&EndpointHop> {
        match self {
            Self::Endpoint(endpoint) => Some(endpoint),
            Self::Profile(_) => None,
        }
    }

    #[must_use]
    pub fn as_profile(&self) -> Option<&ProfileHop> {
        match self {
            Self::Endpoint(_) => None,
            Self::Profile(reference) => Some(reference),
        }
    }

    fn validate(&self) -> std::result::Result<(), ValidationError> {
        match self {
            Self::Endpoint(endpoint) => endpoint.validate(),
            Self::Profile(reference) => {
                if reference.profile_id.trim().is_empty() {
                    return Err(ValidationError::InvalidJumpChain(
                        "jump profile reference cannot be empty".into(),
                    ));
                }
                if reference
                    .profile_id
                    .chars()
                    .any(|character| character.is_control())
                {
                    return Err(ValidationError::InvalidJumpChain(
                        "jump profile reference cannot contain control characters".into(),
                    ));
                }
                Ok(())
            }
        }
    }
}

/// A jump hop written out as `user@host:port`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndpointHop {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    pub host: String,
    pub port: u16,
}

impl EndpointHop {
    #[must_use]
    pub fn destination(&self) -> String {
        match &self.username {
            Some(username) => format!("{username}@{}", self.host),
            None => self.host.clone(),
        }
    }

    #[must_use]
    pub fn authority(&self) -> String {
        let host = if self.host.contains(':') && self.port != 22 {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        let endpoint = match &self.username {
            Some(username) => format!("{username}@{host}"),
            None => host,
        };
        if self.port == 22 {
            endpoint
        } else {
            format!("{endpoint}:{}", self.port)
        }
    }

    fn validate(&self) -> std::result::Result<(), ValidationError> {
        if self.host.trim().is_empty() {
            return Err(ValidationError::InvalidJumpChain(
                "jump host cannot be empty".into(),
            ));
        }
        if self
            .host
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
        {
            return Err(ValidationError::InvalidJumpChain(
                "jump host cannot contain whitespace or control characters".into(),
            ));
        }
        if let Some(username) = &self.username {
            if username.trim().is_empty() {
                return Err(ValidationError::InvalidJumpChain(
                    "jump username cannot be empty".into(),
                ));
            }
            if username
                .chars()
                .any(|character| character.is_control() || character.is_whitespace())
            {
                return Err(ValidationError::InvalidJumpChain(
                    "jump username cannot contain whitespace or control characters".into(),
                ));
            }
        }
        if self.port == 0 {
            return Err(ValidationError::InvalidJumpChain(
                "jump port must be between 1 and 65535".into(),
            ));
        }
        Ok(())
    }
}

/// A jump hop that points at another saved profile.
///
/// The reference is an id rather than a name so that renaming the referenced
/// profile does not break the chain. The referenced profile contributes its
/// endpoint and its key; its own jump chain is not expanded, so references
/// cannot form cycles.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileHop {
    pub profile_id: String,
}

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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub jump_chain: Vec<JumpHop>,
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
            jump_chain: Vec::new(),
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
        for hop in &self.jump_chain {
            hop.validate()?;
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
/// Whether a profile name is safe as a single directory component.
///
/// The rule is the union of the platforms Line ships on rather than the rule
/// of the host, because a saved profile has to keep working when the same
/// `~/.line` is used from another platform. Windows accepts `/` as a separator
/// just like `\`, keeps a set of characters out of file names, and reserves
/// device names such as `NUL`, so all of those are rejected everywhere.
pub fn profile_name_is_path_safe(name: &str) -> bool {
    !name.chars().any(char::is_control)
        && !name.contains(['/', '\\'])
        && !name.contains(['<', '>', ':', '"', '|', '?', '*'])
        && !name.ends_with([' ', '.'])
        && !matches!(name, "." | "..")
        && !name.eq_ignore_ascii_case(".shared")
        && !is_reserved_device_name(name)
}

/// Windows reserves these names, with or without an extension, for devices.
fn is_reserved_device_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name);
    if stem.eq_ignore_ascii_case("CON")
        || stem.eq_ignore_ascii_case("PRN")
        || stem.eq_ignore_ascii_case("AUX")
        || stem.eq_ignore_ascii_case("NUL")
    {
        return true;
    }
    let bytes = stem.as_bytes();
    bytes.len() == 4
        && (stem[..3].eq_ignore_ascii_case("COM") || stem[..3].eq_ignore_ascii_case("LPT"))
        && bytes[3].is_ascii_digit()
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
    #[error("invalid jump chain: {0}")]
    InvalidJumpChain(String),
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

#[cfg(test)]
mod profile_name_tests {
    use super::*;

    #[test]
    fn accepts_names_that_are_safe_everywhere() {
        for name in ["Production", "prod-web-01", "a.b", "生产服务器", "_hidden"] {
            assert!(profile_name_is_path_safe(name), "{name} should be accepted");
        }
    }

    #[test]
    fn rejects_path_separators_from_either_platform() {
        for name in ["prod/root", "prod\\root", "a/b\\c"] {
            assert!(
                !profile_name_is_path_safe(name),
                "{name} should be rejected"
            );
        }
    }

    #[test]
    fn rejects_windows_reserved_names_and_characters() {
        for name in [
            "CON", "nul", "AUX.txt", "COM1", "com9", "LPT0", "prod:1", "a<b", "a|b", "a?b", "a*b",
            "a\"b",
        ] {
            assert!(
                !profile_name_is_path_safe(name),
                "{name} should be rejected"
            );
        }
    }

    #[test]
    fn rejects_names_windows_cannot_end_with() {
        assert!(!profile_name_is_path_safe("prod "));
        assert!(!profile_name_is_path_safe("prod."));
    }
}

#[cfg(test)]
mod jump_hop_tests {
    use super::*;

    #[test]
    fn endpoint_hops_written_before_references_still_load() {
        let json = r#"{"username":"root","host":"10.77.0.2","port":2222}"#;
        let hop: JumpHop = serde_json::from_str(json).expect("legacy endpoint hop");
        let endpoint = hop.as_endpoint().expect("endpoint form");
        assert_eq!(endpoint.username.as_deref(), Some("root"));
        assert_eq!(endpoint.host, "10.77.0.2");
        assert_eq!(endpoint.port, 2222);
        assert!(hop.as_profile().is_none());
    }

    #[test]
    fn profile_references_round_trip() {
        let hop = JumpHop::Profile(ProfileHop {
            profile_id: "e7f0e5d4".into(),
        });
        let json = serde_json::to_string(&hop).expect("serialize hop");
        assert_eq!(json, r#"{"profile_id":"e7f0e5d4"}"#);
        let parsed: JumpHop = serde_json::from_str(&json).expect("parse hop");
        assert_eq!(parsed, hop);
    }

    #[test]
    fn profile_references_reject_an_empty_id() {
        let hop = JumpHop::Profile(ProfileHop {
            profile_id: "  ".into(),
        });
        assert!(matches!(
            hop.validate(),
            Err(ValidationError::InvalidJumpChain(message))
                if message == "jump profile reference cannot be empty"
        ));
    }

    #[test]
    fn endpoint_hops_still_reject_bad_endpoints() {
        assert!(
            JumpHop::endpoint(Some("root".into()), "", 22)
                .validate()
                .is_err()
        );
        assert!(JumpHop::endpoint(None, "host", 0).validate().is_err());
    }
}
