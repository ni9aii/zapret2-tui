# zapret2-tui

**Terminal UI for zapret2 — DPI bypass manager for Linux**

[![CI](https://github.com/ni9aii/zapret2-tui/actions/workflows/ci.yml/badge.svg)](https://github.com/ni9aii/zapret2-tui/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](https://github.com/ni9aii/zapret2-tui#license)

---

## English

### What is zapret2-tui?

A terminal-based interface for managing [zapret2](https://github.com/bol-van/zapret2) — a DPI bypass toolkit for Linux. Control the `nfqws2` daemon and firewall rules without leaving your terminal.

### Features

- **Real-time status monitoring** — See daemon and firewall state at a glance
- **One-key control** — Start/stop/restart with `s`, `r`, `q`
- **Profile management** — Create, edit, delete, and apply profiles from the UI
- **Runs as a normal user** — Privileged actions go through `pkexec`/polkit;
  `direct` mode for root shells and servers
- **Log streaming** — Live output from nfqws2
- **Multi-tab interface** — Status, Profiles, Logs, Settings
- **Safe defaults** — Graceful handling of missing/misconfigured state

### Requirements

- Linux with **nftables** (iptables backend is planned; selecting it fails
  explicitly rather than silently doing nothing) and NFQUEUE kernel support
- Rust toolchain (for building)
- zapret2 installed at `/opt/zapret2` (configurable)
- `pkexec` + polkit for running as a normal user (optional; `direct` mode works
  without it)

### Installation

```bash
git clone https://github.com/ni9aii/zapret2-tui
cd zapret2-tui
cargo install --path .
```

Or build a release binary:

```bash
cargo build --release
sudo install -m 755 target/release/zapret2-tui /usr/local/bin/
```

### Usage

```bash
# Run with default config (privilege mode: auto)
zapret2-tui

# Run with custom config
zapret2-tui -c /path/to/config

# Force a privilege mode: auto (default) | pkexec | direct
zapret2-tui --privilege-mode=direct   # root shells / servers, no pkexec

# Show help
zapret2-tui --help
```

**Controls:**

| Key | Action |
|-----|--------|
| `s` | Toggle start/stop |
| `r` | Restart zapret2 |
| `Tab` / `Shift+Tab` | Switch tabs |
| `↑` / `↓` | Navigate profiles |
| `Enter` | Apply highlighted profile |
| `n` / `e` / `d` | New / edit / delete profile (Profiles tab) |
| `h` / `?` | Help overlay |
| `q` / `Esc` | Quit |

In a dialog: `Tab` / `↑↓` move between fields, `Enter` confirms, `Esc` cancels.

### Privileges

Firewall and daemon control need root. `zapret2-tui` runs unprivileged and
escalates per action:

- `--privilege-mode=auto` (default) — direct if already root, otherwise
  `pkexec`.
- `--privilege-mode=pkexec` — always use `pkexec` (a polkit prompt appears).
- `--privilege-mode=direct` — never use `pkexec`; for root shells, servers, and
  minimal TTYs.

`pkexec` invokes the small `zapret2-helper` binary. Install it and the polkit
policy (see `packaging/README.md`):

```bash
cargo build --release
sudo install -Dm755 target/release/zapret2-helper /usr/libexec/zapret2-helper
sudo install -Dm644 packaging/polkit/io.github.ni9aii.zapret2.policy \
  /usr/share/polkit-1/actions/io.github.ni9aii.zapret2.policy
```

A cancelled polkit prompt is reported distinctly and leaves state unchanged.

### Logs

Logs are written to a file (never the terminal, which the TUI owns):

```text
$XDG_STATE_HOME/zapret2-tui/zapret2-tui.log
# or, if unset: ~/.local/state/zapret2-tui/zapret2-tui.log
```

Set `RUST_LOG` to change verbosity (default `info`).

### Documentation

- [`CHANGELOG.md`](CHANGELOG.md) — release history
- [`docs/architecture.md`](docs/architecture.md) — crates and data flow
- [`docs/privilege-model.md`](docs/privilege-model.md) — pkexec/polkit, modes
- [`docs/profile-management.md`](docs/profile-management.md) — profiles & CRUD

### Configuration

Default config path: `/opt/zapret2/config`

```bash
# Example /opt/zapret2/config
ZAPRET_BASE=/opt/zapret2
NFQWS2_ENABLE=1
NFQWS2_OPT="--qnum=200 --hostlist=/opt/zapret2/files/youtube.txt"
QNUM=200
FWTYPE=nftables
DESYNC_MARK=0x40000000
```

### Architecture

```
src/main.rs           Entry point, CLI args via clap, file logging
src/app.rs            App state, controller integration, key handling
src/ui.rs             ratatui rendering
src/modal.rs          Profile create/edit/delete modal state + validation
src/logging.rs        File-based tracing setup (XDG state dir)
crates/zapret2-core   Core library (config, daemon, firewall, profile,
                      actions, privilege)
crates/zapret2-helper Minimal privileged helper, invoked via pkexec
packaging/polkit      polkit policy for the helper
```

See [`docs/architecture.md`](docs/architecture.md) for details.

### Development

```bash
# Run tests
cargo test --workspace

# Check formatting
cargo fmt -- --check

# Clippy
cargo clippy --all-targets --workspace -- -D warnings
```

### License

Dual-licensed under MIT OR Apache-2.0.
---

Русская версия README: [`docs/README.ru.md`](docs/README.ru.md).
