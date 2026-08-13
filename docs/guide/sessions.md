# Sessions

## SSM shell (`s`)

`s` on a `Connected` host opens an interactive shell through SSM — one
full-screen session, k9s-style. No inbound ports, no key management; the
session is logged in CloudTrail like any other API call.

Under the hood the session runs `smew ssm-session`, which calls
`ssm:StartSession` and hands the channel to `session-manager-plugin` —
the aws CLI is not involved.

## SSH login (`i`)

`i` connects over plain SSH instead of SSM: smew pushes a 60-second
public key with `ec2-instance-connect:SendSSHPublicKey`, then runs
`ssh <user>@<ip>` (public IP if there is one, else private). Use it when
the SSM agent is missing or broken and you can reach port 22.

- the login user comes from `ssh_user` in the
  [config](/guide/configuration) (default `ec2-user`)
- the pushed key is your `~/.ssh/id_*.pub`; the matching private key
  authenticates the ssh process
- the host needs `sshd` and the `ec2-instance-connect` package
  (preinstalled on Amazon Linux and Ubuntu AMIs)

## Inside a session

The keyboard belongs to the remote shell. The leader key (default
`Ctrl+b`, configurable) prefixes session commands:

| Leader + | Action |
| --- | --- |
| `[` | scroll the session's history; `q` / `Esc` / `]` exits |
| `x` / `d` | close the session (confirms) |
| leader twice | send a literal leader key through |

Typing `exit` in the shell ends the session and returns you to the list.

## Run a command on many hosts

For "run this on several machines at once" you don't need interactive
shells: mark hosts with `Space`, press `x`, type the script (multi-line —
`Enter` adds a line, pasting a script works) and send it with `Ctrl+s`.

The script goes out through [Run Command](https://docs.aws.amazon.com/systems-manager/latest/userguide/run-command.html)
(`ssm:SendCommand`, `AWS-RunShellScript`), and the results page polls each
host until it finishes, showing per-host status and output. `x` there
reopens the editor with the same script and targets for a quick re-run.
Nothing is installed server-side; every invocation is logged in
CloudTrail.

## Port forwarding (`F`)

`F` opens the port-forward form:

- leave **remote host** empty to reach a port on the instance itself
- set it to reach something private *through* the instance — an RDS
  endpoint, an internal service
- **local port** defaults to the remote port

The tunnel runs as its own session; close it (leader `x`/`d`) to end the
forward. Example: forward local `15432` to
`mydb.xxxx.rds.amazonaws.com:5432` via any host in the VPC, then
`psql -h 127.0.0.1 -p 15432`.

## ssh / scp / rsync from your terminal

For plain `ssh i-0abc…` outside the TUI (and `scp`, `rsync`, editor
remotes), generate the ProxyCommand block once — see
[SSH over SSM](/guide/ssh-over-ssm).
