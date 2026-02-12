import { Card, cardsEqual } from '../types/card';
import { OutsInfo } from '../types/odds';
import { HandCategory } from '../types/hand';
import { evaluateHand } from '../evaluation/evaluator';
import { detectDraws } from '../evaluation/draws';
import { createDeck } from '../engine/deck';

/**
 * Calculates the number of outs for the hero.
 *
 * For each remaining card in the deck, checks whether adding it to the
 * hero's hand (hole cards + community cards) would improve the hand
 * category or sub-ranking. Also categorises the draws using detectDraws.
 *
 * Only meaningful on flop and turn (when there are cards still to come).
 */
export function calculateOuts(
  holeCards: [Card, Card],
  communityCards: Card[],
): OutsInfo {
  const knownCards = [...holeCards, ...communityCards];
  const remainingDeck = createDeck().filter(
    dc => !knownCards.some(kc => cardsEqual(dc, kc)),
  );

  // Current hand strength (only evaluable with >= 5 cards)
  const canEvaluate = knownCards.length >= 5;
  const currentHand = canEvaluate ? evaluateHand(knownCards) : null;

  // Find cards that improve our hand
  const outCards: Card[] = [];

  for (const card of remainingDeck) {
    const hypothetical = [...knownCards, card];

    // We need at least 5 cards to evaluate
    if (hypothetical.length < 5) continue;

    const hypotheticalHand = evaluateHand(hypothetical);

    if (!currentHand) {
      // If we couldn't evaluate before (< 5 cards), any evaluable hand is an "improvement"
      // This shouldn't normally happen since outs are calculated on flop+ (5+ cards)
      outCards.push(card);
    } else if (
      hypotheticalHand.category > currentHand.category ||
      (hypotheticalHand.category === currentHand.category &&
        hypotheticalHand.rank < currentHand.rank)
    ) {
      // Hand improved: either a higher category or a better rank within the same category
      outCards.push(card);
    }
  }

  // Detect categorised draws
  const draws = detectDraws(holeCards, communityCards, remainingDeck);

  // Build a human-readable description
  const improvementDescription = buildImprovementDescription(
    outCards.length,
    draws,
    currentHand?.category,
  );

  return {
    totalOuts: outCards.length,
    draws,
    outCards,
    improvementDescription,
  };
}

function buildImprovementDescription(
  totalOuts: number,
  draws: { type: string; description: string }[],
  currentCategory?: HandCategory,
): string {
  if (totalOuts === 0) {
    return currentCategory !== undefined && currentCategory >= HandCategory.Flush
      ? 'Strong made hand, no draw needed'
      : 'No apparent outs to improve';
  }

  const drawDescriptions = draws.map(d => d.description);

  if (drawDescriptions.length === 0) {
    return `${totalOuts} out${totalOuts === 1 ? '' : 's'} to improve`;
  }

  return `${totalOuts} out${totalOuts === 1 ? '' : 's'}: ${drawDescriptions.join(', ')}`;
}
