import { ActionType, GameState, PlayerAction } from '../types/game';

export interface AvailableActions {
  canFold: boolean;
  canCheck: boolean;
  canCall: boolean;
  callAmount: number;
  canBet: boolean;
  canRaise: boolean;
  minRaise: number;
  maxRaise: number; // All-in amount
}

/** Determine what actions are available for the active player */
export function getAvailableActions(state: GameState): AvailableActions {
  const player = state.players[state.activePlayerIndex];
  if (!player || player.isFolded || player.isAllIn) {
    return {
      canFold: false, canCheck: false, canCall: false, callAmount: 0,
      canBet: false, canRaise: false, minRaise: 0, maxRaise: 0,
    };
  }

  const toCall = state.currentBet - player.currentBet;

  return {
    canFold: toCall > 0,
    canCheck: toCall === 0,
    canCall: toCall > 0,
    callAmount: Math.min(toCall, player.stack),
    canBet: toCall === 0 && player.stack > 0,
    canRaise: toCall > 0 && player.stack > toCall,
    minRaise: Math.min(state.minRaise, player.stack),
    maxRaise: player.stack,
  };
}

/** Execute a player action and return updated game state */
export function executeAction(
  state: GameState,
  action: ActionType,
  amount: number = 0
): GameState {
  const newState = { ...state, players: state.players.map(p => ({ ...p })) };
  const player = newState.players[newState.activePlayerIndex];

  const playerAction: PlayerAction = {
    playerId: player.id,
    type: action,
    amount: 0,
    street: newState.street,
  };

  switch (action) {
    case ActionType.Fold:
      player.isFolded = true;
      break;

    case ActionType.Check:
      // No money moves
      break;

    case ActionType.Call: {
      const toCall = Math.min(newState.currentBet - player.currentBet, player.stack);
      player.stack -= toCall;
      player.currentBet += toCall;
      player.totalBetThisHand += toCall;
      playerAction.amount = toCall;
      if (player.stack === 0) player.isAllIn = true;
      break;
    }

    case ActionType.Bet: {
      const betAmount = Math.min(amount, player.stack);
      player.stack -= betAmount;
      player.currentBet += betAmount;
      player.totalBetThisHand += betAmount;
      newState.currentBet = player.currentBet;
      newState.minRaise = betAmount;
      playerAction.amount = betAmount;
      if (player.stack === 0) player.isAllIn = true;
      break;
    }

    case ActionType.Raise: {
      const toCall = newState.currentBet - player.currentBet;
      const totalAmount = Math.min(amount, player.stack);
      const raiseAbove = totalAmount - toCall;
      player.stack -= totalAmount;
      player.currentBet += totalAmount;
      player.totalBetThisHand += totalAmount;
      newState.currentBet = player.currentBet;
      newState.minRaise = Math.max(newState.minRaise, raiseAbove);
      playerAction.amount = totalAmount;
      if (player.stack === 0) player.isAllIn = true;
      break;
    }

    case ActionType.AllIn: {
      const allInAmount = player.stack;
      player.currentBet += allInAmount;
      player.totalBetThisHand += allInAmount;
      player.stack = 0;
      player.isAllIn = true;
      if (player.currentBet > newState.currentBet) {
        const raiseAbove = player.currentBet - newState.currentBet;
        newState.minRaise = Math.max(newState.minRaise, raiseAbove);
        newState.currentBet = player.currentBet;
      }
      playerAction.amount = allInAmount;
      break;
    }
  }

  newState.actions = [...state.actions, playerAction];
  return newState;
}

/** Count active (not folded, not all-in) players */
export function countActivePlayers(state: GameState): number {
  return state.players.filter(p => !p.isFolded && !p.isAllIn).length;
}

/** Count non-folded players */
export function countNonFoldedPlayers(state: GameState): number {
  return state.players.filter(p => !p.isFolded).length;
}

/** Get the next active player index (wraps around) */
export function nextActivePlayerIndex(state: GameState, fromIndex: number): number {
  const n = state.players.length;
  let idx = (fromIndex + 1) % n;
  let attempts = 0;
  while (attempts < n) {
    const player = state.players[idx];
    if (!player.isFolded && !player.isAllIn) {
      return idx;
    }
    idx = (idx + 1) % n;
    attempts++;
  }
  return -1; // No active players
}
