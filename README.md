# Poker

A heads-up No-Limit Hold'em trainer for the terminal. The goal is a
near-unbeatable solver-backed opponent plus a coach that prices every decision
you make and explains your mistakes.

## Status

| Step | What | State |
| --- | --- | --- |
| 1 | Rules engine, hand evaluator, hand records, playable CLI against placeholder bots | done |
| 2 | Solver-backed bot (preflop ranges, live postflop solving) | next |
| 3 | Coach: cost of each decision in big blinds, then Claude narration | planned |
| 4 | Leak tracking across sessions, reviews, drills | planned |

## Running

Rust is pinned through [mise](https://mise.jdx.dev) (`mise install`), or use any recent toolchain.

```sh
cargo run --release                      # play against the random bot
cargo run --release -- --bot station     # play against a bot that never folds
cargo run --release -- --seed 7          # same seed, same cards
cargo run --release -- replay            # print and verify hands.jsonl
cargo test --release
```

Both players start every hand with 100 big blinds (`--stack` changes this), and
the button alternates. At the prompt, amounts are in big blinds:

```
f        fold                 b 2.5    bet or raise to 2.5bb in total
x        check                b 75%    bet 75% of the pot
c        call                 a        all-in
h        help                 q        quit
```

Finished hands are appended to `hands.jsonl` (`--log` changes the file).

## Layout

- `src/cards.rs`: cards and the seeded shuffle
- `src/eval.rs`: best five-card hand out of five to seven cards
- `src/game.rs`: betting rules for one hand, as a deterministic state machine
- `src/bot.rs`: the `Bot` trait and the placeholder bots
- `src/history.rs`: hand records, storage and replay verification
- `src/main.rs`: the command-line interface
