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
```

A commented copy ships as
[`config.example.yaml`](https://github.com/siansiansu/skua/blob/main/config.example.yaml).

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
