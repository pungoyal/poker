import { type Card, type Rank, RANK_SYMBOLS } from '../types/card';
import { Position } from '../types/player';
import type { AIProfile } from './types';

// --------------------------------------------------------------------------
// Hand notation helpers
// --------------------------------------------------------------------------

/** Convert a two-card hand into a canonical string such as "AKs", "77", "T9o" */
function handNotation(cards: [Card, Card]): string {
  const [a, b] = cards;
  const highRank = Math.max(a.rank, b.rank) as Rank;
  const lowRank = Math.min(a.rank, b.rank) as Rank;
  const highSym = RANK_SYMBOLS[highRank];
  const lowSym = RANK_SYMBOLS[lowRank];

  if (highRank === lowRank) {
    return `${highSym}${lowSym}`; // pocket pair, e.g. "TT"
  }
  const suited = a.suit === b.suit;
  return `${highSym}${lowSym}${suited ? 's' : 'o'}`;
}

// --------------------------------------------------------------------------
// Preflop hand-strength tiers
// --------------------------------------------------------------------------

export enum HandTier {
  Premium = 'premium',
  Strong = 'strong',
  Medium = 'medium',
  Speculative = 'speculative',
  Trash = 'trash',
}

const PREMIUM: Set<string> = new Set([
  'AA', 'KK', 'QQ', 'AKs', 'AKo',
]);

const STRONG: Set<string> = new Set([
  'JJ', 'TT', 'AQs', 'AQo', 'AJs', 'KQs',
]);

const MEDIUM: Set<string> = new Set([
  '99', '88', '77',
  'ATs', 'A9s', 'A8s', 'A7s',
  'KJs', 'KTs', 'QJs', 'QTs', 'JTs', 'T9s',
  'KQo', 'AJo', 'ATo', 'KJo', 'QJo',
]);

const SPECULATIVE: Set<string> = new Set([
  '66', '55', '44', '33', '22',
  '98s', '87s', '76s', '65s', '54s', '43s',
  'A6s', 'A5s', 'A4s', 'A3s', 'A2s',
  'K9s', 'Q9s', 'J9s', '97s', '86s',
  'KTo', 'QTo', 'JTo', 'A9o',
]);

/** Returns the tier for a canonical hand notation string. */
export function getHandTier(notation: string): HandTier {
  if (PREMIUM.has(notation)) return HandTier.Premium;
  if (STRONG.has(notation)) return HandTier.Strong;
  if (MEDIUM.has(notation)) return HandTier.Medium;
  if (SPECULATIVE.has(notation)) return HandTier.Speculative;
  return HandTier.Trash;
}

// --------------------------------------------------------------------------
// Tier thresholds mapped to approximate VPIP percentages.
//
// Each tier covers a cumulative portion of the ~1326 unique starting hands:
//   Premium  ~  2.5 %   (AA-QQ, AKs, AKo)
//   Strong   ~  5.5 %   (+ JJ-TT, AQs/o, AJs, KQs)
//   Medium   ~ 12   %   (+ 99-77, broadways, suited connectors)
//   Speculative ~ 23 %  (+ small pairs, suited connectors/aces, offsuit broadways)
//   Trash    ~ 100  %
//
// When effectiveVpip exceeds a threshold, that ENTIRE tier is in range.
// The NEXT tier up is played at a scaled borderline probability.
//   vpip > 4   -> Premium fully in range
//   vpip > 8   -> + Strong fully in range
//   vpip > 15  -> + Medium fully in range
//   vpip > 25  -> + Speculative fully in range
//   vpip > 45  -> + Trash played proportionally
// --------------------------------------------------------------------------

const TIER_VPIP_THRESHOLDS: { tier: HandTier; maxVpip: number }[] = [
  { tier: HandTier.Premium, maxVpip: 4 },
  { tier: HandTier.Strong, maxVpip: 8 },
  { tier: HandTier.Medium, maxVpip: 15 },
  { tier: HandTier.Speculative, maxVpip: 25 },
  { tier: HandTier.Trash, maxVpip: 45 },
];

// --------------------------------------------------------------------------
// Position multipliers
//
// Late position (BTN, CO) allows wider ranges; early position (UTG) tightens.
// --------------------------------------------------------------------------

const POSITION_MULTIPLIERS: Record<Position, number> = {
  [Position.UTG]: 0.82,
  [Position.UTG1]: 0.85,
  [Position.MP]: 0.88,
  [Position.LJ]: 0.92,
  [Position.HJ]: 0.96,
  [Position.CO]: 1.0,
  [Position.Button]: 1.15,
  [Position.SmallBlind]: 0.92,
  [Position.BigBlind]: 1.05, // Already invested, defends wider
};

// --------------------------------------------------------------------------
// Public API
// --------------------------------------------------------------------------

/**
 * Determine whether the AI should play a given hand preflop, taking into
 * account its VPIP, PFR, position, and position-awareness setting.
 *
 * The function works in three steps:
 *   1. Classify the hand into a tier (premium .. trash).
 *   2. Compute an effective VPIP by blending the raw VPIP with a positional
 *      adjustment controlled by the profile's `positionAwareness`.
 *   3. If the hand's tier falls within the effective VPIP threshold the hand
 *      is "in range".  For borderline tiers a random roll is used so the AI
 *      is not perfectly deterministic.
 */
export function isHandInRange(
  cards: [Card, Card],
  profile: AIProfile,
  position: Position,
): boolean {
  const notation = handNotation(cards);
  const tier = getHandTier(notation);

  // Always play premium hands
  if (tier === HandTier.Premium) return true;

  // Compute positional adjustment
  const posMult = POSITION_MULTIPLIERS[position];
  // Blend toward the multiplier based on how position-aware the profile is.
  // positionAwareness=1  -> full positional adjustment
  // positionAwareness=0  -> no adjustment (multiplier = 1)
  const effectiveMultiplier = 1 + (posMult - 1) * profile.positionAwareness;
  const effectiveVpip = profile.vpip * effectiveMultiplier;

  // Determine the maximum tier fully allowed by the effective VPIP.
  // When effectiveVpip exceeds a tier's threshold, that tier is FULLY in range.
  let maxAllowedTier: HandTier = HandTier.Premium;
  for (const { tier: t, maxVpip } of TIER_VPIP_THRESHOLDS) {
    if (effectiveVpip > maxVpip) {
      maxAllowedTier = t;
    }
  }

  const TIER_ORDER: HandTier[] = [
    HandTier.Premium,
    HandTier.Strong,
    HandTier.Medium,
    HandTier.Speculative,
    HandTier.Trash,
  ];

  const handTierIndex = TIER_ORDER.indexOf(tier);
  const maxTierIndex = TIER_ORDER.indexOf(maxAllowedTier);

  // Hand is at or below the max allowed tier → fully in range
  if (handTierIndex <= maxTierIndex) {
    return true;
  }

  // Hand is ONE tier above max allowed → borderline, scaled by how close VPIP
  // is to the next threshold. E.g. TAG vpip 28, Speculative threshold 25:
  // already past, so Speculative is fully in range via the check above.
  // But if VPIP is 22, threshold is 25: (22-15)/(25-15) = 70% chance.
  if (handTierIndex === maxTierIndex + 1) {
    const currentThreshold = TIER_VPIP_THRESHOLDS[maxTierIndex]?.maxVpip ?? 0;
    const nextThreshold = TIER_VPIP_THRESHOLDS[maxTierIndex + 1]?.maxVpip ?? 60;
    const range = Math.max(1, nextThreshold - currentThreshold);
    const progress = effectiveVpip - currentThreshold;
    const probability = Math.min(0.9, Math.max(0.08, progress / range));
    return Math.random() < probability;
  }

  // Two tiers outside → small chance to prevent robotic play
  if (handTierIndex === maxTierIndex + 2) {
    return Math.random() < 0.06;
  }

  return false;
}

/**
 * Given that a hand is in range, determine whether the AI should raise
 * (PFR) or just limp/call.  Returns true if the AI should raise.
 *
 * Logic: PFR / VPIP gives the raise-vs-call ratio.  Position further
 * influences the decision via positionAwareness.
 */
export function shouldRaisePreflop(
  cards: [Card, Card],
  profile: AIProfile,
  position: Position,
): boolean {
  const notation = handNotation(cards);
  const tier = getHandTier(notation);

  // Premium hands are always raised
  if (tier === HandTier.Premium) return true;

  // Base raise probability = PFR / VPIP
  const baseRaiseProb = profile.vpip > 0 ? profile.pfr / profile.vpip : 0;

  // Position bonus: in late position, raise more often
  const posMult = POSITION_MULTIPLIERS[position];
  const posBonus = (posMult - 1) * profile.positionAwareness * 0.15;

  // Stronger tiers get a raise probability boost
  const tierBonus =
    tier === HandTier.Strong
      ? 0.15
      : tier === HandTier.Medium
        ? 0.05
        : 0;

  const raiseProb = Math.min(1, baseRaiseProb + posBonus + tierBonus);
  return Math.random() < raiseProb;
}

export { handNotation };
