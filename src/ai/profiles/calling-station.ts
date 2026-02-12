import { AIPersonalityType } from '../../types/player';
import type { AIProfile } from '../types';

/** Calling Station profile
 *
 * The Calling Station plays a wide range of hands passively. They call
 * bets with marginal holdings, rarely raise or bluff, and are very hard
 * to bluff off a hand. They won't fold to c-bets or aggression easily,
 * making them frustrating to play against. Their passive style means
 * they rarely build pots when they have strong hands.
 */
export const profile: AIProfile = {
  type: AIPersonalityType.CallingStation,
  name: 'Calling Station',
  description:
    'Calls too often with weak hands and rarely raises. Nearly impossible to bluff but easy to value-bet against.',
  vpip: 50,
  pfr: 10,
  threeBetFreq: 3,
  cbetFreq: 32,
  foldToCbet: 12,
  bluffFreq: 5,
  foldToAggression: 12,
  aggFreq: 22,
  positionAwareness: 0.3,
  gtoAwareness: 0.1,
};
