import { Card, cardsEqual } from '../types/card';
import { EquityInfo } from '../types/odds';
import { evaluateHand, compareHands } from '../evaluation/evaluator';
import { createDeck, shuffleDeck } from '../engine/deck';

/**
 * Monte Carlo equity calculator.
 *
 * Runs `iterations` simulated showdowns against `numOpponents` random hands
 * with random board completions. For each iteration:
 *   1. Shuffle the remaining deck.
 *   2. Deal random hole cards to each opponent.
 *   3. Complete the community board to 5 cards.
 *   4. Evaluate all hands and record a win / tie / loss for the hero.
 *
 * Returns the hero's equity as a percentage (0-100).
 *
 * @param holeCards       Hero's two hole cards
 * @param communityCards  Current community cards (0-5)
 * @param numOpponents    Number of opposing players (default 1)
 * @param iterations      Number of Monte Carlo iterations (default 10,000)
 */
export function calculateEquity(
  holeCards: [Card, Card],
  communityCards: Card[],
  numOpponents: number = 1,
  iterations: number = 10_000,
): EquityInfo {
  const knownCards = [...holeCards, ...communityCards];

  // Build the stub remaining deck (cards not already known)
  const baseDeck = buildRemainingDeck(knownCards);

  let wins = 0;
  let ties = 0;
  let losses = 0;

  for (let i = 0; i < iterations; i++) {
    // Shuffle a copy of the remaining deck for this iteration
    const deck = shuffleDeck([...baseDeck]);

    let deckIndex = 0;

    // Deal opponent hole cards
    const opponentHoleCards: [Card, Card][] = [];
    for (let opp = 0; opp < numOpponents; opp++) {
      opponentHoleCards.push([deck[deckIndex++], deck[deckIndex++]]);
    }

    // Complete the board to 5 cards
    const cardsNeeded = 5 - communityCards.length;
    const fullBoard = [
      ...communityCards,
      ...deck.slice(deckIndex, deckIndex + cardsNeeded),
    ];
    deckIndex += cardsNeeded;

    // Evaluate hero's hand
    const heroHand = evaluateHand([...holeCards, ...fullBoard]);

    // Evaluate each opponent's hand and find the best one
    let heroBeatAll = true;
    let heroTiedBest = false;

    for (const oppCards of opponentHoleCards) {
      const oppHand = evaluateHand([...oppCards, ...fullBoard]);
      const cmp = compareHands(heroHand, oppHand);

      if (cmp > 0) {
        // Hero loses to this opponent
        heroBeatAll = false;
        heroTiedBest = false;
        break;
      } else if (cmp === 0) {
        // Tie with this opponent (hero might still lose to another)
        heroTiedBest = true;
      }
      // cmp < 0 means hero beats this opponent, continue checking
    }

    if (heroBeatAll && !heroTiedBest) {
      wins++;
    } else if (heroBeatAll && heroTiedBest) {
      ties++;
    } else {
      losses++;
    }
  }

  // Equity: wins count full, ties count half
  const equity = ((wins + ties * 0.5) / iterations) * 100;

  return {
    equity: Math.round(equity * 10) / 10,
    wins,
    ties,
    losses,
    iterations,
  };
}

/**
 * Build the remaining deck by filtering out known cards.
 * Returns all 52 cards that are not in the knownCards array.
 */
function buildRemainingDeck(knownCards: Card[]): Card[] {
  return createDeck().filter(c => !knownCards.some(kc => cardsEqual(c, kc)));
}
