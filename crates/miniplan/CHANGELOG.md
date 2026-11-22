# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0](https://github.com/sunsided/miniplan/compare/v0.2.0...v0.3.0) - 2026-11-22

### Added

- Add registry and file-loading tests
- Add strict workspace lint table
- Add release-plz, crates.io publishing and release binaries
- Add codecov and MSRV badges and README sections
- Add criterion search benchmarks
- Add plan-validity property tests
- Add REUSE licensing metadata and license texts
- Add cargo-deny, typos and MSRV 1.88

### Changed

- Read the total-cost function in any case
- Reject PDDL+ processes and events when grounding
- Move to pddl 0.5
- Link the crate to its page on the planning and search site ([#29](https://github.com/sunsided/miniplan/pull/29))
- Link the crate to its page on the planning and search site
- Describe the released scope and audience in the README
- Tighten TUI tests and lint scripts
- Render library mermaid diagrams on docs.rs
- Check given names when loading two files
- Locate the CLI binary at compile time in TUI smoke tests
- Document and lint-clean the search module
- Document and lint-clean grounding, heuristics and task model
- Apply mechanical pedantic clippy fixes
- Replace tui/mod.rs with tui.rs
- Doctest the README examples
- Apply rustfmt to new tests
- Stop BiDij once forward search is exhausted
- Move integration tests into tests/it

### Fixed

- Fix BibfsUc looping forever on unreachable goals

### Removed

- Remove unused interner and grounding helpers
