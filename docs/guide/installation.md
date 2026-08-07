# Installation

skua ships as a single static binary for macOS and Linux (both x86_64 and
arm64).

## Homebrew (macOS / Linux)

```sh
brew install siansiansu/tap/skua
```

## Shell installer

```sh
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/siansiansu/skua/releases/latest/download/skua-installer.sh | sh
```

## Cargo

```sh
cargo install skua        # builds from source (Rust 1.90+)
cargo binstall skua       # or: fetch the prebuilt release binary
```

## Prebuilt binaries

Tarballs with SHA-256 checksums for every release are on the
[GitHub Releases page](https://github.com/siansiansu/skua/releases).

## From source

```sh
git clone https://github.com/siansiansu/skua && cd skua
cargo build --release     # → target/release/skua
```

## Prerequisites

skua drives the official AWS tooling — you need:

- the [`aws` CLI](https://docs.aws.amazon.com/cli/)
- the [`session-manager-plugin`](https://docs.aws.amazon.com/systems-manager/latest/userguide/session-manager-working-with-install-plugin.html)

```sh
brew install awscli session-manager-plugin
```

### IAM permissions

Minimal: `ssm:StartSession` / `TerminateSession` / `ResumeSession`,
`ssm:DescribeInstanceInformation`, `ec2:DescribeInstances`.

Optional, feature by feature:

| Permission | Enables |
| --- | --- |
| `ec2:DescribeSecurityGroups` / `DescribeSubnets` / `DescribeVpcs` | richer detail view |
| `ec2:RebootInstances` | the reboot action (`R`) |
| `ssm:GetParameter` | the update-available badge |
| `ec2-instance-connect:SendSSHPublicKey` | `--ssh-config` ephemeral mode |

Missing describe-class permissions degrade gracefully — the reason shows in
the status bar, never a hard failure.

## Shell completion

Tab-completion of `--profile` values (reads `~/.aws/config` +
`~/.aws/credentials` directly). The files ship in the repo's
[`completions/`](https://github.com/siansiansu/skua/tree/main/completions)
directory.

::: code-group

```sh [zsh]
mkdir -p ~/.zsh/completions && cp completions/_skua ~/.zsh/completions/
# in ~/.zshrc, before compinit:  fpath=(~/.zsh/completions $fpath)
```

```sh [bash]
cp completions/skua.bash ~/.skua-completion.bash
echo 'source ~/.skua-completion.bash' >> ~/.bashrc
```

:::
