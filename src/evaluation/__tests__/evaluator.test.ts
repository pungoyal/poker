import { describe, it, expect } from 'vitest';
import { evaluateHand, compareHands } from '../evaluator';
import { HandCategory } from '../../types/hand';
import { Card, Rank, Suit } from '../../types/card';

function c(rank: Rank, suit: Suit): Card {
  return { rank, suit };
}

const S = Suit.Spades, H = Suit.Hearts, D = Suit.Diamonds, C = Suit.Clubs;

describe('evaluateHand — 5-card evaluation', () => {
  it('detects Royal Flush', () => {
    const cards = [c(Rank.Ace, S), c(Rank.King, S), c(Rank.Queen, S), c(Rank.Jack, S), c(Rank.Ten, S)];
    const result = evaluateHand(cards);
    expect(result.category).toBe(HandCategory.RoyalFlush);
    expect(result.description).toBe('Royal Flush');
  });

  it('detects Straight Flush (non-royal)', () => {
    const cards = [c(Rank.Nine, H), c(Rank.Eight, H), c(Rank.Seven, H), c(Rank.Six, H), c(Rank.Five, H)];
    const result = evaluateHand(cards);
    expect(result.category).toBe(HandCategory.StraightFlush);
    expect(result.description).toBe('Straight Flush, Nine-high');
  });

  it('detects wheel Straight Flush (A-2-3-4-5) with correct description', () => {
    const cards = [c(Rank.Ace, D), c(Rank.Two, D), c(Rank.Three, D), c(Rank.Four, D), c(Rank.Five, D)];
    const result = evaluateHand(cards);
    expect(result.category).toBe(HandCategory.StraightFlush);
    expect(result.description).toBe('Straight Flush, Five-high');
  });

  it('detects Four of a Kind', () => {
    const cards = [c(Rank.Ace, S), c(Rank.Ace, H), c(Rank.Ace, D), c(Rank.Ace, C), c(Rank.King, S)];
    const result = evaluateHand(cards);
    expect(result.category).toBe(HandCategory.FourOfAKind);
    expect(result.description).toBe('Four Aces');
  });

  it('detects Full House', () => {
    const cards = [c(Rank.King, S), c(Rank.King, H), c(Rank.King, D), c(Rank.Seven, C), c(Rank.Seven, S)];
    const result = evaluateHand(cards);
    expect(result.category).toBe(HandCategory.FullHouse);
    expect(result.description).toBe('Full House, Kings full of Sevens');
  });

  it('detects Flush', () => {
    const cards = [c(Rank.Ace, S), c(Rank.Jack, S), c(Rank.Eight, S), c(Rank.Five, S), c(Rank.Three, S)];
    const result = evaluateHand(cards);
    expect(result.category).toBe(HandCategory.Flush);
    expect(result.description).toBe('Flush, Ace-high');
  });

  it('detects Straight (ace-high)', () => {
    const cards = [c(Rank.Ace, S), c(Rank.King, H), c(Rank.Queen, D), c(Rank.Jack, C), c(Rank.Ten, S)];
    const result = evaluateHand(cards);
    expect(result.category).toBe(HandCategory.Straight);
    expect(result.description).toBe('Straight, Ace-high');
  });

  it('detects Wheel Straight (A-2-3-4-5) with correct description', () => {
    const cards = [c(Rank.Ace, S), c(Rank.Two, H), c(Rank.Three, D), c(Rank.Four, C), c(Rank.Five, S)];
    const result = evaluateHand(cards);
    expect(result.category).toBe(HandCategory.Straight);
    expect(result.description).toBe('Straight, Five-high');
  });

  it('detects Three of a Kind', () => {
    const cards = [c(Rank.Queen, S), c(Rank.Queen, H), c(Rank.Queen, D), c(Rank.Nine, C), c(Rank.Five, S)];
    const result = evaluateHand(cards);
    expect(result.category).toBe(HandCategory.ThreeOfAKind);
    expect(result.description).toBe('Three Queens');
  });

  it('detects Two Pair', () => {
    const cards = [c(Rank.King, S), c(Rank.King, H), c(Rank.Seven, D), c(Rank.Seven, C), c(Rank.Ace, S)];
    const result = evaluateHand(cards);
    expect(result.category).toBe(HandCategory.TwoPair);
    expect(result.description).toBe('Two Pair, Kings and Sevens');
  });

  it('detects Pair', () => {
    const cards = [c(Rank.Ace, S), c(Rank.Ace, H), c(Rank.King, D), c(Rank.Queen, C), c(Rank.Jack, S)];
    const result = evaluateHand(cards);
    expect(result.category).toBe(HandCategory.Pair);
    expect(result.description).toBe('Pair of Aces');
  });

  it('detects High Card', () => {
    const cards = [c(Rank.Ace, S), c(Rank.Jack, H), c(Rank.Eight, D), c(Rank.Five, C), c(Rank.Three, S)];
    const result = evaluateHand(cards);
    expect(result.category).toBe(HandCategory.HighCard);
    expect(result.description).toBe('Ace-high');
  });
});

describe('evaluateHand — 7-card best-5 selection', () => {
  it('finds Royal Flush in 7 cards', () => {
    const cards = [
      c(Rank.Ace, S), c(Rank.King, S), c(Rank.Queen, S), c(Rank.Jack, S), c(Rank.Ten, S),
      c(Rank.Two, H), c(Rank.Three, D),
    ];
    const result = evaluateHand(cards);
    expect(result.category).toBe(HandCategory.RoyalFlush);
  });

  it('finds best Straight in 7 cards (higher straight over wheel)', () => {
    const cards = [
      c(Rank.Ace, S), c(Rank.Two, H), c(Rank.Three, D), c(Rank.Four, C), c(Rank.Five, S),
      c(Rank.Six, H), c(Rank.Seven, D),
    ];
    const result = evaluateHand(cards);
    expect(result.category).toBe(HandCategory.Straight);
    expect(result.description).toBe('Straight, Seven-high');
  });

  it('finds Flush over Straight when both exist in 7 cards', () => {
    const cards = [
      c(Rank.Ace, S), c(Rank.King, S), c(Rank.Queen, S), c(Rank.Jack, S), c(Rank.Nine, S),
      c(Rank.Ten, H), c(Rank.Two, D),
    ];
    const result = evaluateHand(cards);
    // A♠K♠Q♠J♠9♠ is a flush, beats the straight A-K-Q-J-T (mixed suits)
    expect(result.category).toBe(HandCategory.Flush);
  });

  it('board plays — both players split with same best 5', () => {
    const board = [c(Rank.Ace, S), c(Rank.King, S), c(Rank.Queen, S), c(Rank.Jack, S), c(Rank.Ten, C)];
    const playerA = evaluateHand([c(Rank.Two, H), c(Rank.Three, H), ...board]);
    const playerB = evaluateHand([c(Rank.Four, D), c(Rank.Five, D), ...board]);
    expect(compareHands(playerA, playerB)).toBe(0); // tie
  });
});

describe('compareHands — hand ranking', () => {
  it('Royal Flush > Straight Flush', () => {
    const royal = evaluateHand([c(Rank.Ace, S), c(Rank.King, S), c(Rank.Queen, S), c(Rank.Jack, S), c(Rank.Ten, S)]);
    const sf = evaluateHand([c(Rank.King, H), c(Rank.Queen, H), c(Rank.Jack, H), c(Rank.Ten, H), c(Rank.Nine, H)]);
    expect(compareHands(royal, sf)).toBeLessThan(0);
  });

  it('Flush > Straight', () => {
    const flush = evaluateHand([c(Rank.Ace, S), c(Rank.Jack, S), c(Rank.Eight, S), c(Rank.Five, S), c(Rank.Three, S)]);
    const straight = evaluateHand([c(Rank.Ace, S), c(Rank.King, H), c(Rank.Queen, D), c(Rank.Jack, C), c(Rank.Ten, S)]);
    expect(compareHands(flush, straight)).toBeLessThan(0);
  });

  it('Straight > Three of a Kind', () => {
    const straight = evaluateHand([c(Rank.Six, S), c(Rank.Seven, H), c(Rank.Eight, D), c(Rank.Nine, C), c(Rank.Ten, S)]);
    const trips = evaluateHand([c(Rank.Ace, S), c(Rank.Ace, H), c(Rank.Ace, D), c(Rank.King, C), c(Rank.Queen, S)]);
    expect(compareHands(straight, trips)).toBeLessThan(0);
  });

  it('Pair of Aces > Pair of Kings', () => {
    const aces = evaluateHand([c(Rank.Ace, S), c(Rank.Ace, H), c(Rank.King, D), c(Rank.Queen, C), c(Rank.Jack, S)]);
    const kings = evaluateHand([c(Rank.King, S), c(Rank.King, H), c(Rank.Ace, D), c(Rank.Queen, C), c(Rank.Jack, S)]);
    expect(compareHands(aces, kings)).toBeLessThan(0);
  });

  it('same Pair, better kicker wins', () => {
    const aceKicker = evaluateHand([c(Rank.King, S), c(Rank.King, H), c(Rank.Ace, D), c(Rank.Queen, C), c(Rank.Jack, S)]);
    const tenKicker = evaluateHand([c(Rank.King, D), c(Rank.King, C), c(Rank.Ten, S), c(Rank.Nine, H), c(Rank.Eight, D)]);
    expect(compareHands(aceKicker, tenKicker)).toBeLessThan(0);
  });

  it('Six-high straight > Wheel (Five-high straight)', () => {
    const sixHigh = evaluateHand([c(Rank.Six, S), c(Rank.Five, H), c(Rank.Four, D), c(Rank.Three, C), c(Rank.Two, S)]);
    const wheel = evaluateHand([c(Rank.Ace, S), c(Rank.Five, D), c(Rank.Four, C), c(Rank.Three, S), c(Rank.Two, H)]);
    expect(compareHands(sixHigh, wheel)).toBeLessThan(0);
  });

  it('same two pair, kicker breaks tie', () => {
    const aceKicker = evaluateHand([c(Rank.King, S), c(Rank.King, H), c(Rank.Seven, D), c(Rank.Seven, C), c(Rank.Ace, S)]);
    const queenKicker = evaluateHand([c(Rank.King, D), c(Rank.King, C), c(Rank.Seven, S), c(Rank.Seven, H), c(Rank.Queen, D)]);
    expect(compareHands(aceKicker, queenKicker)).toBeLessThan(0);
  });

  it('identical hands tie (rank === 0)', () => {
    const hand1 = evaluateHand([c(Rank.Ace, S), c(Rank.King, H), c(Rank.Queen, D), c(Rank.Jack, C), c(Rank.Nine, S)]);
    const hand2 = evaluateHand([c(Rank.Ace, H), c(Rank.King, D), c(Rank.Queen, C), c(Rank.Jack, S), c(Rank.Nine, H)]);
    expect(compareHands(hand1, hand2)).toBe(0);
  });

  it('Full House: higher trips wins regardless of pair', () => {
    const kingsFullOfTwos = evaluateHand([c(Rank.King, S), c(Rank.King, H), c(Rank.King, D), c(Rank.Two, C), c(Rank.Two, S)]);
    const queensFullOfAces = evaluateHand([c(Rank.Queen, S), c(Rank.Queen, H), c(Rank.Queen, D), c(Rank.Ace, C), c(Rank.Ace, S)]);
    expect(compareHands(kingsFullOfTwos, queensFullOfAces)).toBeLessThan(0);
  });
});

describe('evaluateHand — edge cases', () => {
  it('does NOT misclassify paired board as straight', () => {
    const cards = [c(Rank.Seven, S), c(Rank.Seven, H), c(Rank.Eight, D), c(Rank.Nine, C), c(Rank.Ten, S)];
    const result = evaluateHand(cards);
    expect(result.category).toBe(HandCategory.Pair);
  });

  it('ace-high straight is NOT a wheel', () => {
    const cards = [c(Rank.Ace, S), c(Rank.King, H), c(Rank.Queen, D), c(Rank.Jack, C), c(Rank.Ten, H)];
    const result = evaluateHand(cards);
    expect(result.category).toBe(HandCategory.Straight);
    expect(result.description).toBe('Straight, Ace-high');
  });

  it('flush beats straight (same 5 cards reordered)', () => {
    // All spades: A♠5♠4♠3♠2♠ — this is a straight flush, not just a flush
    const cards = [c(Rank.Ace, S), c(Rank.Five, S), c(Rank.Four, S), c(Rank.Three, S), c(Rank.Two, S)];
    const result = evaluateHand(cards);
    expect(result.category).toBe(HandCategory.StraightFlush);
  });

  it('throws for fewer than 5 cards', () => {
    expect(() => evaluateHand([c(Rank.Ace, S), c(Rank.King, H)])).toThrow();
  });
});
