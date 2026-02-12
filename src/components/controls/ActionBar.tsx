import React, { useCallback, useMemo, useState } from 'react';
import { useGameStore } from '../../store';
import { ActionType, Street } from '../../types/game';
import { Position } from '../../types/player';
import { handNotation, getHandTier, HandTier } from '../../ai/preflop-ranges';
import { getAvailableActions } from '../../engine/actions';
import './ActionBar.css';

export const ActionBar: React.FC = () => {
  const game = useGameStore(s => s.game);
  const playerAction = useGameStore(s => s.playerAction);
  const dealNewHand = useGameStore(s => s.dealNewHand);
  const autoDeal = useGameStore(s => s.autoDeal);
  const toggleAutoDeal = useGameStore(s => s.toggleAutoDeal);
  const settings = useGameStore(s => s.settings);
  const updateSettings = useGameStore(s => s.updateSettings);
  const resetSession = useGameStore(s => s.resetSession);
  const mathAnalysis = useGameStore(s => s.mathAnalysis);
  const isPaused = useGameStore(s => s.isPaused);
  const togglePause = useGameStore(s => s.togglePause);
  const soundEnabled = useGameStore(s => s.soundEnabled);
  const toggleSound = useGameStore(s => s.toggleSound);
  const decisionClockSec = useGameStore(s => s.decisionClockSec);
  const timeBankSec = useGameStore(s => s.timeBankSec);
  const queuedAction = useGameStore(s => s.queuedAction);
  const setQueuedAction = useGameStore(s => s.setQueuedAction);

  const rebuyHero = useGameStore(s => s.rebuyHero);
  const rebuyCount = useGameStore(s => s.rebuyCount);

  const hero = game.players.find(p => p.isHuman);
  const tournamentWinner =
    settings.gameMode === 'tournament' && game.isHandComplete
      ? game.players.find(p => p.stack > 0)
      : null;
  const isHeroBusted = hero != null && hero.stack <= 0 && game.isHandComplete;
  const isHeroTurn = hero && game.activePlayerIndex === hero.seatIndex && !game.isHandComplete;
  const canQueue = !game.isHandComplete && !isHeroTurn && hero && !hero.isFolded && !hero.isAllIn;
  const available = isHeroTurn ? getAvailableActions(game) : null;

  const handleAction = useCallback((action: ActionType, amount?: number) => {
    playerAction(action, amount);
  }, [playerAction]);

  const bb = settings.bigBlind;
  const potSize = game.pots.reduce((s, p) => s + p.amount, 0);
  const isPreflop = game.street === Street.Preflop;

  const canRaiseOrBet = available ? (available.canBet || available.canRaise) : false;
  const raiseAction = available?.canBet ? ActionType.Bet : ActionType.Raise;
  const maxRaise = available?.maxRaise ?? (hero?.stack ?? 0);
  const callAmount = available?.callAmount ?? 0;
  const minRaiseVal = available?.minRaise ?? bb;
  const minRaiseTotal = callAmount + Math.max(minRaiseVal, bb);
  const clamp = useCallback((val: number) => Math.max(minRaiseTotal, Math.min(val, maxRaise)), [minRaiseTotal, maxRaise]);
  const contextKey = `${game.handNumber}-${game.street}-${game.activePlayerIndex}-${minRaiseTotal}-${maxRaise}`;
  const [customState, setCustomState] = useState<{ key: string; amount: number }>({
    key: contextKey,
    amount: minRaiseTotal,
  });
  const [pendingConfirm, setPendingConfirm] = useState<{ action: ActionType; amount?: number } | null>(null);
  const customAmount = customState.key === contextKey ? customState.amount : minRaiseTotal;

  const bbAmount = useCallback((mult: number) => clamp(Math.round(bb * mult)), [bb, clamp]);
  const potAfterCall = potSize + callAmount;
  const potAmount = useCallback((frac: number) => clamp(Math.round(callAmount + potAfterCall * frac)), [callAmount, potAfterCall, clamp]);
  const recommendation = mathAnalysis?.ev;
  const recommendedAction = recommendation?.bestAction;
  const recommendedSize = useMemo(() => {
    if (!canRaiseOrBet) return null;
    if (isPreflop) {
      const tier = mathAnalysis?.context?.preflopTier;
      const targetBb =
        tier === 'PREMIUM' ? 4 :
        tier === 'STRONG' ? 3 :
        tier === 'MEDIUM' ? 2.8 :
        tier === 'SPECULATIVE' ? 2.5 : 2.2;
      const amount = bbAmount(targetBb);
      return { amount, label: `${targetBb} BB` };
    }
    const texture = mathAnalysis?.context?.boardTexture ?? '';
    const spr = mathAnalysis?.context?.spr ?? 0;
    const frac =
      texture.includes('Wet') || texture.includes('Monotone')
        ? (spr > 4 ? 0.75 : 0.6)
        : (spr > 7 ? 0.5 : 0.33);
    const label =
      frac >= 0.74 ? '3/4 Pot' :
      frac >= 0.49 ? '1/2 Pot' : '1/3 Pot';
    return { amount: potAmount(frac), label };
  }, [canRaiseOrBet, isPreflop, mathAnalysis, bbAmount, potAmount]);
  const recommendationText = useMemo(() => {
    if (!recommendation) return null;
    const actionLabel =
      recommendedAction === ActionType.Raise ? 'Raise' :
      recommendedAction === ActionType.Call ? 'Call' :
      recommendedAction === ActionType.Fold ? 'Fold' :
      recommendedAction === ActionType.Check ? 'Check' : 'Act';
    const bestEV = recommendedAction === ActionType.Raise ? recommendation.raiseEV : recommendation.callEV;
    return `${actionLabel} recommended (${bestEV >= 0 ? '+' : ''}${bestEV.toFixed(0)} EV)`;
  }, [recommendation, recommendedAction]);

  // Build hand result summary for the pause screen
  const handResultSummary = game.isHandComplete && game.winners.length > 0
    ? game.winners.map(w => {
        const p = game.players.find(pl => pl.id === w.playerId);
        return `${p?.name ?? '?'} wins $${w.amount}${w.hand ? ` (${w.hand})` : ''}`;
      }).join(' | ')
    : null;

  const preflopChartHint = useMemo(() => {
    if (!isHeroTurn || !hero?.holeCards || game.street !== Street.Preflop || !available) return null;
    const notation = handNotation(hero.holeCards);
    const tier = getHandTier(notation);
    const unopened = available.callAmount === 0;
    const latePos = hero.position === Position.Button || hero.position === Position.CO || hero.position === Position.HJ;
    let text = '';
    if (unopened) {
      if (tier === HandTier.Premium || tier === HandTier.Strong) text = `Chart: Open-raise ${notation} from ${hero.position}.`;
      else if (tier === HandTier.Medium && latePos) text = `Chart: Prefer open-raise ${notation} in late position.`;
      else if (tier === HandTier.Speculative && latePos) text = `Chart: Mix ${notation} in late position.`;
      else text = `Chart: Mostly fold ${notation} from ${hero.position}.`;
    } else {
      if (tier === HandTier.Premium) text = `Chart: 3-bet/get-in with ${notation}.`;
      else if (tier === HandTier.Strong || (tier === HandTier.Medium && latePos)) text = `Chart: Continue with ${notation} (call or 3-bet mix).`;
      else text = `Chart: Lean fold versus pressure with ${notation}.`;
    }
    return text;
  }, [isHeroTurn, hero, game.street, available]);

  const tournamentPressureHint = useMemo(() => {
    if (!isHeroTurn || !mathAnalysis?.context) return null;
    const ctx = mathAnalysis.context;
    const parts: string[] = [];
    if (ctx.shortStackPushFoldHint) {
      parts.push(ctx.shortStackPushFoldHint);
    }
    if (ctx.pressureStage === 'bubble' || ctx.pressureStage === 'final-table') {
      parts.push('Payout pressure: avoid thin stack-offs without strong blocker/equity profiles.');
    }
    if (ctx.heroStackBb != null && ctx.averageStackBb != null && ctx.heroStackBb < ctx.averageStackBb * 0.6) {
      parts.push('Below average stack: prioritize fold-equity spots over marginal calls.');
    }
    return parts.length > 0 ? parts.join(' ') : null;
  }, [isHeroTurn, mathAnalysis]);

  const sizingHintText = isHeroTurn && recommendedSize
    ? `Sizing hint: ${recommendedSize.label} (~$${recommendedSize.amount})`
    : null;
  const guidanceHintText = preflopChartHint ?? tournamentPressureHint;

  const requiresConfirm = (action: ActionType, amount?: number): boolean => {
    if (!hero) return false;
    if (action === ActionType.AllIn) return true;
    if ((action === ActionType.Bet || action === ActionType.Raise) && amount && amount >= hero.stack * 0.7) return true;
    return false;
  };

  const dispatchAction = (action: ActionType, amount?: number) => {
    if (requiresConfirm(action, amount)) {
      setPendingConfirm({ action, amount });
      return;
    }
    handleAction(action, amount);
  };

  return (
    <div className="action-bar">
      <div className="confirm-slot">
        {pendingConfirm ? (
          <div className="confirm-row">
            <span>
              Confirm {pendingConfirm.action} {pendingConfirm.amount ? `$${pendingConfirm.amount}` : ''}?
            </span>
            <button className="confirm-btn confirm-btn-yes" onClick={() => {
              handleAction(pendingConfirm.action, pendingConfirm.amount);
              setPendingConfirm(null);
            }}>
              Confirm
            </button>
            <button className="confirm-btn" onClick={() => setPendingConfirm(null)}>
              Cancel
            </button>
          </div>
        ) : (
          <div className="confirm-placeholder" />
        )}
      </div>

      <div className="action-shell">
        <div className="action-buttons-row">
          {game.isHandComplete && game.handNumber > 0 && isHeroBusted ? (
            <div className="hand-complete-row">
              <span className="hero-busted-text">You're out of chips!</span>
              <div className="hand-complete-controls">
                <button className="action-btn action-btn-rebuy" onClick={() => { rebuyHero(); dealNewHand(); }}>
                  Rebuy ${settings.startingStack}
                </button>
                {rebuyCount > 0 && (
                  <span className="rebuy-count">{rebuyCount} rebuy{rebuyCount !== 1 ? 's' : ''}</span>
                )}
              </div>
            </div>
          ) : tournamentWinner ? (
            <div className="hand-complete-row">
              <span className="tournament-complete-text">
                Tournament Complete: {tournamentWinner.isHuman ? 'You are Champion!' : `${tournamentWinner.name} wins`}
              </span>
              <div className="hand-complete-controls">
                <button className="action-btn action-btn-deal" onClick={resetSession}>
                  New Tournament
                </button>
              </div>
            </div>
          ) : game.isHandComplete && game.handNumber > 0 ? (
            <div className="hand-complete-row">
              {handResultSummary && (
                <span className="hand-result-summary">{handResultSummary}</span>
              )}
              <div className="hand-complete-controls">
                {!autoDeal && (
                  <button className="action-btn action-btn-deal" onClick={dealNewHand}>
                    Deal Next Hand <span className="shortcut">D</span>
                  </button>
                )}
                <label className="auto-deal-toggle">
                  <input type="checkbox" checked={autoDeal} onChange={toggleAutoDeal} />
                  Auto-deal
                </label>
              </div>
            </div>
          ) : isHeroTurn && available ? (
            <div className="action-buttons">
              {available.canFold && (
                <button className={`action-btn action-btn-fold ${recommendedAction === ActionType.Fold ? 'action-btn-recommended' : ''}`} onClick={() => dispatchAction(ActionType.Fold)}>
                  Fold <span className="shortcut">F</span>
                </button>
              )}
              {available.canCheck && (
                <button className={`action-btn action-btn-check ${recommendedAction === ActionType.Check ? 'action-btn-recommended' : ''}`} onClick={() => dispatchAction(ActionType.Check)}>
                  Check <span className="shortcut">C</span>
                </button>
              )}
              {available.canCall && (
                <button className={`action-btn action-btn-call ${recommendedAction === ActionType.Call ? 'action-btn-recommended' : ''}`} onClick={() => dispatchAction(ActionType.Call, available.callAmount)}>
                  Call ${available.callAmount} <span className="shortcut">C</span>
                </button>
              )}
              {hero && hero.stack > 0 && (
                <button className="action-btn action-btn-allin" onClick={() => dispatchAction(ActionType.AllIn)}>
                  All-In ${hero.stack}
                </button>
              )}
            </div>
          ) : (
            <span className="waiting-text">Waiting for action...</span>
          )}
        </div>

        <div className="insight-slot">
          <div className="action-recommendation">{isHeroTurn && recommendationText ? recommendationText : ' '}</div>
          <div className="action-recommendation action-recommendation-chart">{sizingHintText ?? ' '}</div>
          <div className="action-recommendation action-recommendation-pressure">{guidanceHintText ?? ' '}</div>
        </div>

        <div className="control-slot">
          <div className="control-slot-left">
            {isHeroTurn ? (
              <div className="decision-timer-row">
                <span className={`decision-clock ${decisionClockSec <= 5 ? 'decision-clock-low' : ''}`}>
                  Decision: {decisionClockSec}s
                </span>
                <span className={`time-bank ${timeBankSec <= 10 ? 'time-bank-low' : ''}`}>
                  Time Bank: {timeBankSec}s
                </span>
              </div>
            ) : (
              <div className="control-placeholder" />
            )}
            {canQueue ? (
              <div className="queue-row">
                <button
                  className={`queue-btn ${queuedAction === 'check' ? 'queue-btn-active' : ''}`}
                  onClick={() => setQueuedAction(queuedAction === 'check' ? 'none' : 'check')}
                >
                  Queue Check
                </button>
                <button
                  className={`queue-btn ${queuedAction === 'check-fold' ? 'queue-btn-active' : ''}`}
                  onClick={() => setQueuedAction(queuedAction === 'check-fold' ? 'none' : 'check-fold')}
                >
                  Queue Check/Fold
                </button>
                <button
                  className={`queue-btn ${queuedAction === 'call' ? 'queue-btn-active' : ''}`}
                  onClick={() => setQueuedAction(queuedAction === 'call' ? 'none' : 'call')}
                >
                  Queue Call
                </button>
              </div>
            ) : (
              <div className="control-placeholder" />
            )}
          </div>
          <div className="utility-row">
            <button className="utility-btn" onClick={togglePause}>
              {isPaused ? 'Resume AI' : 'Pause AI'}
            </button>
            <button className="utility-btn" onClick={toggleSound}>
              {soundEnabled ? 'Sound On' : 'Sound Off'}
            </button>
            <label className="utility-speed">
              Speed
              <select
                value={settings.gameSpeed}
                onChange={e => updateSettings({ gameSpeed: e.target.value as typeof settings.gameSpeed })}
              >
                <option value="slow">Slow</option>
                <option value="normal">Normal</option>
                <option value="fast">Fast</option>
              </select>
            </label>
          </div>
        </div>

        <div className={`sizing-row ${!canRaiseOrBet ? 'sizing-row-disabled' : ''}`}>
          {isPreflop ? (
            <>
              <button className={`sizing-btn ${recommendedSize?.label === '2.5 BB' ? 'sizing-btn-recommended' : ''}`} disabled={!canRaiseOrBet} onClick={() => canRaiseOrBet && dispatchAction(raiseAction, bbAmount(2.5))}>2.5 BB</button>
              <button className={`sizing-btn ${recommendedSize?.label === '3 BB' ? 'sizing-btn-recommended' : ''}`} disabled={!canRaiseOrBet} onClick={() => canRaiseOrBet && dispatchAction(raiseAction, bbAmount(3))}>3 BB</button>
              <button className={`sizing-btn ${recommendedSize?.label === '4 BB' ? 'sizing-btn-recommended' : ''}`} disabled={!canRaiseOrBet} onClick={() => canRaiseOrBet && dispatchAction(raiseAction, bbAmount(4))}>4 BB</button>
              <button className={`sizing-btn ${recommendedSize?.label === '5 BB' ? 'sizing-btn-recommended' : ''}`} disabled={!canRaiseOrBet} onClick={() => canRaiseOrBet && dispatchAction(raiseAction, bbAmount(5))}>5 BB</button>
            </>
          ) : (
            <>
              <button className={`sizing-btn ${recommendedSize?.label === '1/3 Pot' ? 'sizing-btn-recommended' : ''}`} disabled={!canRaiseOrBet} onClick={() => canRaiseOrBet && dispatchAction(raiseAction, potAmount(0.33))}>1/3 Pot</button>
              <button className={`sizing-btn ${recommendedSize?.label === '1/2 Pot' ? 'sizing-btn-recommended' : ''}`} disabled={!canRaiseOrBet} onClick={() => canRaiseOrBet && dispatchAction(raiseAction, potAmount(0.5))}>1/2 Pot</button>
              <button className={`sizing-btn ${recommendedSize?.label === '3/4 Pot' ? 'sizing-btn-recommended' : ''}`} disabled={!canRaiseOrBet} onClick={() => canRaiseOrBet && dispatchAction(raiseAction, potAmount(0.75))}>3/4 Pot</button>
              <button className="sizing-btn" disabled={!canRaiseOrBet} onClick={() => canRaiseOrBet && dispatchAction(raiseAction, potAmount(1))}>Pot</button>
            </>
          )}
          <div className="custom-size-group">
            <input
              type="range"
              min={Math.max(minRaiseTotal, 1)}
              max={Math.max(maxRaise, Math.max(minRaiseTotal, 1))}
              step={1}
              value={Math.max(customAmount, Math.max(minRaiseTotal, 1))}
              disabled={!canRaiseOrBet}
              onChange={e =>
                setCustomState({
                  key: contextKey,
                  amount: clamp(Number(e.target.value) || minRaiseTotal),
                })
              }
            />
            <input
              className="custom-size-input"
              type="number"
              min={Math.max(minRaiseTotal, 1)}
              max={Math.max(maxRaise, Math.max(minRaiseTotal, 1))}
              step={1}
              value={Math.max(customAmount, Math.max(minRaiseTotal, 1))}
              disabled={!canRaiseOrBet}
              onChange={e =>
                setCustomState({
                  key: contextKey,
                  amount: clamp(Number(e.target.value) || minRaiseTotal),
                })
              }
            />
            <button
              className={`sizing-btn sizing-btn-custom ${recommendedAction === ActionType.Raise ? 'sizing-btn-recommended' : ''}`}
              disabled={!canRaiseOrBet}
              onClick={() => canRaiseOrBet && dispatchAction(raiseAction, clamp(customAmount))}
            >
              {available?.canBet ? 'Bet' : 'Raise'} ${clamp(customAmount)}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
