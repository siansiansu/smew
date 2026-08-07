# Configuration

skua works with zero config. Optionally, create
`~/.config/skua/config.yaml` — all fields are optional, and CLI flags
(`--profile` / `--region`) take precedence.

```yaml
# Default AWS profile when --profile is not given. If empty, skua
# shows the profile picker at startup.
default_profile: ""

# Default AWS region when --region is not given.
default_region: ap-northeast-1

# Auto-refresh interval for the inventory (duration: "30s", "2m", "1h").
# Unset defaults to 30s; "0" disables it (manual "r" only).
refresh_interval: "30s"

# Multiplexer leader (prefix) key inside a split session.
# Examples: "ctrl+a", "ctrl+ " (ctrl+space). Default "ctrl+b".
session_leader: "ctrl+b"

# Mouse support: wheel scrolls, click selects/focuses, double-click
# connects. Default true. Set false to keep plain click-drag selection.
mouse: true

# Update check: the SSM Parameter Store name holding the latest published
# version. Default "/skua/latest-version".
version_param: "/skua/latest-version"
# Set true to turn the update check off entirely.
disable_version_check: false

# Color theme: a built-in name (default, dracula, gruvbox-dark, nord) or a
# custom skin at ~/.config/skua/skins/<name>.yaml.
skin: ""
```

A commented copy ships as
[`config.example.yaml`](https://github.com/siansiansu/skua/blob/main/config.example.yaml).

## Skins

`skin` selects a color theme. Built-ins: `default`, `dracula`,
`gruvbox-dark`, `nord`. For a custom theme, drop a YAML file at
`~/.config/skua/skins/<name>.yaml` and set `skin: <name>` — the
[built-in skin files](https://github.com/siansiansu/skua/tree/main/skins)
are copyable starting points.

A skin sets any subset of the UI color roles (unset roles keep the default
theme's value). Colors are written as `"#rrggbb"`, a 256-color index
(`39`), or `default` for the terminal's own color:

```yaml
# ~/.config/skua/skins/mytheme.yaml
green: "#50fa7b"     # running states, sync timestamp
red: "#ff5555"       # down states, errors, broadcast
orange: "#ffb86c"    # transitional states, key hints, notices
cyan: "#8be9fd"      # section/table headers, region
pink: "#ff79c6"      # focused pane border, picker cursor
sel_fg: 229          # selected row text
sel_bg: 57           # selected row background
```

The full role list lives in
[`src/theme.rs`](https://github.com/siansiansu/skua/blob/main/src/theme.rs).
A misspelled role or malformed color fails at startup with the file and key
named. Session pane content is never themed — it keeps whatever colors the
remote shell emits.

## CLI flags

| Flag | Effect |
| --- | --- |
| `--profile <name>` | AWS profile (default: config / env / shared default) |
| `--region <name>` | AWS region (default: config / profile / env) |
| `--dry-run` | print resolved identity + inventory as a table and exit (no TUI) |
| `--ssh-config` | print an `~/.ssh/config` block for [ssh/scp over SSM](/guide/ssh-over-ssm) and exit |
| `--ssh-static` | with `--ssh-config`: use a static `authorized_keys` key instead of ephemeral EC2 Instance Connect |
| `--ssh-key <path>` | with `--ssh-config` (ephemeral): public key to push (default: auto-detect `~/.ssh/id_*.pub`) |
| `--version` | print version (with git commit + build time) and exit |
