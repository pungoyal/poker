import { GameState, GameSettings, Street, ActionType, DEFAULT_SETTINGS } from '../types/game';
import { Player, Position, AIPersonalityType, createPlayer } from '../types/player';
import { createShuffledDeck } from './deck';
import { calculatePots } from './pot';
import { resolveShowdown, resolveByFold } from './showdown';

const AI_NAMES = ['Doyle', 'Dwan', 'Phil', 'Gus', 'Patrik', 'Vanessa', 'Daniel', 'Maria'];

const ALL_PERSONALITIES: AIPersonalityType[] = [
  AIPersonalityType.TAG,
  AIPersonalityType.LAG,
  AIPersonalityType.Nit,
  AIPersonalityType.Maniac,
  AIPersonalityType.CallingStation,
];

/** Randomly assign personalities to AI players (shuffled each game init) */
function randomPersonality(): AIPersonalityType {
  return ALL_PERSONALITIES[Math.floor(Math.random() * ALL_PERSONALITIES.length)];
}

/** Find the next non-busted seat index (wraps around) */
function nextNonBustedIndex(players: Player[], fromIndex: number): number {
  const n = players.length;
  let idx = fromIndex % n;
  for (let i = 0; i < n; i++) {
    if (players[idx].stack > 0 || !players[idx].isFolded) return idx;
    idx = (idx + 1) % n;
  }
  return fromIndex % n; // fallback
}

/** Get all active (non-busted) player indices in seat order starting from dealer */
function getActiveSeatIndices(players: Player[], dealerIndex: number): number[] {
  const n = players.length;
  const indices: number[] = [];
  for (let i = 0; i < n; i++) {
    const idx = (dealerIndex + i) % n;
    if (!players[idx].isFolded) {
      indices.push(idx);
    }
  }
  return indices;
}

/** Assign positions based on dealer button index, skipping busted players */
function assignPositions(players: Player[], dealerIndex: number): void {
  const activeIndices = getActiveSeatIndices(players, dealerIndex);
  const activeCount = activeIndices.length;

  // Position arrays for different active player counts
  const positions9: Position[] = [
    Position.Button, Position.SmallBlind, Position.BigBlind,
    Position.UTG, Position.UTG1, Position.MP, Position.LJ, Position.HJ, Position.CO,
  ];
  const positions6: Position[] = [
    Position.Button, Position.SmallBlind, Position.BigBlind,
    Position.UTG, Position.MP, Position.CO,
  ];

  const posArray = activeCount >= 8 ? positions9 : positions6;

  // Reset all positions first
  for (const p of players) {
    p.position = Position.MP;
  }

  // Assign positions only to active players in seat order from dealer
  for (let i = 0; i < activeIndices.length; i++) {
    players[activeIndices[i]].position = posArray[i] || Position.MP;
  }
}

/** Create initial game state with all players */
export function createInitialGameState(settings: GameSettings = DEFAULT_SETTINGS): GameState {
  const players: Player[] = [];

  // Human player at seat 0
  players.push(createPlayer('hero', 'You', settings.startingStack, 0, true));

  // AI opponents with randomized personalities
  for (let i = 0; i < settings.numOpponents; i++) {
    const name = AI_NAMES[i] || `Player ${i + 2}`;
    players.push(createPlayer(
      `ai-${i}`,
      name,
      settings.startingStack,
      i + 1,
      false,
      randomPersonality()
    ));
  }

  return {
    handNumber: 0,
    street: Street.Preflop,
    deck: [],
    communityCards: [],
    pots: [],
    currentBet: 0,
    minRaise: settings.bigBlind,
    players,
    dealerIndex: 0,
    activePlayerIndex: 0,
    actions: [],
    isHandComplete: true,
    winners: [],
  };
}

/** Start a new hand: shuffle, deal, post blinds */
export function startNewHand(state: GameState, settings: GameSettings): GameState {
  const preparedPlayers = state.players.map(p => {
    if (!p.isHuman && settings.gameMode === 'cash' && p.stack <= 0) {
      return {
        ...p,
        stack: settings.startingStack,
        isFolded: false,
        isAllIn: false,
      };
    }
    return p;
  });

  // Pre-compute busted status from current stacks before resetting
  const bustedIds = new Set(preparedPlayers.filter(p => p.stack <= 0).map(p => p.id));

  const newState: GameState = {
    ...state,
    handNumber: state.handNumber + 1,
    street: Street.Preflop,
    deck: createShuffledDeck(),
    communityCards: [],
    pots: [],
    currentBet: settings.bigBlind,
    minRaise: settings.bigBlind,
    dealerIndex: 0, // will be set below
    activePlayerIndex: 0,
    actions: [],
    isHandComplete: false,
    winners: [],
    players: preparedPlayers.map(p => ({
      ...p,
      holeCards: null,
      currentBet: 0,
      totalBetThisHand: 0,
      isFolded: bustedIds.has(p.id), // Busted players are auto-folded
      isAllIn: false,
    })),
  };

  // Advance dealer button to the next non-busted player
  const rawDealer = (state.dealerIndex + 1) % newState.players.length;
  newState.dealerIndex = nextNonBustedIndex(newState.players, rawDealer);

  // Assign positions (skips busted players)
  assignPositions(newState.players, newState.dealerIndex);

  // Post blinds — skip busted players
  const n = newState.players.length;
  const activePlayers = newState.players.filter(p => !p.isFolded);

  // Tournament ante posting (before blinds)
  if (settings.gameMode === 'tournament' && settings.anteEnabled && settings.anteBb > 0) {
    const anteAmountRaw = Math.max(1, Math.round(settings.bigBlind * settings.anteBb));
    for (const p of activePlayers) {
      const ante = Math.min(anteAmountRaw, p.stack);
      if (ante <= 0) continue;
      p.stack -= ante;
      p.currentBet += ante;
      p.totalBetThisHand += ante;
      if (p.stack === 0) p.isAllIn = true;
      newState.actions.push({
        playerId: p.id,
        type: ActionType.PostBlind,
        amount: ante,
        street: Street.Preflop,
      });
    }
  }

  const sbIndex = nextNonBustedIndex(newState.players, (newState.dealerIndex + 1) % n);
  let bbIndex = nextNonBustedIndex(newState.players, (sbIndex + 1) % n);
  // Ensure BB is not the same as SB (could happen with only 2 active players)
  if (bbIndex === sbIndex) {
    bbIndex = nextNonBustedIndex(newState.players, (sbIndex + 1) % n);
  }

  const sb = newState.players[sbIndex];
  const sbAmount = Math.min(settings.smallBlind, sb.stack);
  sb.stack -= sbAmount;
  sb.currentBet = sbAmount;
  sb.totalBetThisHand = sbAmount;
  if (sb.stack === 0) sb.isAllIn = true;

  const bb = newState.players[bbIndex];
  const bbAmount = Math.min(settings.bigBlind, bb.stack);
  bb.stack -= bbAmount;
  bb.currentBet = bbAmount;
  bb.totalBetThisHand = bbAmount;
  if (bb.stack === 0) bb.isAllIn = true;

  newState.actions = [
    ...newState.actions,
    { playerId: sb.id, type: ActionType.PostBlind, amount: sbAmount, street: Street.Preflop },
    { playerId: bb.id, type: ActionType.PostBlind, amount: bbAmount, street: Street.Preflop },
  ];

  // Deal hole cards
  let deckIdx = 0;
  for (const player of newState.players) {
    if (!player.isFolded) {
      player.holeCards = [newState.deck[deckIdx], newState.deck[deckIdx + 1]];
      deckIdx += 2;
    }
  }
  newState.deck = newState.deck.slice(deckIdx);

  // Calculate initial pots
  newState.pots = calculatePots(newState.players);

  // First to act preflop: UTG (player after BB)
  const utgIndex = (bbIndex + 1) % n;
  newState.activePlayerIndex = findNextActiveFromIndex(newState, utgIndex);

  return newState;
}

function findNextActiveFromIndex(state: GameState, startIndex: number): number {
  const n = state.players.length;
  let idx = startIndex;
  for (let i = 0; i < n; i++) {
    const player = state.players[idx];
    if (!player.isFolded && !player.isAllIn) {
      return idx;
    }
    idx = (idx + 1) % n;
  }
  return -1;
}

/** Advance to the next street (deal community cards, reset bets) */
export function advanceStreet(state: GameState, bigBlind: number = 10): GameState {
  const newState: GameState = {
    ...state,
    players: state.players.map(p => ({ ...p, currentBet: 0 })),
    currentBet: 0,
    minRaise: bigBlind,
  };

  let newDeck = [...newState.deck];

  switch (newState.street) {
    case Street.Preflop:
      // Deal flop (burn 1, deal 3)
      newDeck = newDeck.slice(1); // burn
      newState.communityCards = [...newState.communityCards, newDeck[0], newDeck[1], newDeck[2]];
      newDeck = newDeck.slice(3);
      newState.street = Street.Flop;
      break;

    case Street.Flop:
      // Deal turn (burn 1, deal 1)
      newDeck = newDeck.slice(1);
      newState.communityCards = [...newState.communityCards, newDeck[0]];
      newDeck = newDeck.slice(1);
      newState.street = Street.Turn;
      break;

    case Street.Turn:
      // Deal river (burn 1, deal 1)
      newDeck = newDeck.slice(1);
      newState.communityCards = [...newState.communityCards, newDeck[0]];
      newDeck = newDeck.slice(1);
      newState.street = Street.River;
      break;

    case Street.River:
      newState.street = Street.Showdown;
      break;
  }

  newState.deck = newDeck;
  newState.pots = calculatePots(newState.players);

  // First to act postflop: first active player after dealer
  if (newState.street !== Street.Showdown) {
    const firstActive = findNextActiveFromIndex(newState, (newState.dealerIndex + 1) % newState.players.length);
    newState.activePlayerIndex = firstActive;
  }

  return newState;
}

/** Check if the current betting round is complete */
export function isBettingRoundComplete(state: GameState): boolean {
  const nonFolded = state.players.filter(p => !p.isFolded);

  // Only one player left
  if (nonFolded.length <= 1) return true;

  // All non-folded players are all-in
  const active = nonFolded.filter(p => !p.isAllIn);
  if (active.length === 0) return true;

  // Only one active player and they've matched the bet (or bet > 0 posted)
  if (active.length === 1) {
    const player = active[0];
    // If no one has bet (all checked), need at least one action from this player on this street
    const streetActions = state.actions.filter(a =>
      a.street === state.street && a.playerId === player.id && a.type !== ActionType.PostBlind
    );
    if (player.currentBet >= state.currentBet && streetActions.length > 0) return true;
    // Still waiting for this player's action
    if (streetActions.length === 0) return false;
    return player.currentBet >= state.currentBet;
  }

  // All active players must have acted this street and matched the current bet
  for (const player of active) {
    const streetActions = state.actions.filter(a =>
      a.street === state.street && a.playerId === player.id && a.type !== ActionType.PostBlind
    );
    if (streetActions.length === 0) return false;
    if (player.currentBet < state.currentBet) return false;
  }

  return true;
}

/** Complete the hand: handle showdown or fold-win */
export function completeHand(state: GameState): GameState {
  const newState = { ...state, players: state.players.map(p => ({ ...p })) };
  newState.pots = calculatePots(newState.players);

  const nonFolded = newState.players.filter(p => !p.isFolded);

  let result;
  if (nonFolded.length === 1) {
    result = resolveByFold(newState.players, newState.pots);
  } else {
    // Need to deal remaining community cards if hand ended early (all-in)
    let deck = [...newState.deck];
    while (newState.communityCards.length < 5) {
      deck = deck.slice(1); // burn
      newState.communityCards = [...newState.communityCards, deck[0]];
      deck = deck.slice(1);
    }
    newState.deck = deck;
    newState.street = Street.Showdown;
    result = resolveShowdown(newState.players, newState.communityCards, newState.pots);
  }

  // Award winnings
  for (const winner of result.winners) {
    const player = newState.players.find(p => p.id === winner.playerId);
    if (player) {
      player.stack += winner.amount;
    }
  }

  newState.winners = result.winners;
  newState.isHandComplete = true;

  return newState;
}
