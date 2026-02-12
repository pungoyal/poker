import { create } from 'zustand';
import { GameState, GameSettings, DEFAULT_SETTINGS, ActionType, Street } from '../types/game';
import { CommentaryEntry } from '../types/commentary';
import { MathAnalysis } from '../types/odds';
import { SessionStats, DEFAULT_SESSION_STATS, OpponentTendency } from '../types/stats';
import { HandHistoryEntry } from '../types/history';
import { createInitialGameState, startNewHand, advanceStreet, completeHand, isBettingRoundComplete } from '../engine/dealer';
import { executeAction, nextActivePlayerIndex, countNonFoldedPlayers, getAvailableActions } from '../engine/actions';
import { calculatePots } from '../engine/pot';
import { generateHeroActionCommentary } from '../commentary/generator';

const MAX_COMMENTARY_ENTRIES = 200;
const PREFS_STORAGE_KEY = 'poker-lab-prefs';

interface StoredPrefs {
  settings: GameSettings;
  showCommentaryPanel: boolean;
  autoDeal: boolean;
  soundEnabled: boolean;
}

function loadPrefs(): Partial<StoredPrefs> {
  if (typeof window === 'undefined') return {};
  try {
    const raw = window.localStorage.getItem(PREFS_STORAGE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as Partial<StoredPrefs>;
    return parsed;
  } catch {
    return {};
  }
}

function persistPrefs(state: Pick<GameStore, 'settings' | 'showCommentaryPanel' | 'autoDeal' | 'soundEnabled'>): void {
  if (typeof window === 'undefined') return;
  const payload: StoredPrefs = {
    settings: state.settings,
    showCommentaryPanel: state.showCommentaryPanel,
    autoDeal: state.autoDeal,
    soundEnabled: state.soundEnabled,
  };
  window.localStorage.setItem(PREFS_STORAGE_KEY, JSON.stringify(payload));
}

const prefs = loadPrefs();
const initialSettings = { ...DEFAULT_SETTINGS, ...(prefs.settings ?? {}) };
const initialGame = createInitialGameState(initialSettings);

function emptyOpponentStats(game: GameState): Record<string, OpponentTendency> {
  const stats: Record<string, OpponentTendency> = {};
  for (const p of game.players) {
    if (p.isHuman) continue;
    stats[p.id] = {
      hands: 0,
      vpipHands: 0,
      pfrHands: 0,
      aggressiveActions: 0,
      passiveActions: 0,
    };
  }
  return stats;
}

interface GameStore {
  // Game state
  game: GameState;
  settings: GameSettings;

  // UI state
  showCommentaryPanel: boolean;
  isAIThinking: boolean;
  autoDeal: boolean;
  isPaused: boolean;
  soundEnabled: boolean;
  decisionClockSec: number;
  timeBankSec: number;
  queuedAction: 'none' | 'check' | 'check-fold' | 'call';
  rebuyCount: number;
  sessionStats: SessionStats;
  currentHandVPIP: boolean;
  currentHandPFR: boolean;
  heroStackAtHandStart: number;
  handHistory: HandHistoryEntry[];
  opponentStats: Record<string, OpponentTendency>;
  currentHandVpipByPlayer: Record<string, boolean>;
  currentHandPfrByPlayer: Record<string, boolean>;
  coachingScore: number;
  goodDecisions: number;
  majorMistakes: number;
  lastDecisionDeltaEv: number | null;
  currentHandGoodDecisions: number;
  currentHandMistakes: number;
  currentHandWorstDeltaEv: number;
  currentHandWorstSpot: string;

  // Commentary
  commentary: CommentaryEntry[];

  // Math analysis
  mathAnalysis: MathAnalysis | null;

  // Actions
  initGame: (settings?: Partial<GameSettings>) => void;
  dealNewHand: () => void;
  playerAction: (action: ActionType, amount?: number) => void;
  advanceGame: () => void;
  setAIThinking: (thinking: boolean) => void;
  toggleCommentaryPanel: () => void;
  toggleAutoDeal: () => void;
  togglePause: () => void;
  toggleSound: () => void;
  setDecisionClockSec: (seconds: number) => void;
  setTimeBankSec: (seconds: number) => void;
  setQueuedAction: (action: 'none' | 'check' | 'check-fold' | 'call') => void;
  rebuyHero: () => void;
  addCommentary: (entry: CommentaryEntry) => void;
  clearCommentary: () => void;
  resetSession: () => void;
  setMathAnalysis: (analysis: MathAnalysis | null) => void;
  updateSettings: (settings: Partial<GameSettings>) => void;
}

export const useGameStore = create<GameStore>((set, get) => ({
  game: initialGame,
  settings: initialSettings,

  showCommentaryPanel: prefs.showCommentaryPanel ?? true,
  isAIThinking: false,
  autoDeal: prefs.autoDeal ?? false,
  isPaused: false,
  soundEnabled: prefs.soundEnabled ?? true,
  decisionClockSec: 12,
  timeBankSec: 30,
  queuedAction: 'none',
  rebuyCount: 0,
  sessionStats: DEFAULT_SESSION_STATS,
  currentHandVPIP: false,
  currentHandPFR: false,
  heroStackAtHandStart: initialSettings.startingStack,
  handHistory: [],
  opponentStats: emptyOpponentStats(initialGame),
  currentHandVpipByPlayer: {},
  currentHandPfrByPlayer: {},
  coachingScore: 100,
  goodDecisions: 0,
  majorMistakes: 0,
  lastDecisionDeltaEv: null,
  currentHandGoodDecisions: 0,
  currentHandMistakes: 0,
  currentHandWorstDeltaEv: 0,
  currentHandWorstSpot: '',

  commentary: [],
  mathAnalysis: null,

  initGame: (partialSettings) => {
    const settings = { ...DEFAULT_SETTINGS, ...partialSettings };
    const game = createInitialGameState(settings);
    const nextState = {
      game,
      settings,
      commentary: [],
      mathAnalysis: null,
      sessionStats: DEFAULT_SESSION_STATS,
      currentHandVPIP: false,
      currentHandPFR: false,
      heroStackAtHandStart: settings.startingStack,
      rebuyCount: 0,
      handHistory: [],
      decisionClockSec: 12,
      timeBankSec: 30,
      queuedAction: 'none' as const,
      opponentStats: emptyOpponentStats(game),
      currentHandVpipByPlayer: {},
      currentHandPfrByPlayer: {},
      coachingScore: 100,
      goodDecisions: 0,
      majorMistakes: 0,
      lastDecisionDeltaEv: null,
      currentHandGoodDecisions: 0,
      currentHandMistakes: 0,
      currentHandWorstDeltaEv: 0,
      currentHandWorstSpot: '',
    };
    set(nextState);
    persistPrefs({ ...get(), ...nextState });
  },

  dealNewHand: () => {
    const { game, settings } = get();
    let nextSettings = settings;
    if (
      settings.gameMode === 'tournament' &&
      game.handNumber > 0 &&
      game.handNumber % settings.blindIncreaseEveryHands === 0
    ) {
      const factor = settings.blindIncreaseFactor;
      nextSettings = {
        ...settings,
        smallBlind: Math.max(1, Math.round(settings.smallBlind * factor)),
        bigBlind: Math.max(2, Math.round(settings.bigBlind * factor)),
      };
      persistPrefs({ ...get(), settings: nextSettings });
      set({ settings: nextSettings });
    }
    const heroBeforeHand = game.players.find(p => p.isHuman)?.stack ?? settings.startingStack;
    const newGame = startNewHand(game, nextSettings);
    const currentHandVpipByPlayer: Record<string, boolean> = {};
    const currentHandPfrByPlayer: Record<string, boolean> = {};
    for (const p of newGame.players) {
      if (!p.isHuman && p.holeCards) {
        currentHandVpipByPlayer[p.id] = false;
        currentHandPfrByPlayer[p.id] = false;
      }
    }
    set({
      game: newGame,
      mathAnalysis: null,
      currentHandVPIP: false,
      currentHandPFR: false,
      heroStackAtHandStart: heroBeforeHand,
      decisionClockSec: 12,
      timeBankSec: 30,
      queuedAction: 'none',
      currentHandVpipByPlayer,
      currentHandPfrByPlayer,
      currentHandGoodDecisions: 0,
      currentHandMistakes: 0,
      currentHandWorstDeltaEv: 0,
      currentHandWorstSpot: '',
    });
  },

  playerAction: (action, amount = 0) => {
    const {
      game,
      commentary,
      currentHandVPIP,
      currentHandPFR,
      currentHandVpipByPlayer,
      currentHandPfrByPlayer,
      opponentStats,
      settings,
      mathAnalysis,
      coachingScore,
      goodDecisions,
      majorMistakes,
    } = get();
    if (game.isHandComplete) return;
    const available = getAvailableActions(game);
    const activePlayer = game.players[game.activePlayerIndex];
    if (!activePlayer) return;

    const clamp = (v: number, min: number, max: number) => Math.max(min, Math.min(v, max));
    let normalizedAction = action;
    let normalizedAmount = amount;

    if (action === ActionType.Fold && !available.canFold) {
      if (available.canCheck) {
        normalizedAction = ActionType.Check;
        normalizedAmount = 0;
      } else {
        return;
      }
    }

    if (action === ActionType.Check && !available.canCheck) {
      if (available.canCall) {
        normalizedAction = ActionType.Call;
        normalizedAmount = available.callAmount;
      } else {
        return;
      }
    }

    if (action === ActionType.Call) {
      if (!available.canCall) {
        if (available.canCheck) {
          normalizedAction = ActionType.Check;
          normalizedAmount = 0;
        } else {
          return;
        }
      } else {
        normalizedAmount = available.callAmount;
      }
    }

    if (action === ActionType.Bet) {
      if (!available.canBet) {
        return;
      }
      const minBet = Math.max(available.minRaise, 1);
      normalizedAmount = clamp(amount || minBet, minBet, available.maxRaise);
    }

    if (action === ActionType.Raise) {
      if (!available.canRaise) {
        return;
      }
      const minTotal = available.callAmount + Math.max(available.minRaise, 1);
      normalizedAmount = clamp(amount || minTotal, minTotal, available.maxRaise);
    }

    if (action === ActionType.AllIn && activePlayer.stack <= 0) {
      return;
    }

    // Generate commentary for hero's actions
    if (activePlayer?.isHuman) {
      const entry = generateHeroActionCommentary(normalizedAction, normalizedAmount, game.street);
      const updated = [...commentary, entry];
      set({ commentary: updated.length > MAX_COMMENTARY_ENTRIES ? updated.slice(-MAX_COMMENTARY_ENTRIES) : updated });
    }

    const newGame = executeAction(game, normalizedAction, normalizedAmount);
    newGame.pots = calculatePots(newGame.players);

    // Move to next player
    const nextIdx = nextActivePlayerIndex(newGame, newGame.activePlayerIndex);
    newGame.activePlayerIndex = nextIdx;

    let nextVPIP = currentHandVPIP;
    let nextPFR = currentHandPFR;
    const nextCurrentHandVpipByPlayer = { ...currentHandVpipByPlayer };
    const nextCurrentHandPfrByPlayer = { ...currentHandPfrByPlayer };
    const nextOpponentStats = { ...opponentStats };
    if (activePlayer?.isHuman && game.street === Street.Preflop) {
      const putsMoneyIn =
        normalizedAction === ActionType.AllIn ||
        ((normalizedAction === ActionType.Call || normalizedAction === ActionType.Bet || normalizedAction === ActionType.Raise) && normalizedAmount > 0);
      if (!nextVPIP && putsMoneyIn) {
        nextVPIP = true;
      }
      const isRaiseAction =
        normalizedAction === ActionType.AllIn ||
        ((normalizedAction === ActionType.Bet || normalizedAction === ActionType.Raise) && normalizedAmount > 0);
      if (!nextPFR && isRaiseAction) {
        nextPFR = true;
      }
    }

    if (!activePlayer.isHuman) {
      if (game.street === Street.Preflop) {
        const aiPutsMoneyIn =
          normalizedAction === ActionType.AllIn ||
          ((normalizedAction === ActionType.Call || normalizedAction === ActionType.Bet || normalizedAction === ActionType.Raise) && normalizedAmount > 0);
        if (aiPutsMoneyIn) {
          nextCurrentHandVpipByPlayer[activePlayer.id] = true;
        }
        const aiRaiseAction =
          normalizedAction === ActionType.AllIn ||
          ((normalizedAction === ActionType.Bet || normalizedAction === ActionType.Raise) && normalizedAmount > 0);
        if (aiRaiseAction) {
          nextCurrentHandPfrByPlayer[activePlayer.id] = true;
        }
      }

      const prev = nextOpponentStats[activePlayer.id] ?? {
        hands: 0,
        vpipHands: 0,
        pfrHands: 0,
        aggressiveActions: 0,
        passiveActions: 0,
      };
      if (normalizedAction === ActionType.Bet || normalizedAction === ActionType.Raise || normalizedAction === ActionType.AllIn) {
        prev.aggressiveActions += 1;
      } else if (normalizedAction === ActionType.Call || normalizedAction === ActionType.Check) {
        prev.passiveActions += 1;
      }
      nextOpponentStats[activePlayer.id] = prev;
    }

    const updates: Partial<GameStore> = {
      game: newGame,
      currentHandVPIP: nextVPIP,
      currentHandPFR: nextPFR,
      currentHandVpipByPlayer: nextCurrentHandVpipByPlayer,
      currentHandPfrByPlayer: nextCurrentHandPfrByPlayer,
      opponentStats: nextOpponentStats,
    };

    if (activePlayer.isHuman && mathAnalysis?.ev) {
      const ev = mathAnalysis.ev;
      const bestEV = Math.max(ev.foldEV, ev.callEV, ev.raiseEV);
      const chosenEV =
        normalizedAction === ActionType.Fold ? ev.foldEV :
        (normalizedAction === ActionType.Call || normalizedAction === ActionType.Check) ? ev.callEV :
        (normalizedAction === ActionType.Bet || normalizedAction === ActionType.Raise || normalizedAction === ActionType.AllIn) ? ev.raiseEV :
        ev.callEV;
      const delta = bestEV - chosenEV;
      const bbLoss = settings.bigBlind > 0 ? delta / settings.bigBlind : 0;
      let nextScore = coachingScore;
      let nextGood = goodDecisions;
      let nextMistakes = majorMistakes;
      const currentHandGood = get().currentHandGoodDecisions;
      const currentHandMistakes = get().currentHandMistakes;
      const currentHandWorstDeltaEv = get().currentHandWorstDeltaEv;
      const currentHandWorstSpot = get().currentHandWorstSpot;
      if (bbLoss <= 0.5) {
        nextScore = Math.min(100, coachingScore + 0.4);
        nextGood += 1;
        updates.currentHandGoodDecisions = currentHandGood + 1;
      } else if (bbLoss >= 3) {
        nextScore = Math.max(0, coachingScore - Math.min(8, bbLoss));
        nextMistakes += 1;
        updates.currentHandMistakes = currentHandMistakes + 1;
      } else {
        nextScore = Math.max(0, coachingScore - 0.8);
      }
      if (delta > currentHandWorstDeltaEv) {
        const contextText = mathAnalysis?.context?.handNotation ? ` with ${mathAnalysis.context.handNotation}` : '';
        updates.currentHandWorstDeltaEv = Number(delta.toFixed(2));
        updates.currentHandWorstSpot = `${game.street.toUpperCase()} ${normalizedAction.toUpperCase()}${contextText}`;
      } else {
        updates.currentHandWorstDeltaEv = currentHandWorstDeltaEv;
        updates.currentHandWorstSpot = currentHandWorstSpot;
      }
      updates.coachingScore = Number(nextScore.toFixed(1));
      updates.goodDecisions = nextGood;
      updates.majorMistakes = nextMistakes;
      updates.lastDecisionDeltaEv = Number(delta.toFixed(2));
    }

    set(updates);
    if (activePlayer.isHuman) {
      set({ queuedAction: 'none' });
    }
  },

  advanceGame: () => {
    const {
      game,
      sessionStats,
      currentHandVPIP,
      currentHandPFR,
      heroStackAtHandStart,
      handHistory,
      opponentStats,
      currentHandVpipByPlayer,
      currentHandPfrByPlayer,
      currentHandGoodDecisions,
      currentHandMistakes,
      currentHandWorstDeltaEv,
      currentHandWorstSpot,
    } = get();
    const finalizeHand = (completed: GameState) => {
      const heroAfter = completed.players.find(p => p.isHuman)?.stack ?? 0;
      const hero = completed.players.find(p => p.isHuman);
      const heroWon = completed.winners.some(w => completed.players.find(p => p.id === w.playerId)?.isHuman);
      const showdownSeen = completed.street === Street.Showdown;

      const nextStats: SessionStats = {
        handsPlayed: sessionStats.handsPlayed + 1,
        handsWon: sessionStats.handsWon + (heroWon ? 1 : 0),
        showdownsSeen: sessionStats.showdownsSeen + (showdownSeen ? 1 : 0),
        showdownsWon: sessionStats.showdownsWon + (showdownSeen && heroWon ? 1 : 0),
        vpipHands: sessionStats.vpipHands + (currentHandVPIP ? 1 : 0),
        pfrHands: sessionStats.pfrHands + (currentHandPFR ? 1 : 0),
        netChips: sessionStats.netChips + (heroAfter - heroStackAtHandStart),
      };
      const pot = completed.pots.reduce((sum, p) => sum + p.amount, 0);
      const handEntry: HandHistoryEntry = {
        handNumber: completed.handNumber,
        endedAt: Date.now(),
        heroCards: hero?.holeCards ?? null,
        board: completed.communityCards,
        pot,
        playerNames: completed.players.reduce<Record<string, string>>((acc, p) => {
          acc[p.id] = p.name;
          return acc;
        }, {}),
        winners: completed.winners.map(w => {
          const winnerPlayer = completed.players.find(p => p.id === w.playerId);
          return {
            playerId: w.playerId,
            playerName: winnerPlayer?.name ?? w.playerId,
            amount: w.amount,
            hand: w.hand,
          };
        }),
        heroNetChips: heroAfter - heroStackAtHandStart,
        actions: completed.actions,
        coach: {
          goodDecisions: currentHandGoodDecisions,
          mistakes: currentHandMistakes,
          worstDeltaEv: Number(currentHandWorstDeltaEv.toFixed(2)),
          worstSpot: currentHandWorstSpot || undefined,
        },
      };
      const nextOpponentStats = { ...opponentStats };
      for (const p of completed.players) {
        if (p.isHuman) continue;
        if (!p.holeCards && p.stack <= 0) continue;
        const prev = nextOpponentStats[p.id] ?? {
          hands: 0,
          vpipHands: 0,
          pfrHands: 0,
          aggressiveActions: 0,
          passiveActions: 0,
        };
        prev.hands += 1;
        if (currentHandVpipByPlayer[p.id]) prev.vpipHands += 1;
        if (currentHandPfrByPlayer[p.id]) prev.pfrHands += 1;
        nextOpponentStats[p.id] = prev;
      }

      set({
        game: completed,
        sessionStats: nextStats,
        currentHandVPIP: false,
        currentHandPFR: false,
        currentHandVpipByPlayer: {},
        currentHandPfrByPlayer: {},
        currentHandGoodDecisions: 0,
        currentHandMistakes: 0,
        currentHandWorstDeltaEv: 0,
        currentHandWorstSpot: '',
        heroStackAtHandStart: heroAfter,
        handHistory: [...handHistory, handEntry],
        opponentStats: nextOpponentStats,
      });
    };

    // Check if only one player remains
    if (countNonFoldedPlayers(game) <= 1) {
      finalizeHand(completeHand(game));
      return;
    }

    // Check if betting round is complete
    if (isBettingRoundComplete(game)) {
      // Check if all active players are all-in or only one active
      const active = game.players.filter(p => !p.isFolded && !p.isAllIn);
      if (active.length <= 1) {
        // Run out remaining community cards and go to showdown
        finalizeHand(completeHand(game));
        return;
      }

      if (game.street === Street.River) {
        finalizeHand(completeHand(game));
      } else {
        const { settings } = get();
        set({ game: advanceStreet(game, settings.bigBlind) });
      }
    }
  },

  setAIThinking: (thinking) => set({ isAIThinking: thinking }),
  toggleCommentaryPanel: () => set(s => {
    const next = { showCommentaryPanel: !s.showCommentaryPanel };
    persistPrefs({ ...s, ...next });
    return next;
  }),
  toggleAutoDeal: () => set(s => {
    const next = { autoDeal: !s.autoDeal };
    persistPrefs({ ...s, ...next });
    return next;
  }),
  togglePause: () => set(s => ({ isPaused: !s.isPaused })),
  toggleSound: () => set(s => {
    const next = { soundEnabled: !s.soundEnabled };
    persistPrefs({ ...s, ...next });
    return next;
  }),
  setDecisionClockSec: (seconds) => set({ decisionClockSec: Math.max(0, Math.floor(seconds)) }),
  setTimeBankSec: (seconds) => set({ timeBankSec: Math.max(0, Math.floor(seconds)) }),
  setQueuedAction: (action) => set({ queuedAction: action }),
  rebuyHero: () => {
    const { game, settings, rebuyCount } = get();
    const newPlayers = game.players.map(p =>
      p.isHuman
        ? { ...p, stack: settings.startingStack, isFolded: false, isAllIn: false }
        : { ...p }
    );
    set({
      game: { ...game, players: newPlayers },
      rebuyCount: rebuyCount + 1,
    });
  },
  addCommentary: (entry) => set(s => {
    const updated = [...s.commentary, entry];
    return { commentary: updated.length > MAX_COMMENTARY_ENTRIES ? updated.slice(-MAX_COMMENTARY_ENTRIES) : updated };
  }),
  clearCommentary: () => set({ commentary: [] }),
  resetSession: () => {
    const { settings } = get();
    const game = createInitialGameState(settings);
    set({
      game,
      commentary: [],
      mathAnalysis: null,
      sessionStats: DEFAULT_SESSION_STATS,
      currentHandVPIP: false,
      currentHandPFR: false,
      heroStackAtHandStart: settings.startingStack,
      handHistory: [],
      opponentStats: emptyOpponentStats(game),
      currentHandVpipByPlayer: {},
      currentHandPfrByPlayer: {},
      coachingScore: 100,
      goodDecisions: 0,
      majorMistakes: 0,
      lastDecisionDeltaEv: null,
      currentHandGoodDecisions: 0,
      currentHandMistakes: 0,
      currentHandWorstDeltaEv: 0,
      currentHandWorstSpot: '',
      rebuyCount: 0,
      isPaused: false,
      decisionClockSec: 12,
      timeBankSec: 30,
      queuedAction: 'none',
    });
  },
  setMathAnalysis: (analysis) => set({ mathAnalysis: analysis }),
  updateSettings: (partialSettings) => set(s => {
    const next = { settings: { ...s.settings, ...partialSettings } };
    persistPrefs({ ...s, ...next });
    return next;
  }),
}));
