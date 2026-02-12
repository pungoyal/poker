import { describe, it, expect } from 'vitest';
import { resolveShowdown, resolveByFold } from '../showdown';
import { calculatePots } from '../pot';
import { Player, Position } from '../../types/player';
import { Card, Rank, Suit } from '../../types/card';

function makePlayer(
  id: string,
  holeCards: [Card, Card] | null,
  totalBet: number,
  opts: { isFolded?: boolean; isAllIn?: boolean; stack?: number } = {}
): Player {
  return {
    id,
    name: id,
    stack: opts.stack ?? 1000,
    holeCards,
    currentBet: 0,
    totalBetThisHand: totalBet,
    isFolded: opts.isFolded ?? false,
    isAllIn: opts.isAllIn ?? false,
    isHuman: false,
    position: Position.Button,
    seatIndex: 0,
  };
}

function c(rank: Rank, suit: Suit): Card {
  return { rank, suit };
}

const S = Suit.Spades, H = Suit.Hearts, D = Suit.Diamonds, C = Suit.Clubs;

describe('calculatePots', () => {
  it('single pot when no all-ins', () => {
    const players = [
      makePlayer('A', null, 100),
      makePlayer('B', null, 100),
      makePlayer('C', null, 100),
    ];
    const pots = calculatePots(players);
    expect(pots).toHaveLength(1);
    expect(pots[0].amount).toBe(300);
    expect(pots[0].eligiblePlayerIds).toEqual(['A', 'B', 'C']);
  });

  it('side pot when one player all-in for less', () => {
    const players = [
      makePlayer('A', null, 100),
      makePlayer('B', null, 50, { isAllIn: true }),
      makePlayer('C', null, 100),
    ];
    const pots = calculatePots(players);
    expect(pots).toHaveLength(2);
    // Main pot: 50 * 3 = 150
    expect(pots[0].amount).toBe(150);
    expect(pots[0].eligiblePlayerIds).toContain('A');
    expect(pots[0].eligiblePlayerIds).toContain('B');
    expect(pots[0].eligiblePlayerIds).toContain('C');
    // Side pot: 50 * 2 = 100
    expect(pots[1].amount).toBe(100);
    expect(pots[1].eligiblePlayerIds).toContain('A');
    expect(pots[1].eligiblePlayerIds).toContain('C');
    expect(pots[1].eligiblePlayerIds).not.toContain('B');
  });

  it('money is conserved with side pots', () => {
    const players = [
      makePlayer('A', null, 200),
      makePlayer('B', null, 50, { isAllIn: true }),
      makePlayer('C', null, 100, { isAllIn: true }),
      makePlayer('D', null, 200),
    ];
    const pots = calculatePots(players);
    const total = pots.reduce((s, p) => s + p.amount, 0);
    expect(total).toBe(550); // 200+50+100+200
  });

  it('folded player money goes into pot but player not eligible', () => {
    const players = [
      makePlayer('A', null, 100),
      makePlayer('B', null, 50, { isFolded: true }),
      makePlayer('C', null, 100),
    ];
    const pots = calculatePots(players);
    expect(pots).toHaveLength(1);
    expect(pots[0].amount).toBe(250); // includes B's 50
    expect(pots[0].eligiblePlayerIds).not.toContain('B');
  });

  it('multiple all-in levels create correct side pots', () => {
    const players = [
      makePlayer('A', null, 30, { isAllIn: true }),
      makePlayer('B', null, 50, { isAllIn: true }),
      makePlayer('C', null, 100),
    ];
    const pots = calculatePots(players);
    expect(pots).toHaveLength(3);
    // Level 30: 30*3 = 90
    expect(pots[0].amount).toBe(90);
    expect(pots[0].eligiblePlayerIds).toEqual(expect.arrayContaining(['A', 'B', 'C']));
    // Level 50: 20*2 = 40
    expect(pots[1].amount).toBe(40);
    expect(pots[1].eligiblePlayerIds).toEqual(expect.arrayContaining(['B', 'C']));
    expect(pots[1].eligiblePlayerIds).not.toContain('A');
    // Level 100: 50*1 = 50
    expect(pots[2].amount).toBe(50);
    expect(pots[2].eligiblePlayerIds).toEqual(['C']);
    // Total conserved
    expect(pots.reduce((s, p) => s + p.amount, 0)).toBe(180);
  });

  it('no side pot when all-in matches max bet', () => {
    const players = [
      makePlayer('A', null, 100, { isAllIn: true }),
      makePlayer('B', null, 100),
    ];
    const pots = calculatePots(players);
    expect(pots).toHaveLength(1);
    expect(pots[0].amount).toBe(200);
  });
});

describe('resolveShowdown', () => {
  it('best hand wins the pot', () => {
    const community: Card[] = [
      c(Rank.Two, S), c(Rank.Three, D), c(Rank.Eight, C), c(Rank.Nine, H), c(Rank.Jack, S),
    ];
    const players = [
      makePlayer('A', [c(Rank.Ace, S), c(Rank.Ace, H)], 100), // Pair of Aces
      makePlayer('B', [c(Rank.King, D), c(Rank.King, C)], 100), // Pair of Kings
    ];
    const pots = [{ amount: 200, eligiblePlayerIds: ['A', 'B'] }];
    const result = resolveShowdown(players, community, pots);
    expect(result.winners).toHaveLength(1);
    expect(result.winners[0].playerId).toBe('A');
    expect(result.winners[0].amount).toBe(200);
  });

  it('ties split the pot', () => {
    const community: Card[] = [
      c(Rank.Ace, S), c(Rank.King, S), c(Rank.Queen, S), c(Rank.Jack, S), c(Rank.Ten, S),
    ];
    // Board is Royal Flush — both players play the board
    const players = [
      makePlayer('A', [c(Rank.Two, H), c(Rank.Three, H)], 100),
      makePlayer('B', [c(Rank.Four, D), c(Rank.Five, D)], 100),
    ];
    const pots = [{ amount: 200, eligiblePlayerIds: ['A', 'B'] }];
    const result = resolveShowdown(players, community, pots);
    expect(result.winners).toHaveLength(2);
    expect(result.winners[0].amount + result.winners[1].amount).toBe(200);
  });

  it('side pot goes to correct winner', () => {
    const community: Card[] = [
      c(Rank.Two, S), c(Rank.Seven, D), c(Rank.Eight, C), c(Rank.Nine, H), c(Rank.Jack, S),
    ];
    const players = [
      makePlayer('A', [c(Rank.Ace, S), c(Rank.Ace, H)], 50, { isAllIn: true }), // Aces — best hand
      makePlayer('B', [c(Rank.King, D), c(Rank.King, C)], 100), // Kings
      makePlayer('C', [c(Rank.Queen, H), c(Rank.Queen, D)], 100), // Queens
    ];
    const pots = calculatePots(players);
    const result = resolveShowdown(players, community, pots);

    // A wins main pot (best hand), B wins side pot (Kings > Queens)
    const aWin = result.winners.find(w => w.playerId === 'A');
    const bWin = result.winners.find(w => w.playerId === 'B');
    expect(aWin).toBeDefined();
    expect(bWin).toBeDefined();
    // Total distributed should equal total pot
    const totalWinnings = result.winners.reduce((s, w) => s + w.amount, 0);
    const totalPot = pots.reduce((s, p) => s + p.amount, 0);
    expect(totalWinnings).toBe(totalPot);
  });

  it('folded player does not win', () => {
    const community: Card[] = [
      c(Rank.Two, S), c(Rank.Three, D), c(Rank.Four, C), c(Rank.Five, H), c(Rank.Seven, S),
    ];
    const players = [
      makePlayer('A', [c(Rank.Ace, S), c(Rank.Ace, H)], 50, { isFolded: true }), // Aces but folded
      makePlayer('B', [c(Rank.Six, D), c(Rank.Eight, C)], 100), // Straight 4-8
    ];
    const pots = [{ amount: 150, eligiblePlayerIds: ['B'] }];
    const result = resolveShowdown(players, community, pots);
    expect(result.winners).toHaveLength(1);
    expect(result.winners[0].playerId).toBe('B');
    expect(result.winners[0].amount).toBe(150);
  });

  it('money conservation: total winnings === total pot', () => {
    const community: Card[] = [
      c(Rank.Ace, S), c(Rank.King, D), c(Rank.Queen, C), c(Rank.Jack, H), c(Rank.Nine, S),
    ];
    const players = [
      makePlayer('A', [c(Rank.Ten, S), c(Rank.Two, H)], 30, { isAllIn: true }), // Straight
      makePlayer('B', [c(Rank.Ace, H), c(Rank.Ace, D)], 80, { isAllIn: true }), // Three Aces
      makePlayer('C', [c(Rank.King, S), c(Rank.King, H)], 200), // Three Kings
      makePlayer('D', [c(Rank.Two, D), c(Rank.Three, C)], 50, { isFolded: true }),
    ];
    const pots = calculatePots(players);
    const totalPot = pots.reduce((s, p) => s + p.amount, 0);
    expect(totalPot).toBe(360); // 30+80+200+50

    const result = resolveShowdown(players, community, pots);
    const totalWinnings = result.winners.reduce((s, w) => s + w.amount, 0);
    expect(totalWinnings).toBe(totalPot);
  });
});

describe('resolveByFold', () => {
  it('sole survivor gets entire pot', () => {
    const players = [
      makePlayer('A', null, 100, { isFolded: true }),
      makePlayer('B', null, 100),
      makePlayer('C', null, 50, { isFolded: true }),
    ];
    const pots = [{ amount: 250, eligiblePlayerIds: ['B'] }];
    const result = resolveByFold(players, pots);
    expect(result.winners).toHaveLength(1);
    expect(result.winners[0].playerId).toBe('B');
    expect(result.winners[0].amount).toBe(250);
    expect(result.winners[0].hand).toBe('Everyone folded');
  });
});
