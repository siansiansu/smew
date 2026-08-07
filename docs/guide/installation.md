# Installation

smew ships as a single static binary for macOS and Linux (both x86_64 and
arm64).

## Homebrew (macOS / Linux)

```sh
brew install siansiansu/tap/smew
```

## Shell installer

```sh
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/siansiansu/smew/releases/latest/download/smew-installer.sh | sh
```

## Cargo

```sh
cargo install smew        # builds from source (Rust 1.88+)
cargo binstall smew       # or: fetch the prebuilt release binary
```

## Prebuilt binaries

Tarballs with SHA-256 checksums for every release are on the
[GitHub Releases page](https://github.com/siansiansu/smew/releases).

## From source

```sh
git clone https://github.com/siansiansu/smew && cd smew
cargo build --release     # → target/release/smew
```

## Docker

The repo ships a multi-stage `Dockerfile` whose image bundles smew with the
AWS CLI v2 and the session-manager-plugin (nothing to install on the host)
and runs as the non-root user `smew`. It is not published to any registry —
build it locally:

```sh
make docker-build         # or: docker build -t smew .
```

Run it with your AWS config mounted read-only (`make docker-run` does the
same):

```sh
docker run -it --rm \
  -v $HOME/.aws:/home/smew/.aws:ro \
  -e AWS_PROFILE -e AWS_REGION \
  smew
```

For SSO profiles, run `aws sso login` on the **host** first — the cached
token under `~/.aws/sso/` comes along with the same mount.

## Prerequisites

smew drives the official AWS tooling — you need:

- the [`aws` CLI](https://docs.aws.amazon.com/cli/)
- the [`session-manager-plugin`](https://docs.aws.amazon.com/systems-manager/latest/userguide/session-manager-working-with-install-plugin.html)

```sh
brew install awscli session-manager-plugin
```

(The [Docker image](#docker) already includes both. `smew --dev` needs
neither — it runs the whole TUI offline against a mock inventory.)

### IAM permissions

Minimal: `ssm:StartSession` / `TerminateSession` / `ResumeSession`,
`ssm:DescribeInstanceInformation`, `ec2:DescribeInstances`.

Optional, feature by feature:

| Permission | Enables |
| --- | --- |
| `ec2:DescribeSecurityGroups` / `DescribeSubnets` / `DescribeVpcs` | richer detail view |
| `ec2:RebootInstances` | the reboot action (`R`) |
| `cloudwatch:GetMetricData` / `ListMetrics` | the `%CPU` / `%MEM` columns (memory also needs the [CloudWatch agent](https://docs.aws.amazon.com/AmazonCloudWatch/latest/monitoring/Install-CloudWatch-Agent.html) on the hosts; without it the column shows `n/a`) |
| `ssm:GetParameter` | the update-available badge |
| `ec2-instance-connect:SendSSHPublicKey` | `--ssh-config` ephemeral mode |

Missing describe-class permissions degrade gracefully — the reason shows in
the status bar, never a hard failure.

## Shell completion

Tab-completion of `--profile` values (reads `~/.aws/config` +
`~/.aws/credentials` directly). The files ship in the repo's
[`completions/`](https://github.com/siansiansu/smew/tree/main/completions)
directory.

::: code-group

```sh [zsh]
mkdir -p ~/.zsh/completions && cp completions/_smew ~/.zsh/completions/
# in ~/.zshrc, before compinit:  fpath=(~/.zsh/completions $fpath)
```

```sh [bash]
cp completions/smew.bash ~/.smew-completion.bash
echo 'source ~/.smew-completion.bash' >> ~/.bashrc
```

:::
