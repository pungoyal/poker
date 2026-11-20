<h1 align="center">♠ poker</h1>

<p align="center">
  <strong>A heads-up No-Limit Hold'em trainer for the terminal.</strong><br>
  Play a realistic opponent with a coach that teaches you how to decide.
</p>

<p align="center">
  <a href="https://github.com/pungoyal/poker/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/pungoyal/poker/actions/workflows/ci.yml/badge.svg"></a>
  <a href="#licence"><img alt="Licence: MIT OR Apache-2.0" src="https://img.shields.io/badge/licence-MIT%20OR%20Apache--2.0-blue.svg"></a>
  <img alt="MSRV 1.88" src="https://img.shields.io/badge/rust-1.88%2B-orange.svg">
</p>

---

A trainer for getting better at heads-up No-Limit Hold'em. You play a sound,
realistic opponent at a full-screen table, and a coach teaches you to make each
decision: before you act it asks you to read the opponent's hand, estimate your
equity and work out the price; after you act it tells you what your choice was
worth, what was best and why. Your mistakes are tracked across sessions, and
`poker drill` replays the costliest ones as practice.

## Features

- **A realistic opponent.** The default `solid` bot plays like a sound regular:
  a range of starting hands that narrows as bets get bigger, value bets,
  semi-bluffs, continuation bets and balanced bluffs, one bet size for value
  and bluffs alike. It folds to about half of postflop bets, as a good player
  does, so the lessons carry over to real tables.
- **A coach that reads the opponent.** Every bot publishes its strategy, so the
  coach narrows the bot's likely hands with each action it takes (Bayes' rule)
  and measures your equity against those hands, not against any two cards. It
  prices every option (fold, check, call and five bet sizes) by how often the
  bot folds and how strong the hands are that continue, and when you check out
  of position it includes the bot betting into you.
- **Questions before you act.** In the default `teach` mode the coach asks you
  to read the bot's range, estimate your equity and work out the pot odds,
  checks each answer, and then reviews the decision you make.
- **An LLM coach.** With an Anthropic API key, Claude does the talking: it asks
  the questions, answers yours (press `/` at any time) and explains every
  review in its own words, working only from the engine's exact numbers.
- **Leaks and drills.** `poker leaks` finds your costliest recurring mistakes
  across sessions; `poker drill` replays them as practice spots.
- **A full-screen table.** Cards on a felt, stacks and chips, an action log, a
  coach panel, one-key actions and a bet slider with pot-sized presets. A plain
  line mode is kept for pipes and transcripts (`--plain`).
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
 ♠ poker   Hand 1  ·  vs solid  ·  session 0bb                                      seed 7
╭──────────────────────────────────────────────────────────╮╭ Coach ─────────────────────────╮
│                 BOT   big blind  ·  83.8bb               ││Coach After its actions, is the │
│                      ╳╳╳╳ ╳╳╳╳                           ││bot's range mostly [s]trong,    │
│                      ╳╳╳╳ ╳╳╳╳                           ││mostly [w]eak, or [m]ixed?      │
│                      ╳╳╳╳ ╳╳╳╳                           ││                                │
│                        ● 9.2bb                           ││                                │
│                                                          ││You s                           │
│               Q    4    A    ┌──┐ ┌──┐                   ││                                │
│                ♣    ♦    ♥   │  │ │  │                   ││Coach Right. strong 80% · medium│
│                  Q    4    A └──┘ └──┘                   ││1% · draws 0% · weak 18%: mostly│
│                 Pot 23.2bb   ·   Flop                    ││strong. Each bet strips out     │
│                                                          ││hands that would fold or check. │
│                      2    3                              ││                                │
│                       ♠    ♣                             ││                                │
│                         2    3                           ││                                │
│               ▶  YOU   button  ·  93bb                   ││                                │
│       ace high · gutshot straight draw (4 outs)          ││                                │
╰──────────────────────────────────────────────────────────╯╰────────────────────────────────╯
 f  fold   c  call 9.2bb   b  bet/raise   1-5  sizes   a  all-in 93bb   /  ask coach   ?  help
```

Both players start every hand with 100 big blinds and the button alternates.

| Key | Action | Key | Action |
| --- | --- | --- | --- |
| `f` | fold | `b` or `r` | bet or raise: pick a size, then enter |
| `c` / `x` | call / check | `1`–`5` | a third, half, three quarters, the pot, all-in |
| `a` | all-in | `←` `→` | adjust the size, or type an amount in bb |
| `/` | ask the coach anything | `esc` | skip the coach's questions and act |
| `n` | show or hide the numbers | `space` | next hand |
| `?` | help | `q` | leave the table |

Options:

| Flag | Default | Meaning |
| --- | --- | --- |
| `--bot <solid\|random\|station>` | `solid` | Opponent: a sound regular, a bot that ignores its cards, or a calling station |
| `--coach <teach\|full\|after\|off>` | `teach` | Questions before each decision and a review after; the numbers before and a review after; a review only; or nothing |
| `--seed <n>` | time-based | Seed for cards and the bot; the same seed deals the same session |
| `--stack <bb>` | `100` | Starting stack in big blinds, reset every hand |
| `--hands <n>` | unlimited | Stop after this many hands |
| `--log <file>` | `hands.jsonl` | File that finished hands are appended to |
| `--plain` | off | Line-by-line interface; also used automatically when input is piped |
| `--no-llm` | off | Use the engine's own words even if an API key is set |

### The coach

The poker engine works out every number; the coach's job is to teach with them,
in as few words as the point needs. After each decision you get a grade, what
your choice was worth against the best option, and the facts that decided it:

```
  Coach · Mistake:
    Three-two offsuit preflop: 28% against its likely hands. Calling 5bb
    -1.08bb; folding 0bb: 1.08bb given up. Its range: strong 80% · medium 1% ·
    draws 0% · weak 18%. Calling 5bb into 9bb needs 36%; you have 28%: just
    short.
```

With `ANTHROPIC_API_KEY` set (or `ANTHROPIC_AUTH_TOKEN`), the coach is Claude
(`claude-opus-5-5` by default; change it with `POKER_COACH_MODEL`). It receives
the engine's facts for each decision (the hand so far, equity, pot odds, the
bot's likely hands, the value of every option and your recurring leaks) and is
told to use only those numbers, lead with the verdict, and cut anything that
does not help the decision. Replies stream into the coach panel. Each
request sends the current hand's conversation to the Anthropic API and is
billed to your key; nothing else leaves your machine.

Values are averages over the bot's likely hands and assume the hand is checked
down afterwards, apart from the bot's reply to a check, so before the river
they are estimates.

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
| 4 | Solver-backed opponent: game-theory-optimal play for exact costs at higher stakes | planned |
| 5 | Opponent profiles: practise against loose, tight and aggressive styles | planned |

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
| [`bot`](src/bot.rs) | The `Bot` trait, the solid bot and the placeholder bots |
| [`coach`](src/coach.rs) | Spot analysis, option values, reviews, questions and the facts given to the LLM |
| [`analysis`](src/analysis.rs) | Re-running the coach over past hands; leaks and drill spots |
| [`session`](src/session.rs) | A session against a bot: deals, the bot's range, score and leaks |
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
