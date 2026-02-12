import { Card } from './card';

export enum HandCategory {
  HighCard = 0,
  Pair = 1,
  TwoPair = 2,
  ThreeOfAKind = 3,
  Straight = 4,
  Flush = 5,
  FullHouse = 6,
  FourOfAKind = 7,
  StraightFlush = 8,
  RoyalFlush = 9,
}

export const HAND_CATEGORY_NAMES: Record<HandCategory, string> = {
  [HandCategory.HighCard]: 'High Card',
  [HandCategory.Pair]: 'Pair',
  [HandCategory.TwoPair]: 'Two Pair',
  [HandCategory.ThreeOfAKind]: 'Three of a Kind',
  [HandCategory.Straight]: 'Straight',
  [HandCategory.Flush]: 'Flush',
  [HandCategory.FullHouse]: 'Full House',
  [HandCategory.FourOfAKind]: 'Four of a Kind',
  [HandCategory.StraightFlush]: 'Straight Flush',
  [HandCategory.RoyalFlush]: 'Royal Flush',
};

export interface HandEvalResult {
  category: HandCategory;
  rank: number;           // Lower = better (for comparison)
  cards: Card[];          // Best 5 cards
  description: string;    // e.g. "Pair of Kings, Ace kicker"
}

export enum DrawType {
  FlushDraw = 'flush-draw',
  OpenEndedStraightDraw = 'oesd',
  GutShotStraightDraw = 'gutshot',
  BackdoorFlushDraw = 'backdoor-flush',
  BackdoorStraightDraw = 'backdoor-straight',
  OverCards = 'overcards',
}

export interface DrawInfo {
  type: DrawType;
  outs: Card[];
  description: string;
}
