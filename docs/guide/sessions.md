# Sessions

## SSM shell (`s`)

`s` on a `Connected` host opens an interactive shell through SSM. No
inbound ports, no key management; the session is logged in CloudTrail
like any other API call.

Mark hosts with `Space` first and `s` opens all of them as tiled panes in
one session. Each pane is its own SSM session on its own PTY.

Under the hood the pane runs `smew ssm-session`, which calls
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

## Panes, broadcast, layouts

Inside a session the keyboard belongs to the remote shell. The leader key
(default `Ctrl+b`, configurable) prefixes pane commands:

| Leader + | Action |
| --- | --- |
| `h` `l` `j` `k` / arrows | move focus between panes |
| `space` | toggle the focused pane in the broadcast group (≥2 members broadcasts 🔊) |
| `b` | broadcast to all panes / clear the group |
| `v` | cycle layout: columns → rows → grid |
| `z` | zoom the focused pane full-screen |
| `[` | scroll the pane's history; `q` / `Esc` / `]` exits |
| `a` | add a pane (pick another host from the list) |
| `x` | close the focused pane |
| `d` | close the whole session (confirms) |
| leader twice | send a literal leader key through |

With broadcast on, keystrokes go to every grouped pane: fleet-wide
commands with nothing installed server-side. Typing `exit` in a shell
closes that pane; the last pane closing returns you to the list.

## Port forwarding (`F`)

`F` opens the port-forward form:

- leave **remote host** empty to reach a port on the instance itself
- set it to reach something private *through* the instance — an RDS
  endpoint, an internal service
- **local port** defaults to the remote port

The tunnel runs as a pane in the session, next to your shells; close the
pane to end it. Example: forward local `15432` to
`mydb.xxxx.rds.amazonaws.com:5432` via any host in the VPC, then
`psql -h 127.0.0.1 -p 15432`.

## ssh / scp / rsync from your terminal

For plain `ssh i-0abc…` outside the TUI (and `scp`, `rsync`, editor
remotes), generate the ProxyCommand block once — see
[SSH over SSM](/guide/ssh-over-ssm).
