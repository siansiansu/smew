---
layout: home

hero:
  name: smew
  text: AWS SSM sessions, multiplexed.
  tagline: >-
    Browse EC2 and its resources; shell in over SSM with split panes and
    broadcast. No server, no inbound port 22 — access
    control and audit stay on AWS.
  image:
    src: /logo.svg
    alt: smew
  actions:
    - theme: brand
      text: Get Started
      link: /guide/installation
    - theme: alt
      text: View on GitHub
      link: https://github.com/siansiansu/smew

features:
  - icon: 🔭
    title: Inventory browser
    details: >-
      A framed, information-dense table: EC2 detail, live SSM reachability, opt-in
      %CPU/%MEM, caller identity in the header, reverse filtering and
      vim-style jumps.
  - icon: ⌨️
    title: Command mode
    details: >-
      A `:` prompt with inline completion — :vol :sg :vpc :eni
      switch resource views; :ctx / :profile switch AWS accounts with fuzzy
      matching.
  - icon: 🧭
    title: Resource views
    details: >-
      Volumes, snapshots, security groups, VPCs, subnets, ENIs, EIPs and
      AMIs in the same table. Enter drills from a vpc/subnet/sg into its
      instances; orphaned and near-exhausted resources light up.
  - icon: 🪟
    title: Multi-pane sessions
    details: >-
      Mark hosts and open them as split panes of live SSM shells. Leader-key
      broadcast, layouts, zoom, and scrollback — a full multiplexer over SSM.
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
brew install siansiansu/tap/smew
```

```sh [Shell]
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/siansiansu/smew/releases/latest/download/smew-installer.sh | sh
```

```sh [Cargo]
cargo install smew
```

:::

Then run `smew` — it opens the AWS profile picker. See the
[quick start](/guide/quick-start) for the two-minute tour.
