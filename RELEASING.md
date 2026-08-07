# Releasing skua

Releases are fully automated by [dist](https://axodotdev.github.io/cargo-dist/)
(`.github/workflows/release.yml`, generated from `dist-workspace.toml`).

## Cutting a release

```sh
# 1. bump `version` in Cargo.toml, commit
# 2. tag and push — the tag triggers everything
git tag v1.0.0
git push origin main --tags
```

CI then builds `aarch64/x86_64-apple-darwin` + `aarch64/x86_64-unknown-linux-gnu`,
creates the GitHub Release with tarballs, checksums, and `skua-installer.sh`,
pushes the Homebrew formula to `siansiansu/homebrew-tap`, and publishes to
crates.io.

## One-time setup

1. **Homebrew tap** — create a public repo `siansiansu/homebrew-tap`
   (can be empty; dist commits `Formula/skua.rb` into it).
2. **`HOMEBREW_TAP_TOKEN`** — a GitHub personal access token with `repo`
   scope on the tap, saved as an Actions secret on *this* repo.
3. **`CARGO_REGISTRY_TOKEN`** — a crates.io API token (crates.io →
   Account Settings → API Tokens, `publish-update` scope), saved as an
   Actions secret on this repo.
4. **GitHub Pages** — repo Settings → Pages → Source: **GitHub Actions**
   (the docs site deploys from `.github/workflows/docs.yml` on push to main).

## Changing the release config

Edit `dist-workspace.toml`, then regenerate — never hand-edit
`release.yml`:

```sh
dist generate       # rewrites .github/workflows/release.yml
dist plan           # sanity-check what a release would produce
```

## Docs site

- Local preview: `npm install && npm run docs:dev`
- Content lives in `docs/` (VitePress); deploys automatically to
  <https://siansiansu.github.io/skua/> on push to main.
