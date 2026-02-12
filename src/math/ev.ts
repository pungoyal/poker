import { ActionType } from '../types/game';
import { EVInfo } from '../types/odds';

/**
 * Calculate expected value (EV) for the three fundamental actions:
 * fold, call, and raise.
 *
 * EV(fold)  = 0  (baseline -- we neither gain nor lose additional chips)
 *
 * EV(call)  = equity * pot  -  (1 - equity) * costToCall
 *   where equity is expressed as a decimal (0-1).
 *
 * EV(raise) = equity_vs_call * (pot + raiseAmount)
 *           - (1 - equity_vs_call) * (costToCall + raiseAmount)
 *           + foldEquity * pot
 *   A simplified model: we assume that when we raise, some fraction of the
 *   time the opponent folds (foldEquity) and the rest of the time we go to
 *   showdown with our normal equity.
 *
 * @param equity       Hero's equity as a percentage (0-100)
 * @param potSize      Total chips in the pot before hero acts
 * @param costToCall   Amount hero must put in to call
 * @param raiseAmount  Total size of hero's raise (including the call portion).
 *                     Pass 0 if raising is not an option.
 * @param foldEquity   Estimated probability (0-1) that opponents fold to a
 *                     raise. Defaults to 0.3 (30%).
 */
export function calculateEV(
  equity: number,
  potSize: number,
  costToCall: number,
  raiseAmount: number = 0,
  foldEquity: number = 0.3,
): EVInfo {
  const eq = equity / 100; // convert percentage to decimal

  // --- Fold EV ---
  const foldEV = 0;

  // --- Call EV ---
  // If we call: we win the pot (pot + our call) when ahead, lose our call when behind.
  // Net gain when we win: pot (what was already there)
  // Net loss when we lose: -costToCall
  const callEV = costToCall === 0
    ? 0 // Checking is free, EV of checking is 0 in this simplified model
    : roundTo2(eq * potSize - (1 - eq) * costToCall);

  // --- Raise EV ---
  let raiseEV: number;
  if (raiseAmount <= 0) {
    // Raising is not an option (e.g. already all-in or no chips to raise)
    raiseEV = -Infinity;
  } else {
    // When we raise to raiseAmount total, two things can happen:
    // 1. Opponent folds (probability = foldEquity): we win current pot
    // 2. Opponent calls (probability = 1 - foldEquity): showdown with
    //    larger pot. Pot becomes potSize + raiseAmount (opponent matches).
    //    If we win: net gain = potSize + raiseAmount (opponent's call portion)
    //    If we lose: net loss = -raiseAmount
    const showdownEV =
      eq * (potSize + raiseAmount) - (1 - eq) * raiseAmount;
    raiseEV = roundTo2(
      foldEquity * potSize + (1 - foldEquity) * showdownEV,
    );
  }

  // Determine best action
  const bestAction = determineBestAction(foldEV, callEV, raiseEV, costToCall);

  // Build human-readable reasoning
  const reasoning = buildReasoning(
    bestAction,
    equity,
    callEV,
    raiseEV,
    potSize,
    costToCall,
    raiseAmount,
  );

  return {
    foldEV,
    callEV,
    raiseEV: raiseEV === -Infinity ? 0 : raiseEV,
    bestAction,
    reasoning,
  };
}

function determineBestAction(
  foldEV: number,
  callEV: number,
  raiseEV: number,
  costToCall: number,
): ActionType {
  // If there's nothing to call, we check (fold makes no sense)
  if (costToCall === 0) {
    if (raiseEV > 0 && raiseEV > callEV) return ActionType.Raise;
    return ActionType.Check;
  }

  // Compare the three EVs
  if (raiseEV > callEV && raiseEV > foldEV) return ActionType.Raise;
  if (callEV > foldEV) return ActionType.Call;
  return ActionType.Fold;
}

function buildReasoning(
  bestAction: ActionType,
  equity: number,
  callEV: number,
  raiseEV: number,
  potSize: number,
  costToCall: number,
  raiseAmount: number,
): string {
  const equityStr = `${equity.toFixed(1)}%`;
  const potOddsPct =
    costToCall > 0
      ? ((costToCall / (potSize + costToCall)) * 100).toFixed(1)
      : '0';

  switch (bestAction) {
    case ActionType.Fold:
      return (
        `With ${equityStr} equity and needing ${potOddsPct}% to call, ` +
        `you are not getting the right price. ` +
        `Fold EV: 0, Call EV: ${callEV.toFixed(1)}. ` +
        `Folding is the least costly line here.`
      );

    case ActionType.Check:
      if (raiseEV > 0) {
        return (
          `Checking is free. With ${equityStr} equity, a raise to ${raiseAmount} ` +
          `has +EV of ${raiseEV.toFixed(1)} but checking keeps the pot controlled. ` +
          `Consider a bet for value or as a bluff depending on board texture.`
        );
      }
      return (
        `Checking is free with ${equityStr} equity. ` +
        `No strong incentive to build the pot -- take a free card.`
      );

    case ActionType.Call:
      return (
        `With ${equityStr} equity and pot odds of ${potOddsPct}%, ` +
        `you are getting the right price to call. ` +
        `Call EV: +${callEV.toFixed(1)} chips. ` +
        `Calling is the most profitable play.`
      );

    case ActionType.Raise:
      return (
        `With ${equityStr} equity, a raise to ${raiseAmount} is the highest-EV play ` +
        `(Raise EV: +${raiseEV.toFixed(1)} vs Call EV: ${callEV.toFixed(1)}). ` +
        `Applying pressure combines fold equity with showdown value.`
      );

    default:
      return `Best action: ${bestAction}. Equity: ${equityStr}.`;
  }
}

function roundTo2(n: number): number {
  return Math.round(n * 100) / 100;
}
