# Keybindings

Press `?` in-app for this reference; the overlay always matches your build
and configured leader key.

## Navigate

| Key | Action |
| --- | --- |
| `↑` / `k`, `↓` / `j` | move up / down |
| `←` / `→` | scroll the table horizontally |
| `gg` / `G` | jump to top / bottom |
| `N gg` / `N G` | jump to row N (e.g. `10gg` → row 10; see the `#` column) |
| `Ctrl+f` / `PgDn`, `Ctrl+b` / `PgUp` | page down / up |
| `Ctrl+d` / `Ctrl+u` | half page down / up |
| `Home` / `End` | jump to top / bottom |

## Commands (`:`)

| Key | Action |
| --- | --- |
| `:` | open the command prompt (inline completion) |
| `Tab` | accept the suggestion; `↑` recalls the last command / cycles |
| `:<view>` | switch resource view — see the table below (AWS aliases work: `ebs`, `fn`, `lb`, `sub`, …) |
| `:profile [name]` / `:ctx` | switch AWS profile — fuzzy-matched with a name, the picker without |
| `:help` / `:q` | help / quit |
| `Esc` | close the prompt, nothing happens |

### Resource views, by AWS category

| Category | Views |
| --- | --- |
| Compute | `:ec2` `:ami` `:lambda` `:asg` |
| Storage | `:vol` `:snap` `:s3` |
| Database | `:rds` `:ddb` |
| Networking & Content Delivery | `:vpc` `:subnet` `:eni` `:eip` `:elb` |
| Security, Identity & Compliance | `:sg` |
| Application Integration | `:sqs` `:sns` |
| Containers | `:ecs` `:eks` |
| Management & Governance | `:cfn` |

In a resource view: `Enter` on a vpc / subnet / sg drills into its
instances (`Esc` there goes back, cursor restored); `Enter` elsewhere and
`d` everywhere open the describe dashboard — bordered panels grouped the
way the AWS console groups things (Details / Networking / Security /
Storage / Tags) so most records fit one screen; `Esc` returns to `:ec2`.

## Filter & sort

| Key | Action |
| --- | --- |
| `/` or `f` | filter: name / id / ip / type / az / vpc / sg / tag (`!` excludes, terms AND) |
| `Enter` | apply the query and close the input |
| `Esc` | clear the filter |
| `N` / `S` / `T` / `A` / `P` | sort by name / state / type / age / ip (again to reverse) |
| `C` / `M` | sort by CPU / memory (with [`metrics`](/guide/configuration) on) |

## Actions

| Key | Action |
| --- | --- |
| `Enter` / `d` | detail view of the selected host |
| `Space` | mark / unmark host for multi-open |
| `s` | connect (marked hosts as split panes, else the selected one) |
| `R` | reboot selected host (running only, confirmation required) |
| `F` | [port forward](/guide/quick-start#port-forward): local port → the instance, or a remote host (e.g. RDS) via it |
| `c` | switch AWS profile |
| `r` / `Ctrl+r` | refresh the active view now |
| `?` | toggle help |
| `q` / `Ctrl+c` | quit |

## Profile picker

Typing filters immediately — no `/` needed. Fuzzy subsequence
matching (`cldprd` finds `Cloud.prod`), matched characters highlighted.
`↑↓` / `Ctrl+p` / `Ctrl+n` move, `Enter` selects, `Esc` clears the query
then cancels, `Ctrl+c` quits.

## Session (leader prefix, default `Ctrl+b`)

Press the leader, then:

| Key | Action |
| --- | --- |
| `h` / `l` (or `←` / `→`, `p` / `n`) | focus previous / next pane |
| `j` / `k` (or `↓` / `↑`) | focus one grid row down / up |
| `space` | add/remove focused pane from broadcast group — ≥2 auto-broadcasts 🔊 |
| `b` | select all / clear the broadcast group |
| `v` | cycle layout: columns → rows → grid |
| `z` | zoom — toggle the focused pane full-screen |
| `[` | scroll the pane's history (shell output only — less/vim scroll inside the app); `q` / `Esc` / `]` exits |
| `a` | add a pane (pick another host from the list) |
| `x` | close the focused pane (disabled while broadcast is on) |
| `d` | close the whole session — ends all SSM sessions (confirms) |
| `?` | toggle help |
| leader twice | send a literal leader key to the shell |

After a focus command the arrow keys keep moving focus without re-pressing
the leader (`space` still toggles broadcast membership). `Esc` / `Enter`
exit this mode without reaching the shell; any other key exits *and* is
forwarded to the shell.

Typing `exit` in a pane's shell ends that pane's SSM session and closes the
pane; closing the last pane returns to the list. A which-key style popup
lists these commands whenever the leader is pending.

## Mouse

On by default (config `mouse: false` disables):

- wheel scrolls the list, detail/help overlays, and a pane's scrollback
- click selects a row / focuses a pane; double-click connects (`Connected` hosts only)
- terminal-native text selection needs `Shift+drag` while mouse capture is on
