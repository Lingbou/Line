#[test]
fn common_path_prefix_handles_unicode() {
    assert_eq!(common_prefix(["连接一", "连接二"].into_iter()), "连接");
    assert_eq!(
        common_prefix(["id_ed25519", "id_ecdsa"].into_iter()),
        "id_e"
    );
}

#[test]
fn failed_session_message_keeps_diagnostics() {
    let result = SessionResult {
        exit_code: Some(255),
        signal: None,
        stderr_tail: "Permission denied (publickey,password).\n".into(),
    };
    assert_eq!(
        session_failure(&result),
        "ssh exited with code 255\n\nPermission denied (publickey,password)."
    );
}
use line::ssh::SessionResult;

use super::{path::common_prefix, session::session_failure};

#[cfg(unix)]
mod explicit_key_import {
    use std::{fs, os::unix::fs::MetadataExt, path::PathBuf, process::Command};

    use line::{
        app::{App, AuthDraft, KeySource, ProfileDraft, SaveMode},
        config::{AuthMethod, ConfigStore},
    };
    use tempfile::tempdir;

    use super::super::persistence::{delete_profile, refresh_key_choices, save_profile};

    fn generate_key(path: &std::path::Path) {
        let status = Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-f"])
            .arg(path)
            .status()
            .expect("tests require OpenSSH ssh-keygen");
        assert!(status.success());
    }

    #[test]
    fn save_profile_uses_the_explicit_public_key_source_path() {
        let temp = tempdir().unwrap();
        let private_key = temp.path().join("private-key");
        generate_key(&private_key);
        let generated_public = PathBuf::from(format!("{}.pub", private_key.display()));
        let explicit_public = temp.path().join("public-key.txt");
        fs::rename(&generated_public, &explicit_public).unwrap();
        let expected_public = fs::read_to_string(&explicit_public).unwrap();
        let store = ConfigStore::at(temp.path().join("line-home"));
        let app = App::new(Vec::new());
        let draft = ProfileDraft {
            mode: SaveMode::Add,
            id: None,
            name: "Imported".into(),
            host: "example.org".into(),
            port: 22,
            username: "root".into(),
            auth: AuthDraft::Key {
                source: KeySource::Import,
                value: private_key.display().to_string(),
                public_key: Some(explicit_public.display().to_string()),
            },
        };

        let (_, saved) = save_profile(&store, &app, draft).unwrap();

        let AuthMethod::Key { public_key, .. } = saved.auth else {
            panic!("expected key authentication");
        };
        assert_eq!(public_key, PathBuf::from("keys/Imported/key.pub"));
        assert_eq!(
            fs::read_to_string(store.root().join(public_key)).unwrap(),
            expected_public
        );
        assert_eq!(
            store.load().unwrap().profiles[0].auth,
            AuthMethod::Key {
                private_key: "keys/Imported/key".into(),
                public_key: "keys/Imported/key.pub".into(),
            }
        );
        assert_eq!(store.key_store().list().unwrap().len(), 1);
    }

    #[test]
    fn stale_edit_cannot_replace_the_effective_profile_key() {
        let temp = tempdir().unwrap();
        let first_key = temp.path().join("first-key");
        let second_key = temp.path().join("second-key");
        generate_key(&first_key);
        generate_key(&second_key);
        let store = ConfigStore::at(temp.path().join("line-home"));
        let empty = App::new(Vec::new());
        let add = |path: &std::path::Path| ProfileDraft {
            mode: SaveMode::Add,
            id: None,
            name: "Production".into(),
            host: "example.org".into(),
            port: 22,
            username: "root".into(),
            auth: AuthDraft::Key {
                source: KeySource::Import,
                value: path.display().to_string(),
                public_key: None,
            },
        };
        let (_, saved) = save_profile(&store, &empty, add(&first_key)).unwrap();
        let visible = store.root().join("keys/Production/key");
        let original_inode = fs::metadata(&visible).unwrap().ino();
        let original_bytes = fs::read(&visible).unwrap();
        let app = App::new(vec![saved.clone()]);
        store
            .modify(|profiles| {
                profiles.profiles[0].host = "changed-elsewhere.example".into();
                Ok(())
            })
            .unwrap();
        let edit = ProfileDraft {
            mode: SaveMode::Edit,
            id: Some(saved.id),
            name: "Production".into(),
            host: "example.org".into(),
            port: 22,
            username: "root".into(),
            auth: AuthDraft::Key {
                source: KeySource::Import,
                value: second_key.display().to_string(),
                public_key: None,
            },
        };

        assert!(save_profile(&store, &app, edit).is_err());
        assert_eq!(fs::metadata(&visible).unwrap().ino(), original_inode);
        assert_eq!(fs::read(&visible).unwrap(), original_bytes);
    }

    #[test]
    fn canonical_key_choice_counts_a_profile_visible_mirror_as_usage() {
        let temp = tempdir().unwrap();
        let private_key = temp.path().join("private-key");
        generate_key(&private_key);
        let store = ConfigStore::at(temp.path().join("line-home"));
        let empty = App::new(Vec::new());
        let draft = ProfileDraft {
            mode: SaveMode::Add,
            id: None,
            name: "Shared".into(),
            host: "example.org".into(),
            port: 22,
            username: "root".into(),
            auth: AuthDraft::Key {
                source: KeySource::Import,
                value: private_key.display().to_string(),
                public_key: None,
            },
        };
        let (_, saved) = save_profile(&store, &empty, draft).unwrap();
        let mut app = App::new(vec![saved]);

        refresh_key_choices(&store, &mut app).unwrap();

        assert_eq!(app.available_keys().len(), 1);
        assert_eq!(app.available_keys()[0].used_by, 1);
        let error = store
            .delete_key_if_unused(&app.available_keys()[0].private_key)
            .unwrap_err();
        assert!(error.to_string().contains("still used by 1 profile"));
    }

    #[test]
    fn rename_and_password_switch_prune_mirrors_only_after_backup_stops_using_them() {
        let temp = tempdir().unwrap();
        let private_key = temp.path().join("private-key");
        generate_key(&private_key);
        let store = ConfigStore::at(temp.path().join("line-home"));
        let empty = App::new(Vec::new());
        let add = ProfileDraft {
            mode: SaveMode::Add,
            id: None,
            name: "Old Name".into(),
            host: "example.org".into(),
            port: 22,
            username: "root".into(),
            auth: AuthDraft::Key {
                source: KeySource::Import,
                value: private_key.display().to_string(),
                public_key: None,
            },
        };
        let (_, first) = save_profile(&store, &empty, add).unwrap();
        let AuthMethod::Key {
            private_key: first_private,
            public_key: first_public,
        } = &first.auth
        else {
            panic!("expected key authentication");
        };
        let app = App::new(vec![first.clone()]);
        let rename = ProfileDraft {
            mode: SaveMode::Edit,
            id: Some(first.id),
            name: "New Name".into(),
            host: "example.org".into(),
            port: 22,
            username: "root".into(),
            auth: AuthDraft::Key {
                source: KeySource::Existing,
                value: first_private.display().to_string(),
                public_key: Some(first_public.display().to_string()),
            },
        };
        let (_, renamed) = save_profile(&store, &app, rename).unwrap();
        assert!(store.root().join("keys/Old Name").is_dir());
        assert!(store.root().join("keys/New Name").is_dir());

        let app = App::new(vec![renamed.clone()]);
        let password = ProfileDraft {
            mode: SaveMode::Edit,
            id: Some(renamed.id.clone()),
            name: renamed.name.clone(),
            host: "example.org".into(),
            port: 22,
            username: "root".into(),
            auth: AuthDraft::Password {
                password: "saved-password".into(),
            },
        };
        let (_, password_profile) = save_profile(&store, &app, password).unwrap();
        assert!(!store.root().join("keys/Old Name").exists());
        assert!(
            store.root().join("keys/New Name").exists(),
            "the recovery backup still references this mirror"
        );

        let app = App::new(vec![password_profile.clone()]);
        let mut password_again = ProfileDraft {
            mode: SaveMode::Edit,
            id: Some(password_profile.id),
            name: password_profile.name,
            host: "changed.example.org".into(),
            port: 22,
            username: "root".into(),
            auth: AuthDraft::Password {
                password: "saved-password".into(),
            },
        };
        password_again.port = 2222;
        save_profile(&store, &app, password_again).unwrap();
        assert!(!store.root().join("keys/New Name").exists());
        assert_eq!(store.key_store().list().unwrap().len(), 1);
    }

    #[test]
    fn deleting_a_profile_preserves_its_recovery_mirror_and_shared_key() {
        let temp = tempdir().unwrap();
        let private_key = temp.path().join("private-key");
        generate_key(&private_key);
        let store = ConfigStore::at(temp.path().join("line-home"));
        let empty = App::new(Vec::new());
        let add = ProfileDraft {
            mode: SaveMode::Add,
            id: None,
            name: "Delete Me".into(),
            host: "example.org".into(),
            port: 22,
            username: "root".into(),
            auth: AuthDraft::Key {
                source: KeySource::Import,
                value: private_key.display().to_string(),
                public_key: None,
            },
        };
        let (_, saved) = save_profile(&store, &empty, add).unwrap();
        let app = App::new(vec![saved.clone()]);

        let current = delete_profile(&store, &app, &saved.id).unwrap();

        assert!(current.profiles.is_empty());
        assert!(store.root().join("keys/Delete Me").is_dir());
        assert_eq!(store.key_store().list().unwrap().len(), 1);
        store.save(&current).unwrap();
        assert!(!store.root().join("keys/Delete Me").exists());
        assert_eq!(store.key_store().list().unwrap().len(), 1);
    }
}
