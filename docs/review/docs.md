---
title: Documentation
applyTo: "**/*.md,docs/**"
summary: Absolute URLs, doctested README examples, tagged code blocks, consistent badges.
---

# Documentation

- `README.md` is the readme of both published crates, so crates.io renders it outside GitHub.
  Every link to a repo file and every image uses an absolute URL
  (`https://github.com/sunsided/miniplan/...`). Only in-page anchors may be relative.
  `task lint:readme` checks this.
- The README's Rust blocks run as doctests through `readme_doctests` in
  `crates/miniplan/src/lib.rs`. They must compile against the current API. Use
  `rust,no_run` or `rust,ignore` only with a stated reason (the library example is `no_run`
  because it reads a file relative to the repository root).
- Every other code block is tagged (`bash`, `toml`, `text`). An untagged block runs as Rust.
- The CLI reference in the README (commands, `solve` flags, exit codes, planner names) must
  match `bins/miniplan-cli/src/main.rs`. Flag drift.
- Keep badges, the MSRV section and the License section consistent with `Cargo.toml` and
  `REUSE.toml`.
- Mermaid diagrams, if added, must match the code and stay under about 15 nodes. Adding the
  first one also needs the docs.rs header setup and the mermaid lint step in CI.
