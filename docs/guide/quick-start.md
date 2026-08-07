# Quick start

## Launch

```sh
skua                                        # opens the AWS profile picker
skua --profile prod --region ap-northeast-1 # skip the picker
skua --dry-run --profile prod               # print inventory as a table, no TUI
```

With no flags, skua lists the profiles from `~/.aws/config` — pick one and it
loads the region's EC2 inventory with live SSM reachability (🟢 reachable /
🔴 not).

## Browse the inventory

- `↑/↓` or `j/k` move; `gg`/`G` jump to top/bottom; `10gg` jumps to row 10
- `/` (or `f`) filters by name / id / ip / type / az / vpc / tag; prefix a
  term with `!` to exclude; `Enter` nests another filter level inside the
  current results, `Esc` pops it
- `N` / `S` / `T` / `A` / `P` sort by name / state / type / age / ip
- `Enter` or `d` opens the detail view; `c` switches AWS profile; `r`
  refreshes now

## Connect

- `s` on a host opens an SSM shell.
- `Space` marks multiple hosts, then `s` opens them **as split panes** in one
  session — an iTerm2-like multiplexer over SSM.

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
