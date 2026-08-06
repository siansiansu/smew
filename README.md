<p align="center">
  <img src="./docs/logo.svg" alt="skua logo" width="140">
</p>

<h1 align="center">skua</h1>

<p align="center">
A local AWS SSM connection tool: an interactive inventory browser plus
iTerm2-like multi-pane / broadcast sessions. No server, no inbound port 22 — access control
and audit stay on AWS (IAM / CloudTrail).
</p>

## Features

- **Inventory browser** — EC2 detail + SSM reachability, sorting,
  nested/reverse filtering, vim-style jumps
- **Multi-pane sessions** — mark hosts with `Space`, `s` opens split panes of
  live SSM shells; leader `Ctrl+b` for broadcast, layouts, zoom, scrollback
- **SSH-over-SSM** via `--ssh-config` — ssh/scp/rsync/VSCode Remote with no
  open port 22 ([guide](./docs/ssh-over-ssm.md))
- AWS profile picker + in-app switching, mouse support, reboot, help,
  auto-refresh, `--dry-run`, shell completion

## Prerequisites

- Rust 1.90+ (to build)
- [`aws` CLI](https://docs.aws.amazon.com/cli/) and
  [`session-manager-plugin`](https://docs.aws.amazon.com/systems-manager/latest/userguide/session-manager-working-with-install-plugin.html)
- IAM (minimal): `ssm:StartSession` / `TerminateSession` / `ResumeSession`,
  `ssm:DescribeInstanceInformation`, `ec2:DescribeInstances`. Optional:
  `ec2:DescribeSecurityGroups` / `DescribeSubnets` / `DescribeVpcs` (detail),
  `ec2:RebootInstances`, `ssm:GetParameter` (update check),
  `ec2-instance-connect:SendSSHPublicKey` (`--ssh-config` ephemeral mode).
  Missing describe-class permissions degrade gracefully with the reason in
  the status bar — never a hard failure.

## Build & run

```sh
cargo build --release              # → target/release/skua

./target/release/skua                                # opens the profile picker
./target/release/skua --profile prod --region ap-northeast-1
./target/release/skua --dry-run --profile prod       # inventory check, no TTY
```

Press `?` in-app for the full keybindings.

## Config

Optional `~/.config/skua/config.yaml` — default profile/region, auto-refresh,
leader key. CLI flags override it. See
[`config.example.yaml`](./config.example.yaml).

## Docs

- [docs/ssh-over-ssm.md](./docs/ssh-over-ssm.md) — SSH / scp / rsync over SSM
