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

Download the latest release package matching your distribution from [Releases](https://github.com/Lingbou/Line/releases):

```bash
# Ubuntu / Debian
sudo apt install ./line_<version>_amd64.deb

# Fedora / RHEL / CentOS
sudo dnf install ./line-<version>-1.x86_64.rpm
```

### Static Binary (Any Linux Distro)

Download `line-v<version>-x86_64-unknown-linux-musl.tar.gz` from [Releases](https://github.com/Lingbou/Line/releases), extract it, and move `line` to `/usr/local/bin/` or `~/.local/bin/`.

### Build from Source

Prerequisites: Rust (1.75+), OpenSSH, and `ssh-keygen`. Saved-password connections use `SSH_ASKPASS` and require OpenSSH 8.4+.

```bash
cargo install --path .
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

## Source Layout

- `src/app/`: UI-independent application state, forms, event routing, and text editing
- `src/ui/`: Ratatui rendering for launcher, forms, and dialog modals
- `src/config/`: JSON persistence, backup rotation, and key deduplication
- `src/ssh/`: OpenSSH invocation, PTY handoff, askpass helper, and host key verification
- `src/runtime/`: Terminal raw mode management and main event loop
