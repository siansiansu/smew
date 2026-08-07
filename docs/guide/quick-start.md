# Quick start

## Launch

```sh
smew                                        # opens the AWS profile picker
smew --profile prod --region ap-northeast-1 # skip the picker
smew --dry-run --profile prod               # print inventory as a table, no TUI
smew --dev                                  # developer mode: no AWS needed
```

With no flags, smew lists the profiles from `~/.aws/config` — the picker
filters fzf-style as you type. Pick one and it loads the region's EC2
inventory: the SSM column reads `Connected` for hosts reachable right now,
and the top panel shows who you are (account id + role/user from STS)
alongside the region and instance counts.

`--dev` runs the whole TUI against a built-in 30-host mock fleet (every
lifecycle state, lost/ancient SSM agents, a second VPC, orphaned volumes,
idle EIPs…): no credentials, no network. "Sessions" open your local shell
in the panes, so filtering, resource views, multi-pane broadcast,
port-forward forms and every keybinding can be tried (or demoed) offline.
Combine with `--dry-run` to print the mock table.

## Browse the inventory

- `↑/↓` or `j/k` move; `gg`/`G` jump to top/bottom; `10gg` jumps to row 10
- `/` (or `f`) filters by name / id / ip / type / az / vpc / sg / tag;
  space-separated terms are AND-ed, prefix a term with `!` to exclude;
  `Enter` applies the query and closes the input, `Esc` clears it
- `N` / `S` / `T` / `A` / `P` sort by name / state / type / age / ip
  (`C` / `M` sort by CPU / memory when the
  [`metrics`](/guide/configuration) columns are on)
- `Enter` or `d` opens the detail view; `c` switches AWS profile; `r`
  refreshes now

## Switch views with `:` (command mode)

Press `:` for a k9s-style command prompt with inline completion (`Tab`
accepts, `↑` recalls the last command):

- `:vol` `:snap` `:sg` `:vpc` `:subnet` `:eni` `:eip` `:ami` switch the
  table to EBS volumes, snapshots, security groups, VPCs, subnets, network
  interfaces, Elastic IPs or AMIs — the usual AWS aliases work too
  (`:ebs`, `:sub`, `:images`, …); `:ec2` returns to instances
- in these views the same navigation and `/` filter apply; `d` opens a
  key/value detail; things that cost money for nothing — unattached
  volumes, idle EIPs, orphaned ENIs — and subnets close to IP exhaustion
  are highlighted
- `Enter` on a **vpc / subnet / sg** row drills down: the instances view
  opens pre-filtered to that resource; `Esc` goes back to where you were
- `:profile <name>` (fuzzy-matched) or `:ctx` switches AWS account;
  `:help` and `:q` do what they say

## Connect

- `s` on a host opens an SSM shell.
- `Space` marks multiple hosts, then `s` opens them **as split panes** in one
  session — a tmux-like multiplexer over SSM.

## Port forward

`F` on a host opens the port-forward form:

- leave **remote host** empty to forward to a port on the instance itself
  (e.g. a local web UI on `:8080`)
- set it to reach a private endpoint *via* the instance — an RDS database,
  an internal service — with no bastion and no open ports
- **local port** defaults to the same as the remote port

The tunnel runs as a session pane (added to the current session, or a new
one), so it lives alongside your shells; close the pane to end it. Example:
forward local `15432` to `mydb.xxxx.rds.amazonaws.com:5432` via a host in
the VPC, then `psql -h 127.0.0.1 -p 15432` locally.

Inside a session, press the leader (default `Ctrl+b`), then:

| Key | Action |
| --- | --- |
| `space` | toggle the focused pane in the broadcast group (≥2 auto-broadcasts 🔊) |
| `b` | broadcast to all panes / clear |
| `v` | cycle layout: columns → rows → grid |
| `z` | zoom the focused pane |
| `d` | close the whole session |

Type in a broadcast session and the keystrokes go to every grouped pane —
fleet-wide commands without any server-side tooling.

Press `?` anywhere for the complete [keybinding reference](/guide/keybindings).

## Next steps

- [Configuration](/guide/configuration) — default profile/region,
  auto-refresh, leader key, mouse
- [SSH / scp / rsync over SSM](/guide/ssh-over-ssm) — file transfer and
  VSCode Remote with no open port 22
