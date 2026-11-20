# Contributing

Thanks for your interest in improving poker! Bug reports, ideas, docs fixes and
code are all welcome. By taking part you agree to follow the
[Code of Conduct](CODE_OF_CONDUCT.md).

## Before you start

- **Bugs:** search the [issues](https://github.com/pungoyal/poker/issues) first,
  then open a bug report. A `--seed` that reproduces the problem, or the line
  from `hands.jsonl`, makes a fix far quicker.
- **Features:** open an issue to discuss the idea before writing a large change,
  so we can agree on the approach. Check the [roadmap](README.md#roadmap) to see
  where the project is heading.
- **Small fixes** (typos, docs, obvious bugs) can go straight to a pull request.

## Development setup

You need Rust 1.88 or newer. The pinned toolchain is managed with
[mise](https://mise.jdx.dev):

```sh
mise install          # or: rustup toolchain install 1.100.0
cargo run -- --seed 7 # play a seeded session (add --plain for line mode)
```

Alternatively, everything runs in Docker and needs nothing else installed:

```sh
docker compose build test
```

## Checks

CI runs the following; please run them before pushing:

```sh
cargo fmt --all --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
```

## Guidelines

- **Determinism is a feature.** The engine must produce identical results for
  the same seed and actions. Never read the clock or a global RNG inside the
  library; take a seed or an `Rng` instead.
- **Keep the library free of I/O.** `src/lib.rs` and its modules are a pure
  engine; terminal handling and network calls belong in `src/main.rs` and
  `src/app/`.
- **A bot's `tendencies` must match its `act`.** The coach reads a bot's range
  from `tendencies`, so a bot whose actions drift from what it reports will
  teach the wrong lessons. The solid bot samples its actions from them.
- **The engine owns the numbers.** The LLM coach only explains facts the engine
  computed (`coach::brief`); never ask the model to calculate. Tests must not
  call the network: the LLM client's parsing is tested on recorded streams.
- **Hand records are a stable format.** If you change `HandRecord`, keep old
  files readable or explain the migration in the changelog.
- **Test the rules.** Betting and evaluation changes need tests covering the
  edge cases (all-ins, min-raises, split pots, kicker ties).
- **Document public items** with doc comments that say what, not how.
- Add an entry under `Unreleased` in [CHANGELOG.md](CHANGELOG.md) for any
  user-visible change.

## Pull requests

1. Fork the repository and create a branch from `main`.
2. Make focused commits with messages in the imperative mood
   ("Add side-pot handling", not "Added…").
3. Open a pull request and fill in the template. CI must pass before merge.

## Licence

Unless you state otherwise, any contribution you intentionally submit for
inclusion in this project, as defined in the Apache-2.0 licence, is dual
licensed as MIT OR Apache-2.0, without any additional terms or conditions.
