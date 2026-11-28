# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.1](https://github.com/sunsided/miniplan/compare/miniplan-cli-v0.3.0...miniplan-cli-v0.3.1) - 2026-11-28

### Changed

- Keep r and w working after tabbing out of a TUI field
- Honor the chosen heuristic in astar and gbfs
- Fit the TUI flash message into narrow terminals
- Type r and w into a TUI field while editing it
- Write CLI logs to stderr

## [0.3.0](https://github.com/sunsided/miniplan/compare/miniplan-cli-v0.2.0...miniplan-cli-v0.3.0) - 2026-11-22

### Added

- Add TUI state and render tests
- Add strict workspace lint table
- Add release-plz, crates.io publishing and release binaries
- Add codecov and MSRV badges and README sections
- Add REUSE licensing metadata and license texts
- Add cargo-deny, typos and MSRV 1.88

### Changed

- Link the crate to its page on the planning and search site ([#29](https://github.com/sunsided/miniplan/pull/29))
- Link the crate to its page on the planning and search site
- Describe the released scope and audience in the README
- Tighten TUI tests and lint scripts
- Cover plan output formats and CLI error paths
- Locate the CLI binary at compile time in TUI smoke tests
- Document and lint-clean the miniplan CLI
- Apply mechanical pedantic clippy fixes
- Doctest the README examples
- Apply rustfmt to new tests
- Move integration tests into tests/it
- Replace humantime with a small duration parser

### Fixed

- Fix TUI Delete key panic on multi-byte input
