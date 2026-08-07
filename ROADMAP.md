# Roadmap

Planned work, in order. Each item ships independently; later items are
re-evaluated (or dropped) as real usage data comes in.

## 1. SSO token expiry detection

The SDK credential chain already supports IAM Identity Center profiles
(`sso_session`), but an expired token surfaces as a cryptic SDK error on the
first API call. Detect that case and show an actionable message — "run
`aws sso login --profile <name>`" — instead of the raw error chain.
Optionally offer to run the login command from the TUI.

- Smallest useful version: pattern-match the error, print the hint.
- No new auth code; this is pure error UX on top of the existing chain.

## 2. Auth support matrix (verify + document)

Because skua rides the SDK default credential chain, most auth methods should
already work. Verify each and document the result as a support matrix in the
docs site:

| Method | Expectation |
| --- | --- |
| Static keys in `~/.aws/credentials` | works today |
| Env vars (`AWS_ACCESS_KEY_ID`, …) | works today |
| `credential_process` (aws-vault, granted, saml2aws) | should work — verify |
| AssumeRole via `source_profile` / `role_arn` | should work — verify |
| SSO / IAM Identity Center (`sso_session`) | works while token fresh (see item 1) |
| MFA-required assume-role (`mfa_serial`) | not supported — see item 5 |

Almost zero code; mostly testing and a docs page.

## 3. Skins (user-selectable color themes)

Let users pick a color theme, following the k9s model (see
`references/k9s/skins/`): a skin is a YAML file mapping named UI roles to
colors, and the config selects one by name.

- Collapse the color constants in `src/tui/view/mod.rs` (plus the stray
  `Color::Indexed` literals in `view/overlays.rs`, `view/session.rs`,
  `view/list.rs`) into a single `Theme` struct with named roles; the current
  values become the built-in default skin.
- Load skins from `~/.config/skua/skins/<name>.yaml`; select via
  `skin: <name>` in `config.yaml`. Missing/invalid skin falls back to the
  default with a warning.
- Support hex (`"#ff79c6"`), 256-color index, and `default` (terminal
  default) values, like k9s.
- Ship a few built-in skins (e.g. dracula, gruvbox, nord) compiled in, so
  the feature is usable with zero setup.
- Out of scope for the first cut: hot-reload on file change, per-pane
  themes, styling the embedded terminal (vt100 content keeps whatever the
  remote shell outputs).

## 4. Port forwarding sessions

Start SSM port-forwarding sessions from the TUI, running inside the existing
pane infrastructure:

- `AWS-StartPortForwardingSession` (local port → instance port)
- `AWS-StartPortForwardingSessionToRemoteHost` (via instance to e.g. RDS)

Flow: pick instance → small form (local port, remote host, remote port) →
session runs as a pane like a shell does. Reuses `PluginDriver`; the only new
surface is the form and the document-name/parameters plumbing.

## 5. MFA prompt for `mfa_serial` profiles (on demand)

The SDK does not interactively prompt for MFA codes on assume-role profiles
with `mfa_serial`. Wire a token provider to a TUI input box. Only build this
if such profiles are actually in use — otherwise skip (YAGNI).

## 6. ECS Exec (on demand)

`aws ecs execute-command` also rides session-manager-plugin, so a container
shell fits the existing pane model. Needs a cluster/task/container picker.
Only build if ECS is actually in use.

## 7. GCP support (separate milestone)

The GCP equivalent is IAP tunneling: `gcloud compute ssh
--tunnel-through-iap`. Approach mirrors the AWS design — shell out to the
official CLI rather than pulling in SDK crates:

- List: `gcloud compute instances list --format=json`
- Connect: `gcloud compute ssh --tunnel-through-iap` in a pane
- Auth: whatever `gcloud auth login` / ADC already provides

The real cost is UI semantics (project/zone instead of profile/region, and
different inventory fields), not the connection itself.

**Design rule: do not pre-abstract.** No `Provider` trait until this
milestone actually starts; extract the minimal interface from the existing
`Inventory` / `PluginDriver` then. Abstractions invented ahead of the second
implementation are almost always wrong.

## Non-goals (for now)

- EKS / kubectl integration — k9s already owns that space
- File-transfer UI — `--ssh-config` + scp/rsync covers it
- Session recording/logging — revisit if a compliance need appears
