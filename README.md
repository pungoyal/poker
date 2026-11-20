<h1 align="center">♠ poker</h1>

<p align="center">
  <strong>A heads-up No-Limit Hold'em trainer for the terminal.</strong><br>
  Play, see your cards and odds on every street, and get your decisions reviewed in big blinds.
</p>

<p align="center">
  <a href="https://github.com/pungoyal/poker/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/pungoyal/poker/actions/workflows/ci.yml/badge.svg"></a>
  <a href="#licence"><img alt="Licence: MIT OR Apache-2.0" src="https://img.shields.io/badge/licence-MIT%20OR%20Apache--2.0-blue.svg"></a>
  <img alt="MSRV 1.88" src="https://img.shields.io/badge/rust-1.88%2B-orange.svg">
</p>

---

The goal is a near-unbeatable opponent plus a coach that tells you what each of
your mistakes cost and why. Today the project ships the foundation: a complete,
deterministic rules engine, a fast hand evaluator, replayable hand histories, a
playable command-line client and a first version of the coach.

## Features

- **Correct rules.** Blinds, min-raises, all-ins, showdowns and split pots are
  modelled as a deterministic state machine with tests for the edge cases.
- **Reproducible sessions.** Every deal comes from a seed: the same `--seed`
  deals the same cards, so any hand can be replayed and any bug reproduced.
- **Verified hand histories.** Finished hands are appended to a JSON Lines file
  and `poker replay` re-runs each one through the engine to check it.
- **A coach at the table.** Pot odds before you decide, and a review after:
  calls and folds priced in big blinds, bets explained as value or bluff.
- **Pleasant to play.** The board and your hand drawn as cards on every street,
  your hand named in plain English with its draws, amounts in big blinds, bets
  by size (`b 2.5`) or by pot fraction (`b 75%`).
- **Runs anywhere.** A single binary, or one `docker compose` command.

## Quick start

With Docker (nothing else needed):

```sh
git clone https://github.com/pungoyal/poker && cd poker
docker compose run --rm poker
```

With Rust 1.88 or newer:

```sh
cargo install --git https://github.com/pungoyal/poker
poker --seed 7
```

Prebuilt binaries for Linux, macOS and Windows are attached to each
[release](https://github.com/pungoyal/poker/releases).

## Playing

Both players start every hand with 100 big blinds and the button alternates.
At the prompt, amounts are in big blinds:

| Input | Action | Input | Action |
| --- | --- | --- | --- |
| `f` | fold | `b 2.5` | bet or raise to 2.5bb in total |
| `x` | check | `b 75%` | bet 75% of the pot |
| `c` | call | `a` | all-in |
| `h` | help | `q` | quit |

Options:

| Flag | Default | Meaning |
| --- | --- | --- |
| `--bot <random\|station>` | `random` | Opponent: a random bot, or a calling station that never folds |
| `--coach <off\|after\|full>` | `after` | Coach output: none, a review after each decision, or also your equity before you decide |
| `--seed <n>` | time-based | Seed for cards and the bot; the same seed deals the same session |
| `--stack <bb>` | `100` | Starting stack in big blinds, reset every hand |
| `--hands <n>` | unlimited | Stop after this many hands |
| `--log <file>` | `hands.jsonl` | File that finished hands are appended to |

`poker replay [file]` prints the stored hands and verifies each one.

### The coach

```
── Flop · pot 5bb ──
   Board               Your hand
  ┌────┐┌────┐┌────┐  ┌────┐┌────┐
  │ 9♥ ││ 8♦ ││ 3♣ │  │ K♦ ││ 3♦ │
  └────┘└────┘└────┘  └────┘└────┘
  You have a pair of threes.
```

Facing a bet, you see the pot odds first. After you act, the coach reviews the
decision:

```
  Pot odds: call 0.5bb to win 1.5bb → you need to win more than 25% of the time.
  Coach · Inaccuracy: You needed 25% equity to call 0.5bb into 1.5bb and had about 45%. Folding gave up about 0.4bb.
```

Two simplifications apply until the solver-backed bot lands. Equity is measured
against a random hand, which is exact for the placeholder bots because their
play ignores their cards. Prices assume no further betting, so they are
estimates before the river.

With Docker, hand histories live in the `hands` volume and survive between runs:

```sh
docker compose run --rm poker --bot station --seed 7
docker compose run --rm poker replay
```

## Roadmap

| Step | What | State |
| --- | --- | --- |
| 1 | Rules engine, hand evaluator, hand records, playable CLI against placeholder bots | ✅ done |
| 1b | Basic coach: hand reading, pot odds, equity against a random hand, priced calls and folds | ✅ done |
| 2 | Solver-backed bot: preflop ranges, live postflop solving | 🚧 next |
| 3 | Full coach: exact cost of every decision from the solver, with plain-language explanations | planned |
| 4 | Leak tracking across sessions, reviews and drills | planned |

Ideas and help on any of these are welcome: see [Contributing](#contributing).

## Architecture

The crate is a pure library (no I/O, no clocks, no global state) with a thin
command-line client on top.

| Module | Responsibility |
| --- | --- |
| [`cards`](src/cards.rs) | Cards, the deck and the seeded shuffle |
| [`eval`](src/eval.rs) | Best five-card hand out of five to seven cards |
| [`game`](src/game.rs) | Betting rules for one hand as a deterministic state machine |
| [`bot`](src/bot.rs) | The `Bot` trait and the placeholder bots |
| [`coach`](src/coach.rs) | Hand descriptions, equity, pot odds and decision reviews |
| [`history`](src/history.rs) | Hand records, storage and replay verification |
| [`main`](src/main.rs) | The terminal client |

Writing a new opponent means implementing one trait:

```rust
pub trait Bot {
    fn name(&self) -> &'static str;
    /// Chooses an action for `seat`, which is the player to act.
    fn act(&mut self, state: &HandState, seat: usize) -> Action;
}
```

## Development

```sh
cargo test                                   # unit tests
cargo clippy --all-targets -- -D warnings    # lints
docker compose build test                    # everything CI runs, in Docker
```

The toolchain is pinned with [mise](https://mise.jdx.dev) (`mise install`).
See [CONTRIBUTING.md](CONTRIBUTING.md) for the full workflow.

## Contributing

Contributions of all sizes are welcome. Please read the
[contributing guide](CONTRIBUTING.md) and the [code of conduct](CODE_OF_CONDUCT.md).
Report security issues privately as described in [SECURITY.md](SECURITY.md).

## Licence

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT licence ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this work, as defined in the Apache-2.0 licence, shall be dual
licensed as above, without any additional terms or conditions.
