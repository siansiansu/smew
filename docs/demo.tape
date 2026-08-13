# Records docs/public/demo.gif from `smew --dev` (offline mock fleet, no AWS calls).
# Run from the repo root: `vhs docs/demo.tape`, then shrink the result with
# `gifsicle -O3 --lossy=40 --colors 96 docs/public/demo.gif -o docs/public/demo.gif`.
#
# The hidden setup block points HOME at a throwaway directory and clears every
# AWS_* variable, so no credential, profile name, or shell theme from the
# recording machine can reach a frame. It also pins SHELL to /bin/sh: dev mode
# spawns a local shell for the session pane, and a plain prompt keeps that pane
# free of personal prompt content.
#
# The recording ends on the help overlay — never back at a shell prompt.
#
# Keys match smew 1.2.1: single-pane sessions, `space` marks run-command
# targets, `x` opens the command editor (ctrl+s runs it).

Output docs/public/demo.gif

Set Shell bash
Set FontSize 16
Set Width 1200
Set Height 720
Set Padding 10
Set Framerate 25
Set TypingSpeed 45ms
Set WaitTimeout 30s

Hide
Type "export HOME=$(mktemp -d) SHELL=/bin/sh PS1='$ '"
Enter
Type "export XDG_CONFIG_HOME=$HOME/.config"
Enter
Type "unset AWS_PROFILE AWS_DEFAULT_PROFILE AWS_REGION AWS_DEFAULT_REGION AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY AWS_SESSION_TOKEN AWS_SECURITY_TOKEN AWS_CREDENTIAL_EXPIRATION AWS_ROLE_ARN AWS_VAULT AWS_SDK_LOAD_CONFIG"
Enter
Type "clear"
Enter

# Launch while hidden so the first frame is already the fleet list.
Type "smew --dev"
Enter
Sleep 2s
Show

# --- fleet list -----------------------------------------------------------
Sleep 2s

# --- sort by name ---------------------------------------------------------
Down 5
Sleep 600ms
Type "N"
Sleep 1200ms

# --- detail view ----------------------------------------------------------
Down 8
Enter
Sleep 2200ms
Escape
Sleep 600ms

# --- command mode: resource views ----------------------------------------
Type ":vol"
Enter
Sleep 1800ms
Type ":rds"
Enter
Sleep 1800ms
Type ":vpc"
Enter
Sleep 1400ms

# Enter on a vpc drills into the instances inside it
Enter
Sleep 1800ms
Escape
Sleep 600ms

# Escape only pops the drill, so ask for the fleet back explicitly — the keys
# below only mean anything in the instance list.
Type ":ec2"
Enter
Sleep 1200ms

# --- profile picker (fuzzy) ----------------------------------------------
Type "c"
Sleep 900ms
Type "prod"
Sleep 900ms
Enter
Sleep 1600ms

# --- filter down to the web tier -----------------------------------------
Type "/"
Sleep 400ms
Type "web"
Enter
Sleep 1400ms

# --- run one command across two marked hosts -----------------------------
Type "gg"
Space
Sleep 300ms
Down 1
Space
Sleep 600ms
Type "x"
Sleep 800ms
Type "uptime"
Sleep 600ms
Ctrl+S
Sleep 3s
Escape
Sleep 600ms

# --- session on the selected host ----------------------------------------
Type "s"
Sleep 2500ms

# leader (^b) shows the session commands
Ctrl+b
Sleep 1200ms

# ^b d ends the session, y confirms, list comes back
Type "d"
Sleep 1000ms
Type "y"
Sleep 1600ms
Escape
Sleep 600ms

# --- help overlay ---------------------------------------------------------
Type "?"
Sleep 2500ms
