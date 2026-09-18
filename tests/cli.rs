#[cfg(unix)]
mod unix {
    use std::env;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::process::Command;

    use tempfile::TempDir;

    fn write_profile(root: &Path) {
        fs::write(
            root.join("profiles.json"),
            r#"{
                "schema_version": 1,
                "profiles": [
                    {
                        "id": "example",
                        "name": "Example",
                        "host": "example.com",
                        "port": 22,
                        "username": "root",
                        "auth": {
                            "type": "key",
                            "private_key": "keys/Example/key",
                            "public_key": "keys/Example/key.pub"
                        }
                    }
                ]
            }"#,
        )
        .expect("write profiles");
    }

    fn fake_ssh(root: &TempDir) -> std::path::PathBuf {
        let path = root.path().join("fake-bin/ssh");
        fs::create_dir_all(path.parent().expect("fake bin parent")).expect("create fake bin");
        fs::write(&path, "#!/bin/sh\nprintf 'ssh diagnostic\\n' >&2\nexit 7\n")
            .expect("write fake ssh");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).expect("chmod fake ssh");
        path
    }

    #[test]
    fn list_returns_failure_when_profiles_cannot_be_loaded() {
        let root = TempDir::new().expect("temp dir");
        fs::write(root.path().join("profiles.json"), b"not json")
            .expect("write malformed profiles");

        let output = Command::new(env!("CARGO_BIN_EXE_line"))
            .env("LINE_CONFIG_DIR", root.path())
            .arg("--list")
            .output()
            .expect("run line --list");

        assert_eq!(output.status.code(), Some(1));
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("failed to load profiles"),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn direct_connect_forwards_ssh_stderr_through_a_pipe() {
        let root = TempDir::new().expect("temp dir");
        write_profile(root.path());
        fake_ssh(&root);

        let path = env::join_paths(
            std::iter::once(root.path().join("fake-bin"))
                .chain(env::split_paths(&env::var_os("PATH").unwrap_or_default())),
        )
        .expect("join PATH");
        let output = Command::new(env!("CARGO_BIN_EXE_line"))
            .env("LINE_CONFIG_DIR", root.path())
            .env("PATH", path)
            .arg("Example")
            .output()
            .expect("run line Example");

        assert_eq!(output.status.code(), Some(7));
        assert_eq!(String::from_utf8_lossy(&output.stderr), "ssh diagnostic\n");
    }
}
