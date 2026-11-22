# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- A web front end, now the way to play. The engine, the opponents and the
  coach are compiled to WebAssembly and run in the browser, on a laptop or a
  phone; nothing of the game runs on a server. It is a static site (`web/www`
  after `web/build.sh`), served by the Docker image and published to GitHub
  Pages by the Deploy workflow.
- The table: cards and the pot on a felt, each player's bet and last action
  beside their seat, one tap or key per action, and bet sizes as presets, a
  slider and an amount in big blinds. An all-in deals the remaining streets
  one after another before the result.
- Reviews as cards: a sound play in one line, a mistake opened in full with
  every option drawn as a bar, yours and the best marked, how often each bet
  gets a fold and your equity when called.
- Questions answered with one tap, with the right answer and yours marked.
- Panels for the opponent (its likely hands as a grid, what they are made of,
  its tendencies as gauges, the four kinds it could be) and the session (the
  result hand by hand as a chart, what mistakes cost and on which street,
  what to work on, question scores, the hand's action log and earlier hands).
- Layouts for every screen: four columns on a very wide one, three or two on
  a laptop, and on a phone the table and the buttons with the rest behind
  tabs, upright or on its side.
- Sessions that survive a reload: a session is saved as its seed and your
  actions and resumed by replaying them, on the same hand. A save that does
  not replay to the same results is refused, and you are told so. Only one
  tab plays the session at a time: another tab that picks it up makes the
  first one stand aside instead of overwriting it. If the engine fails, it
  is restarted from the last save.
- Leaks and drills in the menu, from the hands kept in the browser (in
  IndexedDB, one record per hand); export and import of those hands as a
  file.
- Claude as the coach with your own API key (Settings), called straight from
  the browser.
- Offline play once loaded, and a manifest for adding it to a home screen.

### Changed

- The coach measures against the pot, not in big blinds. After the flop
  bets read as a share of the pot ("raising 50% pot") and a call names the
  bet it faces ("calling a 75% pot bet"); before it, where the pot is only
  the blinds, sizes are multiples of the bet faced ("raising to 2.5x",
  "calling a raise to 3x"). Each option is shown by how far it trails the
  best ("−2.1% pot"), and a mistake by its cost ("2.1% of the pot given
  up"); pot-odds questions work in pots too. Grades follow the cost as a
  share of the pot: close under 1%, an inaccuracy under 5%, a mistake under
  25%, a blunder beyond, measured against at least 5bb after the flop and
  20bb before it, which keeps preflop grades where the big-blind cut-offs
  had them. Claude is told to speak the same way. Hand results and session
  totals stay in big blinds, since they add up across pots of different
  sizes.
- Requests to Claude are smaller: within a hand the facts and the hand's
  actions are sent once and then only when they change, the facts are sent
  compact, and the conversation so far is cached for follow-ups. The coach
  no longer describes opponents that were removed.

### Removed

- The terminal client: the full-screen table, the plain line mode and the
  `replay`, `leaks` and `drill` subcommands, along with the release binaries.
  The web front end replaces them; drills and leaks are in its menu.

### Changed

- After a hand with an all-in, the bot's cards are shown even when someone
  folded to the shove, on the table, in the result line and in the list of
  earlier hands.
- The Docker image serves the web app on port 8080 instead of running the
  terminal client.

### Earlier, in the terminal client

- A layout for wide terminals (170 columns or more): the table, a middle
  column of what is known, and the coach. The felt is no taller or wider than
  its cards need, and draws them large when there is room.
- A range grid: the 169 starting hands lit by how likely the opponent is to
  hold each, with your hand marked. It is one decision behind in `teach` mode
  (`n` shows it live) and shows the final range when a hand ends.
- A session panel: the result hand by hand as a chart, what mistakes cost,
  decisions by grade, chips given up by street, the two costliest leaks, the
  current run of sound decisions and question scores by skill.
- The opponent's tendencies as gauges, a list of earlier hands (cards, seat,
  result, a dot per decision, the cards shown down) and, against a mystery
  opponent, the four kinds it could be.
- Two more questions: how many cards complete your draw, and how often the
  opponent folds to a bet of a given size.
- A guess at the mystery opponent: after 12 hands, and again every 15, the
  coach asks which kind of player you are facing. It is marked when you leave.
- `d` opens or closes the last review in full.

### Changed

- Questions adapt. A skill answered wrong or not yet tried comes up most; one
  answered right several times running is asked about less and less (down to
  one decision in five), never twice in succession, and at most two questions
  are asked per hand once the routine is familiar.
- A sound decision is reviewed in one line (the play, what it was worth and
  the next best option) instead of a full card; the language-model coach is
  asked to explain only the decisions shown in full.
- An answered question takes one line, and a right answer skips the
  explanation. A skipped question is removed.

- Opponent styles: `station` (limps, calls far too much), `nit` (folds to
  pressure) and `maniac` (raises and bluffs too often) alongside `solid`. Each
  comes with a one-line description and how to beat it, shown at the table.
- A mystery opponent, now the default (`--bot mystery`): one of the four
  styles, chosen in secret and named when you leave. An opponent panel shows
  what it has been seen to do: hands played, raises before the flop, folds to
  bets and bets against calls.
- One-key questions: in `teach` mode the coach asks one question per decision
  (the skill practised least), answered with a number key. Scores by skill
  are kept in the title bar.
- Review cards: the verdict, its cost, the best options side by side with the
  one chosen, and the facts that decided it, in place of a paragraph.
- The decision in numbers under your seat (the bet, the pot, the price, stacks
  against the pot); `n` adds your equity and the best play.
- A session report on leaving: result, what mistakes cost per 100 hands,
  decisions by grade, question scores, the three costliest leaks and the
  mystery opponent's name.
- The hand's grades as a row of dots at the end of each hand.

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
- Coaching text is terse: the verdict first, then only the numbers and
  principle that decided the spot, with no filler. The LLM coach is held to
  the same standard.
- A full-screen table: cards on a felt, stacks and chips, an action log, a
  streaming coach panel, one-key actions and a bet slider with pot presets.
  `--plain` keeps the line mode, which is also used when input is piped.
  The coach panel marks the end of each hand (or drill spot) with its result.
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

- The `solid` bot defends against every bet size (it no longer folds half the
  time to a small bet), calls shoves with its best hands, checks some strong
  hands and defends what it checked. Betting any two cards at it no longer
  wins; it did, by about 46bb per 100 hands.
- Option values include an estimate of the betting still to come instead of
  assuming the hand is checked down: weak hands are discounted before the
  flop, hands ahead gain from later streets and hands behind pay on them. The
  coach no longer advises raising nearly every hand before the flop.
- An all-in of more than twice the pot is priced when chosen but no longer
  offered as the best option. Standard bet sizes are rounded to half a big
  blind.
- With an API key the coach's questions are the same one-key questions; Claude
  explains reviews and answers what you ask.
- `--bot station` is the new calling station, which bets its good hands; hand
  records of the old one still replay.
- In a narrow terminal the table and coach stack, and the latest action is
  shown on the table.

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
- The coach panel no longer hides the end of its newest message: text is
  wrapped by word with an exact line count, so it always scrolls fully into view.

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
