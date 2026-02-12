import { Card } from './card';

export enum Position {
  Button = 'BTN',
  SmallBlind = 'SB',
  BigBlind = 'BB',
  UTG = 'UTG',
  UTG1 = 'UTG+1',
  MP = 'MP',
  LJ = 'LJ',
  HJ = 'HJ',
  CO = 'CO',
}

export enum AIPersonalityType {
  TAG = 'tag',
  LAG = 'lag',
  Nit = 'nit',
  Maniac = 'maniac',
  CallingStation = 'calling-station',
}

export interface Player {
  id: string;
  name: string;
  stack: number;
  holeCards: [Card, Card] | null;
  currentBet: number;
  totalBetThisHand: number;
  isFolded: boolean;
  isAllIn: boolean;
  isHuman: boolean;
  position: Position;
  seatIndex: number;
  personality?: AIPersonalityType;
}

export function createPlayer(
  id: string,
  name: string,
  stack: number,
  seatIndex: number,
  isHuman: boolean,
  personality?: AIPersonalityType
): Player {
  return {
    id,
    name,
    stack,
    holeCards: null,
    currentBet: 0,
    totalBetThisHand: 0,
    isFolded: false,
    isAllIn: false,
    isHuman,
    position: Position.Button,
    seatIndex,
    personality,
  };
}
