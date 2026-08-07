---
layout: home

hero:
  name: skua
  text: AWS SSM sessions, multiplexed.
  tagline: >-
    An interactive EC2/SSM inventory browser with iTerm2-like split panes and
    broadcast. No server, no inbound port 22 — access control and audit stay
    on AWS.
  image:
    src: /logo.svg
    alt: skua
  actions:
    - theme: brand
      text: Get Started
      link: /guide/installation
    - theme: alt
      text: View on GitHub
      link: https://github.com/siansiansu/skua

features:
  - icon: 🔭
    title: Inventory browser
    details: >-
      EC2 detail + SSM reachability at a glance, with sorting, nested and
      reverse filtering, and vim-style jumps.
  - icon: 🪟
    title: Multi-pane sessions
    details: >-
      Mark hosts and open them as split panes of live SSM shells. Leader-key
      broadcast, layouts, zoom, and scrollback — like tmux or iTerm2.
  - icon: 🔐
    title: SSH over SSM
    details: >-
      ssh, scp, rsync, and VSCode Remote through an SSM tunnel with one
      generated ssh_config block. Ephemeral keys by default.
  - icon: 🛡️
    title: IAM-native
    details: >-
      No agent, no bastion, no open port 22. Permissions are plain IAM;
      every session lands in CloudTrail.
---

## Install

::: code-group

```sh [Homebrew]
brew install siansiansu/tap/skua
```

```sh [Shell]
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/siansiansu/skua/releases/latest/download/skua-installer.sh | sh
```

```sh [Cargo]
cargo install skua
```

:::

Then run `skua` — it opens the AWS profile picker. See the
[quick start](/guide/quick-start) for the two-minute tour.
