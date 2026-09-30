#[cfg(target_os = "linux")]
mod linux {
    use std::{
        fs,
        net::{TcpListener, TcpStream},
        path::{Path, PathBuf},
        process::{Child, Command, Output, Stdio},
        thread,
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };

    use line::config::{AuthMethod, ConfigStore, JumpHop, Profile, ProfileHop, Profiles};
    use tempfile::TempDir;

    const STARTUP_TIMEOUT: Duration = Duration::from_secs(20);

    #[test]
    fn line_connects_through_a_single_containerized_jump_host() {
        let Some(fixture) = Fixture::start(&workspace(), 1) else {
            return;
        };

        assert_success(&fixture.run_line());
    }

    #[test]
    fn line_connects_through_two_containerized_jump_hosts() {
        let Some(fixture) = Fixture::start(&workspace(), 2) else {
            return;
        };

        assert_success(&fixture.run_line());
    }

    #[test]
    fn line_connects_through_three_containerized_jump_hosts() {
        let Some(fixture) = Fixture::start(&workspace(), 3) else {
            return;
        };

        // Three hops nest the command three deep, so the innermost tokens are
        // escaped once per level.
        assert_success(&fixture.run_line());
    }

    #[test]
    fn line_connects_through_a_saved_profile_jump_host() {
        let Some(mut fixture) = Fixture::start(&workspace(), 1) else {
            return;
        };
        fixture.use_saved_profile_hop();

        assert_success(&fixture.run_line());
    }

    #[test]
    fn line_reports_rejected_credentials() {
        let Some(fixture) = Fixture::start(&workspace(), 1) else {
            return;
        };
        fixture.use_unauthorized_identity();

        let output = fixture.run_line();
        assert_failed(&output, "Permission denied");
        fixture.assert_no_lingering_ssh();
    }

    #[test]
    fn line_reports_an_unavailable_hop() {
        let Some(fixture) = Fixture::start(&workspace(), 2) else {
            return;
        };
        fixture.stop_hop(1);
        let unavailable = fixture.hop_host(1);

        let output = fixture.run_line();
        // A hop that is down is noticed by the hop before it, so OpenSSH
        // reports the failed forward rather than naming the unreachable host.
        assert_failed_with_any(
            &output,
            &[
                "stdio forwarding failed",
                "Connection refused",
                &unavailable,
            ],
        );
        fixture.assert_no_lingering_ssh();
    }

    #[test]
    fn line_reports_a_reversed_chain() {
        let Some(mut fixture) = Fixture::start(&workspace(), 2) else {
            return;
        };
        fixture.reverse_chain();
        // The second hop is only reachable through the first, so reversing the
        // chain makes it the first thing the client tries to resolve.
        let unreachable = fixture.hop_host(1);

        let output = fixture.run_line();
        assert_failed(&output, &unreachable);
        fixture.assert_no_lingering_ssh();
    }

    fn workspace() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    fn assert_success(output: &Output) {
        assert!(
            output.status.success(),
            "line failed with {:?}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn assert_failed_with_any(output: &Output, expected: &[&str]) {
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            output.status.code(),
            Some(255),
            "expected the ssh exit code, got {:?}\nstderr:\n{stderr}",
            output.status
        );
        assert!(
            expected.iter().any(|needle| stderr.contains(needle)),
            "expected one of {expected:?} in the diagnostics:\n{stderr}"
        );
    }

    fn assert_failed(output: &Output, expected: &str) {
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            output.status.code(),
            Some(255),
            "expected the ssh exit code, got {:?}\nstderr:\n{stderr}",
            output.status
        );
        assert!(
            stderr.contains(expected),
            "expected {expected:?} in the diagnostics:\n{stderr}"
        );
    }

    /// sshd containers plus the Line configuration that reaches them.
    ///
    /// The first jump is published on the host, so Line can only start the
    /// chain there. Every later hop and the target are reachable strictly
    /// through the hop before them, resolved by Docker's network DNS.
    struct Fixture {
        _temp: TempDir,
        cleanup: Cleanup,
        line_root: PathBuf,
        jump_names: Vec<String>,
        target_name: String,
        host_port: u16,
        client_key: PathBuf,
        authorized_keys: PathBuf,
    }

    impl Fixture {
        fn start(workspace: &Path, hops: usize) -> Option<Self> {
            if !docker_usable() {
                eprintln!("skipping containerized ProxyJump test: Docker is unavailable");
                return None;
            }
            assert!(hops >= 1, "a jump chain needs at least one hop");

            let fixture = workspace.join("tests/fixtures/sshd");
            let suffix = unique_suffix();
            let image = format!("line-sshd-test:{suffix}");
            let network = format!("line-proxyjump-net-{suffix}");
            let jump_names: Vec<String> = (1..=hops)
                .map(|index| format!("line-proxyjump-j{index}-{suffix}"))
                .collect();
            let target_name = format!("line-proxyjump-target-{suffix}");

            let temp = TempDir::new().expect("temp dir");
            let client_key = temp.path().join("client_key");
            let authorized_keys = temp.path().join("authorized_keys");
            create_client_identity(&client_key, &authorized_keys);

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

            let mut containers = jump_names.clone();
            containers.push(target_name.clone());
            let cleanup = Cleanup::new(network.clone(), image.clone(), containers);
            run(Command::new("docker").args(["network", "create", &network]));

            let host_port = free_port();
            run(&mut docker_run(
                &jump_names[0],
                &network,
                &image,
                Some(host_port),
                &authorized_keys,
                false,
            ));
            for name in jump_names.iter().skip(1) {
                run(&mut docker_run(
                    name,
                    &network,
                    &image,
                    None,
                    &authorized_keys,
                    false,
                ));
            }
            run(&mut docker_run(
                &target_name,
                &network,
                &image,
                None,
                &authorized_keys,
                true,
            ));
            wait_for_port(host_port);

            let line_root = temp.path().join("line");
            let mut this = Self {
                _temp: temp,
                cleanup,
                line_root,
                jump_names,
                target_name,
                host_port,
                client_key,
                authorized_keys,
            };
            this.install_target(this.endpoint_chain());
            Some(this)
        }

        /// The chain in connection order, all hops written as endpoints.
        fn endpoint_chain(&self) -> Vec<JumpHop> {
            let mut chain = vec![JumpHop::endpoint(
                Some("root".into()),
                "127.0.0.1",
                self.host_port,
            )];
            for name in self.jump_names.iter().skip(1) {
                chain.push(JumpHop::endpoint(Some("root".into()), name.clone(), 22));
            }
            chain
        }

        fn hop_host(&self, index: usize) -> String {
            if index == 0 {
                "127.0.0.1".to_owned()
            } else {
                self.jump_names[index].clone()
            }
        }

        /// Replace the saved target profile with one that uses `chain`.
        fn install_target(&mut self, chain: Vec<JumpHop>) {
            self.write_key_pair(
                "Target",
                &self.client_key.clone(),
                &self.authorized_keys.clone(),
            );
            self.save_target(chain, "Target");
        }

        fn save_target(&self, chain: Vec<JumpHop>, key_name: &str) {
            self.save_profiles(vec![Profile {
                id: "target".into(),
                name: "Target".into(),
                host: self.target_name.clone(),
                port: 22,
                username: "root".into(),
                jump_chain: chain,
                auth: AuthMethod::Key {
                    private_key: PathBuf::from(format!("keys/{key_name}/key")),
                    public_key: PathBuf::from(format!("keys/{key_name}/key.pub")),
                },
            }]);
        }

        fn save_profiles(&self, profiles: Vec<Profile>) {
            ConfigStore::at(&self.line_root)
                .save(&Profiles {
                    profiles,
                    ..Profiles::default()
                })
                .expect("save test profiles");
        }

        fn write_key_pair(&self, name: &str, private: &Path, public: &Path) {
            let key_dir = self.line_root.join("keys").join(name);
            fs::create_dir_all(&key_dir).expect("key dir");
            let private_key = key_dir.join("key");
            let public_key = key_dir.join("key.pub");
            fs::copy(private, &private_key).expect("private key copy");
            fs::copy(public, &public_key).expect("public key copy");
            set_private_file_mode(&private_key);
            set_private_file_mode(&public_key);
        }

        /// Point the target at a saved profile instead of a raw endpoint.
        fn use_saved_profile_hop(&mut self) {
            self.write_key_pair(
                "Bastion",
                &self.client_key.clone(),
                &self.authorized_keys.clone(),
            );
            let bastion = Profile {
                id: "bastion".into(),
                name: "Bastion".into(),
                host: "127.0.0.1".into(),
                port: self.host_port,
                username: "root".into(),
                jump_chain: Vec::new(),
                auth: AuthMethod::Key {
                    private_key: PathBuf::from("keys/Bastion/key"),
                    public_key: PathBuf::from("keys/Bastion/key.pub"),
                },
            };
            let mut target = Profile {
                id: "target".into(),
                name: "Target".into(),
                host: self.target_name.clone(),
                port: 22,
                username: "root".into(),
                jump_chain: vec![JumpHop::Profile(ProfileHop {
                    profile_id: "bastion".into(),
                })],
                auth: AuthMethod::Key {
                    private_key: PathBuf::from("keys/Target/key"),
                    public_key: PathBuf::from("keys/Target/key.pub"),
                },
            };
            target.jump_chain = vec![JumpHop::Profile(ProfileHop {
                profile_id: bastion.id.clone(),
            })];
            self.save_profiles(vec![bastion, target]);
        }

        /// Swap the identity saved in Line for one the containers reject.
        fn use_unauthorized_identity(&self) {
            let rejected = self._temp.path().join("rejected_key");
            let rejected_public = self._temp.path().join("rejected_authorized");
            create_client_identity(&rejected, &rejected_public);
            self.write_key_pair("Target", &rejected, &rejected_public);
        }

        fn reverse_chain(&mut self) {
            let mut chain = self.endpoint_chain();
            chain.reverse();
            self.install_target(chain);
        }

        fn stop_hop(&self, index: usize) {
            run(Command::new("docker").args(["stop", &self.jump_names[index]]));
        }

        fn run_line(&self) -> Output {
            let mut child = Command::new(env!("CARGO_BIN_EXE_line"))
                .env("LINE_CONFIG_DIR", &self.line_root)
                .arg("Target")
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("spawn line");
            wait_for_child(&mut child, STARTUP_TIMEOUT);
            child.wait_with_output().expect("collect line output")
        }

        /// A failed chain must not leave its ProxyCommand `ssh` processes
        /// behind, holding the terminal or the forwarded connection.
        fn assert_no_lingering_ssh(&self) {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                let listing = Command::new("pgrep")
                    .args(["-af", "ssh"])
                    .output()
                    .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
                    .unwrap_or_default();
                let lingering: Vec<&String> = self
                    .jump_names
                    .iter()
                    .filter(|name| listing.contains(name.as_str()))
                    .collect();
                if lingering.is_empty() {
                    return;
                }
                if Instant::now() >= deadline {
                    panic!("ssh processes outlived the failed chain: {lingering:?}\n{listing}");
                }
                thread::sleep(Duration::from_millis(100));
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            self.cleanup.finish();
        }
    }

    fn create_client_identity(client_key: &Path, authorized_keys: &Path) {
        run(Command::new("ssh-keygen").args([
            "-q",
            "-t",
            "ed25519",
            "-N",
            "",
            "-f",
            client_key.to_str().expect("client key path"),
        ]));
        fs::copy(client_key.with_extension("pub"), authorized_keys).expect("authorized key copy");
        fs::set_permissions(
            authorized_keys,
            std::os::unix::fs::PermissionsExt::from_mode(0o600),
        )
        .expect("authorized key mode");
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
