# Quick start

## Launch

```sh
smew                                        # opens the AWS profile picker
smew --profile prod --region ap-northeast-1 # skip the picker
smew --dry-run --profile prod               # print inventory as a table, no TUI
smew --dev                                  # developer mode: no AWS needed
```

With no flags, smew lists the profiles from `~/.aws/config` — the picker
fuzzy-filters as you type. Pick one and it loads the region's EC2
inventory: the SSM column reads `Connected` for hosts reachable right now,
and the top panel shows who you are (account id + role/user from STS)
alongside the region and instance counts.

`--dev` runs the whole TUI against a built-in mock account — a 30-host
fleet plus fixtures for every resource view (orphaned volumes, idle EIPs, a
filling SQS DLQ, an under-capacity ASG, a rolled-back stack…): no
credentials, no network. "Sessions" open your local shell, run-command
pretends to succeed, so filtering, resource views, describe dashboards,
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
- `Enter` or `d` opens the describe dashboard; `c` switches AWS profile;
  `r` refreshes now
- `Esc` / `q` go back (close the filter, pop a drill-down, leave a view);
  quitting is `:q` or `Ctrl+c`

## Switch views with `:` (command mode)

Press `:` for a command prompt with inline completion (`Tab`
accepts, `↑` recalls the last command):

- `:` + an AWS abbreviation switches the table to another resource view —
  the EC2 family (`:vol` `:snap` `:sg` `:vpc` `:subnet` `:eni` `:eip`
  `:ami`) and the rest of the account: `:s3` `:lambda` `:asg` `:rds` `:ddb`
  `:elb` `:sqs` `:sns` `:ecs` `:eks` `:cfn`. The usual AWS aliases work too
  (`:ebs`, `:fn`, `:lb`, `:stacks`, …); `:ec2` returns to instances and `?`
  lists every view by category
- in these views the same navigation and `/` filter apply; things that cost
  money for nothing — unattached volumes, idle EIPs, orphaned ENIs — and
  trouble — subnets near IP exhaustion, filling DLQs, under-capacity ASGs,
  rolled-back stacks — are highlighted
- `Enter` (or `d`) opens the **describe dashboard**: bordered panels
  grouped like the AWS console (Details / Networking / Security / Storage /
  Monitoring / Tags), sized to fit one screen instead of one long column
- `Enter` on a **vpc / subnet / sg** row drills down instead: the instances
  view opens pre-filtered to that resource; `Esc` goes back to where you were
- `:profile <name>` (fuzzy-matched) or `:ctx` switches AWS account;
  `:help` and `:q` do what they say

## Connect

- `s` on a host opens a full-screen SSM shell — no open ports, works for
  any `Connected` host. `exit` (or leader `x`/`d`) ends it.
- `i` logs in over plain SSH instead: smew pushes a 60-second key via EC2
  Instance Connect, then runs `ssh <user>@<ip>` (public IP preferred). For
  hosts whose port 22 you can reach — no SSM agent required. The login
  user comes from `ssh_user` in the config (default `ec2-user`).

## Run a command on many hosts

`Space` marks hosts, `x` opens a multi-line command editor (`Enter` adds
a line, pasting a script works), `Ctrl+s` sends it via SSM Run Command
(`AWS-RunShellScript`). The results page polls every host and shows
per-host status + output; `x` there edits and re-runs, `Esc` goes back.
With nothing marked, `x` targets the host under the cursor.

## Port forward

`F` on a host opens the port-forward form:

- leave **remote host** empty to forward to a port on the instance itself
  (e.g. a local web UI on `:8080`)
- set it to reach a private endpoint *via* the instance — an RDS database,
  an internal service — with no bastion and no open ports
- **local port** defaults to the same as the remote port

The tunnel runs as its own session; close it (leader `x`/`d`) to end it.
Example: forward local `15432` to `mydb.xxxx.rds.amazonaws.com:5432` via a
host in the VPC, then `psql -h 127.0.0.1 -p 15432` locally.

Inside a session, press the leader (default `Ctrl+b`), then `[` for
scrollback, `x`/`d` to close the session, `?` for help.

Press `?` anywhere for the complete [keybinding reference](/guide/keybindings).

## Next steps

- [Configuration](/guide/configuration) — default profile/region,
  auto-refresh, leader key, mouse
- [SSH / scp / rsync over SSM](/guide/ssh-over-ssm) — file transfer and
  VSCode Remote with no open port 22
