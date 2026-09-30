# Line

[English](README.md) | [中文说明](README_CN.md)

Line is a lightweight Linux TUI for quickly opening SSH connections without repeatedly typing hostnames, usernames, or passwords.

```text
╭──────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ line  /  connections                                                          Ctrl+T  New connection │
│                                                                                                      │
│  / Find by name, user or host…                                                             13 saved  │
│                                                                                                      │
│   CONNECTION                             AUTH  DESTINATION                                           │
│ › prod-api-cluster                       KEY   deploy@198.51.100.24:22                               │
│   staging-k8s-node-01                    KEY   root@203.0.113.88:22                                  │
│   bastion-gateway-singapore              KEY   admin@192.0.2.15:2222                                 │
│   backup-database-us                     PWD   root@198.51.100.200:22                                │
│ ──────────────────────────────────────────────────────────────────────────────────────────────────── │
│ SSH key  keys/prod-api-cluster/key                                                                   │
│                                                                                                      │
│  Enter  Connect   Ctrl+E  Edit   Ctrl+D  Delete                                                      │
│                                                                                                      │
│ ↑↓ select   ·   type to filter                                                           Ctrl+C Quit │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

## Installation

### Pre-built Packages (.deb & .rpm)

Download the latest release package matching your distribution and CPU architecture from [Releases](https://github.com/Lingbou/Line/releases):

```bash
# Check which architecture you are on: amd64/x86_64 or arm64/aarch64
dpkg --print-architecture   # Debian/Ubuntu
uname -m                    # RPM-based distributions

# Ubuntu / Debian
sudo apt install ./line_<version>_amd64.deb    # x86_64
sudo apt install ./line_<version>_arm64.deb    # aarch64

# Fedora / RHEL / CentOS
sudo dnf install ./line-<version>-1.x86_64.rpm  # x86_64
sudo dnf install ./line-<version>-1.aarch64.rpm # aarch64
```

### Static Binary (Any Linux Distro)

Download the tarball for your architecture from [Releases](https://github.com/Lingbou/Line/releases):

- `line-v<version>-x86_64-unknown-linux-musl.tar.gz` (Intel/AMD)
- `line-v<version>-aarch64-unknown-linux-musl.tar.gz` (ARM64, e.g. Raspberry Pi, ARM servers)

Extract it and move `line` to `/usr/local/bin/` or `~/.local/bin/`.

The binary itself is static, but Line still invokes the system OpenSSH client and `ssh-keygen`.

### Arch Linux and Alpine Linux

Both distributions run the static binary directly. Install the distribution OpenSSH programs first, then unpack the tarball above:

```bash
# Arch Linux (openssh provides ssh and ssh-keygen)
sudo pacman -S openssh

# Alpine Linux (openssh-client pulls in the separately packaged ssh-keygen)
sudo apk add openssh-client openssh-keygen
```

The static build needs no libc package from either distribution, so no `glibc` or `musl` dependency has to be installed for Line itself.

### Build from Source

Prerequisites: Rust (1.88+), OpenSSH, and `ssh-keygen`. Saved-password connections use `SSH_ASKPASS` and require OpenSSH 8.4+.

```bash
cargo install --path . --locked
```

## Command-Line Usage

```bash
# Launch interactive TUI launcher (default)
line

# Connect directly to a saved server (skips TUI)
line <name>

# Connect to a profile whose name begins with '-'
line -- <name>

# List all saved connections in plain text
line -l
line --list

# Print version
line -v
line --version

# Print help
line -h
line --help
```

## Controls

### Connection List

| Key | Action |
| --- | --- |
| `↑` / `↓` or `←` / `→` | Select connection |
| `Enter` | Connect via SSH |
| `Type text` | Filter connections by name, user, or host |
| `Esc` | Clear filter |
| `Ctrl+T` | Add connection |
| `Ctrl+E` | Edit selected connection |
| `Ctrl+D` | Delete selected connection |
| `Ctrl+C` | Quit |
| `Home` / `End` | Jump to first / last connection |
| `PageUp` / `PageDown` | Scroll by 5 connections |
| `Ctrl+W` / `Ctrl+Backspace` | Delete previous word in search query |
| `Ctrl+U` | Clear search query |

Mouse clicks and scroll wheel are also supported for selection and navigation.

### Form Editing

| Key | Action |
| --- | --- |
| `Tab` / `Shift+Tab` | Next / previous field |
| `Enter` | Save connection |
| `Esc` | Cancel and exit form |
| `Ctrl+Left` / `Ctrl+Right` | Move cursor backward / forward by word |
| `Alt+Left` / `Alt+Right` | Move cursor backward / forward by word |
| `Ctrl+W` / `Ctrl+Backspace` | Delete previous word |
| `Ctrl+Delete` / `Alt+D` | Delete next word |
| `Ctrl+A` / `Home` | Move cursor to start of line |
| `Ctrl+E` / `End` | Move cursor to end of line |
| `Ctrl+U` | Delete to start of line |
| `Ctrl+K` | Delete to end of line |
| `Space` | Toggle auth method or show/hide password |

When adding a connection:
- `Username` defaults to `root`; `Port` defaults to `22`.
- You can paste command shorthands like `ssh -p 2222 root@192.0.2.1` or `user@host:port` directly into the Host field; Line automatically parses the components.

## Configuration

All state is kept under `~/.line/` (or `$LINE_CONFIG_DIR`):

```text
~/.line/
├── profiles.json       # Connection profiles (mode 0600)
├── profiles.json.bak   # Atomic backup from previous save
├── known_hosts         # Line-managed host keys (mode 0600)
└── keys/
    ├── <Connection Name>/
    │   ├── key         # Private key (mode 0600)
    │   └── key.pub     # Public key
    └── .shared/        # Content-addressed deduplicated key storage
```

If only a private key is imported without a public key, Line automatically derives the corresponding `.pub` file using `ssh-keygen`.

Password profiles currently store the password as plaintext in `profiles.json` and its backup, protected by directory and file permissions. Use key authentication if plaintext credential storage is not acceptable.

## OpenSSH Behavior

Line deliberately keeps connection behavior deterministic and does not load `~/.ssh/config`. It loads the system `/etc/ssh/ssh_config`, ignores the user's and system-wide `known_hosts`, and uses `~/.line/known_hosts` instead. SSH agent identities and passphrase-protected private keys are not supported in this version.

## Source Layout

- `src/app/`: UI-independent application state, forms, event routing, and text editing
- `src/ui/`: Ratatui rendering for launcher, forms, and dialog modals
- `src/config/`: JSON persistence, backup rotation, and key deduplication
- `src/ssh/`: OpenSSH invocation, PTY handoff, askpass helper, and host key verification
- `src/runtime/`: Terminal raw mode management and main event loop
