use crate::config::{AuthMethod, Profile, profile_name_is_path_safe, profile_names_equal};

use super::{
    AuthDraft, DEFAULT_PORT, FormField, KeySource, ProfileDraft, SaveMode,
    helpers::{parse_endpoint_shorthand, path_to_string},
};

impl AuthDraft {
    fn from_auth(auth: &AuthMethod) -> Self {
        match auth {
            AuthMethod::Password { password } => Self::Password {
                password: password.clone(),
            },
            AuthMethod::Key {
                private_key,
                public_key,
            } => Self::Key {
                source: KeySource::Existing,
                value: path_to_string(private_key),
                public_key: Some(path_to_string(public_key)),
            },
        }
    }
}

/// Mutable data rendered by the form screen.
#[derive(Clone, PartialEq, Eq)]
pub struct FormState {
    pub mode: SaveMode,
    pub profile_id: Option<String>,
    pub name: String,
    pub host: String,
    pub port: String,
    pub username: String,
    pub auth: AuthDraft,
    pub field: FormField,
    pub cursor: usize,
    pub show_password: bool,
    /// A validation error local to the form. It is rendered below the fields
    /// and does not leave the form, so a typo can be fixed immediately.
    pub validation_error: Option<String>,
    pub(super) original_password: Option<String>,
}

impl std::fmt::Debug for FormState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FormState")
            .field("mode", &self.mode)
            .field("profile_id", &self.profile_id)
            .field("name", &self.name)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("auth", &self.auth)
            .field("field", &self.field)
            .field("cursor", &self.cursor)
            .field("show_password", &self.show_password)
            .field("validation_error", &self.validation_error)
            .finish()
    }
}

impl FormState {
    pub(super) fn add(default_name: String) -> Self {
        let cursor = default_name.chars().count();
        Self {
            mode: SaveMode::Add,
            profile_id: None,
            name: default_name,
            host: String::new(),
            port: DEFAULT_PORT.to_string(),
            username: "root".to_owned(),
            auth: AuthDraft::Password {
                password: String::new(),
            },
            field: FormField::Name,
            cursor,
            show_password: false,
            validation_error: None,
            original_password: None,
        }
    }

    pub(super) fn edit(profile: &Profile) -> Self {
        let (auth, original_password) = match &profile.auth {
            AuthMethod::Password { password } => (
                AuthDraft::Password {
                    password: String::new(),
                },
                Some(password.clone()),
            ),
            auth => (AuthDraft::from_auth(auth), None),
        };
        Self {
            mode: SaveMode::Edit,
            profile_id: Some(profile.id.clone()),
            name: profile.name.clone(),
            host: profile.host.clone(),
            port: profile.port.to_string(),
            username: profile.username.clone(),
            auth,
            field: FormField::Name,
            cursor: profile.name.chars().count(),
            show_password: false,
            validation_error: None,
            original_password,
        }
    }

    pub(super) fn fields(&self) -> Vec<FormField> {
        let mut fields = vec![
            FormField::Name,
            FormField::Username,
            FormField::Host,
            FormField::Port,
            FormField::Authentication,
        ];
        match &self.auth {
            AuthDraft::Password { .. } => {
                fields.push(FormField::Password);
                fields.push(FormField::ShowPassword);
            }
            AuthDraft::Key { source, .. } => {
                fields.push(FormField::KeySource);
                if matches!(source, KeySource::Import | KeySource::Paste) {
                    fields.push(FormField::KeyValue);
                    fields.push(FormField::PublicKey);
                } else {
                    fields.push(FormField::KeyValue);
                }
            }
        }
        fields
    }

    pub(super) fn set_field(&mut self, field: FormField) {
        self.field = field;
        self.cursor = self
            .current_text()
            .map(|text| text.chars().count())
            .unwrap_or(0);
    }

    pub(super) fn current_text(&self) -> Option<&str> {
        match self.field {
            FormField::Name => Some(&self.name),
            FormField::Host => Some(&self.host),
            FormField::Port => Some(&self.port),
            FormField::Username => Some(&self.username),
            FormField::Password => match &self.auth {
                AuthDraft::Password { password } => Some(password),
                _ => None,
            },
            FormField::KeyValue => match &self.auth {
                AuthDraft::Key { value, .. } => Some(value),
                _ => None,
            },
            FormField::PublicKey => match &self.auth {
                AuthDraft::Key { public_key, .. } => public_key.as_deref(),
                _ => None,
            },
            FormField::Authentication | FormField::ShowPassword | FormField::KeySource => None,
        }
    }

    pub(super) fn current_text_mut(&mut self) -> Option<&mut String> {
        match self.field {
            FormField::Name => Some(&mut self.name),
            FormField::Host => Some(&mut self.host),
            FormField::Port => Some(&mut self.port),
            FormField::Username => Some(&mut self.username),
            FormField::Password => match &mut self.auth {
                AuthDraft::Password { password } => Some(password),
                _ => None,
            },
            FormField::KeyValue => match &mut self.auth {
                AuthDraft::Key { value, .. } => Some(value),
                _ => None,
            },
            FormField::PublicKey => match &mut self.auth {
                AuthDraft::Key { public_key, .. } => {
                    if public_key.is_none() {
                        *public_key = Some(String::new());
                    }
                    public_key.as_mut()
                }
                _ => None,
            },
            FormField::Authentication | FormField::ShowPassword | FormField::KeySource => None,
        }
    }

    pub(super) fn toggle_auth(&mut self, delta: i32) {
        let next = match (&self.auth, delta >= 0) {
            (AuthDraft::Password { .. }, true) => AuthDraft::Key {
                source: KeySource::Existing,
                value: String::new(),
                public_key: None,
            },
            (AuthDraft::Key { .. }, false) => AuthDraft::Password {
                password: String::new(),
            },
            (AuthDraft::Password { .. }, false) => AuthDraft::Key {
                source: KeySource::Existing,
                value: String::new(),
                public_key: None,
            },
            (AuthDraft::Key { .. }, true) => AuthDraft::Password {
                password: String::new(),
            },
        };
        self.auth = next;
        self.set_field(FormField::Authentication);
    }

    pub(super) fn draft(&self, profiles: &[Profile]) -> Result<ProfileDraft, String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err("Name cannot be empty".into());
        }
        if !profile_name_is_path_safe(name) {
            return Err("Name cannot contain '/' or use a reserved directory name".into());
        }
        if profiles.iter().any(|profile| {
            profile_names_equal(&profile.name, name)
                && self.profile_id.as_deref() != Some(profile.id.as_str())
        }) {
            return Err("A connection with this name already exists".into());
        }

        let mut host = self.host.trim().to_owned();
        let mut username = self.username.trim().to_owned();
        let mut port_text = self.port.trim().to_owned();
        // Permit the familiar `user@host:port` shorthand when a dedicated
        // field was left blank. Bracketed IPv6 is handled as well.
        parse_endpoint_shorthand(&mut host, &mut username, &mut port_text);
        if username.is_empty() {
            username = "root".to_owned();
        }
        if port_text.is_empty() {
            port_text = DEFAULT_PORT.to_string();
        }
        if host.is_empty() {
            return Err("Host cannot be empty".into());
        }
        let port: u16 = port_text
            .parse()
            .map_err(|_| "Port must be a number between 1 and 65535".to_owned())?;
        if port == 0 {
            return Err("Port must be a number between 1 and 65535".into());
        }

        let auth = match &self.auth {
            AuthDraft::Password { password } => {
                if password.contains(['\0', '\n', '\r']) {
                    return Err("Password cannot contain NUL or line breaks".into());
                }
                let password = if password.is_empty() {
                    self.original_password
                        .clone()
                        .ok_or_else(|| "Password cannot be empty".to_owned())?
                } else {
                    password.clone()
                };
                AuthDraft::Password { password }
            }
            AuthDraft::Key {
                source,
                value,
                public_key,
            } => {
                if value.trim().is_empty() {
                    return Err(match source {
                        KeySource::Paste => "Private key cannot be empty",
                        _ => "Key path cannot be empty",
                    }
                    .into());
                }
                AuthDraft::Key {
                    source: *source,
                    value: value.clone(),
                    public_key: public_key.clone().filter(|key| !key.trim().is_empty()),
                }
            }
        };

        Ok(ProfileDraft {
            mode: self.mode,
            id: self.profile_id.clone(),
            name: name.to_owned(),
            host,
            port,
            username,
            auth,
        })
    }
}
