import { AIPersonalityType } from '../../types/player';
import type { AIProfile } from '../types';

/** Maniac profile
 *
 * The Maniac plays an extremely wide range and bets/raises at every opportunity.
 * They have almost no regard for position or hand strength and will fire
 * multiple barrels as bluffs. Highly unpredictable and creates massive pots.
 * Can be very dangerous when they hit, but bleeds chips in the long run.
 */
export const profile: AIProfile = {
  type: AIPersonalityType.Maniac,
  name: 'Maniac',
  description:
    'Plays nearly every hand with maximum aggression. Unpredictable and creates huge pots, but bleeds chips long-term.',
  vpip: 58,
  pfr: 45,
  threeBetFreq: 18,
  cbetFreq: 82,
  foldToCbet: 15,
  bluffFreq: 50,
  foldToAggression: 10,
  aggFreq: 88,
  positionAwareness: 0.2,
  gtoAwareness: 0.1,
};
