---
title: Crates and releases
applyTo: "**/Cargo.toml,Cargo.lock,deny.toml,REUSE.toml,release-plz.toml,.github/workflows/release-plz.yml,.github/workflows/publish-crates-io.yml,.github/workflows/release-binaries.yml,**/CHANGELOG.md"
summary: Crate metadata, shared version group, deny policy, release-plz and trusted publishing.
---

# Crates and releases

- Versions are SemVer 2.0 (Cargo's interpretation). Pre-releases (`-rc.1`) are fine, build
  metadata (`+...`) is not. In `0.x`, a minor bump is breaking.
- `miniplan` and `miniplan-cli` share `version.workspace` and the release-plz
  `version_group = "miniplan"`. They are always released together. The library is tagged
  `vX.Y.Z`, the CLI `miniplan-cli-vX.Y.Z`. Do not split the versions without changing
  `release-plz.toml` and both publish workflows.
- Versions and `CHANGELOG.md` files are managed by release-plz release PRs. Do not bump
  versions or edit generated changelog entries in feature PRs.
- Every publishable crate has `description`, `license`, `repository`, `readme`, `keywords`
  (at most 5), `categories` (valid crates.io slugs) and `rust-version.workspace = true`.
  The workspace path dependency on `miniplan` carries a `version`.
- New dependencies must pass `cargo deny check`: allowed licenses, no advisories, no git or
  unknown-registry sources. Advisory ignores need an `id`, a `reason` and maintainer approval.
- `rust-version` (1.88) is a signal, not a contract. Raising it is allowed when a dependency
  or feature benefits. It needs a minor release and a changelog note, never a patch release.
- release-plz runs in git-only mode: it tags and creates GitHub Releases.
  `publish-crates-io.yml` publishes from the tag in the `crates-io` environment via trusted
  publishing (`id-token: write`, no `CARGO_REGISTRY_TOKEN`). Keep it that way.
- Changelog groups and minor bumps come from imperative subject verbs (`release-plz.toml`).
  A subject that hides a feature behind "Update ..." lands under Changed. Flag it.
- License texts in `LICENSES/` come from `reuse download`. Each crate links to it
  (`crates/*/LICENSES`, `bins/*/LICENSES`), so `cargo package --list` must show them.
- The library packages `docs/mermaid.html` as a symlink to the root copy, which docs.rs passes
  to rustdoc through `[package.metadata.docs.rs]`. `cargo package --list -p miniplan` must
  show it.
