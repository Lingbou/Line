#[cfg(unix)]
mod unix {
    use std::fs;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::{Arc, Barrier};
    use std::thread;

    use line::config::{AuthMethod, KeyError, KeyStore, Profile, Profiles};
    use tempfile::tempdir;

    fn generate_key(directory: &Path, name: &str, passphrase: &str) -> PathBuf {
        let path = directory.join(name);
        let status = Command::new("ssh-keygen")
            .args([
                "-q",
                "-t",
                "ed25519",
                "-N",
                passphrase,
                "-C",
                "line-test",
                "-f",
            ])
            .arg(&path)
            .status()
            .expect("tests require OpenSSH ssh-keygen");
        assert!(status.success());
        path
    }

    fn public_path(private: &Path) -> PathBuf {
        PathBuf::from(format!("{}.pub", private.display()))
    }

    #[test]
    fn profile_names_cannot_escape_or_claim_the_shared_key_directory() {
        let profile = |name: &str| {
            Profile::with_id(
                "id",
                name,
                "example.org",
                22,
                "root",
                AuthMethod::password("password"),
            )
        };

        for unsafe_name in [".", "..", ".shared", "folder/connection"] {
            assert!(
                profile(unsafe_name).validate().is_err(),
                "{unsafe_name:?} must not be usable as a key-directory name"
            );
        }
        assert!(profile("生产 SSH").validate().is_ok());
    }

    #[test]
    fn imported_key_has_one_hidden_canonical_pair_and_named_profile_mirrors() {
        let temp = tempdir().unwrap();
        let source = generate_key(temp.path(), "source", "");
        let root = temp.path().join("line-home");
        let store = KeyStore::at(&root);

        let canonical = store.import_files(&source, None).unwrap();
        let first = store
            .materialize_for_profile("Example-Server", &canonical)
            .unwrap();
        let second = store
            .materialize_for_profile("另一条连接", &canonical)
            .unwrap();

        assert_eq!(
            canonical.private_key.parent(),
            Some(Path::new("keys/.shared"))
        );
        assert_eq!(first.private_key, PathBuf::from("keys/Example-Server/key"));
        assert_eq!(
            first.public_key,
            PathBuf::from("keys/Example-Server/key.pub")
        );
        assert_eq!(second.private_key, PathBuf::from("keys/另一条连接/key"));

        let canonical_metadata = fs::metadata(root.join(&canonical.private_key)).unwrap();
        let first_metadata = fs::metadata(root.join(&first.private_key)).unwrap();
        let second_metadata = fs::metadata(root.join(&second.private_key)).unwrap();
        assert_eq!(canonical_metadata.dev(), first_metadata.dev());
        assert_eq!(canonical_metadata.ino(), first_metadata.ino());
        assert_eq!(canonical_metadata.ino(), second_metadata.ino());
        assert_eq!(
            fs::metadata(root.join(&canonical.public_key))
                .unwrap()
                .ino(),
            fs::metadata(root.join(&first.public_key)).unwrap().ino()
        );
    }

    #[test]
    fn file_import_derives_missing_public_key_and_deduplicates() {
        let temp = tempdir().unwrap();
        let source = generate_key(temp.path(), "source", "");
        fs::remove_file(public_path(&source)).unwrap();
        let root = temp.path().join("line-home");
        let store = KeyStore::at(&root);

        let first = store.import_files(&source, None).unwrap();
        let second = store.import_files(&source, None).unwrap();

        assert_eq!(first, second);
        assert_eq!(first.private_key.parent(), Some(Path::new("keys/.shared")));
        assert_eq!(first.public_key.parent(), Some(Path::new("keys/.shared")));
        assert!(first.fingerprint.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(first.fingerprint.len(), 64);
        let private = root.join(&first.private_key);
        let public = root.join(&first.public_key);
        assert!(private.is_file());
        assert!(public.is_file());
        assert!(
            fs::read_to_string(public)
                .unwrap()
                .starts_with("ssh-ed25519 ")
        );
        assert_eq!(
            fs::metadata(private).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::read_dir(root.join("keys/.shared")).unwrap().count(),
            2,
            "one shared private/public pair should be stored"
        );
    }

    #[test]
    fn adjacent_public_key_is_validated_and_preserved() {
        let temp = tempdir().unwrap();
        let source = generate_key(temp.path(), "source", "");
        let expected_public = fs::read_to_string(public_path(&source)).unwrap();
        let root = temp.path().join("line-home");

        let pair = KeyStore::at(&root).import_files(&source, None).unwrap();

        assert_eq!(
            fs::read_to_string(root.join(pair.public_key)).unwrap(),
            expected_public
        );
    }

    #[test]
    fn explicit_public_key_path_can_be_imported_from_a_non_adjacent_file() {
        let temp = tempdir().unwrap();
        let source = generate_key(temp.path(), "source", "");
        let explicit_public = temp.path().join("shared-public-key.txt");
        fs::rename(public_path(&source), &explicit_public).unwrap();
        let expected_public = fs::read_to_string(&explicit_public).unwrap();
        let root = temp.path().join("line-home");

        let pair = KeyStore::at(&root)
            .import_files(&source, Some(&explicit_public))
            .unwrap();

        assert_eq!(
            fs::read_to_string(root.join(pair.public_key)).unwrap(),
            expected_public
        );
    }

    #[test]
    fn full_filename_public_sidecar_wins_for_pem_keys() {
        let temp = tempdir().unwrap();
        let source = generate_key(temp.path(), "source.pem", "");
        // Keep both possible sidecar names. The standard `<file>.pub` must be
        // preferred over the lossy extension replacement `source.pub`.
        let expected = fs::read_to_string(public_path(&source)).unwrap();
        fs::write(temp.path().join("source.pub"), "ssh-ed25519 AAAAwrong").unwrap();
        let root = temp.path().join("line-home");

        let pair = KeyStore::at(&root).import_files(&source, None).unwrap();

        assert_eq!(
            fs::read_to_string(root.join(pair.public_key)).unwrap(),
            expected
        );
    }

    #[test]
    fn pasted_private_key_derives_public_key_and_reuses_existing_pair() {
        let temp = tempdir().unwrap();
        let source = generate_key(temp.path(), "source", "");
        let private = fs::read_to_string(&source).unwrap();
        let public = fs::read_to_string(public_path(&source)).unwrap();
        let root = temp.path().join("line-home");
        let store = KeyStore::at(&root);

        let derived = store.import_pasted(&private, None).unwrap();
        let supplied = store.import_pasted(&private, Some(&public)).unwrap();

        assert_eq!(derived, supplied);
        assert_eq!(store.list().unwrap(), vec![derived]);
    }

    #[test]
    fn reimport_repairs_an_interrupted_key_pair_write() {
        let temp = tempdir().unwrap();
        let source = generate_key(temp.path(), "source", "");
        let root = temp.path().join("line-home");
        let store = KeyStore::at(&root);
        let pair = store.import_files(&source, None).unwrap();
        fs::write(root.join(&pair.private_key), b"truncated").unwrap();
        fs::write(root.join(&pair.public_key), b"truncated").unwrap();

        let repaired = store.import_files(&source, None).unwrap();

        assert_eq!(repaired, pair);
        assert_eq!(
            fs::read(root.join(pair.private_key)).unwrap(),
            fs::read(source).unwrap()
        );
        assert!(
            fs::read_to_string(root.join(pair.public_key))
                .unwrap()
                .starts_with("ssh-ed25519 ")
        );
    }

    #[test]
    fn mismatched_public_key_is_rejected_without_saving_files() {
        let temp = tempdir().unwrap();
        let first = generate_key(temp.path(), "first", "");
        let second = generate_key(temp.path(), "second", "");
        let private = fs::read_to_string(first).unwrap();
        let wrong_public = fs::read_to_string(public_path(&second)).unwrap();
        let root = temp.path().join("line-home");
        let store = KeyStore::at(&root);

        let error = store
            .import_pasted(&private, Some(&wrong_public))
            .unwrap_err();

        assert!(matches!(error, KeyError::PublicKeyMismatch));
        assert_eq!(fs::read_dir(root.join("keys/.shared")).unwrap().count(), 0);
    }

    #[test]
    fn encrypted_private_key_is_rejected_without_prompting() {
        let temp = tempdir().unwrap();
        let source = generate_key(temp.path(), "protected", "secret");
        let store = KeyStore::at(temp.path().join("line-home"));

        let error = store.import_files(source, None).unwrap_err();

        assert!(matches!(error, KeyError::EncryptedPrivateKey));
    }

    #[test]
    fn shared_key_cannot_be_deleted_while_a_profile_references_it() {
        let temp = tempdir().unwrap();
        let source = generate_key(temp.path(), "source", "");
        let root = temp.path().join("line-home");
        let store = KeyStore::at(&root);
        let pair = store.import_files(source, None).unwrap();
        let profile = |id: &str, name: &str| {
            Profile::with_id(id, name, "example.org", 22, "root", pair.auth_method())
        };
        let mut profiles = Profiles {
            profiles: vec![profile("one", "One"), profile("two", "Two")],
            ..Profiles::default()
        };

        let error = store
            .delete_if_unused(&pair.private_key, &profiles)
            .unwrap_err();
        assert!(matches!(error, KeyError::InUse(2)));
        profiles.remove("one").unwrap();
        profiles.remove("two").unwrap();
        assert!(root.join(&pair.private_key).is_file());

        store
            .delete_if_unused(&pair.private_key, &profiles)
            .unwrap();
        assert!(!root.join(&pair.private_key).exists());
        assert!(!root.join(&pair.public_key).exists());
    }

    #[test]
    fn deleting_an_unused_canonical_key_removes_its_unreferenced_mirrors() {
        let temp = tempdir().unwrap();
        let source = generate_key(temp.path(), "source", "");
        let root = temp.path().join("line-home");
        let store = KeyStore::at(&root);
        let canonical = store.import_files(source, None).unwrap();
        let visible = store
            .materialize_for_profile("Unused connection", &canonical)
            .unwrap();

        store
            .delete_if_unused(&canonical.private_key, &Profiles::default())
            .unwrap();

        assert!(!root.join(canonical.private_key).exists());
        assert!(!root.join(canonical.public_key).exists());
        assert!(!root.join(visible.private_key).exists());
        assert!(!root.join("keys/Unused connection").exists());
    }

    #[test]
    fn absolute_or_traversing_key_paths_are_invalid_profile_data() {
        let profile = |path: PathBuf| {
            Profile::with_id(
                "id",
                "Name",
                "example.org",
                22,
                "root",
                AuthMethod::Key {
                    private_key: path,
                    public_key: PathBuf::from("keys/key.pub"),
                },
            )
        };

        assert!(profile(PathBuf::from("/tmp/key")).validate().is_err());
        assert!(
            profile(PathBuf::from("keys/../elsewhere"))
                .validate()
                .is_err()
        );
        assert!(profile(PathBuf::from("somewhere/key")).validate().is_err());
    }

    #[test]
    fn profile_key_paths_must_match_the_profile_directory() {
        let profile = |private_key: &str, public_key: &str| {
            Profile::with_id(
                "id",
                "Name",
                "example.org",
                22,
                "root",
                AuthMethod::Key {
                    private_key: PathBuf::from(private_key),
                    public_key: PathBuf::from(public_key),
                },
            )
        };
        assert!(
            profile("keys/Name/key", "keys/Name/key.pub")
                .validate()
                .is_ok()
        );
        assert!(matches!(
            profile(
                "keys/.shared/key-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "keys/.shared/key-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.pub",
            )
            .validate(),
            Err(line::config::ValidationError::InvalidKeyLayout(name)) if name == "Name"
        ));
        assert!(matches!(
            profile("keys/Other/key", "keys/Other/key.pub").validate(),
            Err(line::config::ValidationError::InvalidKeyLayout(name)) if name == "Name"
        ));
    }

    #[test]
    fn key_listing_ignores_unsupported_files_in_the_keys_root() {
        let temp = tempdir().unwrap();
        let source = generate_key(temp.path(), "source", "");
        let root = temp.path().join("line-home");
        let store = KeyStore::at(&root);
        let managed = store.import_files(&source, None).unwrap();
        let unsupported = root.join("keys/key-unsupported");
        fs::copy(&source, &unsupported).unwrap();

        assert_eq!(store.list().unwrap(), vec![managed]);
        assert!(unsupported.is_file());
        assert!(!unsupported.with_extension("pub").exists());
        assert!(matches!(
            store.canonical_for_paths("keys/key-unsupported", None),
            Err(KeyError::InvalidPath(path)) if path == Path::new("keys/key-unsupported")
        ));
    }

    #[test]
    fn concurrent_imports_of_the_same_key_converge() {
        let temp = tempdir().unwrap();
        let source = generate_key(temp.path(), "source", "");
        let root = temp.path().join("line-home");
        let barrier = Arc::new(Barrier::new(3));
        let workers: Vec<_> = (0..2)
            .map(|_| {
                let source = source.clone();
                let root = root.clone();
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    KeyStore::at(root).import_files(source, None)
                })
            })
            .collect();
        barrier.wait();
        let pairs: Vec<_> = workers
            .into_iter()
            .map(|worker| worker.join().unwrap().unwrap())
            .collect();

        assert_eq!(pairs[0], pairs[1]);
        assert_eq!(KeyStore::at(root).list().unwrap(), vec![pairs[0].clone()]);
    }
}
