# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- The board and the player's hand drawn as cards on every street, with the
  hand named in plain English and its straight and flush draws.
- A basic coach: pot odds before a decision and a review after it (`--coach`).
  Every option, including five bet sizes, is priced against the opponent's
  known fold rate and equity versus a random hand; the review names the best
  option and the facts that decided it. `--coach full` shows the pick up front.
- A banner marks the start and end of each hand.

### Fixed

- `--stack` values too large for the chip arithmetic are rejected.
- `poker replay` reports malformed records as errors instead of panicking, and
  checks the recorded cards against the recorded seed.
- Nearby `--seed` values no longer deal overlapping sessions.
- Straight draws are only reported when they use a hole card, and double
  gutshots are no longer called open-ended.
- Preflop equity no longer depends on the order of the hole cards.
- The session's coaching summary only counts hands that were finished.
- Hand evaluation no longer allocates, making flop equity much faster.
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
