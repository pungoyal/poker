<h1 align="center">♠ poker</h1>

<p align="center">
  <strong>A heads-up No-Limit Hold'em trainer for the terminal.</strong><br>
  Play the kinds of player you meet at a home game, with a coach that teaches you to read and beat each one.
</p>

<p align="center">
  <a href="https://github.com/pungoyal/poker/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/pungoyal/poker/actions/workflows/ci.yml/badge.svg"></a>
  <a href="#licence"><img alt="Licence: MIT OR Apache-2.0" src="https://img.shields.io/badge/licence-MIT%20OR%20Apache--2.0-blue.svg"></a>
  <img alt="MSRV 1.88" src="https://img.shields.io/badge/rust-1.88%2B-orange.svg">
</p>

---

A trainer for getting better at heads-up No-Limit Hold'em. You play the four
kinds of player every home game has (a sound regular, a calling station, a nit
and a maniac) at a full-screen table. By default you are not told which one you
face: you read it from what it does. A coach watches every decision: before you
act it asks one quick question, and after you act it shows what your choice was
worth next to the better options, and why. Your mistakes are tracked across
sessions, and `poker drill` replays the costliest ones as practice.

## Features

- **Opponents with real leaks.** `solid` is a sound regular that defends
  enough against every bet size that betting any two cards at it does not
  pay. `station` limps and calls far too much, `nit` folds to pressure, and
  `maniac` raises and bluffs too often. Each needs a different game from you.
- **A mystery opponent, by default.** One of the four is chosen in secret. The
  opponent panel shows what it has been seen to do (hands played, raises,
  folds to bets, bets against calls); you work out who it is and adjust. It is
  named when you leave.
- **A coach that reads the opponent.** Every bot publishes its strategy, so the
  coach narrows the bot's likely hands with each action it takes (Bayes' rule)
  and measures your equity against those hands, not against any two cards. It
  prices every option (fold, check, call and the standard bet sizes) by how
  often the bot folds, how strong the hands that continue are, and what the
  streets still to come add.
- **One question, one key.** In the default `teach` mode the coach asks one
  thing before each decision (read the bot's range, estimate your equity, or
  work out the price), you answer with a number key, and it keeps score by
  skill. It asks about what you have practised least.
- **Reviews you can scan.** After each decision: the verdict, what it cost,
  the top options side by side with yours marked, and the two or three facts
  that decided it.
- **An LLM coach.** With an Anthropic API key, Claude explains every review in
  its own words and answers your questions (press `/` at any time), working
  only from the engine's exact numbers.
- **Leaks and drills.** `poker leaks` finds your costliest recurring mistakes
  across sessions; `poker drill` replays them as practice spots.
- **A full-screen table.** Cards on a felt, the decision in numbers under your
  seat, the opponent panel, an action log, one-key actions and a bet slider
  with pot-sized presets. A plain line mode is kept for pipes and transcripts
  (`--plain`).
- **Correct, reproducible and verifiable.** A deterministic rules engine, a fast
  evaluator, seeded deals (`--seed`), and hand histories that `poker replay`
  re-runs and checks against their seed.

## Quick start

With Docker (nothing else needed):

```sh
git clone https://github.com/pungoyal/poker && cd poker
docker compose run --rm poker
```

With Rust 1.88 or newer:

```sh
cargo install --git https://github.com/pungoyal/poker
poker
```

For the LLM coach, set an API key first (otherwise the coach uses the engine's
own words):

```sh
export ANTHROPIC_API_KEY=sk-ant-...
poker
```

Prebuilt binaries for Linux, macOS and Windows are attached to each
[release](https://github.com/pungoyal/poker/releases).

## Playing

```
 ♠ poker   Hand 1  ·  vs station  ·  result 0bb  ·  questions Equity 0/1                                seed 5
╭──────────────────────────────────────────────────────────────────╮╭ Opponent · station ──────────────────────╮
│                     STATION   big blind  ·  98bb                 ││A calling station: plays most hands,      │
│                            ╳╳╳╳ ╳╳╳╳                             ││limps, calls far too much, rarely raises. │
│                            ╳╳╳╳ ╳╳╳╳                             ││Beat it: Never bluff. Bet your good hands │
│                            ╳╳╳╳ ╳╳╳╳                             ││big and often; its raises mean strength.  │
│                               ● 1bb                              ││Seen: after the flop bets 100% / calls 0% │
│                                                                  ││Holding: shown once you act (n to peek).  │
│                                                                  │╰──────────────────────────────────────────╯
│                                                                  │╭ Coach ───────────────────────────────────╮
│                     3    4    9    3    ┌──┐                     ││✗ Mistake · 1.66bb given up               │
│                      ♠    ♥    ♣    ♥   │  │                     ││A pair of nines on the flop · 82% vs its  │
│                        3    4    9    3 └──┘                     ││likely hands                              │
│                        Pot 3bb   ·   Turn                        ││  Betting 2bb (100% pot)   +4.21bb  ★ best│
│                           Bot bets 1bb.                          ││  Betting 1.5bb (75% pot)  +4.05bb        │
│                                                                  ││  Betting 1bb (50% pot)    +3.81bb        │
│                            K    9                                ││▶ Checking                 +2.55bb  you   │
│                             ♥    ♦                               ││· Betting 2bb (100% pot) gets folds 35%;  │
│                               K    9                             ││called, you win 80%. Folds alone need 50%:│
│                     ▶  YOU   button  ·  99bb                     ││folds fall short; the value is in the     │
│                    two pair, nines and threes                    ││calls.                                    │
│     To call 1bb into 3bb   ·   Price ?   ·   Stacks 32.7× pot    ││· Checking lets worse hands reach showdown│
│             n  peek at your equity and the best play             ││free.                                     │
╰──────────────────────────────────────────────────────────────────╯│                                          │
╭ Hand ────────────────────────────────────────────────────────────╮│Coach Calling 1bb into 3bb: how often must│
│── Turn 3♠ 4♥ 9♣ 3♥                                               ││you win?                                  │
│Bot bets 1bb.                                                     ││ 1  20%   2  25%   3  33%   4  50%        │
╰──────────────────────────────────────────────────────────────────╯╰──────────────────────────────────────────╯
 1-4  answer the coach   esc  skip   or act:   f  fold   c  call 1bb   b  raise   a  all-in 99bb
```

Both players start every hand with 100 big blinds and the button alternates.

| Key | Action | Key | Action |
| --- | --- | --- | --- |
| `f` | fold | `b` or `r` | bet or raise: pick a size, then enter |
| `c` / `x` | call / check | `1`–`5` | a third, half, three quarters, the pot, all-in |
| `a` | all-in | `←` `→` | adjust the size, or type an amount in bb |
| `/` | ask the coach anything | `1`–`6` | answer the coach's question (`esc` skips it, or just act) |
| `n` | peek at your equity, the price and the best play | `space` | next hand |
| `?` | help | `q` | leave the table |

Options:

| Flag | Default | Meaning |
| --- | --- | --- |
| `--bot <mystery\|solid\|station\|nit\|maniac\|random>` | `mystery` | Opponent: one of the four styles chosen in secret, a sound regular, a calling station, a nit, a maniac, or a bot that ignores its cards |
| `--coach <teach\|full\|after\|off>` | `teach` | One question before each decision and a review after; the numbers before and a review after; a review only; or nothing |
| `--seed <n>` | time-based | Seed for cards and the bot; the same seed deals the same session |
| `--stack <bb>` | `100` | Starting stack in big blinds, reset every hand |
| `--hands <n>` | unlimited | Stop after this many hands |
| `--log <file>` | `hands.jsonl` | File that finished hands are appended to |
| `--plain` | off | Line-by-line interface; also used automatically when input is piped |
| `--no-llm` | off | Use the engine's own words even if an API key is set |

### The coach

The poker engine works out every number; the coach's job is to teach with them,
in as few words as the point needs. After each decision you get a grade, what
it cost, the best options side by side with yours, and the facts that decided
it. The title bar keeps the two measures that matter: what your mistakes have
cost so far (per 100 hands, which is far steadier than your result), and your
score on the coach's questions by skill.

With `ANTHROPIC_API_KEY` set (or `ANTHROPIC_AUTH_TOKEN`), the coach is Claude
(`claude-opus-5-5` by default; change it with `POKER_COACH_MODEL`). It receives
the engine's facts for each decision (the hand so far, equity, pot odds, the
bot's likely hands, the value of every option and your recurring leaks) and is
told to use only those numbers, lead with the verdict, and cut anything that
does not help the decision. Replies stream into the coach panel. Each
request sends the current hand's conversation to the Anthropic API and is
billed to your key; nothing else leaves your machine.

Values are averages over the bot's likely hands. On the river they are exact.
Before it they add an estimate of the betting still to come: a hand ahead of
what continues gains from later streets, a hand behind pays on them, and before
the flop a weak hand is counted at less than its raw equity. An all-in of more
than twice the pot is priced if you choose it, but is not offered as "best".

### Leaks and drills

Every finished hand is appended to `hands.jsonl`. Between sessions:

```sh
poker leaks            # your costliest recurring mistakes, with the principle that fixes each
poker drill            # replay the 10 costliest past mistakes as practice spots
poker replay           # print and verify every stored hand
```

```
Leaks, costliest first:
  1. checking when betting was better on the flop — 21 time(s), about 54.69bb
     Bet your strong hands and some of your draws: checking lets worse hands
     see cards for free and wins smaller pots.
  2. calling when folding was better on the river — 6 time(s), about 46.59bb
     A bet narrows the bettor's hands towards strong ones. Judge your equity
     against what they bet with, not against any two cards.
```

With Docker, hand histories live in the `hands` volume and survive between
runs, and the API key is passed through from your shell:

```sh
docker compose run --rm poker --seed 7
docker compose run --rm poker leaks
docker compose run --rm poker drill
```

## Roadmap

| Step | What | State |
| --- | --- | --- |
| 1 | Rules engine, hand evaluator, hand records, playable CLI | ✅ done |
| 2 | A realistic opponent, and a coach that reads its range and prices every option | ✅ done |
| 3 | Teaching: questions before each decision, an LLM coach, leak tracking and drills | ✅ done |
| 4 | Opponent styles (station, nit, maniac), a mystery opponent to read, one-key questions and scannable reviews | ✅ done |
| 5 | Tables of three to nine players, with positions, limpers and multiway pots | planned |
| 6 | Opponents fitted to real players from hands you log after a game | planned |
| 7 | Later streets played out rather than estimated; a solver baseline | planned |

Ideas and help on any of these are welcome: see [Contributing](#contributing).

## Architecture

The library is pure (no I/O, no clocks, no global state); the terminal client
sits on top.

| Module | Responsibility |
| --- | --- |
| [`cards`](src/cards.rs) | Cards, the deck and the seeded shuffle |
| [`eval`](src/eval.rs) | Best five-card hand out of five to seven cards; straight and flush draws |
| [`game`](src/game.rs) | Betting rules for one hand as a deterministic state machine |
| [`range`](src/range.rs) | The 1326 two-card combinations, hand strength on a board, ranges and equity against them |
| [`bot`](src/bot.rs) | The `Bot` trait, the playing styles (solid, station, nit, maniac) and the placeholder bots |
| [`coach`](src/coach.rs) | Spot analysis, option values, reviews, questions and the facts given to the LLM |
| [`analysis`](src/analysis.rs) | Re-running the coach over past hands; leaks and drill spots |
| [`session`](src/session.rs) | A session against a bot: deals, the bot's range, what it has been seen to do, score and leaks |
| [`history`](src/history.rs) | Hand records, storage and replay verification |
| [`app`](src/app) | The terminal client: full-screen table, plain mode and the LLM coach |

A bot implements one trait. `tendencies` describes its strategy (its chances
of folding, calling and raising with any hand), which is what lets the coach
read its range:

```rust
pub trait Bot {
    fn name(&self) -> &'static str;
    /// Chooses an action for `seat`, which is the player to act.
    fn act(&mut self, state: &HandState, seat: usize) -> Action;
    /// How it plays the player to act in `state` when holding `hole`.
    fn tendencies(&self, state: &HandState, hole: [Card; 2], strengths: &Strengths) -> Mix;
    // Optional: its bet size, and a describable style.
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
