import { Card, Rank, RANK_NAMES } from '../types/card';
import { HandCategory, HandEvalResult } from '../types/hand';

/**
 * Hand evaluator for Texas Hold'em.
 *
 * Evaluates 5-card hands using rank/suit analysis.
 * For 7-card evaluation, finds the best 5-card combination out of C(7,5)=21.
 *
 * Rank value: lower rank number = better hand.
 * Category 9 (Royal Flush) is best, category 0 (High Card) is worst.
 * Within each category, sub-ranking breaks ties.
 */

interface FiveCardResult {
  category: HandCategory;
  subRank: number; // Higher = better within category
  bestCards: Card[];
}

function evaluateFiveCards(cards: Card[]): FiveCardResult {
  const ranks = cards.map(c => c.rank).sort((a, b) => b - a);
  const suits = cards.map(c => c.suit);

  // Check flush
  const isFlush = suits.every(s => s === suits[0]);

  // Check straight
  let isStraight = false;
  let straightHigh = 0;

  // Normal straight check
  if (ranks[0] - ranks[4] === 4 && new Set(ranks).size === 5) {
    isStraight = true;
    straightHigh = ranks[0];
  }
  // Wheel (A-2-3-4-5)
  if (!isStraight && ranks[0] === Rank.Ace && ranks[1] === Rank.Five &&
      ranks[2] === Rank.Four && ranks[3] === Rank.Three && ranks[4] === Rank.Two) {
    isStraight = true;
    straightHigh = Rank.Five; // 5-high straight
  }

  // Count rank occurrences
  const rankCounts = new Map<Rank, number>();
  for (const r of ranks) {
    rankCounts.set(r, (rankCounts.get(r) || 0) + 1);
  }

  const counts = Array.from(rankCounts.entries())
    .sort((a, b) => b[1] - a[1] || b[0] - a[0]); // Sort by count desc, then rank desc

  // Determine hand category
  if (isFlush && isStraight) {
    if (straightHigh === Rank.Ace) {
      return { category: HandCategory.RoyalFlush, subRank: 14, bestCards: cards };
    }
    return { category: HandCategory.StraightFlush, subRank: straightHigh, bestCards: cards };
  }

  if (counts[0][1] === 4) {
    const quadRank = counts[0][0];
    const kicker = counts[1][0];
    return {
      category: HandCategory.FourOfAKind,
      subRank: quadRank * 15 + kicker,
      bestCards: cards,
    };
  }

  if (counts[0][1] === 3 && counts[1][1] === 2) {
    const tripRank = counts[0][0];
    const pairRank = counts[1][0];
    return {
      category: HandCategory.FullHouse,
      subRank: tripRank * 15 + pairRank,
      bestCards: cards,
    };
  }

  if (isFlush) {
    const subRank = ranks[0] * 15 ** 4 + ranks[1] * 15 ** 3 + ranks[2] * 15 ** 2 + ranks[3] * 15 + ranks[4];
    return { category: HandCategory.Flush, subRank, bestCards: cards };
  }

  if (isStraight) {
    return { category: HandCategory.Straight, subRank: straightHigh, bestCards: cards };
  }

  if (counts[0][1] === 3) {
    const tripRank = counts[0][0];
    const kickers = counts.slice(1).map(c => c[0]).sort((a, b) => b - a);
    return {
      category: HandCategory.ThreeOfAKind,
      subRank: tripRank * 15 * 15 + kickers[0] * 15 + kickers[1],
      bestCards: cards,
    };
  }

  if (counts[0][1] === 2 && counts[1][1] === 2) {
    const highPair = Math.max(counts[0][0], counts[1][0]);
    const lowPair = Math.min(counts[0][0], counts[1][0]);
    const kicker = counts[2][0];
    return {
      category: HandCategory.TwoPair,
      subRank: highPair * 15 * 15 + lowPair * 15 + kicker,
      bestCards: cards,
    };
  }

  if (counts[0][1] === 2) {
    const pairRank = counts[0][0];
    const kickers = counts.slice(1).map(c => c[0]).sort((a, b) => b - a);
    return {
      category: HandCategory.Pair,
      subRank: pairRank * 15 ** 3 + kickers[0] * 15 ** 2 + kickers[1] * 15 + kickers[2],
      bestCards: cards,
    };
  }

  // High card
  const subRank = ranks[0] * 15 ** 4 + ranks[1] * 15 ** 3 + ranks[2] * 15 ** 2 + ranks[3] * 15 + ranks[4];
  return { category: HandCategory.HighCard, subRank, bestCards: cards };
}

/** Convert category + subRank to a single comparable rank (lower = better) */
function toComparableRank(category: HandCategory, subRank: number): number {
  // Higher category and higher subRank should produce LOWER rank number
  return -((category * 1_000_000) + subRank);
}

/** Generate all C(n,5) five-card combinations from an array */
function* fiveCardCombinations(cards: Card[]): Generator<Card[]> {
  const n = cards.length;
  for (let i = 0; i < n - 4; i++) {
    for (let j = i + 1; j < n - 3; j++) {
      for (let k = j + 1; k < n - 2; k++) {
        for (let l = k + 1; l < n - 1; l++) {
          for (let m = l + 1; m < n; m++) {
            yield [cards[i], cards[j], cards[k], cards[l], cards[m]];
          }
        }
      }
    }
  }
}

function describeHand(category: HandCategory, cards: Card[]): string {
  const ranks = cards.map(c => c.rank).sort((a, b) => b - a);
  const rankCounts = new Map<Rank, number>();
  for (const r of ranks) {
    rankCounts.set(r, (rankCounts.get(r) || 0) + 1);
  }
  const counts = Array.from(rankCounts.entries())
    .sort((a, b) => b[1] - a[1] || b[0] - a[0]);

  const rn = (r: Rank) => RANK_NAMES[r];
  const pluralRn = (r: Rank) => r === Rank.Six ? 'Sixes' : `${RANK_NAMES[r]}s`;

  // Detect wheel (A-2-3-4-5): Ace plays as low, so it's a Five-high straight
  const isWheel = ranks[0] === Rank.Ace && ranks[1] === Rank.Five &&
    ranks[2] === Rank.Four && ranks[3] === Rank.Three && ranks[4] === Rank.Two;

  switch (category) {
    case HandCategory.RoyalFlush:
      return 'Royal Flush';
    case HandCategory.StraightFlush:
      return `Straight Flush, ${rn(isWheel ? Rank.Five : ranks[0])}-high`;
    case HandCategory.FourOfAKind:
      return `Four ${pluralRn(counts[0][0])}`;
    case HandCategory.FullHouse:
      return `Full House, ${pluralRn(counts[0][0])} full of ${pluralRn(counts[1][0])}`;
    case HandCategory.Flush:
      return `Flush, ${rn(ranks[0])}-high`;
    case HandCategory.Straight:
      return `Straight, ${rn(isWheel ? Rank.Five : ranks[0])}-high`;
    case HandCategory.ThreeOfAKind:
      return `Three ${pluralRn(counts[0][0])}`;
    case HandCategory.TwoPair:
      return `Two Pair, ${pluralRn(Math.max(counts[0][0], counts[1][0]))} and ${pluralRn(Math.min(counts[0][0], counts[1][0]))}`;
    case HandCategory.Pair:
      return `Pair of ${pluralRn(counts[0][0])}`;
    case HandCategory.HighCard:
      return `${rn(ranks[0])}-high`;
  }
}

/** Evaluate the best 5-card hand from any number of cards (typically 5-7) */
export function evaluateHand(cards: Card[]): HandEvalResult {
  if (cards.length < 5) {
    // Can still evaluate partial hands for display, just use what we have padded
    // But typically shouldn't be called with < 5 cards
    throw new Error(`Need at least 5 cards, got ${cards.length}`);
  }

  if (cards.length === 5) {
    const result = evaluateFiveCards(cards);
    return {
      category: result.category,
      rank: toComparableRank(result.category, result.subRank),
      cards: result.bestCards,
      description: describeHand(result.category, result.bestCards),
    };
  }

  // For 6 or 7 cards, find best 5-card combination
  let bestResult: FiveCardResult | null = null;
  let bestCards: Card[] = [];

  for (const combo of fiveCardCombinations(cards)) {
    const result = evaluateFiveCards(combo);
    if (!bestResult ||
        result.category > bestResult.category ||
        (result.category === bestResult.category && result.subRank > bestResult.subRank)) {
      bestResult = result;
      bestCards = combo;
    }
  }

  return {
    category: bestResult!.category,
    rank: toComparableRank(bestResult!.category, bestResult!.subRank),
    cards: bestCards,
    description: describeHand(bestResult!.category, bestCards),
  };
}

/** Compare two hand eval results. Returns negative if a wins, positive if b wins, 0 for tie. */
export function compareHands(a: HandEvalResult, b: HandEvalResult): number {
  // Lower rank = better hand, so a.rank - b.rank: negative means a is better
  return a.rank - b.rank;
}
