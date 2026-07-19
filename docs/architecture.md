# Architecture

zapret2-tui is a Cargo workspace: a TUI binary, a core library, and a small
privileged helper.

```
zapret2-tui (bin)            Terminal UI
├── src/main.rs              CLI (clap), file logging, event loop
├── src/app.rs               App state, key handling, controller calls
├── src/ui.rs                ratatui rendering (tabs + modals)
├── src/modal.rs             Profile form / delete-confirm state + validation
└── src/logging.rs           File-based tracing setup

crates/zapret2-core (lib)    All the logic, no terminal concerns
├── config.rs                Parse the shell-style zapret2 config
├── daemon.rs                nfqws2 process lifecycle (tracked + detached)
├── firewall.rs              nftables rules (iptables = explicit error)
├── profile.rs               Profile model + on-disk ProfileManager
├── actions.rs               Stateless privileged ops (shared with helper)
├── privilege.rs             PrivilegedExecutor + Direct/Pkexec/Mock, modes
└── lib.rs                   ZapretController ties config+daemon+firewall

crates/zapret2-helper (bin)  Minimal root helper, invoked via pkexec
packaging/polkit             polkit policy authorizing the helper
```

## Data flow

1. `main` parses args, initializes file logging, builds `App`.
2. `App::new` constructs a `ZapretController`, resolves the privilege mode,
   loads profiles, and takes the nfqws2 log channel.
3. The event loop polls key events and a tick. Keys mutate `App` state and call
   controller methods; the tick drains the log channel and refreshes status.
4. `ui::draw` renders the current tab and any open modal from `App` state.

## Controller and privilege boundary

`ZapretController::start`/`stop` branch on the resolved privilege mode:

- **Direct** — call `firewall`/`daemon` in-process. Keeps the tracked child so
  nfqws2 logs stream into the TUI.
- **Pkexec** — delegate to `PkexecExecutor`, which runs
  `pkexec zapret2-helper …`. The helper performs the same operations via
  `core::actions`, so the nftables script and the nfqws2 option whitelist have
  a single source of truth.

See [`privilege-model.md`](privilege-model.md).

## Testing

All logic is host-testable without root: config parsing, pid-file handling,
profile validation, privilege-mode resolution, pkexec arg/exit-code handling,
modal form validation, and log-path resolution. Operations that need root or a
desktop (applying nft rules, the polkit prompt) are validated manually — see
the checklists in `packaging/README.md` and the phase logs.
