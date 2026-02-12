import { Card } from '../types/card';
import { Pot } from '../types/game';
import { Player } from '../types/player';
import { evaluateHand, compareHands } from '../evaluation/evaluator';
import { HandEvalResult } from '../types/hand';

export interface ShowdownResult {
  winners: { playerId: string; amount: number; hand: string }[];
  playerHands: Map<string, HandEvalResult>;
}

/** Resolve showdown: evaluate all hands and distribute pots */
export function resolveShowdown(
  players: Player[],
  communityCards: Card[],
  pots: Pot[]
): ShowdownResult {
  const playerHands = new Map<string, HandEvalResult>();

  // Evaluate each non-folded player's hand
  for (const player of players) {
    if (!player.isFolded && player.holeCards) {
      const allCards = [...player.holeCards, ...communityCards];
      if (allCards.length >= 5) {
        const result = evaluateHand(allCards);
        playerHands.set(player.id, result);
      }
    }
  }

  const winners: { playerId: string; amount: number; hand: string }[] = [];

  // For each pot, determine winner(s)
  for (const pot of pots) {
    const eligibleHands: { playerId: string; result: HandEvalResult }[] = [];

    for (const playerId of pot.eligiblePlayerIds) {
      const result = playerHands.get(playerId);
      if (result) {
        eligibleHands.push({ playerId, result });
      }
    }

    if (eligibleHands.length === 0) continue;

    // Sort by hand rank (lower rank = better)
    eligibleHands.sort((a, b) => compareHands(a.result, b.result));

    // Find all players tied for best hand
    const bestRank = eligibleHands[0].result.rank;
    const potWinners = eligibleHands.filter(h => h.result.rank === bestRank);

    // Split pot among winners
    const share = Math.floor(pot.amount / potWinners.length);
    const remainder = pot.amount - share * potWinners.length;

    for (let i = 0; i < potWinners.length; i++) {
      const existingWinner = winners.find(w => w.playerId === potWinners[i].playerId);
      const amount = share + (i === 0 ? remainder : 0); // First winner gets remainder
      if (existingWinner) {
        existingWinner.amount += amount;
      } else {
        winners.push({
          playerId: potWinners[i].playerId,
          amount,
          hand: potWinners[i].result.description,
        });
      }
    }
  }

  return { winners, playerHands };
}

/** Handle the case where all but one player has folded */
export function resolveByFold(
  players: Player[],
  pots: Pot[]
): ShowdownResult {
  const winner = players.find(p => !p.isFolded);
  if (!winner) return { winners: [], playerHands: new Map() };

  const totalPot = pots.reduce((sum, p) => sum + p.amount, 0);
  return {
    winners: [{ playerId: winner.id, amount: totalPot, hand: 'Everyone folded' }],
    playerHands: new Map(),
  };
}
