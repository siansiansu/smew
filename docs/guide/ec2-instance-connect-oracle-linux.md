# EC2 Instance Connect on Oracle Linux (for ephemeral SSH-over-SSM)

`skua --ssh-config` defaults to **ephemeral** mode: each `ssh`/`scp` connection
pushes a 60-second temporary public key via **EC2 Instance Connect (EIC)** — no
permanent `authorized_keys` entry. EIC is preinstalled on Amazon Linux / Ubuntu
but **not on Oracle Linux** (RHEL family), so it must be baked into the golden
image. This guide covers install + config + the common gotchas.

> If you'd rather not run EIC, use `skua --ssh-config --ssh-static` instead
> (your public key must already be in the host's `authorized_keys`). No EIC and
> no `ec2-instance-connect:SendSSHPublicKey` IAM needed.

## How EIC works

- **Client**: `aws ec2-instance-connect send-ssh-public-key` uploads your public
  key; it is valid for ~60 seconds and delivered to the instance via IMDS.
- **Instance**: the `ec2-instance-connect` package sets sshd's
  `AuthorizedKeysCommand` to `/opt/aws/bin/eic_run_authorized_keys`, which
  fetches the pushed key from IMDS during authentication. Key expires after 60s.

Without the sshd `AuthorizedKeysCommand` wired up, pushed keys are never read and
auth fails — installing the RPM alone is **not** enough on Oracle Linux.

## 1. Install the package

Oracle Linux has no EIC RPM in its repos; build it from AWS's open-source repo:

```sh
sudo dnf install -y git make rpm-build          # or yum on older OL
git clone https://github.com/aws/aws-ec2-instance-connect-config.git
cd aws-ec2-instance-connect-config
make rpm
sudo dnf install -y ./ec2-instance-connect-*.rpm
```

Verify:

```sh
rpm -q ec2-instance-connect                      # e.g. ec2-instance-connect-1.1-18.noarch
rpm -ql ec2-instance-connect | grep eic_run_authorized_keys   # → /opt/aws/bin/eic_run_authorized_keys
id ec2-instance-connect                          # the AuthorizedKeysCommandUser must exist
```

## 2. Wire up sshd (the step the RPM may skip on OL)

Check whether sshd already calls the EIC command:

```sh
sudo sshd -T | grep -i authorizedkeyscommand
```

If it prints `authorizedkeyscommand none` (i.e. unset), add a drop-in. Oracle
Linux 8+ `sshd_config` includes `/etc/ssh/sshd_config.d/*.conf`:

```sh
sudo tee /etc/ssh/sshd_config.d/60-ec2-instance-connect.conf >/dev/null <<'EOF'
AuthorizedKeysCommand /opt/aws/bin/eic_run_authorized_keys %u %f
AuthorizedKeysCommandUser ec2-instance-connect
EOF

sudo sshd -t && sudo systemctl restart sshd
```

(If `sshd_config` has no `Include`, append those two lines to
`/etc/ssh/sshd_config` directly instead.)

Verify it took effect:

```sh
sudo sshd -T | grep -i authorizedkeyscommand
# authorizedkeyscommand /opt/aws/bin/eic_run_authorized_keys %u %f
# authorizedkeyscommanduser ec2-instance-connect
```

## 3. SELinux (Oracle Linux is usually Enforcing)

sshd running an `AuthorizedKeysCommand` needs the right SELinux context, or auth
fails silently (denials land in the audit log, not the ssh output):

```sh
sudo getenforce                          # Enforcing?
sudo restorecon -Rv /opt/aws/bin/        # fix contexts on the eic scripts
# if it still fails, look for AVC denials and address them:
sudo ausearch -m avc -ts recent
# (last resort while testing: sudo setenforce 0 — do NOT ship this)
```

## 4. Prerequisites checklist

- [ ] `sshd` running.
- [ ] `ec2-instance-connect` installed and `/opt/aws/bin/eic_run_authorized_keys` present.
- [ ] sshd `AuthorizedKeysCommand` points at it (`sshd -T` confirms).
- [ ] SELinux context OK (no AVC denials).
- [ ] IMDS reachable from the instance (EIC reads the pushed key from metadata).
- [ ] Caller IAM has `ec2-instance-connect:SendSSHPublicKey` and
      `ssm:StartSession` on `AWS-StartSSHSession`.

## 5. End-to-end test

From your laptop (with `skua --ssh-config >> ~/.ssh/config` already applied):

```sh
ssh ec2-user@i-0abc...          # marketplace Oracle Linux default user is ec2-user
scp ./file ec2-user@i-0abc...:/tmp/
```

Connects → EIC + ephemeral works. Auth failure → re-check steps 2 and 3.

## 6. Bake into the golden image

Do steps 1–2 (and any SELinux policy from step 3) in your image pipeline
(Packer/etc.) so every instance supports ephemeral SSH out of the box. No
per-host `authorized_keys` management is then required.
