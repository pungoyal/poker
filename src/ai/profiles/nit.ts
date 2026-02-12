import { AIPersonalityType } from '../../types/player';
import type { AIProfile } from '../types';

/** Nit profile
 *
 * The Nit plays an extremely tight range and rarely bluffs. They only enter
 * pots with premium or very strong hands, and tend to fold at the first sign
 * of aggression unless they have the goods. Predictable but hard to extract
 * value from because they only continue with strong holdings.
 */
export const profile: AIProfile = {
  type: AIPersonalityType.Nit,
  name: 'Nit',
  description:
    'Plays only premium hands and rarely bluffs. Extremely tight and predictable but hard to extract value from.',
  vpip: 18,
  pfr: 12,
  threeBetFreq: 5,
  cbetFreq: 58,
  foldToCbet: 48,
  bluffFreq: 8,
  foldToAggression: 48,
  aggFreq: 45,
  positionAwareness: 0.5,
  gtoAwareness: 0.4,
};
