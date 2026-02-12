import { useEffect } from 'react';
import { useGameStore } from '../store';
import { Street } from '../types/game';
import { MathAnalysis } from '../types/odds';
import { getProfile } from '../ai/profiles';
import { handNotation, getHandTier } from '../ai/preflop-ranges';

export function useOddsCalculation() {
  const game = useGameStore(s => s.game);
  const setMathAnalysis = useGameStore(s => s.setMathAnalysis);

  useEffect(() => {
    if (game.isHandComplete || game.handNumber === 0) {
      return;
    }

    const hero = game.players.find(p => p.isHuman);
    if (!hero?.holeCards || hero.isFolded) {
      setMathAnalysis(null);
      return;
    }
    const holeCards = hero.holeCards;

    // Only calculate when it's hero's turn — skip during AI actions
    if (game.activePlayerIndex !== hero.seatIndex) {
      return;
    }

    const calculate = async () => {
      try {
        const { calculateOuts } = await import('../math/outs');
        const { calculatePotOdds } = await import('../math/pot-odds');
        const { calculateEquity } = await import('../math/equity');
        const { calculateEV } = await import('../math/ev');

        const analysis: MathAnalysis = {
          handStrength: null,
          outs: null,
          potOdds: null,
          equity: null,
          ev: null,
          context: null,
        };

        // Preflop strategic guidance
        if (game.street === Street.Preflop) {
          const notation = handNotation(holeCards);
          const tier = getHandTier(notation);
          analysis.context = {
            ...(analysis.context ?? {}),
            handNotation: notation,
            preflopTier: tier.toUpperCase(),
          };
        }

        // Hand strength (postflop only — need 5+ cards)
        if (game.communityCards.length >= 3) {
          const { evaluateHand } = await import('../evaluation/evaluator');
          const { HAND_CATEGORY_NAMES } = await import('../types/hand');
          const allCards = [...holeCards, ...game.communityCards];
          const result = evaluateHand(allCards);
          analysis.handStrength = {
            category: HAND_CATEGORY_NAMES[result.category],
            description: result.description,
          };
        }

        // Outs (only on flop and turn, when draws are relevant)
        if (game.street === Street.Flop || game.street === Street.Turn) {
          analysis.outs = calculateOuts(holeCards, game.communityCards);
        }

        // Equity (Monte Carlo) — calculate BEFORE pot odds so we can pass it
        const numOpponents = game.players.filter(p => !p.isFolded && p.id !== hero.id).length;
        if (numOpponents > 0 && game.communityCards.length >= 3) {
          analysis.equity = calculateEquity(
            holeCards,
            game.communityCards,
            numOpponents,
            5000
          );
        }

        // Pot odds (when there's a bet to call)
        const costToCall = game.currentBet - hero.currentBet;
        const potSize = game.pots.reduce((s, p) => s + p.amount, 0);
        const effectiveStack = game.players
          .filter(p => !p.isFolded && !p.isHuman)
          .reduce((m, p) => Math.min(m, p.stack), hero.stack);
        const spr = potSize > 0 ? effectiveStack / potSize : 0;
        let boardTexture: string | undefined;
        if (game.communityCards.length >= 3) {
          const suits = game.communityCards.map(c => c.suit);
          const uniqueSuits = new Set(suits).size;
          const ranks = game.communityCards.map(c => c.rank).sort((a, b) => b - a);
          const connected = ranks[0] - ranks[ranks.length - 1] <= 4;
          const paired = new Set(ranks).size < ranks.length;
          if (paired) boardTexture = 'Paired board';
          else if (uniqueSuits === 1) boardTexture = 'Monotone (very wet)';
          else if (uniqueSuits === 2 && connected) boardTexture = 'Wet and connected';
          else if (uniqueSuits === 3 && !connected) boardTexture = 'Dry rainbow';
          else boardTexture = 'Semi-wet board';
        }
        analysis.context = {
          ...(analysis.context ?? {}),
          spr: Number.isFinite(spr) ? spr : 0,
          boardTexture,
        };
        if (costToCall > 0) {
          analysis.potOdds = calculatePotOdds(
            potSize,
            costToCall,
            analysis.equity?.equity // pass equity so isGettingOdds is computed
          );
        }

        // EV — derive fold equity from opponents' profiles
        if (analysis.equity && costToCall > 0) {
          const opponents = game.players.filter(p => !p.isFolded && !p.isHuman && p.personality);
          let foldEquity = 0.3; // fallback
          if (opponents.length > 0) {
            const totalFoldToAgg = opponents.reduce((sum, p) => {
              const prof = getProfile(p.personality!);
              return sum + prof.foldToAggression;
            }, 0);
            foldEquity = (totalFoldToAgg / opponents.length) / 100;
          }

          analysis.ev = calculateEV(
            analysis.equity.equity,
            potSize,
            costToCall,
            costToCall * 2.5,
            foldEquity
          );
        }

        setMathAnalysis(analysis);
      } catch (err) {
        console.error('Odds calculation error:', err);
      }
    };

    const timer = setTimeout(calculate, 100);
    return () => clearTimeout(timer);
  }, [game, setMathAnalysis]);
}
