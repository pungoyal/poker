import { Card } from './card';
import { PlayerAction } from './game';

export interface HandHistoryEntry {
  handNumber: number;
  endedAt: number;
  heroCards: [Card, Card] | null;
  board: Card[];
  pot: number;
  playerNames: Record<string, string>;
  winners: { playerId: string; playerName: string; amount: number; hand?: string }[];
  heroNetChips: number;
  actions: PlayerAction[];
  marked: boolean;
  coach: {
    goodDecisions: number;
    mistakes: number;
    worstDeltaEv: number;
    worstSpot?: string;
  };
}
