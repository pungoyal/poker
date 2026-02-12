import { PotOddsInfo } from '../types/odds';

/**
 * Calculate pot odds given the current pot size and cost to call.
 *
 * Pot odds tell us the ratio of the current pot to the cost of a call.
 * If pot odds as a percentage exceed the equity we need to break even,
 * we are "getting the right price" to call.
 *
 * @param potSize   Total chips already in the pot (before hero's call)
 * @param costToCall  Amount hero must put in to continue
 * @param equityNeeded  Hero's required equity to break even (0-100). Optional.
 *                      When provided, isGettingOdds is based on this value.
 *                      When omitted, isGettingOdds defaults to false.
 */
export function calculatePotOdds(
  potSize: number,
  costToCall: number,
  equityNeeded?: number,
): PotOddsInfo {
  if (costToCall <= 0) {
    // Free check -- infinite odds, always "getting the right price"
    return {
      potSize,
      costToCall: 0,
      ratio: 'free',
      percentage: 0,
      isGettingOdds: true,
    };
  }

  // Pot odds ratio = pot : cost-to-call
  const ratioValue = potSize / costToCall;
  const ratio = `${roundToOneDp(ratioValue)}:1`;

  // Pot odds as a percentage: costToCall / (pot + costToCall) * 100
  // This is the minimum equity we need to break even on a call.
  const percentage = roundToOneDp((costToCall / (potSize + costToCall)) * 100);

  // If the equity we need to win is lower than the pot-odds percentage
  // requirement, we are getting the right price. Equivalently, if our
  // actual equity exceeds the break-even percentage, calling is +EV.
  // When equityNeeded is supplied it represents hero's estimated equity
  // (0-100), so we compare: equity > break-even %.
  const isGettingOdds =
    equityNeeded !== undefined ? equityNeeded > percentage : false;

  return {
    potSize,
    costToCall,
    ratio,
    percentage,
    isGettingOdds,
  };
}

function roundToOneDp(n: number): number {
  return Math.round(n * 10) / 10;
}
