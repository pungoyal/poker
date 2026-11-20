# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- A realistic opponent, the `solid` bot, now the default: it plays a range of
  starting hands that narrows as bets grow, bets for value, semi-bluffs, takes
  continuation bets and bluffs in balance, with one size for value and bluffs.
- Range reading: every bot publishes its strategy (`Bot::tendencies`), and the
  coach narrows the bot's likely hands with each of its actions. Equity is
  measured against those hands, and bets are priced by how often the bot folds
  and how strong the hands that continue are. Checks out of position include
  the bot's bet in reply.
- `teach` coaching, now the default: before each decision the coach asks you
  to read the bot's range, estimate your equity and work out the pot odds, and
  checks your answers.
- An LLM coach: with `ANTHROPIC_API_KEY` set, Claude asks the questions,
  answers yours (`/` at the table, `? question` in plain mode) and explains
  each review, using only the numbers the engine computed. `--no-llm` turns it
  off.
- A full-screen table: cards on a felt, stacks and chips, an action log, a
  streaming coach panel, one-key actions and a bet slider with pot presets.
  `--plain` keeps the line mode, which is also used when input is piped.
- `poker leaks`: the costliest recurring mistakes across your hand history,
  with the principle that fixes each.
- `poker drill`: replays your costliest past mistakes as practice spots.
- A banner marks the start and end of each hand in plain mode.
- The board and the player's hand drawn as cards on every street, with the
  hand named in plain English and its straight and flush draws.
- A basic coach: pot odds before a decision and a review after it (`--coach`).
- Dual MIT / Apache-2.0 licence, contributor guide, code of conduct and
  security policy.
- Native CI on Linux, macOS and Windows, an MSRV check (Rust 1.88) and
  documentation builds.
- Release workflow that attaches prebuilt binaries to tagged releases.

### Changed

- The default opponent is the `solid` bot and the default coaching is `teach`.
- The coach's equity and prices are against the bot's likely hands rather than
  any two cards, once its actions have narrowed them.
- Hand records of the `solid` bot can be analysed by `leaks` and `drill`;
  records from earlier versions still replay.

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
- `poker replay` narrates each hand street by street.

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
