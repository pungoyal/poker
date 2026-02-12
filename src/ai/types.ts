import type { AIPersonalityType } from '../types/player';

export interface AIProfile {
  type: AIPersonalityType;
  name: string;
  description: string;
  vpip: number;             // Voluntarily put $ in pot (0-100)
  pfr: number;              // Pre-flop raise percentage (0-100)
  threeBetFreq: number;     // 3-bet frequency (0-100)
  cbetFreq: number;         // Continuation bet frequency (0-100)
  foldToCbet: number;       // Fold to c-bet frequency (0-100)
  bluffFreq: number;        // Bluff frequency (0-100)
  foldToAggression: number; // Fold to aggression (0-100)
  aggFreq: number;          // Aggression frequency (0-100)
  positionAwareness: number; // 0-1, how much position affects decisions
  gtoAwareness: number;     // 0-1, how close to GTO the player plays
}
