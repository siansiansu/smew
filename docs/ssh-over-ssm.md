# SSH / scp / rsync over SSM

`ssh`, `scp`, `rsync`, `sftp` and VSCode Remote work by tunneling SSH through an
SSM session (no inbound port 22 is opened). Add the config once:

```sh
skua --ssh-config --profile prod --region ap-northeast-1 >> ~/.ssh/config
```

Then use instance ids as hostnames:

```sh
ssh ec2-user@i-0abc...
scp ./file ec2-user@i-0abc...:/tmp/
rsync -av ./dir/ ubuntu@i-0abc...:/srv/
```

**Default mode is ephemeral (EC2 Instance Connect)**: each connection pushes a
60-second temporary public key — no permanent `authorized_keys` entry, better
for least-standing-access. Requires the `ec2-instance-connect` package on the
host (preinstalled on AL2/AL2023/Ubuntu; see
[ec2-instance-connect-oracle-linux.md](./ec2-instance-connect-oracle-linux.md)
for Oracle Linux) and IAM `ec2-instance-connect:SendSSHPublicKey` +
`ssm:StartSession` on `AWS-StartSSHSession`.

- `--ssh-key <path>` — public key to push (default: auto-detected `~/.ssh/id_*.pub`).
- `--ssh-static` — use a key already in the host's `authorized_keys` instead
  (no Instance Connect / no `SendSSHPublicKey` permission needed).

Either way `sshd` must be running on the host. The detail view (`d`) shows the
ready commands per host.
