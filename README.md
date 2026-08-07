<p align="center">
  <img src="./docs/public/logo.svg" alt="smew logo" width="140">
</p>

<h1 align="center">smew</h1>

<p align="center">
k9s for your AWS fleet: browse EC2 and its resources, and open SSM shell
or port-forwarding sessions with split panes and broadcast.
No server, no inbound port 22 — access control and audit stay on AWS.
</p>

<p align="center">
  <a href="https://crates.io/crates/smew"><img src="https://img.shields.io/crates/v/smew.svg" alt="crates.io"></a>
  <a href="https://github.com/siansiansu/smew/actions/workflows/release.yml"><img src="https://github.com/siansiansu/smew/actions/workflows/release.yml/badge.svg" alt="Release"></a>
  <a href="https://siansiansu.github.io/smew/"><img src="https://img.shields.io/badge/docs-siansiansu.github.io%2Fsmew-blue.svg" alt="Documentation"></a>
  <a href="./LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License: MIT"></a>
</p>

## Features

- **Inventory browser** — a k9s-style framed table: EC2 detail, live SSM
  reachability, opt-in %CPU/%MEM, caller identity in the header, sorting,
  filtering, vim-style navigation
- **Resource views** — `:vol` `:snap` `:sg` `:vpc` `:subnet` `:eni` `:eip`
  `:ami` switch the table to the rest of the EC2 family; `Enter` on a
  vpc / subnet / sg drills into its instances; orphaned volumes, idle EIPs
  and subnets near IP exhaustion light up
- **Command mode** — a k9s-style `:` prompt with inline completion; switch
  AWS profiles with `:ctx` / `:profile <name>` (fuzzy) or the fzf-style
  picker
- **Multi-pane sessions** — mark hosts, open split panes of live SSM shells;
  broadcast input, layouts, zoom, scrollback
- **Port forwarding** — SSM port-forwarding sessions from the same list
- **SSH-over-SSM** — ssh / scp / rsync / VS Code Remote with no open port 22
- Skins, mouse support, `--dry-run`, `--dev` mode

## Installation

```sh
brew install siansiansu/tap/smew        # Homebrew (macOS / Linux)
```

```sh
cargo install smew                      # from source (Rust 1.88+)
```

```sh
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/siansiansu/smew/releases/latest/download/smew-installer.sh | sh
```

Prebuilt binaries for macOS / Linux (x86_64 + arm64) are on the
[releases page](https://github.com/siansiansu/smew/releases).
To run in Docker: `make docker-build && make docker-run`.

Requires the [`aws` CLI](https://docs.aws.amazon.com/cli/) and the
[`session-manager-plugin`](https://docs.aws.amazon.com/systems-manager/latest/userguide/session-manager-working-with-install-plugin.html).

## Quick start

```sh
smew                                        # opens the profile picker
smew --profile prod --region ap-northeast-1
```

Press `?` in-app for the full keybindings.

## Documentation

Everything else — configuration, keybindings, skins, IAM permissions, and
SSH / scp / rsync over SSM — lives at
**[siansiansu.github.io/smew](https://siansiansu.github.io/smew/)**.

## License

[MIT](./LICENSE)
