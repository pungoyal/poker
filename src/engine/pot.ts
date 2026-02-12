import { Player } from '../types/player';
import { Pot } from '../types/game';

/**
 * Calculate pots including side pots for all-in situations.
 *
 * Side pots are created ONLY when a player is all-in for less than
 * the maximum bet. Otherwise, all money goes into a single main pot.
 *
 * Algorithm:
 * 1. Find all-in players whose total bet is less than the max (these create side pot boundaries)
 * 2. For each boundary level, calculate each player's contribution to that slice
 * 3. Only non-folded players who contributed up to a level are eligible for that pot
 */
export function calculatePots(players: Player[]): Pot[] {
  const totalMoney = players.reduce((sum, p) => sum + p.totalBetThisHand, 0);
  if (totalMoney === 0) return [];

  const nonFolded = players.filter(p => !p.isFolded);
  const allNonFoldedIds = nonFolded.map(p => p.id);
  if (allNonFoldedIds.length === 0) return [];

  // Only all-in players who bet LESS than the max create real side pots
  const maxBet = Math.max(...players.map(p => p.totalBetThisHand));
  const shortAllIns = nonFolded
    .filter(p => p.isAllIn && p.totalBetThisHand < maxBet)
    .map(p => p.totalBetThisHand);

  // No short all-ins → single pot with all money
  if (shortAllIns.length === 0) {
    return [{ amount: totalMoney, eligiblePlayerIds: allNonFoldedIds }];
  }

  // Build side pots from all-in boundaries
  const levels = [...new Set([...shortAllIns, maxBet])].sort((a, b) => a - b);
  const bettingPlayers = players.filter(p => p.totalBetThisHand > 0);
  const pots: Pot[] = [];
  let previousLevel = 0;

  for (const level of levels) {
    let potAmount = 0;
    for (const p of bettingPlayers) {
      potAmount += Math.max(0,
        Math.min(p.totalBetThisHand, level) - Math.min(p.totalBetThisHand, previousLevel)
      );
    }

    // Eligible: non-folded players who contributed at least up to this level
    const eligible = nonFolded
      .filter(p => p.totalBetThisHand >= level)
      .map(p => p.id);

    if (potAmount > 0 && eligible.length > 0) {
      pots.push({ amount: potAmount, eligiblePlayerIds: eligible });
    } else if (potAmount > 0 && pots.length > 0) {
      // Dead money (all eligible players folded) → add to previous pot
      pots[pots.length - 1].amount += potAmount;
    }

    previousLevel = level;
  }

  return pots.length > 0 ? pots : [{ amount: totalMoney, eligiblePlayerIds: allNonFoldedIds }];
}

/** Get total pot size across all pots */
export function totalPotSize(pots: Pot[]): number {
  return pots.reduce((sum, p) => sum + p.amount, 0);
}
