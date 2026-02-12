import type { Card } from '../types/card';
import { ActionType, Street, type GameState } from '../types/game';
import { Position, type Player } from '../types/player';
import { HandCategory } from '../types/hand';
import { evaluateHand } from '../evaluation/evaluator';
import { getAvailableActions, type AvailableActions } from '../engine/actions';
import type { AIProfile } from './types';
import { isHandInRange, shouldRaisePreflop, handNotation, getHandTier, HandTier } from './preflop-ranges';

// --------------------------------------------------------------------------
// Public API
// --------------------------------------------------------------------------

export interface AIDecisionResult {
  action: ActionType;
  amount: number;
  reasoning: string;
}

/**
 * Core decision function. Given the current player, game state, and their AI
 * profile, returns the action to take, the amount (for bets/raises), and a
 * human-readable reasoning string for the commentary system.
 */
export function makeAIDecision(
  player: Player,
  gameState: GameState,
  profile: AIProfile,
): AIDecisionResult {
  const available = getAvailableActions(gameState);

  // Safety: if no actions are available, check (should not happen in practice)
  if (!available.canFold && !available.canCheck && !available.canCall &&
      !available.canBet && !available.canRaise) {
    return { action: ActionType.Check, amount: 0, reasoning: `${player.name} has no available actions.` };
  }

  if (gameState.street === Street.Preflop) {
    return makePreflopDecision(player, gameState, profile, available);
  }

  return makePostflopDecision(player, gameState, profile, available);
}

// --------------------------------------------------------------------------
// Preflop decision logic
// --------------------------------------------------------------------------

function makePreflopDecision(
  player: Player,
  gameState: GameState,
  profile: AIProfile,
  available: AvailableActions,
): AIDecisionResult {
  const cards = player.holeCards;
  if (!cards) {
    return foldOrCheck(player, available, 'no hole cards dealt');
  }

  const notation = handNotation(cards);
  const tier = getHandTier(notation);
  const inRange = isHandInRange(cards, profile, player.position);

  // ---------- Hand is NOT in range -> fold (or check from BB) ----------
  if (!inRange) {
    return foldOrCheck(player, available, `${notation} is outside their range`);
  }

  // ---------- Hand IS in range ----------
  const facingRaise = available.callAmount > 0;
  const shouldRaise = shouldRaisePreflop(cards, profile, player.position);

  // Facing a raise: decide to 3-bet, call, or fold
  if (facingRaise) {
    return handleFacingRaisePreflop(player, profile, available, notation, tier);
  }

  // No raise to face (limped to us, or we are first to act in an unopened pot)
  if (shouldRaise && (available.canBet || available.canRaise)) {
    const raiseAmount = choosePreflopRaiseSize(player, gameState, profile, available);
    const actionType = available.canRaise ? ActionType.Raise : ActionType.Bet;
    const posName = positionName(player.position);
    return {
      action: actionType,
      amount: raiseAmount,
      reasoning: `${player.name} opens with a raise from ${posName}, representing a ${tierDescription(tier)} range.`,
    };
  }

  // Limp / check
  if (available.canCheck) {
    return {
      action: ActionType.Check,
      amount: 0,
      reasoning: `${player.name} checks with ${notation} from ${positionName(player.position)}.`,
    };
  }

  if (available.canCall) {
    return {
      action: ActionType.Call,
      amount: available.callAmount,
      reasoning: `${player.name} limps in with ${notation} from ${positionName(player.position)}.`,
    };
  }

  return foldOrCheck(player, available, 'cannot find a suitable action');
}

function handleFacingRaisePreflop(
  player: Player,
  profile: AIProfile,
  available: AvailableActions,
  notation: string,
  tier: HandTier,
): AIDecisionResult {
  const posName = positionName(player.position);

  // Premium hands: 3-bet most of the time
  if (tier === HandTier.Premium) {
    if (available.canRaise) {
      const threeBetAmount = Math.min(
        available.callAmount * 3,
        available.maxRaise,
      );
      return {
        action: ActionType.Raise,
        amount: Math.max(threeBetAmount, available.minRaise),
        reasoning: `${player.name} 3-bets from ${posName} with a premium hand.`,
      };
    }
    return {
      action: ActionType.Call,
      amount: available.callAmount,
      reasoning: `${player.name} calls the raise from ${posName} with a premium hand, trapping.`,
    };
  }

  // 3-bet bluff / value based on threeBetFreq
  const threeBetRoll = Math.random() * 100;
  if (threeBetRoll < profile.threeBetFreq && available.canRaise) {
    const threeBetAmount = Math.min(
      available.callAmount * 3,
      available.maxRaise,
    );
    return {
      action: ActionType.Raise,
      amount: Math.max(threeBetAmount, available.minRaise),
      reasoning: `${player.name} puts in a 3-bet from ${posName}, applying pressure.`,
    };
  }

  // Decide call vs fold based on hand strength and fold-to-aggression
  // Stronger tiers call more; higher foldToAggression folds more marginal hands
  const callThreshold = tierCallThreshold(tier, profile);
  const callRoll = Math.random() * 100;

  if (callRoll < callThreshold) {
    return {
      action: ActionType.Call,
      amount: available.callAmount,
      reasoning: `${player.name} calls the raise from ${posName} with a ${tierDescription(tier)} hand.`,
    };
  }

  return {
    action: ActionType.Fold,
    amount: 0,
    reasoning: `${player.name} folds to the raise from ${posName}, not willing to continue with ${notation}.`,
  };
}

// --------------------------------------------------------------------------
// Postflop decision logic
// --------------------------------------------------------------------------

function makePostflopDecision(
  player: Player,
  gameState: GameState,
  profile: AIProfile,
  available: AvailableActions,
): AIDecisionResult {
  const cards = player.holeCards;
  if (!cards) {
    return foldOrCheck(player, available, 'no hole cards');
  }

  const allCards = [...cards, ...gameState.communityCards];
  if (allCards.length < 5) {
    return foldOrCheck(player, available, 'not enough cards to evaluate');
  }

  const handResult = evaluateHand(allCards);
  const handStrength = evaluateRelativeHandStrength(handResult.category, gameState, cards);
  const hasDraws = detectDraws(cards, gameState.communityCards);
  const isLastAggressor = wasLastAggressor(player, gameState);
  const facingBet = available.callAmount > 0;
  const streetName = gameState.street;
  const posName = positionName(player.position);

  // ------- Facing a bet / raise -------
  if (facingBet) {
    return handleFacingBetPostflop(
      player, gameState, profile, available,
      handStrength, hasDraws, handResult.description, posName, streetName,
    );
  }

  // ------- Not facing a bet (we can check or bet) -------
  return handleNoBetPostflop(
    player, gameState, profile, available,
    handStrength, hasDraws, isLastAggressor, handResult.description, posName, streetName,
  );
}

function handleFacingBetPostflop(
  player: Player,
  gameState: GameState,
  profile: AIProfile,
  available: AvailableActions,
  handStrength: number,
  hasDraws: boolean,
  handDesc: string,
  posName: string,
  street: Street,
): AIDecisionResult {
  const potSize = totalPotSize(gameState);
  const potOdds = available.callAmount / (potSize + available.callAmount);
  const checkedThisStreet = didCheckThisStreet(player, gameState);

  // Monster hand (sets+, flushes, etc.): check-raise or raise aggressively
  if (handStrength >= 0.85 && available.canRaise) {
    // Check-raise: if we checked earlier, high raise frequency
    const checkRaiseFreq = checkedThisStreet
      ? 50 + profile.gtoAwareness * 35  // 50-85% depending on sophistication
      : profile.aggFreq;
    if (Math.random() * 100 < checkRaiseFreq) {
      const raiseAmount = choosePostflopRaiseSize(player, gameState, profile, available);
      const verb = checkedThisStreet ? 'check-raises' : 'raises';
      return {
        action: ActionType.Raise,
        amount: raiseAmount,
        reasoning: `${player.name} ${verb} on the ${street} from ${posName} with ${handDesc}, springing the trap!`,
      };
    }
    return {
      action: ActionType.Call,
      amount: available.callAmount,
      reasoning: `${player.name} smooth-calls on the ${street} with ${handDesc}, keeping opponents in.`,
    };
  }

  // Very strong hand (top pair+): raise for value
  if (handStrength >= 0.7) {
    if (available.canRaise && Math.random() * 100 < profile.aggFreq) {
      const raiseAmount = choosePostflopRaiseSize(player, gameState, profile, available);
      return {
        action: ActionType.Raise,
        amount: raiseAmount,
        reasoning: `${player.name} raises on the ${street} from ${posName} with ${handDesc}, building the pot.`,
      };
    }
    return {
      action: ActionType.Call,
      amount: available.callAmount,
      reasoning: `${player.name} smooth-calls on the ${street} with ${handDesc}, disguising strength.`,
    };
  }

  // Decent hand (middle pair, weak top pair): mostly call, sometimes fold to heavy aggression
  if (handStrength >= 0.4) {
    const foldRoll = Math.random() * 100;
    // Larger bets relative to pot increase fold probability
    const betPotRatio = available.callAmount / Math.max(1, potSize);
    const adjustedFoldChance = profile.foldToAggression * betPotRatio;

    if (foldRoll < adjustedFoldChance) {
      return {
        action: ActionType.Fold,
        amount: 0,
        reasoning: `${player.name} folds on the ${street} facing a large bet, unwilling to continue with ${handDesc}.`,
      };
    }

    return {
      action: ActionType.Call,
      amount: available.callAmount,
      reasoning: `${player.name} calls on the ${street} with ${handDesc}, getting a reasonable price.`,
    };
  }

  // Drawing hand: call if odds are reasonable
  if (hasDraws) {
    // Approximate drawing probability: generous for flush/straight draws
    const impliedDrawOdds = 0.35; // ~35% for strong draws
    if (potOdds < impliedDrawOdds) {
      return {
        action: ActionType.Call,
        amount: available.callAmount,
        reasoning: `${player.name} calls on the ${street} chasing a draw with good pot odds.`,
      };
    }
  }

  // Weak hand facing a bet: decide based on foldToAggression and bluffFreq
  // Possibility of a bluff-raise
  if (Math.random() * 100 < profile.bluffFreq * 0.3 && available.canRaise) {
    const raiseAmount = choosePostflopRaiseSize(player, gameState, profile, available);
    return {
      action: ActionType.Raise,
      amount: raiseAmount,
      reasoning: `${player.name} bluff-raises on the ${street} from ${posName}, putting pressure on the bettor.`,
    };
  }

  // Calling station behavior: call with weak hands
  if (Math.random() * 100 > profile.foldToAggression) {
    return {
      action: ActionType.Call,
      amount: available.callAmount,
      reasoning: `${player.name} makes a stubborn call on the ${street} with ${handDesc}.`,
    };
  }

  return {
    action: ActionType.Fold,
    amount: 0,
    reasoning: `${player.name} folds on the ${street} from ${posName}, giving up with ${handDesc}.`,
  };
}

function handleNoBetPostflop(
  player: Player,
  gameState: GameState,
  profile: AIProfile,
  available: AvailableActions,
  handStrength: number,
  hasDraws: boolean,
  isLastAggressor: boolean,
  handDesc: string,
  posName: string,
  street: Street,
): AIDecisionResult {
  // Slow-play / trap with monster hands: check to induce bluffs or set up check-raise
  if (handStrength >= 0.85 && available.canBet) {
    // Slow-play frequency scales with GTO awareness — smarter players trap more
    // Also more likely on dry boards (fewer draws to protect against)
    const boardWetness = estimateBoardWetness(gameState.communityCards);
    const slowPlayFreq = profile.gtoAwareness * 35 * (1 - boardWetness * 0.5);
    if (Math.random() * 100 < slowPlayFreq) {
      return {
        action: ActionType.Check,
        amount: 0,
        reasoning: `${player.name} checks on the ${street} from ${posName} with ${handDesc}, setting a trap.`,
      };
    }
  }

  // C-bet opportunity: we were the last aggressor on the previous street
  // Frequency and sizing adjust by board texture — c-bet more on dry boards, less on wet
  if (isLastAggressor && available.canBet) {
    const boardWet = estimateBoardWetness(gameState.communityCards);
    // Dry boards (0.0): +30% freq boost. Wet boards (0.8+): -25% freq reduction.
    const cbetFreqAdjusted = profile.cbetFreq * (1.3 - boardWet * 0.7);
    const cbetRoll = Math.random() * 100;
    if (cbetRoll < cbetFreqAdjusted) {
      const betAmount = chooseCbetSize(gameState, profile, available);
      const boardDesc = boardWet >= 0.5 ? 'wet' : boardWet <= 0.15 ? 'dry' : '';
      const texturePart = boardDesc ? ` on a ${boardDesc} board` : '';
      return {
        action: ActionType.Bet,
        amount: betAmount,
        reasoning: `${player.name} fires a continuation bet on the ${street} from ${posName}${texturePart}.`,
      };
    }
  }

  // Strong hand: bet for value
  if (handStrength >= 0.6 && available.canBet) {
    const betRoll = Math.random() * 100;
    if (betRoll < profile.aggFreq) {
      const betAmount = chooseValueBetSize(gameState, profile, available);
      return {
        action: ActionType.Bet,
        amount: betAmount,
        reasoning: `${player.name} bets on the ${street} from ${posName} for value with ${handDesc}.`,
      };
    }
  }

  // Bluff opportunity with a weak hand
  if (handStrength < 0.3 && available.canBet) {
    const bluffRoll = Math.random() * 100;
    if (bluffRoll < profile.bluffFreq) {
      const betAmount = chooseBluffSize(gameState, profile, available);
      return {
        action: ActionType.Bet,
        amount: betAmount,
        reasoning: `${player.name} fires a bluff on the ${street} from ${posName}.`,
      };
    }
  }

  // Semi-bluff with draws
  if (hasDraws && available.canBet) {
    const semiBluffRoll = Math.random() * 100;
    const semiBluffFreq = (profile.bluffFreq + profile.aggFreq) / 2;
    if (semiBluffRoll < semiBluffFreq * 0.5) {
      const betAmount = chooseCbetSize(gameState, profile, available);
      return {
        action: ActionType.Bet,
        amount: betAmount,
        reasoning: `${player.name} semi-bluffs on the ${street} from ${posName} with a draw.`,
      };
    }
  }

  // Default: check
  return {
    action: ActionType.Check,
    amount: 0,
    reasoning: `${player.name} checks on the ${street} from ${posName} with ${handDesc}.`,
  };
}

// --------------------------------------------------------------------------
// Hand strength evaluation (relative, 0-1)
// --------------------------------------------------------------------------

/**
 * Estimate hand strength on a 0-1 scale relative to the board.
 * This is a simplified heuristic, not a Monte Carlo simulation.
 *
 * 0.0 = trash (high card on a scary board)
 * 0.5 = marginal (middle pair)
 * 1.0 = nuts (quads, straight flush, etc.)
 */
function evaluateRelativeHandStrength(
  category: HandCategory,
  gameState: GameState,
  holeCards?: [Card, Card],
): number {
  // Base strength from hand category
  const categoryStrengths: Record<HandCategory, number> = {
    [HandCategory.RoyalFlush]: 1.0,
    [HandCategory.StraightFlush]: 0.98,
    [HandCategory.FourOfAKind]: 0.95,
    [HandCategory.FullHouse]: 0.90,
    [HandCategory.Flush]: 0.82,
    [HandCategory.Straight]: 0.75,
    [HandCategory.ThreeOfAKind]: 0.65,
    [HandCategory.TwoPair]: 0.55,
    [HandCategory.Pair]: 0.38,
    [HandCategory.HighCard]: 0.15,
  };

  let strength = categoryStrengths[category] ?? 0.15;

  // Differentiate pair quality: top pair vs middle pair vs bottom pair / pocket pair
  if (category === HandCategory.Pair && holeCards && gameState.communityCards.length >= 3) {
    const boardRanks = gameState.communityCards.map(c => c.rank).sort((a, b) => b - a);
    const heroRanks = holeCards.map(c => c.rank);
    const boardHighCard = boardRanks[0];

    // Check if hero has a pocket pair (overpair, underpair)
    if (heroRanks[0] === heroRanks[1]) {
      const pairRank = heroRanks[0];
      if (pairRank > boardHighCard) {
        strength = 0.62; // Overpair (e.g. QQ on T-7-3)
      } else {
        strength = 0.42; // Underpair
      }
    } else {
      // One hole card pairs with a board card — determine which
      const pairedRank = heroRanks.find(r => boardRanks.includes(r));
      if (pairedRank !== undefined) {
        if (pairedRank === boardHighCard) {
          // Top pair — differentiate by kicker
          const kicker = heroRanks.find(r => r !== pairedRank) ?? 0;
          // Top pair top kicker ~0.60, top pair weak kicker ~0.48
          strength = 0.48 + Math.min(0.14, (kicker / 14) * 0.14);
        } else if (pairedRank === boardRanks[1]) {
          strength = 0.40; // Middle pair
        } else {
          strength = 0.30; // Bottom pair
        }
      }
    }
  }

  // Differentiate two pair quality
  if (category === HandCategory.TwoPair && holeCards && gameState.communityCards.length >= 3) {
    const heroRanks = holeCards.map(c => c.rank);
    const boardRanks = gameState.communityCards.map(c => c.rank);
    // Two pair using both hole cards is much stronger than board-paired two pair
    const bothHoleCardsUsed = heroRanks.every(r => boardRanks.includes(r));
    if (bothHoleCardsUsed) {
      strength = 0.60; // Two pair with both hole cards
    }
    // Board already has a pair → our "two pair" is really just one pair + board pair = weaker
    const boardRankCounts = new Map<number, number>();
    for (const r of boardRanks) {
      boardRankCounts.set(r, (boardRankCounts.get(r) || 0) + 1);
    }
    if (Array.from(boardRankCounts.values()).some(c => c >= 2)) {
      strength = Math.min(strength, 0.50); // Board-paired two pair is weaker
    }
  }

  // Board texture adjustments: paired boards, flush-possible, straight-possible
  const community = gameState.communityCards;
  if (community.length >= 3) {
    const suitCounts = new Map<string, number>();
    const rankCounts = new Map<number, number>();
    for (const c of community) {
      suitCounts.set(c.suit, (suitCounts.get(c.suit) || 0) + 1);
      rankCounts.set(c.rank, (rankCounts.get(c.rank) || 0) + 1);
    }

    // If the board is very wet (flush/straight possible), non-nut hands are weaker
    const maxSuitCount = Math.max(...suitCounts.values());
    if (maxSuitCount >= 3 && category < HandCategory.Flush) {
      strength *= 0.85; // Flush possible, devalue non-flush hands
    }

    // Paired board makes full houses possible, devalue flushes/straights slightly
    const hasPairedBoard = Array.from(rankCounts.values()).some(c => c >= 2);
    if (hasPairedBoard && category === HandCategory.Flush) {
      strength *= 0.92;
    }
    if (hasPairedBoard && category === HandCategory.Straight) {
      strength *= 0.88;
    }
  }

  // Later streets: weak made hands get weaker (more cards = ranges are more defined)
  // Only apply to hands that are already marginal (< 0.5), not strong top pairs/overpairs
  if (gameState.street === Street.River && strength < 0.5) {
    strength *= 0.85;
  }
  if (gameState.street === Street.Turn && category <= HandCategory.Pair) {
    strength *= 0.92;
  }

  return Math.min(1, Math.max(0, strength));
}

// --------------------------------------------------------------------------
// Draw detection
// --------------------------------------------------------------------------

function detectDraws(holeCards: [Card, Card], community: Card[]): boolean {
  if (community.length < 3) return false;

  const allCards = [...holeCards, ...community];

  // Flush draw: 4 cards of the same suit involving at least one hole card
  const suitCounts = new Map<string, number>();
  for (const c of allCards) {
    suitCounts.set(c.suit, (suitCounts.get(c.suit) || 0) + 1);
  }
  for (const [suit, count] of suitCounts) {
    if (count === 4 && holeCards.some(hc => hc.suit === suit)) {
      return true;
    }
  }

  // Open-ended straight draw (simplified): 4 consecutive ranks
  const uniqueRanks = [...new Set(allCards.map(c => c.rank))].sort((a, b) => a - b);
  for (let i = 0; i <= uniqueRanks.length - 4; i++) {
    if (uniqueRanks[i + 3] - uniqueRanks[i] === 3) {
      // 4 consecutive ranks -- check if a hole card is involved
      const segment = new Set([
        uniqueRanks[i], uniqueRanks[i + 1], uniqueRanks[i + 2], uniqueRanks[i + 3],
      ]);
      if (holeCards.some(hc => segment.has(hc.rank))) {
        return true;
      }
    }
  }

  // Gutshot: 4 out of 5 consecutive ranks (one gap)
  for (let i = 0; i <= uniqueRanks.length - 4; i++) {
    if (uniqueRanks[i + 3] - uniqueRanks[i] === 4) {
      const segment = new Set([
        uniqueRanks[i], uniqueRanks[i + 1], uniqueRanks[i + 2], uniqueRanks[i + 3],
      ]);
      if (holeCards.some(hc => segment.has(hc.rank))) {
        return true;
      }
    }
  }

  return false;
}

// --------------------------------------------------------------------------
// Aggressor tracking
// --------------------------------------------------------------------------

function wasLastAggressor(player: Player, gameState: GameState): boolean {
  // Look at the previous street's actions to see if this player was the last raiser/bettor
  const previousStreet = getPreviousStreet(gameState.street);
  if (!previousStreet) return false;

  const previousActions = gameState.actions.filter(a => a.street === previousStreet);
  for (let i = previousActions.length - 1; i >= 0; i--) {
    const action = previousActions[i];
    if (action.type === ActionType.Bet || action.type === ActionType.Raise) {
      return action.playerId === player.id;
    }
  }
  return false;
}

function getPreviousStreet(street: Street): Street | null {
  switch (street) {
    case Street.Flop: return Street.Preflop;
    case Street.Turn: return Street.Flop;
    case Street.River: return Street.Turn;
    default: return null;
  }
}

// --------------------------------------------------------------------------
// Bet sizing helpers
// --------------------------------------------------------------------------

function totalPotSize(gameState: GameState): number {
  return gameState.pots.reduce((sum, p) => sum + p.amount, 0);
}

function getBigBlind(gameState: GameState): number {
  // Infer BB from the first post-blind action or default to a common value
  const blindActions = gameState.actions.filter(a => a.type === ActionType.PostBlind);
  if (blindActions.length >= 2) {
    return Math.max(blindActions[0].amount, blindActions[1].amount);
  }
  if (blindActions.length === 1) {
    return blindActions[0].amount;
  }
  return 10; // fallback
}

function choosePreflopRaiseSize(
  _player: Player,
  gameState: GameState,
  profile: AIProfile,
  available: AvailableActions,
): number {
  const bb = getBigBlind(gameState);

  // Standard open: 2.5-3.5 BB depending on aggression
  const multiplier = 2.5 + (profile.aggFreq / 100);
  let raiseAmount = Math.round(bb * multiplier);

  // Add 1 BB per limper
  const limpers = gameState.actions.filter(
    a => a.street === Street.Preflop && a.type === ActionType.Call,
  ).length;
  raiseAmount += limpers * bb;

  // Clamp to valid range
  raiseAmount = Math.max(available.minRaise, Math.min(raiseAmount, available.maxRaise));

  return raiseAmount;
}

function choosePostflopRaiseSize(
  _player: Player,
  _gameState: GameState,
  profile: AIProfile,
  available: AvailableActions,
): number {
  // Raise 2x-3x the current bet, capped by stack
  const multiplier = 2 + (profile.aggFreq / 100);
  let raiseAmount = Math.round(available.callAmount * multiplier);
  raiseAmount = Math.max(available.minRaise, Math.min(raiseAmount, available.maxRaise));

  return raiseAmount;
}

function chooseCbetSize(
  gameState: GameState,
  profile: AIProfile,
  available: AvailableActions,
): number {
  const pot = totalPotSize(gameState);
  const boardWet = estimateBoardWetness(gameState.communityCards);

  // Base c-bet sizing: 50-75% of pot depending on aggression
  const baseFraction = 0.5 + (profile.aggFreq / 400); // 0.5 to 0.75
  // Adjust: dry boards → smaller (1/3 pot enough), wet boards → larger (charge draws)
  // Dry (0.0): -0.15, Wet (0.8): +0.10
  const textureAdjust = (boardWet - 0.3) * 0.3;
  const fraction = Math.max(0.25, Math.min(0.85, baseFraction + textureAdjust));

  let betAmount = Math.round(pot * fraction);
  betAmount = Math.max(available.minRaise, Math.min(betAmount, available.maxRaise));

  return betAmount;
}

function chooseValueBetSize(
  gameState: GameState,
  profile: AIProfile,
  available: AvailableActions,
): number {
  const pot = totalPotSize(gameState);

  // Value bet: 60-90% of pot
  const fraction = 0.6 + (profile.aggFreq / 300); // 0.6 to ~0.9
  let betAmount = Math.round(pot * fraction);
  betAmount = Math.max(available.minRaise, Math.min(betAmount, available.maxRaise));

  return betAmount;
}

function chooseBluffSize(
  gameState: GameState,
  profile: AIProfile,
  available: AvailableActions,
): number {
  const pot = totalPotSize(gameState);

  // Bluffs: 55-80% of pot (need fold equity)
  const fraction = 0.55 + (profile.bluffFreq / 200); // 0.55 to 0.775
  let betAmount = Math.round(pot * fraction);
  betAmount = Math.max(available.minRaise, Math.min(betAmount, available.maxRaise));

  return betAmount;
}

// --------------------------------------------------------------------------
// Helpers
// --------------------------------------------------------------------------

/** Check if this player already checked on the current street (for check-raise detection) */
function didCheckThisStreet(player: Player, gameState: GameState): boolean {
  return gameState.actions.some(
    a => a.street === gameState.street &&
         a.playerId === player.id &&
         a.type === ActionType.Check,
  );
}

/**
 * Estimate board wetness (0 = dry, 1 = very wet).
 * Wet boards have flush draws, straight draws, or connected cards.
 */
function estimateBoardWetness(community: Card[]): number {
  if (community.length < 3) return 0;

  let wetness = 0;

  // Flush potential: 3+ of same suit
  const suitCounts = new Map<string, number>();
  for (const c of community) {
    suitCounts.set(c.suit, (suitCounts.get(c.suit) || 0) + 1);
  }
  const maxSuit = Math.max(...suitCounts.values());
  if (maxSuit >= 3) wetness += 0.4;

  // Straight potential: connected cards (small gaps between ranks)
  const ranks = [...new Set(community.map(c => c.rank))].sort((a, b) => a - b);
  if (ranks.length >= 3) {
    const span = ranks[ranks.length - 1] - ranks[0];
    // 3 cards within span of 4 = very connected
    if (span <= 4) wetness += 0.4;
    else if (span <= 6) wetness += 0.2;
  }

  // Paired board is slightly drier (fewer combo draws)
  const rankCounts = new Map<number, number>();
  for (const c of community) {
    rankCounts.set(c.rank, (rankCounts.get(c.rank) || 0) + 1);
  }
  if (Array.from(rankCounts.values()).some(c => c >= 2)) {
    wetness -= 0.15;
  }

  return Math.max(0, Math.min(1, wetness));
}

function foldOrCheck(
  player: Player,
  available: AvailableActions,
  reason: string,
): AIDecisionResult {
  if (available.canCheck) {
    return {
      action: ActionType.Check,
      amount: 0,
      reasoning: `${player.name} checks -- ${reason}.`,
    };
  }
  return {
    action: ActionType.Fold,
    amount: 0,
    reasoning: `${player.name} folds -- ${reason}.`,
  };
}

/**
 * Determine how likely the player is to call a preflop raise based on
 * hand tier and profile tendencies.
 */
function tierCallThreshold(tier: HandTier, profile: AIProfile): number {
  // Base call probability varies by tier — must be high enough
  // that in-range hands usually continue facing a standard raise
  const baseCalls: Record<HandTier, number> = {
    [HandTier.Premium]: 100,
    [HandTier.Strong]: 95,
    [HandTier.Medium]: 85,
    [HandTier.Speculative]: 75,
    [HandTier.Trash]: 45,
  };

  const base = baseCalls[tier];
  // Adjust for fold-to-aggression: higher fold-to-agg = lower call threshold
  const foldAdjustment = (profile.foldToAggression - 30) * 0.4; // centered at 30%
  return Math.max(0, Math.min(100, base - foldAdjustment));
}

function tierDescription(tier: HandTier): string {
  switch (tier) {
    case HandTier.Premium: return 'premium';
    case HandTier.Strong: return 'strong';
    case HandTier.Medium: return 'solid';
    case HandTier.Speculative: return 'speculative';
    case HandTier.Trash: return 'marginal';
  }
}

function positionName(position: Position): string {
  switch (position) {
    case Position.Button: return 'the button';
    case Position.SmallBlind: return 'the small blind';
    case Position.BigBlind: return 'the big blind';
    case Position.UTG: return 'under the gun';
    case Position.UTG1: return 'UTG+1';
    case Position.MP: return 'middle position';
    case Position.LJ: return 'the lojack';
    case Position.HJ: return 'the hijack';
    case Position.CO: return 'the cutoff';
  }
}
