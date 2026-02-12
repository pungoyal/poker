import { AIPersonalityType } from '../../types/player';
import type { AIProfile } from '../types';

/** Tight-Aggressive (TAG) profile
 *
 * The TAG plays a narrow range of strong hands but plays them aggressively.
 * This is considered the most fundamentally sound style and is the baseline
 * for winning poker strategy. TAGs are selective preflop, bet and raise
 * frequently when they enter pots, and are highly position-aware.
 */
export const profile: AIProfile = {
  type: AIPersonalityType.TAG,
  name: 'Tight-Aggressive',
  description:
    'Plays a selective range of hands aggressively. Strong positional awareness and solid fundamentals.',
  vpip: 28,
  pfr: 22,
  threeBetFreq: 8,
  cbetFreq: 72,
  foldToCbet: 38,
  bluffFreq: 18,
  foldToAggression: 30,
  aggFreq: 68,
  positionAwareness: 0.85,
  gtoAwareness: 0.8,
};
