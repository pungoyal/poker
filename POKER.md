# Poker Ready Reckoner (Pro Level)

This is a high-density reference for NLHE (primary), with transferable concepts for PLO and tournaments.

## 1) Core Vocabulary

- **EV (Expected Value):** Average long-run value of an action.
- **Realization (R):** Fraction of equity you actually convert into winnings.
- **Equity:** Share of pot if all cards were run out from current state.
- **Fold Equity (FE):** EV gained from opponent folds.
- **MDF (Minimum Defense Frequency):** Defense needed to prevent auto-profit bluffs.
- **MOF (Minimum Offense Frequency):** Bluff share needed to keep opponent indifferent.
- **Pot Odds:** Price you get on a call relative to final pot.
- **Implied Odds:** Future money you expect to win when you hit.
- **Reverse Implied Odds:** Future money you expect to lose when dominated.
- **SPR (Stack-to-Pot Ratio):** Effective stack / pot at street start.
- **Nutted Region:** Strongest hands in a range.
- **Capped Range:** Range unlikely to contain top-strength hands.
- **Polarized Range:** Mostly very strong hands + bluffs.
- **Merged/Linear Range:** Broad value-heavy distribution without extreme bluff split.
- **Blockers:** Cards reducing opponent combos of a hand class.
- **Unblockers:** Cards that do not block likely folds (useful for bluffing).
- **Range Advantage:** More total equity distribution on board.
- **Nut Advantage:** More combinations of top-end hands on board.
- **Node Lock:** Forcing assumptions in solver at a decision node.
- **ICM:** Tournament chip value is non-linear in money terms.

## 2) Fast Math (Must-Know)

- **Pot odds break-even call %** = `call / (pot + call)`
- **Break-even bluff frequency (river, 1 bet size)** = `bet / (pot + bet)`
- **Optimal river bluff:value ratio (by combos)** = `bet / pot`
- **MDF vs bet size** = `pot / (pot + bet)`
- **Required equity for jam/call** = `risk / (risk + reward)`
- **Approx outs to river:** `outs * 4` (flop), `outs * 2` (turn)
- **Combo counting:**
- Pocket pair = 6 combos
- Suited hand XYs = 4 combos
- Offsuit hand XYo = 12 combos

## 3) Preflop Fundamentals

- **RFI (Raise First In):** Position-driven opening ranges; expand from EP to BTN/SB.
- **3-bet strategy:** Linear vs loose openers/callers; polarized vs tight folders.
- **4-bet strategy:** Value-heavy at low stakes, mixed at high stakes.
- **Squeeze:** Increased value/bluff EV due to dead money and capped callers.
- **Cold call discipline:** Prefer fewer cold calls OOP; avoid dominated broadways.
- **Iso-raise:** Punish limps with larger sizings, especially in position.
- **Defending blinds:** Wide but structured; avoid dominated offsuit trash.

### Preflop Heuristics (Cash, 100bb)

- Open sizes: 2.0-2.5x (larger in rake-heavy/live games).
- 3-bet sizes:
- IP: ~3x open
- OOP: ~4x open
- Vs 3-bet 4-bet size:
- IP: ~2.1-2.3x
- OOP: ~2.3-2.6x
- Use lower SPR for value-heavy lines with overpairs/AK on many textures.

## 4) Board Texture Classification

- **Static boards:** `K72r`, `AA5r` -> equities change less on later streets.
- **Dynamic boards:** `JT9ss`, `876tt` -> turn/river shift equities heavily.
- **High-card boards:** Better for preflop raiser c-bets.
- **Low/connected boards:** Better for caller/BB defense ranges.
- **Paired boards:** Favor range betting at small sizes in many formations.
- **Monotone boards:** Nut advantage and blocker effects dominate.

## 5) C-Betting Framework

- C-bet more when:
- You have range + nut advantage.
- Opponent overfolds to flop c-bets.
- Your hand benefits from fold equity now.
- Check more when:
- Board heavily favors caller.
- Your range has realization issues OOP.
- Multiway pots (substantially reduce bluff frequency).
- **Sizing logic:**
- Small size (20-40%): High frequency, range pressure.
- Medium (50-80%): More selective value/protection.
- Large/overbet: Polarized, nut-advantage spots, pressure capped ranges.

## 6) Turn and River Strategy

- Turn is where range separation accelerates.
- Keep barreling with:
- Value that can get called by worse.
- Bluffs with strong blockers + equity/unblockers.
- Slow down with:
- Marginal SDV that dislikes check-raise.
- Bluffs that block folds and unblock calls.
- River:
- Decide at range level: value threshold + bluff candidates.
- Prefer bluffs blocking calls, unblocking folds.
- Against population under-bluffing, overfold less bluff-catchers.

## 7) Bet Sizing Theory (Practical)

- **Small bet:** Realize equity cheaply, deny overcards, force high-frequency defense.
- **Large bet:** Extract from inelastic value region and generate fold pressure.
- **Overbet:** Use when opponent is capped and your range retains nuts.
- **Jam thresholds:** Best when SPR low or nut/value density high.
- **Geometric sizing:** Choose bet sequence that efficiently stacks by river.

## 8) Check-Raising and Probe Lines

- **Check-raise for value:** Strong made hands + robust draws.
- **Check-raise bluff:** Equity + blockers + future barrel cards.
- **Probe bet (after missed c-bet):**
- Strong in single-raised pots where aggressor over-checks.
- Size up on dynamic boards and vs capped checks.

## 9) Common Leaks to Punish

- **Over-fold to 3-bets:** Increase linear value + selective bluffs.
- **Over-cbet flop, under-barrel turn:** Float wider, attack turns.
- **Under-defend vs small bets:** Print with high-frequency range bets.
- **Under-bluff river:** Hero-fold more bluff-catchers.
- **Face-up sizing tells:** Adjust exploitatively by size class.
- **Calling too wide preflop in rake games:** Tighten marginal continues.

## 10) Exploitative Adjustments

- Baseline from GTO, deviate where data supports.
- **Vs nits:** Overfold to aggression, bluff small pots less, steal more preflop.
- **Vs stations:** Value bet thinner, bluff less, size bigger for value.
- **Vs maniacs:** Widen bluff-catchers, trap more strong hands, reduce thin bluffs.
- **Vs regs:** Attack node-specific leaks (fold vs XR, turn overfold, river size imbalances).
- Recalibrate when pool composition changes (time/day/stakes/site).

## 11) Range Construction (Street by Street)

- **Flop:** Wide continuation on favorable boards; retain backdoor-heavy bluffs.
- **Turn:** Remove low-equity bluffs; choose blockers intentionally.
- **River:** Polarize; avoid medium-strength hands in big-bet range unless exploit.
- Track combo budget:
- Value combos available.
- Bluff:value ratio by size (`b:p` where `b` is bet and `p` is pot).

## 12) Multiway and Rake Realities

- Multiway drastically reduces bluff EV; tighten aggression.
- Value hands go up; thin bluffs go down.
- In high-rake environments:
- Tighten preflop opens/calls.
- Prefer 3-bet or fold vs marginal flats.
- Avoid small negative-EV defend habits from low-rake solver outputs.

## 13) Tournament Addendum (MTT/SNG)

- **ICM pressure:** Tighten calling, widen shoving in correct spots.
- **Risk premium:** Need more equity to call all-ins as pay jumps matter.
- **Stack depth bands:**
- 40bb+: near cash-like postflop play with ICM overlay.
- 15-25bb: rejam, high-leverage 3-bet jam strategies.
- <=12bb: push/fold dominates.
- **Bubble and FT:** Abuse medium stacks under payout pressure.

## 14) PLO Addendum (If You Cross-Train)

- Equity runs closer; nuts and redraws are critical.
- Position and nut potential dominate pair strength.
- Blocker-led bluffing is mandatory on later streets.
- Avoid non-nut bluff-catching on coordinated runouts.

## 15) Population Notes (Online/Live)

- Low stakes pools often:
- Over-call flop, over-fold turn/river vs big sizes.
- Under-bluff river check-raise/jam lines.
- Live pools:
- Larger preflop sizings work.
- Value bet bigger; bluff less frequently, better targeted.

## 16) Session Process (Pro Discipline)

- Pre-session:
- Define focus node (e.g., BTN vs BB SRP turns).
- Warm up with 10-15 marked hands from prior session.
- In-session:
- Tag uncertain hands by node/spot.
- Track emotional triggers and pace.
- Post-session:
- Review biggest pots + highest-EV uncertainty spots.
- Build one concrete heuristic update per session.

## 17) Study System (How Pros Improve)

- Split study:
- 40% solver/theory
- 40% database/population exploit work
- 20% mental game/process
- Use node clusters, not random hand review.
- Convert outputs into simple executable heuristics.
- Periodically verify exploit assumptions against fresh samples.

## 18) Quick Reference Tables

### Bet Size -> MDF / Bluff BE

- 25% pot: MDF 80.0%, bluff BE 20.0%
- 33% pot: MDF 75.2%, bluff BE 24.8%
- 50% pot: MDF 66.7%, bluff BE 33.3%
- 75% pot: MDF 57.1%, bluff BE 42.9%
- 100% pot: MDF 50.0%, bluff BE 50.0%
- 150% pot: MDF 40.0%, bluff BE 60.0%
- 200% pot: MDF 33.3%, bluff BE 66.7%

### Bet Size -> Target Bluff:Value Ratio

- 25% pot: 1 bluff per 4 value combos
- 33% pot: 1 bluff per 3 value combos
- 50% pot: 1 bluff per 2 value combos
- 75% pot: 3 bluffs per 4 value combos
- 100% pot: 1 bluff per 1 value combo
- 150% pot: 3 bluffs per 2 value combos
- 200% pot: 2 bluffs per 1 value combo

### Common Mental Checklist (Per Decision)

- What is my opponent's range by this node?
- Who has range advantage and nut advantage?
- What size best expresses my range composition?
- Which hands are value, which are bluffs, which are pure checks?
- What exploit read/data justifies deviation?

## 19) Red Flags (Immediate Corrections)

- Auto-cbetting all flops regardless of texture.
- Calling river "because blockers" without combo accounting.
- Bluffing ranges that block folds.
- Ignoring rake impact preflop.
- Using solver lines without population translation.
- Studying hands without mapping them to recurring nodes.

## 20) Minimal Commandments

- Think in ranges, not hands.
- Size with purpose, not habit.
- Count combos before hero actions.
- Separate theory baseline from exploit deviations.
- Prioritize position, nut potential, and realization.
- Protect mental game to protect win rate.
