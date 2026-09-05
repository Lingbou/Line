# Line

Line is a lightweight Linux TUI for saving SSH connections and opening them
without repeatedly typing `user@host` or passwords.

Type a connection name, username, or host to filter the list, then press Enter
to connect. The launcher keeps destinations together in a compact table and
switches to two-line entries in narrow terminals.

## Build and run

Line requires Rust, OpenSSH, and the `ssh-keygen` command. Saved-password
profiles require OpenSSH 8.4 or newer; unencrypted-key profiles also work with
older OpenSSH clients.

```bash
cargo build --release
./target/release/line
```

The application stores its state in `~/.line/`:

```text
~/.line/
├── profiles.json
├── profiles.json.bak
├── known_hosts
└── keys/
    ├── <Connection Name>/
    │   ├── key
    │   └── key.pub
    └── .shared/            # hidden, content-deduplicated key storage
```

For isolated testing, `LINE_CONFIG_DIR` can point Line at another directory;
normal runs always default to `~/.line/`.

Passwords are intentionally stored as plaintext in `profiles.json` for this
first convenience-focused version. The directory is mode `0700`, while the
configuration, known-hosts file, and private keys are mode `0600`.

For key authentication, `Import new key` accepts a required private-key path
and an optional public-key path. When the public path is blank, Line uses a
matching adjacent `.pub` file or derives the public key with `ssh-keygen`.

## Controls

| Key | Action |
| --- | --- |
| Type or paste text | Filter connections by name, username, or host |
| `↑` / `↓` or `←` / `→` | Select a connection |
| `Enter` | Connect |
| `Ctrl+T` | Add a connection |
| `Ctrl+E` | Edit the selected connection |
| `Ctrl+D` | Delete the selected connection |
| `Ctrl+C` | Quit |
| `Tab` / `Shift+Tab` | Move through a form |
| `Esc` | Clear the filter, or cancel/close a dialog |

The mouse can select connections, fields, buttons, and scroll error details. SSH owns
the terminal while connected; after it exits, Line restores the TUI and keeps
the same connection selected.

The connection editor places Name / Username together and gives Host its own
wide row alongside Port. Narrow terminals stack the identity fields, and the
launcher wraps its action bar while keeping `Ctrl+…` shortcuts readable.
Private/public key inputs stay together; a blank optional public key is derived
during import.

## Source layout

- `src/app/`: storage-independent application state, forms, and input events
- `src/ui/`: Ratatui rendering for browsing, forms, and dialogs
- `src/config/`: profile models, JSON persistence, backups, and shared keys
- `src/ssh/`: OpenSSH runner, AskPass, process handling, and host keys
- `src/runtime/`: adapters joining the TUI, persistence, terminal, and SSH
