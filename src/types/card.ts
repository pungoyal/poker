export enum Suit {
  Spades = 's',
  Hearts = 'h',
  Diamonds = 'd',
  Clubs = 'c',
}

export enum Rank {
  Two = 2,
  Three = 3,
  Four = 4,
  Five = 5,
  Six = 6,
  Seven = 7,
  Eight = 8,
  Nine = 9,
  Ten = 10,
  Jack = 11,
  Queen = 12,
  King = 13,
  Ace = 14,
}

export interface Card {
  rank: Rank;
  suit: Suit;
}

export const SUIT_SYMBOLS: Record<Suit, string> = {
  [Suit.Spades]: '\u2660',
  [Suit.Hearts]: '\u2665',
  [Suit.Diamonds]: '\u2666',
  [Suit.Clubs]: '\u2663',
};

export const RANK_SYMBOLS: Record<Rank, string> = {
  [Rank.Two]: '2',
  [Rank.Three]: '3',
  [Rank.Four]: '4',
  [Rank.Five]: '5',
  [Rank.Six]: '6',
  [Rank.Seven]: '7',
  [Rank.Eight]: '8',
  [Rank.Nine]: '9',
  [Rank.Ten]: 'T',
  [Rank.Jack]: 'J',
  [Rank.Queen]: 'Q',
  [Rank.King]: 'K',
  [Rank.Ace]: 'A',
};

export const RANK_NAMES: Record<Rank, string> = {
  [Rank.Two]: 'Two',
  [Rank.Three]: 'Three',
  [Rank.Four]: 'Four',
  [Rank.Five]: 'Five',
  [Rank.Six]: 'Six',
  [Rank.Seven]: 'Seven',
  [Rank.Eight]: 'Eight',
  [Rank.Nine]: 'Nine',
  [Rank.Ten]: 'Ten',
  [Rank.Jack]: 'Jack',
  [Rank.Queen]: 'Queen',
  [Rank.King]: 'King',
  [Rank.Ace]: 'Ace',
};

export function cardToString(card: Card): string {
  return `${RANK_SYMBOLS[card.rank]}${card.suit}`;
}

export function cardDisplayName(card: Card): string {
  return `${RANK_NAMES[card.rank]} of ${card.suit === Suit.Spades ? 'Spades' : card.suit === Suit.Hearts ? 'Hearts' : card.suit === Suit.Diamonds ? 'Diamonds' : 'Clubs'}`;
}

export function cardsEqual(a: Card, b: Card): boolean {
  return a.rank === b.rank && a.suit === b.suit;
}
