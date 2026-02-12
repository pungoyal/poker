# Poker Lab

Poker Lab is a 9-max No-Limit Texas Hold'em training simulator focused on decision quality for tournament and high-level cash play.

You play as Hero against personality-based AI opponents while the app explains each spot with equity, pot odds, EV, stack pressure context, and post-hand coaching signals.

## Screenshot

![Poker Lab table screenshot](./screenshot.png)

## Current Gameplay Logic

## Core Hand Engine

- Full hand loop: preflop -> flop -> turn -> river -> showdown.
- Side-pot calculation and winner resolution.
- Position/dealer rotation and blind posting.
- Auto-advance streets after betting rounds complete.
- AI turn delays based on speed setting (`slow`, `normal`, `fast`).

## Game Modes

### Cash
- Static blinds.
- Hero can rebuy on bust.

### Tournament
- Blind levels increase every `blindIncreaseEveryHands` by `blindIncreaseFactor`.
- Antes supported via `anteEnabled` and `anteBb`.
- Ends when one player remains.

## AI Opponent Logic

Personalities with distinct frequencies and aggression models:

- TAG
- LAG
- Nit
- Maniac
- Calling Station

AI decisions combine:

- profile frequencies (VPIP/PFR/3-bet/c-bet/bluff/fold-to-aggression)
- hand strength approximation
- board texture
- positional awareness
- skill level adjustments (`novice`, `standard`, `elite`)
- live adaptation to Hero tendencies (VPIP/PFR pressure response)

## Hero Decision Intelligence

When it is Hero's turn, the app computes live `MathAnalysis`:

- hand strength (postflop)
- draws/outs (flop/turn)
- pot odds
- Monte Carlo equity
- EV ladder (fold/call/raise)
- SPR and board texture
- preflop notation + tier
- tournament pressure context

## Tournament Pressure Engine

The strategy context now includes:

- `heroStackBb`
- `averageStackBb`
- `playersRemaining`
- `heroRankByStack`
- `mRatio`
- `stackZone` (`comfort`, `pressure`, `push-fold`, `critical`)
- `pressureStage` (`early`, `middle`, `late`, `final-table`, `bubble`)
- short-stack push/fold hints

This context powers HUD metrics, stage messaging, and action guidance.

## Action Bar (Redesigned)

The bottom action panel is a fixed-shell layout (stable UX, no jumpy resizing) with 4 consistent sections:

1. action controls (fold/check/call/all-in + hand-complete controls)
2. insight slot (stage banner, EV ladder, strategy hint)
3. control slot (no timer indicator, queued actions, utility controls)
4. sizing slot (preset sizings + custom slider/input)

### Decision Timing

- **No hero timer**.
- No auto-timeout fold/check/call.
- Hero can take unlimited time per decision.

## Real-Time Training Features

- EV Ladder display in decision spots (`Fold | Call | Raise`).
- Stage banner for tournament phase guidance.
- Preflop blueprint guidance (position + tier + stack zone aware).
- Recommended sizing highlight from context.
- Commentary stream (situation, math, recommendation, AI action, showdown, hand review).

## Right-Side Panels

### Rival Spotlight

- identifies exploit target from opponent stats
- shows VPIP/PFR/AF + confidence
- provides exploit line + tactical plan

### Leak Detector (new)

- session-level leak checks (looseness, passivity gap, EV bleed)
- severity-ranked fixes (`high`, `medium`, `low`)
- updates as sample size grows

### Hand Replay

- step-by-step action replay per hand
- coach summary for that hand (good decisions, mistakes, worst EV leak)
- mark/unmark hands
- filter by `Marked only`

## Coaching and Review Data

## Session Coaching

Tracked live:

- coaching score
- good decisions
- major mistakes
- last decision EV delta

## Per-Hand Coaching Snapshot

Saved in hand history:

- `goodDecisions`
- `mistakes`
- `worstDeltaEv`
- `worstSpot`
- `marked`

## Hand Export JSON

Export includes:

- hand metadata (number/time)
- hero cards and board
- pot and winners
- full action timeline
- hero net chips
- player map
- coach summary
- marked status

## UX + Input

## Keyboard

- `F`: Fold
- `C`: Check/Call
- `R`: Raise/Bet
- `Space`: Pause/Resume AI
- `D`: Deal next hand (when complete and auto-deal off)

## Utility Controls

- pause/resume AI
- sound on/off
- game speed selector
- auto-deal toggle

## Persistence

Stored in localStorage (`poker-lab-prefs`):

- game settings
- commentary panel open/closed
- auto-deal preference
- sound preference

## Tech Stack

- React 19
- TypeScript
- Vite 7
- Zustand
- Vitest
- ESLint
- Biome

## Project Structure

```text
src/
  ai/                 # profiles, ranges, AI decision engine
  commentary/         # dynamic commentary generation
  components/
    controls/         # Hero HUD + action shell
    panels/           # Spotlight, Leak Detector, Commentary, Replay
    table/            # table, seats, community cards, pot
  engine/             # dealing, betting flow, showdown, pots
  evaluation/         # hand ranking and draw logic
  hooks/              # game flow, odds engine, audio cues
  math/               # outs, equity, EV, pot odds
  store/              # central game/session state
  types/              # strongly typed game contracts
```

## Getting Started

```bash
npm ci
npm run dev
```

Open the Vite URL (typically `http://localhost:5173`).

## Scripts

- `npm run dev`
- `npm run build`
- `npm run preview`
- `npm test`
- `npm run test:watch`
- `npm run lint`
- `npm run biome`

## CI

GitHub Actions runs on every push/PR:

1. `npm ci`
2. `npm run biome`
3. `npm test`
4. `npm run build`

## Tests

Current suites:

- `src/evaluation/__tests__/evaluator.test.ts`
- `src/engine/__tests__/showdown.test.ts`

## Notes

- `dist/` and `node_modules/` are gitignored.
- `screenshot.png` is used by this README.
