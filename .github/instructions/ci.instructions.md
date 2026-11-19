---
applyTo: ".github/workflows/**,.github/dependabot.yml,Taskfile.dist.yaml,fuzz/Taskfile.dist.yaml,.pre-commit-config.yaml,scripts/**"
---

<!-- generated from docs/review/ci.md by scripts/review-index.py; edit the source instead -->

# CI, Taskfile and hooks stay in sync

- `task ci` in `Taskfile.dist.yaml` mirrors the jobs in `.github/workflows/ci.yml`: `branch`,
  `fmt`, `clippy`, `style`, `doc`, `test`, `deny`, `msrv` and `fuzz`. A change that adds,
  removes or renames a job, or changes its flags, makes the same change to the matching task,
  and the other way round. `coverage` and `semver` are CI-only on purpose. The `dependencies`
  job (SBOM submission on `main`) is CI-only too.
- clippy and rustdoc get `-D warnings` explicitly, in CI and in the tasks. No global
  `RUSTFLAGS`.
- Local hooks in `.pre-commit-config.yaml` call Taskfile tasks. They must not duplicate
  commands. Hook revs stay frozen to SHAs (`prek auto-update --freeze`).
- Every `uses:` is pinned to a full 40-character commit SHA with a `# vX.Y.Z` comment. Reject
  tag or branch refs. `actionlint` and `zizmor` (`task lint:actions`) must pass.
- Workflows set `permissions: {}` at the top, and each job the least privilege it needs.
  Every `actions/checkout` sets `persist-credentials: false`.
- No `pull_request_target`. No `${{ github.event.* }}` or other untrusted expressions
  directly in `run:`. Pass them through `env:`.
- Do not rename `publish-crates-io.yml` or its `crates-io` environment. crates.io trusted
  publishing is bound to both. Publishing `needs:` the `verify` job.
- `release-binaries.yml` triggers on `miniplan-cli-v*` tags only. Keep its target list in
  sync with the README's install section.
- The fuzz crate (`fuzz/`) is a separate workspace on nightly. Dependabot watches `/` and
  `/fuzz`.
- `scripts/check-branch.sh` is the single definition of allowed branch names, used by the
  Taskfile, the pre-push hook and the CI `branch` job.
- New jobs that should gate merges must also be added to the ruleset's required checks.
  Mention that in the PR.
