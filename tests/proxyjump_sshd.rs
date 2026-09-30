#[cfg(target_os = "linux")]
mod linux {
    use std::{
        fs,
        net::{TcpListener, TcpStream},
        path::{Path, PathBuf},
        process::{Child, Command, Stdio},
        thread,
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };

    use line::config::{AuthMethod, ConfigStore, JumpHop, Profile, Profiles};
    use tempfile::TempDir;

    const STARTUP_TIMEOUT: Duration = Duration::from_secs(15);

    #[test]
    fn line_connects_through_a_single_containerized_jump_host() {
        if !docker_usable() {
            eprintln!("skipping containerized ProxyJump test: Docker is unavailable");
            return;
        }

        let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let fixture = workspace.join("tests/fixtures/sshd");
        let image = format!("line-sshd-test:{}", unique_suffix());
        let network = format!("line-proxyjump-net-{}", unique_suffix());
        let jump_name = format!("line-proxyjump-jump-{}", unique_suffix());
        let target_name = format!("line-proxyjump-target-{}", unique_suffix());
        let temp = TempDir::new().expect("temp dir");
        let client_key = temp.path().join("client_key");
        let authorized_keys = temp.path().join("authorized_keys");
        let line_root = temp.path().join("line");

        run(Command::new("ssh-keygen").args([
            "-q",
            "-t",
            "ed25519",
            "-N",
            "",
            "-f",
            client_key.to_str().expect("client key path"),
        ]));
        fs::copy(client_key.with_extension("pub"), &authorized_keys).expect("authorized key copy");
        fs::set_permissions(
            &authorized_keys,
            std::os::unix::fs::PermissionsExt::from_mode(0o600),
        )
        .expect("authorized key mode");

        run(Command::new("docker").args([
            "build",
            "-t",
            &image,
            "-f",
            fixture
                .join("Dockerfile")
                .to_str()
                .expect("dockerfile path"),
            fixture.to_str().expect("fixture path"),
        ]));
        let mut cleanup = Cleanup::new(
            network.clone(),
            image.clone(),
            vec![jump_name.clone(), target_name.clone()],
        );
        run(Command::new("docker").args(["network", "create", &network]));

        let jump_port = free_port();
        run(&mut docker_run(
            &jump_name,
            &network,
            &image,
            Some(jump_port),
            &authorized_keys,
            false,
        ));
        run(&mut docker_run(
            &target_name,
            &network,
            &image,
            None,
            &authorized_keys,
            true,
        ));
        wait_for_port(jump_port);

        let key_dir = line_root.join("keys/Target");
        fs::create_dir_all(&key_dir).expect("key dir");
        let private_key = key_dir.join("key");
        let public_key = key_dir.join("key.pub");
        fs::copy(&client_key, &private_key).expect("private key copy");
        fs::copy(&authorized_keys, &public_key).expect("public key copy");
        set_private_file_mode(&private_key);
        set_private_file_mode(&public_key);

        let profile = Profile {
            id: "target".into(),
            name: "Target".into(),
            host: target_name.clone(),
            port: 22,
            username: "root".into(),
            jump_chain: vec![JumpHop {
                username: Some("root".into()),
                host: "127.0.0.1".into(),
                port: jump_port,
            }],
            auth: AuthMethod::Key {
                private_key: PathBuf::from("keys/Target/key"),
                public_key: PathBuf::from("keys/Target/key.pub"),
            },
        };
        ConfigStore::at(&line_root)
            .save(&Profiles {
                profiles: vec![profile],
                ..Profiles::default()
            })
            .expect("save test profile");

        let mut child = Command::new(env!("CARGO_BIN_EXE_line"))
            .env("LINE_CONFIG_DIR", &line_root)
            .arg("Target")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn line");
        let status = wait_for_child(&mut child, STARTUP_TIMEOUT);
        let output = child.wait_with_output().expect("collect line output");
        cleanup.finish();

        assert!(
            status.success(),
            "line failed with {status:?}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    struct Cleanup {
        network: String,
        image: String,
        containers: Vec<String>,
        finished: bool,
    }

    impl Cleanup {
        fn new(network: String, image: String, containers: Vec<String>) -> Self {
            Self {
                network,
                image,
                containers,
                finished: false,
            }
        }

        fn finish(&mut self) {
            if self.finished {
                return;
            }
            for container in &self.containers {
                let _ = Command::new("docker")
                    .args(["rm", "-f", container])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
            let _ = Command::new("docker")
                .args(["network", "rm", &self.network])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            let _ = Command::new("docker")
                .args(["image", "rm", &self.image])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            self.finished = true;
        }
    }

    impl Drop for Cleanup {
        fn drop(&mut self) {
            self.finish();
        }
    }

    fn docker_run(
        name: &str,
        network: &str,
        image: &str,
        host_port: Option<u16>,
        authorized_keys: &Path,
        force_command: bool,
    ) -> Command {
        let mut command = Command::new("docker");
        command.args(["run", "-d", "--name", name, "--network", network]);
        if let Some(host_port) = host_port {
            command.arg("-p").arg(format!("127.0.0.1:{host_port}:22"));
        }
        command
            .arg("-v")
            .arg(format!("{}:/authorized_keys:ro", authorized_keys.display()))
            .arg(image)
            .arg("sh")
            .arg("-c");
        let mut script = String::from(
            "mkdir -p /root/.ssh \
             && cp /authorized_keys /root/.ssh/authorized_keys \
             && chmod 700 /root/.ssh \
             && chmod 600 /root/.ssh/authorized_keys \
             && exec /usr/sbin/sshd -D -e \
                -o PermitRootLogin=yes \
                -o PasswordAuthentication=no \
                -o KbdInteractiveAuthentication=no \
                -o PubkeyAuthentication=yes \
                -o AllowTcpForwarding=yes \
                -o UsePAM=no",
        );
        if force_command {
            script.push_str(" -o ForceCommand=/bin/true");
        }
        command.arg(script);
        command
    }

    fn wait_for_child(child: &mut Child, timeout: Duration) -> std::process::ExitStatus {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = child.try_wait().expect("poll line process") {
                return status;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                panic!("line did not finish within {timeout:?}");
            }
            thread::sleep(Duration::from_millis(50));
        }
    }

    fn docker_usable() -> bool {
        Command::new("docker")
            .arg("info")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    fn free_port() -> u16 {
        TcpListener::bind(("127.0.0.1", 0))
            .expect("bind free port")
            .local_addr()
            .expect("free port address")
            .port()
    }

    fn wait_for_port(port: u16) {
        let deadline = Instant::now() + STARTUP_TIMEOUT;
        while Instant::now() < deadline {
            if TcpStream::connect(("127.0.0.1", port)).is_ok() {
                return;
            }
            thread::sleep(Duration::from_millis(100));
        }
        panic!("sshd did not listen on port {port}");
    }

    fn run(command: &mut Command) {
        let output = command.output().expect("run command");
        assert!(
            output.status.success(),
            "command failed: {:?}\nstdout:\n{}\nstderr:\n{}",
            command,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn set_private_file_mode(path: &Path) {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).expect("private file mode");
    }

    fn unique_suffix() -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        format!("{}-{nanos}", std::process::id())
    }
}
