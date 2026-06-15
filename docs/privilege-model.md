# Privilege model

Firewall rules (nftables) and the nfqws2 daemon require root. zapret2-tui is
designed to run as a normal user and escalate only for those specific actions,
via `pkexec`/polkit and a small helper binary.

## Modes

Selected with `--privilege-mode` (default `auto`):

| Mode     | Behavior |
|----------|----------|
| `auto`   | Direct if already root, otherwise `pkexec` (falls back to direct if `pkexec` is absent). |
| `pkexec` | Always invoke `pkexec zapret2-helper …`; a polkit prompt appears. |
| `direct` | Never use `pkexec`. For root shells, servers, and minimal TTYs. |

`auto` resolves once at startup from the effective uid and whether `pkexec` is
on `PATH`.

## The helper

`zapret2-helper` is a deliberately small, auditable binary:

- strict `clap` argument parsing; unknown/missing subcommands are rejected;
- it never invokes a shell;
- subcommands: `check`, `firewall apply|remove`, `daemon start|stop|status`;
- one privileged operation per invocation, then it exits.

It reuses `zapret2-core::actions`, so it runs exactly the same code as the
in-process `DirectExecutor` — no divergence in the privileged path.

`daemon start` spawns nfqws2 **detached** (it writes the pid file and survives
the helper's exit). Consequence: in pkexec mode, live nfqws2 log streaming into
the TUI is not available (the daemon's output goes to the system journal);
status is still tracked via the pid file.

## polkit

`packaging/polkit/io.github.ni9aii.zapret2.policy` defines the action that
authorizes running the helper (annotated
`org.freedesktop.policykit.exec.path` → `/usr/libexec/zapret2-helper`), with
defaults requiring admin authentication. Three semantic actions
(`manage-firewall`, `manage-daemon`, `modify-profiles`) are reserved for a
future per-subcommand model.

Install:

```bash
sudo install -Dm755 target/release/zapret2-helper /usr/libexec/zapret2-helper
sudo install -Dm644 packaging/polkit/io.github.ni9aii.zapret2.policy \
  /usr/share/polkit-1/actions/io.github.ni9aii.zapret2.policy
```

## Cancellation vs failure

A cancelled or unauthorized polkit prompt (`pkexec` exit 126) is mapped to
`ZapretError::AuthCancelled` and reported distinctly from an operational
failure. No firewall/daemon state changes when authentication is cancelled.

## Known limitations

- pkexec mode loses live nfqws2 log streaming into the TUI (see above).
- Profile writes (`/opt/zapret2/profiles`) are not yet routed through the
  helper, so creating/editing profiles as a normal user currently fails with a
  permission error. The `modify-profiles` polkit action is reserved for this.
- The iptables firewall backend is not implemented; selecting it returns an
  explicit error rather than silently succeeding.
