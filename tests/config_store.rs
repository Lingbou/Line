#[cfg(unix)]
mod unix {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::{Arc, Barrier};
    use std::thread;

    use line::config::{AuthMethod, ConfigStore, Profile, Profiles};
    use tempfile::tempdir;

    #[test]
    fn first_load_is_empty_and_creates_private_layout() {
        let temp = tempdir().unwrap();
        let root = temp.path().join(".line");
        let store = ConfigStore::at(&root);

        let profiles = store.load().unwrap();

        assert!(profiles.profiles.is_empty());
        assert_eq!(
            fs::metadata(&root).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(root.join("keys"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert!(!root.join("profiles.json").exists());
    }

    #[test]
    fn unsupported_root_key_paths_are_rejected_without_mutating_data() {
        let temp = tempdir().unwrap();
        let root = temp.path().join(".line");
        let store = ConfigStore::at(&root);
        store.load().unwrap();
        let root_private = root.join("keys/key-old");
        let root_public = root.join("keys/key-old.pub");
        fs::write(&root_private, b"private key bytes").unwrap();
        fs::write(&root_public, b"public key bytes").unwrap();
        let profiles = Profiles {
            profiles: vec![Profile::with_id(
                "unsupported",
                "Example-Server",
                "192.0.2.10",
                22,
                "root",
                AuthMethod::Key {
                    private_key: "keys/key-old".into(),
                    public_key: "keys/key-old.pub".into(),
                },
            )],
            ..Profiles::default()
        };
        let original_json = serde_json::to_vec_pretty(&profiles).unwrap();
        fs::write(store.profiles_path(), &original_json).unwrap();

        let error = store.load().unwrap_err();

        assert!(matches!(
            error,
            line::config::ConfigError::Validation(line::config::ValidationError::InvalidKeyLayout(
                _
            ))
        ));
        assert_eq!(fs::read(store.profiles_path()).unwrap(), original_json);
        assert_eq!(fs::read(root_private).unwrap(), b"private key bytes");
        assert_eq!(fs::read(root_public).unwrap(), b"public key bytes");
        assert!(!root.join("keys/Example-Server").exists());
    }

    #[test]
    fn saved_profiles_round_trip_and_the_json_is_private() {
        let temp = tempdir().unwrap();
        let root = temp.path().join(".line");
        let store = ConfigStore::at(&root);
        let profiles = Profiles {
            schema_version: 1,
            profiles: vec![Profile {
                id: "prod-root".into(),
                name: "Production".into(),
                host: "192.0.2.10".into(),
                port: 2222,
                username: "root".into(),
                auth: AuthMethod::Password {
                    password: "correct horse".into(),
                },
            }],
        };

        store.save(&profiles).unwrap();

        assert_eq!(store.load().unwrap(), profiles);
        assert_eq!(
            fs::metadata(root.join("profiles.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        let json = fs::read_to_string(root.join("profiles.json")).unwrap();
        assert!(json.contains("\"schema_version\": 1"));
        assert!(json.ends_with('\n'));
    }

    #[test]
    fn second_save_keeps_previous_json_as_backup() {
        let temp = tempdir().unwrap();
        let store = ConfigStore::at(temp.path().join(".line"));
        let first = Profiles {
            profiles: vec![Profile {
                id: "one".into(),
                name: "One".into(),
                host: "one.example".into(),
                username: "alice".into(),
                port: 22,
                auth: AuthMethod::Password {
                    password: "pw1".into(),
                },
            }],
            ..Profiles::default()
        };
        let mut second = first.clone();
        second.profiles[0].host = "two.example".into();

        store.save(&first).unwrap();
        store.save(&second).unwrap();

        let backup = fs::read_to_string(store.backup_path()).unwrap();
        assert!(backup.contains("one.example"));
        assert!(!backup.contains("two.example"));
        assert_eq!(store.load().unwrap(), second);
    }

    #[test]
    fn malformed_json_is_reported_without_falling_back_or_overwriting() {
        let temp = tempdir().unwrap();
        let store = ConfigStore::at(temp.path().join(".line"));
        store.save(&Profiles::default()).unwrap();
        fs::write(store.profiles_path(), b"{ definitely not json").unwrap();

        let error = store.load().unwrap_err();
        assert!(matches!(error, line::config::ConfigError::Json { .. }));
        assert_eq!(
            fs::read(store.profiles_path()).unwrap(),
            b"{ definitely not json"
        );
    }

    #[test]
    fn persisted_json_requires_the_complete_current_schema() {
        let temp = tempdir().unwrap();
        let store = ConfigStore::at(temp.path().join(".line"));
        store.load().unwrap();
        let current = serde_json::json!({
            "schema_version": 1,
            "profiles": [{
                "id": "one",
                "name": "One",
                "host": "one.example",
                "port": 22,
                "username": "root",
                "auth": {
                    "type": "password",
                    "password": "pw"
                }
            }]
        });
        let mut incompatible_documents = Vec::new();

        let mut missing_schema = current.clone();
        missing_schema
            .as_object_mut()
            .unwrap()
            .remove("schema_version");
        incompatible_documents.push(missing_schema);

        let mut missing_profiles = current.clone();
        missing_profiles.as_object_mut().unwrap().remove("profiles");
        incompatible_documents.push(missing_profiles);

        let mut missing_port = current.clone();
        missing_port["profiles"][0]
            .as_object_mut()
            .unwrap()
            .remove("port");
        incompatible_documents.push(missing_port);

        let mut unknown_envelope_field = current.clone();
        unknown_envelope_field["legacy"] = serde_json::json!(true);
        incompatible_documents.push(unknown_envelope_field);

        let mut unknown_profile_field = current.clone();
        unknown_profile_field["profiles"][0]["legacy"] = serde_json::json!(true);
        incompatible_documents.push(unknown_profile_field);

        let mut unknown_auth_field = current.clone();
        unknown_auth_field["profiles"][0]["auth"]["legacy"] = serde_json::json!(true);
        incompatible_documents.push(unknown_auth_field);

        for document in incompatible_documents {
            fs::write(
                store.profiles_path(),
                serde_json::to_vec_pretty(&document).unwrap(),
            )
            .unwrap();
            assert!(matches!(
                store.load(),
                Err(line::config::ConfigError::Json { .. })
            ));
        }

        let mut old_schema = current;
        old_schema["schema_version"] = serde_json::json!(0);
        fs::write(
            store.profiles_path(),
            serde_json::to_vec_pretty(&old_schema).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            store.load(),
            Err(line::config::ConfigError::Validation(
                line::config::ValidationError::UnsupportedSchema(0)
            ))
        ));
    }

    #[test]
    fn save_refuses_to_overwrite_a_malformed_existing_file() {
        let temp = tempdir().unwrap();
        let store = ConfigStore::at(temp.path().join(".line"));
        store.load().unwrap();
        fs::write(store.profiles_path(), b"broken-json").unwrap();

        let error = store.save(&Profiles::default()).unwrap_err();

        assert!(matches!(error, line::config::ConfigError::Json { .. }));
        assert_eq!(fs::read(store.profiles_path()).unwrap(), b"broken-json");
    }

    #[test]
    fn duplicate_names_are_rejected_case_insensitively() {
        let temp = tempdir().unwrap();
        let store = ConfigStore::at(temp.path().join(".line"));
        let profile = |id: &str, name: &str| Profile {
            id: id.into(),
            name: name.into(),
            host: "example.org".into(),
            username: "root".into(),
            port: 22,
            auth: AuthMethod::Password {
                password: "pw".into(),
            },
        };
        let profiles = Profiles {
            profiles: vec![profile("1", "Prod"), profile("2", "prod")],
            ..Profiles::default()
        };

        let error = store.save(&profiles).unwrap_err();
        assert!(matches!(error, line::config::ConfigError::Validation(_)));
    }

    #[test]
    fn concurrent_modify_transactions_preserve_both_changes() {
        let temp = tempdir().unwrap();
        let root = temp.path().join(".line");
        ConfigStore::at(&root).save(&Profiles::default()).unwrap();
        let barrier = Arc::new(Barrier::new(3));
        let workers: Vec<_> = [("one", "One"), ("two", "Two")]
            .into_iter()
            .map(|(id, name)| {
                let root = root.clone();
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    ConfigStore::at(root).modify(|profiles| {
                        profiles.insert(Profile::with_id(
                            id,
                            name,
                            format!("{id}.example"),
                            22,
                            "root",
                            AuthMethod::password("pw"),
                        ))
                    })
                })
            })
            .collect();

        barrier.wait();
        for worker in workers {
            worker.join().unwrap().unwrap();
        }

        let profiles = ConfigStore::at(root).load().unwrap();
        assert_eq!(profiles.profiles.len(), 2);
        assert!(
            profiles
                .profiles
                .iter()
                .any(|profile| profile.name == "One")
        );
        assert!(
            profiles
                .profiles
                .iter()
                .any(|profile| profile.name == "Two")
        );
    }

    #[test]
    fn corrupt_current_file_can_be_explicitly_restored_from_backup() {
        let temp = tempdir().unwrap();
        let store = ConfigStore::at(temp.path().join(".line"));
        let mut profiles = Profiles {
            profiles: vec![Profile::with_id(
                "one",
                "One",
                "old.example",
                22,
                "root",
                AuthMethod::password("pw"),
            )],
            ..Profiles::default()
        };
        store.save(&profiles).unwrap();
        profiles.profiles[0].host = "new.example".into();
        store.save(&profiles).unwrap();
        fs::write(store.profiles_path(), b"truncated").unwrap();

        assert_eq!(store.load_backup().unwrap().profiles[0].host, "old.example");
        let restored = store.restore_backup().unwrap();

        assert_eq!(restored.profiles[0].host, "old.example");
        assert_eq!(store.load().unwrap(), restored);
        assert_eq!(store.load_backup().unwrap().profiles[0].host, "old.example");
        let preserved = fs::read_dir(store.root())
            .unwrap()
            .filter_map(Result::ok)
            .find(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("profiles.json.corrupt-")
            })
            .expect("damaged primary should be preserved before recovery");
        assert_eq!(fs::read(preserved.path()).unwrap(), b"truncated");
        assert_eq!(
            fs::metadata(preserved.path()).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn host_and_username_control_characters_are_rejected() {
        let profile = |host: &str, username: &str| {
            Profile::with_id("id", "Name", host, 22, username, AuthMethod::password("pw"))
        };
        assert!(matches!(
            profile("example\nhost", "root").validate(),
            Err(line::config::ValidationError::InvalidHost)
        ));
        assert!(matches!(
            profile("example.org", "root\nadmin").validate(),
            Err(line::config::ValidationError::InvalidUsername)
        ));
    }

    #[test]
    fn debug_output_never_contains_a_saved_password() {
        let profile = Profile::with_id(
            "id",
            "Name",
            "example.org",
            22,
            "root",
            AuthMethod::password("do-not-print-this"),
        );

        let debug = format!("{profile:?}");

        assert!(!debug.contains("do-not-print-this"));
        assert!(debug.contains("<redacted>"));
    }
}
