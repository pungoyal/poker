import { Card } from './card';
import { Player } from './player';

export enum Street {
  Preflop = 'preflop',
  Flop = 'flop',
  Turn = 'turn',
  River = 'river',
  Showdown = 'showdown',
}

export enum ActionType {
  Fold = 'fold',
  Check = 'check',
  Call = 'call',
  Bet = 'bet',
  Raise = 'raise',
  AllIn = 'all-in',
  PostBlind = 'post-blind',
}

export interface PlayerAction {
  playerId: string;
  type: ActionType;
  amount: number;
  street: Street;
}

export interface Pot {
  amount: number;
  eligiblePlayerIds: string[];
}

export interface GameState {
  handNumber: number;
  street: Street;
  deck: Card[];
  communityCards: Card[];
  pots: Pot[];
  currentBet: number;
  minRaise: number;
  players: Player[];
  dealerIndex: number;
  activePlayerIndex: number;
  actions: PlayerAction[];
  isHandComplete: boolean;
  winners: { playerId: string; amount: number; hand?: string }[];
}

export interface GameSettings {
  startingStack: number;
  smallBlind: number;
  bigBlind: number;
  numOpponents: number;
  gameSpeed: 'slow' | 'normal' | 'fast';
  aiSkill: 'novice' | 'standard' | 'elite';
  gameMode: 'cash' | 'tournament';
  blindIncreaseEveryHands: number;
  blindIncreaseFactor: number;
  anteEnabled: boolean;
  anteBb: number;
}

export const DEFAULT_SETTINGS: GameSettings = {
  startingStack: 1000,
  smallBlind: 5,
  bigBlind: 10,
  numOpponents: 8,
  gameSpeed: 'normal',
  aiSkill: 'standard',
  gameMode: 'cash',
  blindIncreaseEveryHands: 10,
  blindIncreaseFactor: 1.5,
  anteEnabled: true,
  anteBb: 0.1,
};
