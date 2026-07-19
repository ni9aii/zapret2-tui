# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-07-19

Initial public release.

### Added
- Terminal UI (ratatui) for managing zapret2 / `nfqws2` on Linux.
- Multi-platform CI matrix: distros (ubuntu, debian, fedora, alpine/musl)
  × toolchains (stable, beta, nightly, MSRV 1.88) × architectures (x86_64 and
  aarch64 on native arm runners), see `.github/workflows/ci.yml`.
- `integration` feature gating real-zapret2 end-to-end tests
  (`cargo test --features integration` drives a built `nfqws2` + nftables in
  direct mode; requires root).
- Real-time daemon and firewall status monitoring.
- One-key control: start/stop (`s`), restart (`r`), quit (`q`).
- Multi-tab interface: Status, Profiles, Logs, Settings.
- Profile management (create / edit / delete / apply) from the UI, with
  field validation that rejects bad names and forbidden options before write.
- Live `nfqws2` log streaming to a dedicated Logs tab.
- Privilege executor abstraction with three modes: `auto` (default),
  `pkexec`, and `direct` (root shells / servers, no polkit).
- Small privileged `zapret2-helper` binary invoked via `pkexec`; ships with a
  polkit policy (`packaging/polkit/io.github.ni9aii.zapret2.policy`).
- File-based tracing via `$XDG_STATE_HOME/zapret2-tui/zapret2-tui.log`
  (falls back to `~/.local/state/...`), keeping the terminal free for the TUI.
- Panic hook that restores the terminal on unwind.
- Distinct UI reporting when a polkit prompt is cancelled (state unchanged).
- Documentation: `docs/architecture.md`, `docs/privilege-model.md`,
  `docs/profile-management.md`, `packaging/README.md`, `CONTRIBUTING.md`,
  `SECURITY.md`.

### Fixed
- Daemon stdio safety and pid-file write propagation.
- `nft` double-delete on stop; daemon stop discovered via pidfile.
- Graceful terminal restore on panics.

### Security
- Root-only work (firewall rules, daemon control) is routed through the
  privilege executor; the TUI itself never runs privileged.
- See `SECURITY.md` and `docs/privilege-model.md` for the threat model.

[0.1.0]: https://github.com/ni9aii/zapret2-tui/releases/tag/v0.1.0
