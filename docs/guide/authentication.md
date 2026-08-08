# Authentication

smew rides the AWS SDK's standard credential chain — the same resolution the
aws CLI uses. It never implements its own credential handling: pick a profile
(or rely on env vars / the default profile) and whatever works for
`aws sts get-caller-identity` works for smew.

Resolution order: explicit `--profile` flag → `default_profile` in
`config.yaml` → environment variables → shared config default profile.

## Support matrix

| Method | Status | Notes |
| --- | --- | --- |
| Static keys in `~/.aws/credentials` | ✅ Supported | |
| Environment variables (`AWS_ACCESS_KEY_ID`, `AWS_SESSION_TOKEN`, …) | ✅ Supported | Used when no profile is selected |
| SSO / IAM Identity Center (`sso_session` or legacy `sso_start_url`) | ✅ Supported | Expired/missing token shows a `run: aws sso login --profile <name>` hint |
| AssumeRole profiles (`role_arn` + `source_profile`) | ✅ Supported | Handled by the SDK profile chain, including chained roles |
| `credential_process` (aws-vault, granted, saml2aws, …) | ✅ Supported | The SDK invokes the external process; anything it can mint works |
| Web identity (`AWS_WEB_IDENTITY_TOKEN_FILE` / `web_identity_token_file`) | ✅ Supported | |
| MFA-required AssumeRole (`mfa_serial`) | ❌ Not yet | The SDK cannot prompt for a token code; see the [roadmap](https://github.com/siansiansu/smew/blob/main/ROADMAP.md). Workaround: mint session credentials externally (e.g. aws-vault) and expose them via `credential_process` or env vars |

Two things to know beyond the matrix:

- **Profile listing is offline.** The profile picker enumerates
  `~/.aws/config` and `~/.aws/credentials` directly (honoring
  `AWS_CONFIG_FILE` / `AWS_SHARED_CREDENTIALS_FILE`) — no network call, no
  credential use, until you actually select a profile.
- **One session process, one profile.** Each session pane re-invokes smew
  with the profile name; that process resolves the same credentials through
  the SDK, calls `ssm:StartSession`, and hands the encrypted channel to
  session-manager-plugin. No secrets are forwarded between processes.

## SSO token expiry

With an Identity Center profile, credentials come from a cached token that
expires (typically every 8–12 hours). When it does, smew shows:

```
AWS SSO session expired or missing — run: aws sso login --profile <name>
```

Run that command in another terminal, then refresh (`r` in the instance
list). smew does not run `aws sso login` for you — the device-code flow
needs a browser and its own terminal interaction.

## Verifying a profile

`smew --profile <name> --dry-run` prints the resolved caller identity
(`sts:GetCallerIdentity`), the resolved region, and the instance inventory —
the quickest way to confirm a profile works before opening the TUI.
