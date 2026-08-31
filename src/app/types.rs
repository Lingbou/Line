use std::path::PathBuf;

/// The default SSH port used when a form is opened.
pub const DEFAULT_PORT: u16 = 22;

/// A value being assembled by the profile form.
///
/// For key authentication, [`KeySource`] tells the adapter whether `value` is
/// a file path, a relative path to an existing key, or pasted private-key
/// text. `public_key` is an optional source path for an imported key and
/// optional pasted public-key text for a pasted key. The storage adapter can
/// derive the public half with `ssh-keygen` when it is absent.
#[derive(Clone, PartialEq, Eq)]
pub enum AuthDraft {
    Password {
        password: String,
    },
    Key {
        source: KeySource,
        value: String,
        public_key: Option<String>,
    },
}

impl std::fmt::Debug for AuthDraft {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Password { .. } => formatter.write_str("Password { password: <redacted> }"),
            Self::Key { source, .. } => formatter
                .debug_struct("Key")
                .field("source", source)
                .field("value", &"<redacted>")
                .field("public_key", &"<redacted>")
                .finish(),
        }
    }
}

/// Whether a key field is a path to import, a path already in `~/.line/keys`,
/// or literal pasted key material.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeySource {
    Import,
    Existing,
    Paste,
}

impl KeySource {
    pub const ALL: [Self; 3] = [Self::Import, Self::Existing, Self::Paste];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Import => "Import new key",
            Self::Existing => "Choose existing key",
            Self::Paste => "Paste key",
        }
    }

    pub(super) fn next(self, delta: i32) -> Self {
        let current = Self::ALL
            .iter()
            .position(|candidate| *candidate == self)
            .unwrap_or(0) as i32;
        let next = (current + delta).rem_euclid(Self::ALL.len() as i32) as usize;
        Self::ALL[next]
    }
}

/// Whether a form creates or edits a profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveMode {
    Add,
    Edit,
}

/// Data emitted when the user submits the profile form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileDraft {
    pub mode: SaveMode,
    pub id: Option<String>,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthDraft,
}

/// Actions that require work outside the TUI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppAction {
    /// No external operation is requested.
    None,
    /// Replace the TUI with an OpenSSH process for this profile.
    Connect(String),
    /// Persist a newly-created or edited profile. Key import/paste is
    /// intentionally left to the storage adapter.
    Save(ProfileDraft),
    /// Remove a profile after the adapter has deleted it from storage.
    Delete(String),
    /// Ask the filesystem adapter to complete an imported private-key path.
    CompletePath(String),
    /// Delete an imported key after the user confirmed it is unused.
    DeleteKey(PathBuf),
    /// End the event loop.
    Quit,
}

/// A reusable key discovered by the storage layer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyChoice {
    pub label: String,
    pub private_key: PathBuf,
    pub public_key: PathBuf,
    pub used_by: usize,
}

/// High-level screen currently visible.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Browse,
    Form,
    ConfirmDelete,
    ConfirmDeleteKey,
    Error,
}

/// Form field receiving keyboard input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormField {
    Name,
    Host,
    Port,
    Username,
    Authentication,
    Password,
    ShowPassword,
    KeySource,
    KeyValue,
    PublicKey,
}

impl FormField {
    pub(super) fn is_text(self) -> bool {
        matches!(
            self,
            Self::Name
                | Self::Host
                | Self::Port
                | Self::Username
                | Self::Password
                | Self::KeyValue
                | Self::PublicKey
        )
    }
}
