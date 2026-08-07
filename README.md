<p align="center">
  <img src="./docs/public/logo.svg" alt="skua logo" width="140">
</p>

<h1 align="center">skua</h1>

<p align="center">
A local AWS SSM connection tool: an interactive inventory browser plus
iTerm2-like multi-pane / broadcast sessions. No server, no inbound port 22 — access control
and audit stay on AWS (IAM / CloudTrail).
</p>

<p align="center">
  <a href="https://siansiansu.github.io/skua/">Documentation</a> ·
  <a href="https://siansiansu.github.io/skua/guide/installation">Install</a> ·
  <a href="https://siansiansu.github.io/skua/guide/quick-start">Quick start</a>
</p>

## Install

```sh
brew install siansiansu/tap/skua        # Homebrew (macOS / Linux)
```

```sh
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/siansiansu/skua/releases/latest/download/skua-installer.sh | sh
```

```sh
cargo install skua                      # from source (Rust 1.90+)
```

Prebuilt binaries for macOS / Linux (x86_64 + arm64) are on the
[releases page](https://github.com/siansiansu/skua/releases).

## Features

- **Inventory browser** — EC2 detail + SSM reachability, sorting,
  nested/reverse filtering, vim-style jumps
- **Multi-pane sessions** — mark hosts with `Space`, `s` opens split panes of
  live SSM shells; leader `Ctrl+b` for broadcast, layouts, zoom, scrollback
- **SSH-over-SSM** via `--ssh-config` — ssh/scp/rsync/VSCode Remote with no
  open port 22 ([guide](./docs/guide/ssh-over-ssm.md))
- AWS profile picker + in-app switching, mouse support, reboot, help,
  auto-refresh, `--dry-run`, shell completion

## Prerequisites

- [`aws` CLI](https://docs.aws.amazon.com/cli/) and
  [`session-manager-plugin`](https://docs.aws.amazon.com/systems-manager/latest/userguide/session-manager-working-with-install-plugin.html)
- IAM (minimal): `ssm:StartSession` / `TerminateSession` / `ResumeSession`,
  `ssm:DescribeInstanceInformation`, `ec2:DescribeInstances`. Optional:
  `ec2:DescribeSecurityGroups` / `DescribeSubnets` / `DescribeVpcs` (detail),
  `ec2:RebootInstances`, `ssm:GetParameter` (update check),
  `ec2-instance-connect:SendSSHPublicKey` (`--ssh-config` ephemeral mode).
  Missing describe-class permissions degrade gracefully with the reason in
  the status bar — never a hard failure.

## Run

```sh
skua                                        # opens the profile picker
skua --profile prod --region ap-northeast-1
skua --dry-run --profile prod               # inventory check, no TTY
```

To build from source: `cargo build --release` (Rust 1.90+) →
`target/release/skua`.

Press `?` in-app for the full keybindings.

## Config

Optional `~/.config/skua/config.yaml` — default profile/region, auto-refresh,
leader key. CLI flags override it. See
[`config.example.yaml`](./config.example.yaml).

## Docs

Full documentation lives at
**[siansiansu.github.io/skua](https://siansiansu.github.io/skua/)** —
installation, quick start, keybindings, configuration, and
[SSH / scp / rsync over SSM](./docs/guide/ssh-over-ssm.md).
