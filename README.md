# Poker Lab

A single-player **No-Limit Texas Hold'em training app** built with React + TypeScript.
You play as hero at a 9-max table against personality-driven AI opponents while the app explains decisions with pot odds, equity, outs, EV, and coaching feedback.

## Screenshot

![Poker Lab table screenshot](./screenshot.png)

## What This Project Includes

- Full 9-max table flow: preflop, flop, turn, river, showdown.
- Cash and tournament modes.
- AI opponents with distinct personalities (TAG, LAG, Nit, Maniac, Calling Station).
- Hero coaching HUD with real-time math and recommended actions.
- Action controls with preset sizings, custom slider, all-in confirm, and queued pre-actions.
- Commentary feed (situation, math insight, recommendations, AI actions, showdown, hand review).
- Rival Spotlight panel with exploit notes from observed stats.
- Hand replay panel with step-by-step action timeline.
- Session tracking (hands, net chips, VPIP/PFR, bb/100, coaching score).
- Hand history export to JSON.

## Design Goals

Poker Lab is designed around four principles:

1. **Fast decision loops**: minimal clicks, keyboard actions, and clear action states.
2. **Explainability**: every recommendation is backed by transparent numbers.
3. **Exploit practice**: opponents have recognizable leaks and tendencies.
4. **Reviewability**: replay, commentary, and hand exports support study.

## Core UX Layout

- **Center area**
  - Poker table (seats, community cards, pot, dealer button, action ticker)
  - Hero HUD (math + session/coach indicators)
  - Action bar (buttons, sizings, timer, utility toggles)
- **Right panel**
  - Rival Spotlight
  - Commentary
  - Hand Replay

## Game Modes

### Cash
- Static blinds.
- Rebuy available when hero busts.

### Tournament
- Blinds increase every N hands (`blindIncreaseEveryHands`) by multiplier (`blindIncreaseFactor`).
- Optional antes using `anteBb` (fraction of big blind).
- Ends when one player has chips.

## AI System

### Personalities
- **TAG**: selective, fundamentally strong, position-aware.
- **LAG**: wide and aggressive, high pressure and bluff frequency.
- **Nit**: very tight, low bluff rate, straightforward ranges.
- **Maniac**: hyper-aggressive, wide and volatile.
- **Calling Station**: over-calls, under-raises, hard to bluff.

### Skill Levels
- **Novice**: lower strategic awareness, more mistakes.
- **Standard**: baseline profile behavior.
- **Elite**: stronger positional/GTO-like adjustments and pressure.

## Math + Coaching Engine

When it is hero's turn, the app computes context-aware analysis:

- Hand strength (postflop)
- Outs (flop/turn)
- Pot odds
- Monte Carlo equity
- EV estimate (call/raise/fold framing)
- Board texture and SPR context
- Preflop notation/tier hints

The results feed:

- Hero HUD display
- Commentary recommendations
- Action bar recommended move/sizing highlights
- Coaching score updates and per-decision EV delta

## Controls

### Keyboard
- `F`: Fold
- `C`: Check / Call
- `R`: Raise / Bet
- `Space`: Pause/Resume AI
- `D`: Deal next hand (when hand complete and auto-deal is off)

### Action Bar
- Primary actions: Fold, Check, Call, All-In
- Sizing shortcuts:
  - Preflop: `2.5 BB`, `3 BB`, `4 BB`, `5 BB`
  - Postflop: `1/3 Pot`, `1/2 Pot`, `3/4 Pot`, `Pot`
- Custom raise slider
- Queued actions: Check, Check/Fold, Call
- Utility: pause AI, sound toggle, speed selector

## Session Data and Persistence

The app stores user preferences in browser local storage (`poker-lab-prefs`):

- game settings
- commentary panel visibility
- auto-deal preference
- sound preference

During play, it also tracks:

- session totals (hands, wins, VPIP, PFR, net chips, bb/100)
- coaching stats (score, good decisions, major mistakes, last EV delta)
- opponent tendency summaries
- hand history for replay/export

## Tech Stack

- React 19
- TypeScript
- Vite 7
- Zustand (state management)
- Vitest (tests)
- ESLint
- Biome (configured and run in CI)

## Project Structure

```text
src/
  ai/                 # profiles, ranges, decision engine
  commentary/         # commentary generation
  components/         # table, controls, and right-side panels
  engine/             # dealer flow, actions, pots, showdown
  evaluation/         # hand ranking + draw helpers
  hooks/              # game loop, odds calc, audio cues
  math/               # outs, equity, pot odds, EV
  store/              # Zustand game/session store
  types/              # game, player, hand, odds, stats types
  utils/              # helpers (avatars, etc.)
```

## Getting Started

### 1. Install

```bash
npm ci
```

### 2. Run dev server

```bash
npm run dev
```

### 3. Open app

Use the local URL shown by Vite (typically `http://localhost:5173`).

## Scripts

- `npm run dev` - start local development server
- `npm run build` - type-check and production build
- `npm run preview` - preview built app
- `npm run test` - run unit tests once
- `npm run test:watch` - run tests in watch mode
- `npm run lint` - run ESLint
- `npm run biome` - run Biome lint over `src`

## CI

GitHub Actions (`.github/workflows/ci.yml`) runs on every push and pull request:

1. `npm ci`
2. `npm run biome`
3. `npm test`
4. `npm run build`

## Testing Scope

Current test suites cover core evaluation and showdown behavior:

- `src/evaluation/__tests__/evaluator.test.ts`
- `src/engine/__tests__/showdown.test.ts`

## Exported Hand History Format

Exported JSON includes:

- hand number + timestamp
- hero cards + board cards
- pot and winners
- per-action timeline
- hero net chips
- player ID-to-name mapping

## Notes

- `dist/` and `node_modules/` are intentionally gitignored.
- `screenshot.png` in repo root is used by this README.
