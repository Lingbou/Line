use std::{
    io::{self, Write},
    path::PathBuf,
    sync::atomic::AtomicBool,
};

use line::{
    app::{App, AuthDraft, KeyChoice, KeySource, ProfileDraft, SaveMode},
    config::{AuthMethod, ConfigError, ConfigStore, Profile, Profiles, ValidationError},
};
use uuid::Uuid;

use super::{
    DynError,
    path::expand_tilde,
    terminal::{TuiSession, wait_for_terminal_line},
};

pub(super) fn load_with_recovery(
    store: &ConfigStore,
    shutdown: Option<&AtomicBool>,
) -> Result<Profiles, DynError> {
    match store.load() {
        Ok(profiles) => Ok(profiles),
        Err(error @ (ConfigError::Json { .. } | ConfigError::Validation(_))) => {
            if store.load_backup().is_err() {
                return Err(Box::new(error));
            }

            eprintln!("line could not read {}:", store.profiles_path().display());
            eprintln!("  {error}");
            eprint!("Restore the last valid backup? [y/N] ");
            io::stderr().flush()?;
            if let Some(shutdown) = shutdown {
                wait_for_terminal_line(shutdown)?;
            }
            let mut answer = String::new();
            io::stdin().read_line(&mut answer)?;
            if matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
                Ok(store.restore_backup()?)
            } else {
                Err(Box::new(error))
            }
        }
        Err(error) => Err(Box::new(error)),
    }
}

pub(super) fn maybe_recover_runtime_config(
    store: &ConfigStore,
    terminal: &mut TuiSession,
    shutdown: &AtomicBool,
) -> Result<Option<Profiles>, DynError> {
    match store.load() {
        Err(ConfigError::Json { .. } | ConfigError::Validation(_)) => {
            terminal.suspend()?;
            let recovery = load_with_recovery(store, Some(shutdown));
            let resume = terminal.resume();
            if let Err(error) = resume {
                return Err(Box::new(error));
            }
            recovery.map(Some)
        }
        _ => Ok(None),
    }
}

pub(super) fn save_profile(
    store: &ConfigStore,
    app: &App,
    draft: ProfileDraft,
) -> Result<(Profiles, Profile), String> {
    let expected = draft
        .id
        .as_deref()
        .and_then(|id| app.profiles().iter().find(|profile| profile.id == id))
        .cloned();
    let auth = resolve_canonical_auth(store, &draft.auth)?;
    let id = match draft.mode {
        SaveMode::Add => Uuid::new_v4().to_string(),
        SaveMode::Edit => draft
            .id
            .clone()
            .ok_or_else(|| "Edited connection has no stable id".to_owned())?,
    };
    let profile = Profile::with_id(id, draft.name, draft.host, draft.port, draft.username, auth);
    let mode = draft.mode;
    let saved_id = profile.id.clone();
    let latest = store
        .modify_with(move |locked_store, profiles| match mode {
            SaveMode::Add => {
                // Validate identity/name uniqueness before touching the
                // visible mirror for that name.
                profiles.insert(profile_for_preflight(&profile))?;
                let materialized = materialize_profile_auth(locked_store, profile)?;
                ensure_required_keys_exist(locked_store, &materialized)?;
                profiles.update(materialized)?;
                Ok(())
            }
            SaveMode::Edit => {
                let current = profiles.by_id(&profile.id);
                if current != expected.as_ref() {
                    return Err(ValidationError::StaleProfile(profile.id.clone()).into());
                }
                // Duplicate/path validation also precedes mirror replacement.
                profiles.update(profile_for_preflight(&profile))?;
                let materialized = materialize_profile_auth(locked_store, profile)?;
                ensure_required_keys_exist(locked_store, &materialized)?;
                profiles.update(materialized)?;
                Ok(())
            }
        })
        .map_err(|error| error.to_string())?;
    let saved = latest
        .by_id(&saved_id)
        .cloned()
        .ok_or_else(|| "Saved connection disappeared".to_owned())?;
    Ok((latest, saved))
}

/// Validate profile identity and naming before creating its key directory.
/// Canonical `.shared` paths are an internal import result and are never
/// persisted, so a password placeholder is used for this in-memory preflight.
fn profile_for_preflight(profile: &Profile) -> Profile {
    let mut profile = profile.clone();
    if matches!(profile.auth, AuthMethod::Key { .. }) {
        profile.auth = AuthMethod::password("");
    }
    profile
}

fn materialize_profile_auth(
    store: &ConfigStore,
    mut profile: Profile,
) -> line::config::Result<Profile> {
    if let AuthMethod::Key {
        private_key,
        public_key,
    } = &profile.auth
    {
        let keys = store.key_store();
        let canonical = keys.canonical_for_paths(private_key, Some(public_key))?;
        profile.auth = keys
            .materialize_for_profile(&profile.name, &canonical)?
            .auth_method();
    }
    Ok(profile)
}

fn ensure_required_keys_exist(store: &ConfigStore, profile: &Profile) -> line::config::Result<()> {
    if let AuthMethod::Key {
        private_key,
        public_key,
    } = &profile.auth
    {
        for path in [
            store.root().join(private_key),
            store.root().join(public_key),
        ] {
            if !path.is_file() {
                return Err(ValidationError::MissingKey(path).into());
            }
        }
    }
    Ok(())
}

pub(super) fn delete_profile(store: &ConfigStore, app: &App, id: &str) -> Result<Profiles, String> {
    let expected = app
        .profiles()
        .iter()
        .find(|profile| profile.id == id)
        .cloned()
        .ok_or_else(|| "Connection no longer exists".to_owned())?;
    let id = id.to_owned();
    store
        .modify(move |profiles| {
            if profiles.by_id(&id) != Some(&expected) {
                return Err(ValidationError::StaleProfile(id.clone()));
            }
            profiles.remove(&id).map(|_| ())
        })
        .map_err(|error| error.to_string())
}

fn resolve_canonical_auth(store: &ConfigStore, draft: &AuthDraft) -> Result<AuthMethod, String> {
    match draft {
        AuthDraft::Password { password } => Ok(AuthMethod::password(password.clone())),
        AuthDraft::Key {
            source,
            value,
            public_key,
        } => {
            let keys = store.key_store();
            let pair = match source {
                KeySource::Import => {
                    let private_key = expand_tilde(value)?;
                    let public_key = public_key
                        .as_deref()
                        .filter(|value| !value.trim().is_empty())
                        .map(expand_tilde)
                        .transpose()?;
                    keys.import_files(private_key, public_key.as_deref())
                }
                KeySource::Paste => keys.import_pasted(value, public_key.as_deref()),
                KeySource::Existing => {
                    keys.canonical_for_paths(value, public_key.as_deref().map(std::path::Path::new))
                }
            }
            .map_err(|error| error.to_string())?;
            Ok(pair.auth_method())
        }
    }
}

pub(super) fn refresh_key_choices(store: &ConfigStore, app: &mut App) -> Result<(), DynError> {
    let keys = store.key_store();
    let choices = keys
        .list()?
        .into_iter()
        .map(|pair| {
            let used_by = app
                .profiles()
                .iter()
                .filter(|profile| {
                    matches!(
                        &profile.auth,
                        AuthMethod::Key { private_key, .. }
                            if keys
                                .same_private_key(private_key, &pair.private_key)
                                .unwrap_or(false)
                    )
                })
                .count();
            let short = pair.fingerprint.get(..12).unwrap_or(&pair.fingerprint);
            KeyChoice {
                label: format!("Key {short}"),
                private_key: pair.private_key,
                public_key: pair.public_key,
                used_by,
            }
        })
        .collect::<Vec<_>>();
    let existing = app.form().and_then(|form| match &form.auth {
        AuthDraft::Key {
            source: KeySource::Existing,
            value,
            ..
        } => Some(PathBuf::from(value)),
        _ => None,
    });
    if let Some(existing) = existing
        && let Some(choice) = choices.iter().find(|choice| {
            keys.same_private_key(&existing, &choice.private_key)
                .unwrap_or(false)
        })
        && let Some(form) = app.form_mut()
        && let AuthDraft::Key {
            source: KeySource::Existing,
            value,
            public_key,
        } = &mut form.auth
    {
        *value = choice.private_key.to_string_lossy().into_owned();
        *public_key = Some(choice.public_key.to_string_lossy().into_owned());
    }
    app.set_available_keys(choices);
    Ok(())
}
