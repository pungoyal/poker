export interface SessionStats {
  handsPlayed: number;
  handsWon: number;
  showdownsSeen: number;
  showdownsWon: number;
  vpipHands: number;
  pfrHands: number;
  netChips: number;
}

export interface OpponentTendency {
  hands: number;
  vpipHands: number;
  pfrHands: number;
  aggressiveActions: number;
  passiveActions: number;
}

export const DEFAULT_SESSION_STATS: SessionStats = {
  handsPlayed: 0,
  handsWon: 0,
  showdownsSeen: 0,
  showdownsWon: 0,
  vpipHands: 0,
  pfrHands: 0,
  netChips: 0,
};
