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
| `Ctrl+f` / `Ctrl+b` | page down / up |
| `Ctrl+d` / `Ctrl+u` | half page down / up |

## Filter & sort

| Key | Action |
| --- | --- |
| `/` or `f` | filter: name / id / ip / type / az / vpc / tag (`!` excludes) |
| `Enter` | add a nested filter level (narrows within results) |
| `Esc` | pop the last filter level |
| `N` / `S` / `T` / `A` / `P` | sort by name / state / type / age / ip (again to reverse) |

## Actions

| Key | Action |
| --- | --- |
| `Enter` / `d` | detail view of the selected host |
| `Space` | mark / unmark host for multi-open |
| `s` | connect (marked hosts as split panes, else the selected one) |
| `R` | reboot selected host (running only, confirmation required) |
| `F` | port forward: local port → the instance, or a remote host (e.g. RDS) via it |
| `c` | switch AWS profile |
| `r` / `Ctrl+r` | refresh inventory now |
| `?` | toggle help |
| `q` / `Ctrl+c` | quit |

## Session (leader prefix, default `Ctrl+b`)

Press the leader, then:

| Key | Action |
| --- | --- |
| `h` / `l` / `↑` / `↓` | focus pane (`↑`/`↓` move by grid row); arrows keep moving |
| `space` | add/remove focused pane from broadcast group — ≥2 auto-broadcasts 🔊 |
| `b` | select all / clear the broadcast group |
| `v` | cycle layout: columns → rows → grid |
| `z` | zoom — toggle the focused pane full-screen |
| `[` | scroll the pane's history (shell output only — less/vim scroll inside the app) |
| `a` | add a pane (pick another host from the list) |
| `x` | close the focused pane (disabled while broadcast is on) |
| `d` | close the whole session — ends all SSM sessions (confirms) |
| leader twice | send a literal leader key to the shell |

Typing `exit` in a pane's shell ends that pane's SSM session and closes the
pane; closing the last pane returns to the list. A which-key style popup
lists these commands whenever the leader is pending.

## Mouse

On by default (config `mouse: false` disables):

- wheel scrolls the list, detail/help overlays, and a pane's scrollback
- click selects a row / focuses a pane; double-click connects (🟢 only)
- terminal-native text selection needs `Shift+drag` while mouse capture is on
