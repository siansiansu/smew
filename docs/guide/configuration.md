# Configuration

smew works with zero config. Optionally, create
`~/.config/smew/config.yaml` — all fields are optional, and CLI flags
(`--profile` / `--region`) take precedence.

```yaml
# Default AWS profile when --profile is not given. If empty, smew
# shows the profile picker at startup.
default_profile: ""

# Default AWS region when --region is not given.
default_region: ap-northeast-1

# Auto-refresh interval for the active view (duration: "10s", "2m", "1h").
# Unset defaults to 10s (floor 5s); "0" disables it (manual refresh only).
refresh_interval: "10s"

# Multiplexer leader (prefix) key inside a split session.
# Examples: "ctrl+a", "ctrl+ " (ctrl+space). Default "ctrl+b".
session_leader: "ctrl+b"

# Mouse support: wheel scrolls, click selects/focuses, double-click
# connects. Default true. Set false to keep plain click-drag selection.
mouse: true

# Update check: the SSM Parameter Store name holding the latest published
# version. Default "/smew/latest-version".
version_param: "/smew/latest-version"
# Set true to turn the update check off entirely.
disable_version_check: false

# Color theme: a built-in name (default, dracula, gruvbox-dark, nord) or a
# custom skin at ~/.config/smew/skins/<name>.yaml.
skin: ""

# Login user for the SSH connect action (`i` in the instance list) and the
# EC2 Instance Connect key push. Default "ec2-user"; ubuntu images want
# "ubuntu".
ssh_user: "ec2-user"

# %CPU / %MEM columns fed from CloudWatch, refreshed every 5 minutes.
# Only free-tier-eligible APIs (GetMetricStatistics + ListMetrics). Memory
# needs the CloudWatch agent on the hosts ("n/a" without it). Default
# false — when off, smew makes no CloudWatch calls at all.
metrics: false
```

A commented copy ships as
[`config.example.yaml`](https://github.com/siansiansu/smew/blob/main/config.example.yaml).

::: tip Typos fail loudly
Unknown keys are rejected: a misspelled key (say `defaut_region`) is a
startup error naming the file and the key, rather than being silently
ignored while the default stays in effect.
:::

## Skins

`skin` selects a color theme. Built-ins: `default`, `dracula`,
`gruvbox-dark`, `nord`. For a custom theme, drop a YAML file at
`~/.config/smew/skins/<name>.yaml` and set `skin: <name>` — the
[built-in skin files](https://github.com/siansiansu/smew/tree/main/skins)
are copyable starting points.

A skin sets any subset of the UI color roles (unset roles keep the default
theme's value). Colors are written as `"#rrggbb"`, a 256-color index
(`39`), or `default` for the terminal's own color:

```yaml
# ~/.config/smew/skins/mytheme.yaml
green: "#50fa7b"     # running states, sync timestamp
red: "#ff5555"       # down states, errors, broadcast
orange: "#ffb86c"    # transitional states, key hints, notices
cyan: "#8be9fd"      # section/table headers, region
pink: "#ff79c6"      # focused pane border, picker cursor
sel_fg: 229          # selected row text
sel_bg: 57           # selected row background
```

The full role list lives in
[`src/theme.rs`](https://github.com/siansiansu/smew/blob/main/src/theme.rs).
A misspelled role or malformed color fails at startup with the file and key
named. Session pane content is never themed — it keeps whatever colors the
remote shell emits.

## CLI flags

| Flag | Effect |
| --- | --- |
| `--profile <name>` | AWS profile (default: config / env / shared default) |
| `--region <name>` | AWS region (default: config / profile / env) |
| `--dry-run` | print resolved identity + inventory as a table and exit (no TUI) |
| `--dev` | developer mode: mock inventory + local-shell sessions, no AWS access ([details](/guide/quick-start#launch)) |
| `--ssh-config` | print an `~/.ssh/config` block for [ssh/scp over SSM](/guide/ssh-over-ssm) and exit |
| `--ssh-static` | with `--ssh-config`: use a static `authorized_keys` key instead of ephemeral EC2 Instance Connect |
| `--ssh-key <path>` | with `--ssh-config` (ephemeral): public key to push (default: auto-detect `~/.ssh/id_*.pub`) |
| `--version` | print version (with git commit + build time) and exit |
