# Profile management

A profile is a named bundle of nfqws2 options and metadata. Selecting one
patches the runtime config so the next start/restart uses it.

## Storage

Profiles live as TOML files under `/opt/zapret2/profiles/` (derived from
`ZAPRET_BASE`). One file per profile, named `<name>.toml`:

```toml
name = "youtube"
description = "YouTube + Discord bypass"
strategy = "split"
hostlists = ["youtube.txt", "discord.txt"]
nfqws_opts = "--qnum=200 --dpi-desync"
```

`ProfileManager` keeps disk and in-memory state consistent: `load()` rebuilds
the set from disk (read-only — it does not create the directory), `save_profile`
writes the file and updates memory, and `remove` deletes both.

## Using the Profiles tab

| Key | Action |
|-----|--------|
| `↑` / `↓` | Move selection |
| `Enter` | Apply the selected profile to runtime config |
| `n` | New profile (empty form) |
| `e` | Edit the selected profile |
| `d` | Delete the selected profile (confirm with `y`) |

In the form: `Tab` / `↑↓` move between fields (Name, Description, Strategy,
nfqws opts, Hostlists), `Enter` saves, `Esc` cancels. Hostlists are entered as
a comma- or space-separated list.

## Validation

Before anything is written, the form validates:

- **Name** — non-empty, no path separators or `..`, no spaces/odd characters
  (it becomes a filename).
- **nfqws opts** — shell-split and checked against the allowed-option whitelist
  in `zapret2-core::daemon` (prevents arbitrary arguments).

Invalid input keeps the dialog open with an inline error and writes nothing.

## Applying vs persisting

- **Applying** (`Enter`) updates the in-memory runtime config and active
  profile name. It does **not** write `/opt/zapret2/config`.
- **Creating/editing** writes the profile's own TOML file. This needs root for
  `/opt/zapret2/profiles`; as a normal user it currently fails with a
  permission error (routing profile writes through the pkexec helper is a
  planned follow-up — see [`privilege-model.md`](privilege-model.md)).
