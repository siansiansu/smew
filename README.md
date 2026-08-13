<p align="center">
  <img src="./docs/public/logo.svg" alt="smew logo" width="140">
</p>

<h1 align="center">smew</h1>

<p align="center">
A terminal UI for AWS. Browse EC2, S3, Lambda, RDS and the rest of your
account from one table, shell into instances over SSM, and run a command
across many hosts at once with per-host results. No bastion, no inbound
port 22; access control and audit stay on AWS.
</p>

<p align="center">
  <a href="https://crates.io/crates/smew"><img src="https://img.shields.io/crates/v/smew.svg" alt="crates.io"></a>
  <a href="https://github.com/siansiansu/smew/actions/workflows/release.yml"><img src="https://github.com/siansiansu/smew/actions/workflows/release.yml/badge.svg" alt="Release"></a>
  <a href="https://siansiansu.github.io/smew/"><img src="https://img.shields.io/badge/docs-siansiansu.github.io%2Fsmew-blue.svg" alt="Documentation"></a>
  <a href="./LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License: MIT"></a>
</p>

---

## Documentation

The full manual (configuration, IAM, SSH recipes, skins) lives at
[siansiansu.github.io/smew](https://siansiansu.github.io/smew/). Everything
below is the short version.

---

## Installation

- Homebrew (macOS / Linux):

  ```sh
  brew install siansiansu/tap/smew
  ```

- Shell installer:

  ```sh
  curl --proto '=https' --tlsv1.2 -LsSf \
    https://github.com/siansiansu/smew/releases/latest/download/smew-installer.sh | sh
  ```

- Cargo:

  ```sh
  cargo install smew          # build from source (Rust 1.88+)
  cargo binstall smew         # or fetch the prebuilt binary
  ```

- Tarballs with checksums for macOS and Linux (x86_64 + arm64) are on the
  [releases page](https://github.com/siansiansu/smew/releases).

- Docker: `make docker-build && make docker-run` (bundles the session
  plugin, mounts `~/.aws` read-only).

---

## PreFlight checks

- Opening sessions needs AWS's
  [session-manager-plugin](https://docs.aws.amazon.com/systems-manager/latest/userguide/session-manager-working-with-install-plugin.html)
  on your machine: `brew install --cask session-manager-plugin`. That is
  the only external binary; smew makes every API call itself, so the aws
  CLI is not required. Browsing works without the plugin.
- Credentials come from the standard chain: `~/.aws/config` profiles
  (including SSO and assume-role), env vars, `credential_process`.
  Whatever works for `aws sts get-caller-identity` works here.
- Minimal IAM to browse and connect: `ec2:DescribeInstances`,
  `ssm:DescribeInstanceInformation`, `ssm:StartSession`. Everything else
  degrades to a status-bar warning, not a failure. The
  [permission table](https://siansiansu.github.io/smew/guide/installation#iam-permissions)
  maps each optional permission to the feature it unlocks.

---

## The command line

```sh
# run with the profile picker
smew

# straight into a profile/region
smew --profile prod --region ap-northeast-1

# print identity + inventory as a table and exit (no TUI)
smew --dry-run --profile prod

# full TUI against a built-in mock account: no credentials, no network
smew --dev

# generate the ~/.ssh/config block for ssh/scp/rsync over SSM
smew --ssh-config >> ~/.ssh/config

# version with build info
smew --version
```

---

## Key bindings

The `?` page inside the app always matches your build. The core set:

| Action | Key | Comment |
| --- | --- | --- |
| Move / jump | `↑↓` `j k`, `gg` `G`, `10gg` | vim motions; `#` column shows row numbers |
| Switch resource view | `:` then an alias | `:s3`, `:rds`, `:sg`, … see the table below |
| Filter | `/` text | substring; space = AND, `!term` excludes |
| Sort | `N` `S` `T` `A` `P` (`C` `M`) | name / state / type / age / ip (cpu / mem); repeat to reverse |
| Describe | `Enter` or `d` | dashboard of panels, grouped like the console |
| Drill down | `Enter` on a vpc / subnet / sg | its instances, pre-filtered; `Esc` goes back |
| Connect (SSM) | `s` | full-screen shell to the selected host |
| SSH login | `i` | EC2 Instance Connect key push, then `ssh user@ip` |
| Run command | `Space` marks, `x` runs | multi-line script via Run Command; per-host results |
| Port forward | `F` | to the instance, or through it to e.g. RDS |
| Reboot | `R` | running hosts, with confirmation |
| Switch AWS profile | `c`, `:ctx`, `:profile name` | fuzzy-matched |
| Refresh now | `r` / `Ctrl+r` | on top of the 10s auto-refresh |
| Help | `?` / `Ctrl+a` | every view and key |
| Back | `Esc` / `q` | close prompt, pop drill-down, leave view |
| Quit | `:q` / `Ctrl+c` | |

Inside a session, keys go to the remote shell; the leader (default
`Ctrl+b`) prefixes session commands: scrollback, close session.

---

## Resource views

`:` + an abbreviation switches the main table. The usual AWS aliases work
(`:ebs`, `:fn`, `:lb`, `:sub`, `:stacks`, …), and `:ec2` is home.

| AWS category | Views |
| --- | --- |
| Compute | `ec2` instances · `ami` images · `lambda` functions · `asg` Auto Scaling groups |
| Storage | `vol` EBS volumes · `snap` snapshots · `s3` buckets |
| Database | `rds` DB instances · `ddb` DynamoDB tables |
| Networking & Content Delivery | `vpc` VPCs · `subnet` subnets · `eni` network interfaces · `eip` Elastic IPs · `elb` load balancers |
| Security, Identity & Compliance | `sg` security groups |
| Application Integration | `sqs` queues · `sns` topics |
| Containers | `ecs` clusters · `eks` clusters |
| Management & Governance | `cfn` CloudFormation stacks |

Rows that cost money for nothing or need attention light up orange or
red: unattached volumes, idle Elastic IPs, orphaned network interfaces,
subnets close to IP exhaustion, dead-letter queues with messages in them,
Auto Scaling groups running under their desired capacity, topics nobody
subscribes to, stacks stuck in rollback, RDS instances that are not
`available`.

`Enter` on any row opens the describe page: bordered panels named the way
the console names its tabs (Details, Networking, Security, Storage,
Monitoring, Tags), packed into columns so the record fits one screen. On
a vpc, subnet or security group, `Enter` drills into the instances inside
it instead; `d` always describes.

---

## Sessions & run command

`s` opens a full-screen SSM shell to the selected host (leader `[`
scrolls its history; `exit` or leader `x`/`d` ends it).

To run something on several machines at once, mark them with `Space` and
press `x`: a multi-line editor opens, `Ctrl+s` sends the script through
SSM Run Command (`AWS-RunShellScript`), and a results page polls each
host until it finishes — per-host status and output, `x` to edit and
re-run. No interactive session needed, nothing installed server-side.

`i` logs in over plain SSH instead of SSM: smew pushes a 60-second key
via EC2 Instance Connect and runs `ssh user@ip`. Useful when the SSM
agent is absent or broken and port 22 is reachable. Set `ssh_user` in the
config if your images are not `ec2-user`.

`F` starts an SSM port-forwarding pane: a local port to the instance, or
through the instance to something private like an RDS endpoint.

`smew --ssh-config` prints a `ProxyCommand` block that makes `ssh`,
`scp`, `rsync` and editor remotes work against instance ids with no open
port 22 (ephemeral Instance Connect keys by default, `--ssh-static` for
authorized_keys).

---

## Configuration

Optional file at `~/.config/smew/config.yaml`. Unknown keys are an
error, so typos fail loudly. Defaults shown:

```yaml
# Profile/region to open with (empty profile = show the picker).
default_profile: ""
default_region: ap-northeast-1

# Auto-refresh for the active view. Floor 5s; "0" turns it off.
refresh_interval: "10s"

# Leader (prefix) key inside a split session.
session_leader: "ctrl+b"

# Wheel scroll, click to select, double-click to connect.
mouse: true

# SSM parameter checked for a newer version (badge in the header).
version_param: "/smew/latest-version"
disable_version_check: false

# Built-in: default, dracula, gruvbox-dark, nord — or a file in
# ~/.config/smew/skins/<name>.yaml (the repo's skins/ has examples).
skin: ""

# Login user for the SSH connect action (i) and the Instance Connect push.
ssh_user: "ec2-user"

# %CPU/%MEM columns from CloudWatch (free-tier APIs, 5-minute polling).
metrics: false
```

---

## Developer mode

`smew --dev` runs the entire TUI against a built-in mock account: a
30-host fleet plus fixtures for every resource view, with the edge cases
included (an errored volume, a half-finished snapshot, a stack in
rollback). Sessions open your local shell. Nothing touches the network,
so every flow above can be tried, tested or demoed offline.

---

## License

[MIT](./LICENSE)
