# Packaging: polkit integration

zapret2-tui runs unprivileged and performs root-only work (firewall rules and
the nfqws2 daemon) by calling `zapret2-helper` through `pkexec`.

## Files

- `polkit/io.github.ni9aii.zapret2.policy` — polkit action definitions. The
  operative action `io.github.ni9aii.zapret2.run-helper` carries the
  `org.freedesktop.policykit.exec.path` annotation that authorizes
  `pkexec /usr/libexec/zapret2-helper`, so an admin prompt appears before any
  privileged action.

## Install

```bash
# Helper binary at the path the policy annotates
install -Dm755 target/release/zapret2-helper /usr/libexec/zapret2-helper

# polkit policy
install -Dm644 packaging/polkit/io.github.ni9aii.zapret2.policy \
  /usr/share/polkit-1/actions/io.github.ni9aii.zapret2.policy
```

## Privilege modes

`zapret2-tui --privilege-mode=<auto|pkexec|direct>`:

- `auto` (default) — direct if already root, otherwise pkexec.
- `pkexec` — always go through pkexec (desktop use as a normal user).
- `direct` — never use pkexec; for root shells, servers, and minimal TTYs.

A cancelled or unauthorized polkit prompt is reported distinctly from an
operational failure (`ZapretError::AuthCancelled`).

## Manual desktop validation (not host-testable in CI)

1. Build release: `cargo build --release`.
2. Install the helper and policy as above.
3. Run `zapret2-tui` as a normal user (default `auto` → resolves to pkexec).
4. Trigger start/stop; confirm the polkit prompt appears.
5. Cancel the prompt → state unchanged, "authentication cancelled" reported.
6. Authenticate → action succeeds.
7. As root, `zapret2-tui --privilege-mode=direct` still works with no prompt.
