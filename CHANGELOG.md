# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Dual MIT / Apache-2.0 licence, contributor guide, code of conduct and
  security policy.
- Native CI on Linux, macOS and Windows, an MSRV check (Rust 1.88) and
  documentation builds.
- Release workflow that attaches prebuilt binaries to tagged releases.

## [0.1.0]

### Added

- Heads-up No-Limit Hold'em rules engine as a deterministic state machine.
- Five-to-seven card hand evaluator.
- Seeded, reproducible shuffles.
- Hand records stored as JSON lines, with replay verification.
- Playable terminal client against a random bot and a calling station.
- Docker images for testing and running, and CI.

[Unreleased]: https://github.com/pungoyal/poker/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/pungoyal/poker/releases/tag/v0.1.0
