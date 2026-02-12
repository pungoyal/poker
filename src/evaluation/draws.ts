import { Card, Rank, Suit } from '../types/card';
import { DrawInfo, DrawType } from '../types/hand';

/** Count cards of each suit */
function suitCounts(cards: Card[]): Map<Suit, Card[]> {
  const map = new Map<Suit, Card[]>();
  for (const c of cards) {
    if (!map.has(c.suit)) map.set(c.suit, []);
    map.get(c.suit)!.push(c);
  }
  return map;
}

/** Get all unique ranks sorted ascending */
function uniqueRanksSorted(cards: Card[]): number[] {
  return [...new Set(cards.map(c => c.rank))].sort((a, b) => a - b);
}

/** Detect all draws from hole cards + community cards */
export function detectDraws(
  holeCards: [Card, Card],
  communityCards: Card[],
  remainingDeck: Card[]
): DrawInfo[] {
  const allCards = [...holeCards, ...communityCards];
  const draws: DrawInfo[] = [];

  // --- Flush Draws ---
  const sc = suitCounts(allCards);
  for (const [suit, suited] of sc) {
    if (suited.length === 4) {
      // Flush draw (9 outs)
      const outs = remainingDeck.filter(c => c.suit === suit);
      draws.push({
        type: DrawType.FlushDraw,
        outs,
        description: `Flush draw (${outs.length} outs)`,
      });
    } else if (suited.length === 3 && communityCards.length <= 3) {
      // Backdoor flush draw (only on flop)
      const outs = remainingDeck.filter(c => c.suit === suit);
      draws.push({
        type: DrawType.BackdoorFlushDraw,
        outs: outs.slice(0, 1), // Approximate: need runner-runner
        description: `Backdoor flush draw`,
      });
    }
  }

  // --- Straight Draws ---
  const ranks = uniqueRanksSorted(allCards);

  // Find consecutive sequences of length 4 within a span of 4 (OESD) or 5 (gutshot)
  const allRanksSet = new Set(ranks);
  const hasLowAce = allCards.some(c => c.rank === Rank.Ace);

  // Check for OESD: 4 consecutive ranks, open on both ends
  // Check for Gutshot: 4 out of 5 consecutive ranks with one gap
  for (let start = (hasLowAce ? 1 : 2); start <= 10; start++) {
    const window = [start, start + 1, start + 2, start + 3, start + 4];
    const have = window.filter(r => {
      if (r === 1) return allRanksSet.has(Rank.Ace);
      return allRanksSet.has(r as Rank);
    });

    if (have.length === 4) {
      const missing = window.filter(r => {
        if (r === 1) return !allRanksSet.has(Rank.Ace);
        return !allRanksSet.has(r as Rank);
      });

      const missingRank = missing[0];
      const targetRank = missingRank === 1 ? Rank.Ace : missingRank as Rank;

      // OESD: missing card is at either end of the window
      const isOESD = missingRank === start || missingRank === start + 4;
      // Gutshot: missing card is in the middle
      const isGutshot = !isOESD;

      // But for OESD we actually need TWO cards that complete it
      // (one on each end). Let's do a proper check.
      if (isOESD) {
        // Actually check: do we have 4 in a row? Then the open ends are outs.
        const outs = remainingDeck.filter(c => c.rank === targetRank);
        if (outs.length > 0 && !draws.some(d => d.type === DrawType.OpenEndedStraightDraw)) {
          // For a true OESD, we also need to check the other end
          const otherEnd = missingRank === start ? start + 4 + 1 : start - 1;
          const otherRank = otherEnd === 1 ? Rank.Ace : otherEnd as Rank;
          const otherOuts = (otherEnd >= 2 && otherEnd <= 14)
            ? remainingDeck.filter(c => c.rank === otherRank)
            : [];
          const allOuts = [...outs, ...otherOuts];
          draws.push({
            type: DrawType.OpenEndedStraightDraw,
            outs: allOuts,
            description: `Open-ended straight draw (${allOuts.length} outs)`,
          });
        }
      } else if (isGutshot) {
        const outs = remainingDeck.filter(c => c.rank === targetRank);
        if (outs.length > 0 && !draws.some(d =>
          d.type === DrawType.GutShotStraightDraw &&
          d.outs.some(o => o.rank === targetRank)
        )) {
          draws.push({
            type: DrawType.GutShotStraightDraw,
            outs,
            description: `Gutshot straight draw (${outs.length} outs)`,
          });
        }
      }
    }
  }

  // --- Overcards ---
  if (communityCards.length >= 3) {
    const boardHighCard = Math.max(...communityCards.map(c => c.rank));
    const overcards = holeCards.filter(c => c.rank > boardHighCard);
    if (overcards.length > 0) {
      const outs = remainingDeck.filter(c =>
        overcards.some(oc => oc.rank === c.rank)
      );
      draws.push({
        type: DrawType.OverCards,
        outs,
        description: `${overcards.length} overcard${overcards.length > 1 ? 's' : ''} (${outs.length} outs)`,
      });
    }
  }

  return draws;
}
