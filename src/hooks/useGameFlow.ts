import { useCallback, useEffect, useRef } from 'react';
import { useGameStore } from '../store';
import { ActionType, Street } from '../types/game';
import { CommentaryType } from '../types/commentary';
import { isBettingRoundComplete } from '../engine/dealer';
import { countNonFoldedPlayers, getAvailableActions } from '../engine/actions';
import { evaluateHand } from '../evaluation/evaluator';
import {
  generateDealCommentary,
  generateFlopCommentary,
  generateStreetCommentary,
  generateAIActionCommentary,
  generateShowdownCommentary,
  generateHandReview,
  generateHandDivider,
  generateMathCommentary,
  generateRecommendation,
} from '../commentary/generator';

export function useGameFlow() {
  const game = useGameStore(s => s.game);
  const settings = useGameStore(s => s.settings);
  const initGame = useGameStore(s => s.initGame);
  const dealNewHand = useGameStore(s => s.dealNewHand);
  const setAIThinking = useGameStore(s => s.setAIThinking);
  const isPaused = useGameStore(s => s.isPaused);
  const togglePause = useGameStore(s => s.togglePause);
  const decisionClockSec = useGameStore(s => s.decisionClockSec);
  const timeBankSec = useGameStore(s => s.timeBankSec);
  const setDecisionClockSec = useGameStore(s => s.setDecisionClockSec);
  const setTimeBankSec = useGameStore(s => s.setTimeBankSec);
  const queuedAction = useGameStore(s => s.queuedAction);
  const setQueuedAction = useGameStore(s => s.setQueuedAction);
  const sessionStats = useGameStore(s => s.sessionStats);
  const addCommentary = useGameStore(s => s.addCommentary);
  const playerAction = useGameStore(s => s.playerAction);
  const advanceGame = useGameStore(s => s.advanceGame);
  const mathAnalysis = useGameStore(s => s.mathAnalysis);
  const aiTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastStreetRef = useRef<Street | null>(null);
  const lastHandRef = useRef<number>(0);
  const lastMathCommentaryKey = useRef<string>('');
  const lastHeroDecisionKeyRef = useRef<string>('');

  // Generate commentary when a new hand starts
  useEffect(() => {
    if (game.handNumber > 0 && game.handNumber !== lastHandRef.current) {
      lastHandRef.current = game.handNumber;
      addCommentary(generateHandDivider(game.handNumber));
      if (
        settings.gameMode === 'tournament' &&
        game.handNumber > 1 &&
        (game.handNumber - 1) % settings.blindIncreaseEveryHands === 0
      ) {
        addCommentary({
          id: `blind-level-${Date.now()}`,
          type: CommentaryType.Situation,
          street: Street.Preflop,
          text: `Blinds increased: ${settings.smallBlind}/${settings.bigBlind}. Pressure is rising.`,
          timestamp: Date.now(),
          highlight: true,
        });
      }
      const hero = game.players.find(p => p.isHuman);
      if (hero?.holeCards) {
        addCommentary(generateDealCommentary(hero.holeCards, hero.position, game.street));
      }
    }
  }, [
    addCommentary,
    game.handNumber,
    game.players,
    game.street,
    settings.bigBlind,
    settings.blindIncreaseEveryHands,
    settings.gameMode,
    settings.smallBlind,
  ]);

  // Generate commentary when the street changes
  useEffect(() => {
    if (game.street !== lastStreetRef.current && game.handNumber > 0) {
      lastStreetRef.current = game.street;

      const hero = game.players.find(p => p.isHuman);

      if (game.street === Street.Flop && hero?.holeCards && game.communityCards.length >= 3) {
        const allCards = [...hero.holeCards, ...game.communityCards];
        if (allCards.length >= 5) {
          const handResult = evaluateHand(allCards);
          addCommentary(generateFlopCommentary(hero.holeCards, game.communityCards, handResult, game.street));
        }
      } else if ((game.street === Street.Turn || game.street === Street.River) && hero?.holeCards) {
        const newCard = game.communityCards[game.communityCards.length - 1];
        const allCards = [...hero.holeCards, ...game.communityCards];
        if (allCards.length >= 5) {
          const handResult = evaluateHand(allCards);
          addCommentary(generateStreetCommentary(newCard, handResult, game.street));
        }
      } else if (game.street === Street.Showdown || game.isHandComplete) {
        if (game.winners.length > 0) {
          addCommentary(generateShowdownCommentary(game.winners, game.players, Street.Showdown));
          addCommentary(generateHandReview(game, mathAnalysis, sessionStats));
        }
      }
    }
  }, [addCommentary, game, game.isHandComplete, game.street, mathAnalysis, sessionStats]);

  // Generate math insight + recommendation when hero faces a decision
  useEffect(() => {
    if (game.isHandComplete || game.handNumber === 0 || !mathAnalysis) return;

    const hero = game.players.find(p => p.isHuman);
    if (!hero || game.activePlayerIndex !== hero.seatIndex || hero.isFolded) return;

    // Deduplicate: only generate once per (hand, street, action count)
    const key = `${game.handNumber}-${game.street}-${game.actions.length}`;
    if (key === lastMathCommentaryKey.current) return;
    lastMathCommentaryKey.current = key;

    const mathEntry = generateMathCommentary(mathAnalysis, game.street);
    if (mathEntry) addCommentary(mathEntry);

    const recEntry = generateRecommendation(mathAnalysis, game.street);
    if (recEntry) addCommentary(recEntry);
  }, [
    addCommentary,
    game.activePlayerIndex,
    game.actions.length,
    game.handNumber,
    game.isHandComplete,
    game.players,
    game.street,
    mathAnalysis,
  ]);

  // Hero decision timer + queued actions
  useEffect(() => {
    if (game.isHandComplete || game.handNumber === 0 || isPaused) return;

    const hero = game.players.find(p => p.isHuman);
    if (!hero || hero.isFolded || hero.isAllIn) return;
    const isHeroTurn = game.activePlayerIndex === hero.seatIndex;
    if (!isHeroTurn) return;

    const decisionKey = `${game.handNumber}-${game.street}-${game.actions.length}`;
    if (lastHeroDecisionKeyRef.current !== decisionKey) {
      lastHeroDecisionKeyRef.current = decisionKey;
      setDecisionClockSec(12);

      // Execute queued pre-action immediately when legal.
      const available = getAvailableActions(game);
      if (queuedAction === 'check' && available.canCheck) {
        setQueuedAction('none');
        playerAction(ActionType.Check);
        return;
      }
      if (queuedAction === 'check-fold') {
        setQueuedAction('none');
        if (available.canCheck) {
          playerAction(ActionType.Check);
        } else if (available.canFold) {
          playerAction(ActionType.Fold);
        }
        return;
      }
      if (queuedAction === 'call' && available.canCall) {
        setQueuedAction('none');
        playerAction(ActionType.Call, available.callAmount);
        return;
      }
    }

    const timer = setTimeout(() => {
      if (decisionClockSec > 0) {
        setDecisionClockSec(decisionClockSec - 1);
        return;
      }
      if (timeBankSec > 0) {
        setTimeBankSec(timeBankSec - 1);
        return;
      }

      const available = getAvailableActions(game);
      if (available.canCheck) {
        playerAction(ActionType.Check);
      } else if (available.canFold) {
        playerAction(ActionType.Fold);
      } else if (available.canCall) {
        playerAction(ActionType.Call, available.callAmount);
      }
      addCommentary({
        id: `timeout-${Date.now()}`,
        type: CommentaryType.Situation,
        street: game.street,
        text: 'Time expired. Auto-action selected.',
        timestamp: Date.now(),
      });
    }, 1000);

    return () => clearTimeout(timer);
  }, [
    addCommentary,
    game,
    game.activePlayerIndex,
    game.actions.length,
    game.handNumber,
    game.isHandComplete,
    game.street,
    isPaused,
    playerAction,
    decisionClockSec,
    setDecisionClockSec,
    setQueuedAction,
    setTimeBankSec,
    timeBankSec,
    queuedAction,
  ]);

  // Check if we need to advance the game (after each action)
  useEffect(() => {
    if (game.isHandComplete || game.handNumber === 0) return;

    // Check if only one player left
    if (countNonFoldedPlayers(game) <= 1) {
      advanceGame();
      return;
    }

    // Check if betting round is complete
    if (isBettingRoundComplete(game)) {
      const dramaticDelay =
        game.street === Street.River ? 900 :
        game.street === Street.Turn ? 550 :
        game.street === Street.Flop ? 380 : 260;
      const timer = setTimeout(() => {
        advanceGame();
      }, dramaticDelay);
      return () => clearTimeout(timer);
    }
  }, [advanceGame, game, game.actions.length, game.activePlayerIndex]);

  // Handle AI turns
  useEffect(() => {
    if (game.isHandComplete || game.handNumber === 0) return;
    if (isPaused) {
      setAIThinking(false);
      return;
    }

    const activePlayer = game.players[game.activePlayerIndex];
    if (!activePlayer || activePlayer.isHuman || activePlayer.isFolded || activePlayer.isAllIn) return;

    // It's an AI player's turn
    setAIThinking(true);

    const delay = settings.gameSpeed === 'fast' ? 400 : settings.gameSpeed === 'slow' ? 2000 : 1000;

    aiTimerRef.current = setTimeout(async () => {
      try {
        const { makeAIDecision } = await import('../ai/decision-engine');
        const { getProfile } = await import('../ai/profiles');

        const baseProfile = getProfile(activePlayer.personality!);
        const heroHands = Math.max(1, sessionStats.handsPlayed);
        const heroVPIP = (sessionStats.vpipHands / heroHands) * 100;
        const heroPFR = (sessionStats.pfrHands / heroHands) * 100;
        const profile = {
          ...baseProfile,
          foldToAggression: Math.max(5, Math.min(95, baseProfile.foldToAggression + (heroVPIP > 34 ? -8 : 6))),
          bluffFreq: Math.max(2, Math.min(95, baseProfile.bluffFreq + (heroPFR < 15 ? 8 : -4))),
        };
        if (settings.aiSkill === 'novice') {
          profile.gtoAwareness = Math.max(0, profile.gtoAwareness - 0.28);
          profile.positionAwareness = Math.max(0, profile.positionAwareness - 0.22);
          profile.foldToAggression = Math.min(95, profile.foldToAggression + 10);
        } else if (settings.aiSkill === 'elite') {
          profile.gtoAwareness = Math.min(1, profile.gtoAwareness + 0.2);
          profile.positionAwareness = Math.min(1, profile.positionAwareness + 0.16);
          profile.foldToAggression = Math.max(5, profile.foldToAggression - 8);
          profile.bluffFreq = Math.min(95, profile.bluffFreq + 5);
        }
        const decision = makeAIDecision(activePlayer, game, profile);

        addCommentary(generateAIActionCommentary(
          activePlayer,
          decision.action,
          decision.amount,
          game.street,
        ));

        playerAction(decision.action, decision.amount);
      } catch (err) {
        console.error('AI decision error:', err);
        // Safe fallback: check if possible, otherwise fold
        const toCall = game.currentBet - activePlayer.currentBet;
        if (toCall === 0) {
          playerAction(ActionType.Check);
        } else {
          playerAction(ActionType.Fold);
        }
      }

      setAIThinking(false);
    }, delay);

    return () => {
      if (aiTimerRef.current) {
        clearTimeout(aiTimerRef.current);
      }
    };
  }, [
    addCommentary,
    game,
    game.activePlayerIndex,
    game.handNumber,
    game.isHandComplete,
    isPaused,
    playerAction,
    sessionStats.handsPlayed,
    sessionStats.pfrHands,
    sessionStats.vpipHands,
    setAIThinking,
    settings.aiSkill,
    settings.gameSpeed,
  ]);

  // Auto-deal next hand after hand completes (only if autoDeal is on)
  const autoDeal = useGameStore(s => s.autoDeal);
  const isTournamentComplete =
    settings.gameMode === 'tournament' &&
    game.players.filter(p => p.stack > 0).length <= 1 &&
    game.isHandComplete;

  useEffect(() => {
    if (!game.isHandComplete || game.handNumber === 0 || !autoDeal || isTournamentComplete) return;

    // Don't auto-deal if hero is busted — they need to rebuy first
    const hero = game.players.find(p => p.isHuman);
    if (hero && hero.stack <= 0) return;

    const timer = setTimeout(() => {
      dealNewHand();
    }, 2500);

    return () => clearTimeout(timer);
  }, [autoDeal, dealNewHand, game.handNumber, game.isHandComplete, game.players, isTournamentComplete]);

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // "D" to deal next hand when hand is complete and auto-deal is off
      if (game.isHandComplete && game.handNumber > 0 && !autoDeal) {
        if (isTournamentComplete) return;
        if (e.key.toLowerCase() === 'd') {
          dealNewHand();
        }
        return;
      }

      if (game.isHandComplete) return;
      const hero = game.players.find(p => p.isHuman);
      if (!hero || game.activePlayerIndex !== hero.seatIndex) return;

      switch (e.key.toLowerCase()) {
        case ' ':
          e.preventDefault();
          togglePause();
          return;
        case 'f':
          playerAction(ActionType.Fold);
          break;
        case 'c':
          if (game.currentBet > hero.currentBet) {
            playerAction(ActionType.Call, Math.min(game.currentBet - hero.currentBet, hero.stack));
          } else {
            playerAction(ActionType.Check);
          }
          break;
        case 'r':
          {
            const raiseAmount = Math.min(game.currentBet * 2 + settings.bigBlind, hero.stack);
            playerAction(
              game.currentBet > 0 ? ActionType.Raise : ActionType.Bet,
              raiseAmount
            );
          }
          break;
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [
    autoDeal,
    dealNewHand,
    game.activePlayerIndex,
    game.currentBet,
    game.handNumber,
    game.isHandComplete,
    game.players,
    playerAction,
    settings.bigBlind,
    togglePause,
    isTournamentComplete,
  ]);

  const startGame = useCallback((customSettings?: Partial<typeof settings>) => {
    initGame(customSettings);
    dealNewHand();
  }, [initGame, dealNewHand]);

  return { startGame };
}
