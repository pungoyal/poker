import { AIPersonalityType } from '../../types/player';
import type { AIProfile } from '../types';

/** Loose-Aggressive (LAG) profile
 *
 * The LAG plays a wide range of hands aggressively. This is a high-variance
 * style that puts constant pressure on opponents. LAGs are difficult to play
 * against because they can have anything, and they bet and raise relentlessly.
 * They are position-aware and capable of creative bluffs.
 */
export const profile: AIProfile = {
  type: AIPersonalityType.LAG,
  name: 'Loose-Aggressive',
  description:
    'Plays a wide range of hands with relentless aggression. Hard to read and puts constant pressure on opponents.',
  vpip: 40,
  pfr: 32,
  threeBetFreq: 13,
  cbetFreq: 78,
  foldToCbet: 28,
  bluffFreq: 35,
  foldToAggression: 22,
  aggFreq: 75,
  positionAwareness: 0.75,
  gtoAwareness: 0.7,
};
