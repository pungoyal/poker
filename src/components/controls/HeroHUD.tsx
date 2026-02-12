import React from 'react';
import { useGameStore } from '../../store';
import { ActionType } from '../../types/game';
import './HeroHUD.css';

const ACTION_LABELS: Record<string, string> = {
  [ActionType.Fold]: 'Fold',
  [ActionType.Call]: 'Call',
  [ActionType.Check]: 'Check',
  [ActionType.Bet]: 'Bet',
  [ActionType.Raise]: 'Raise',
  [ActionType.AllIn]: 'All-In',
};

const STACK_ZONE_LABELS = {
  comfort: 'Comfort',
  pressure: 'Pressure',
  'push-fold': 'Push/Fold',
  critical: 'Critical',
} as const;

export const HeroHUD: React.FC = () => {
  const analysis = useGameStore(s => s.mathAnalysis);
  const game = useGameStore(s => s.game);
  const stats = useGameStore(s => s.sessionStats);
  const coachingScore = useGameStore(s => s.coachingScore);
  const goodDecisions = useGameStore(s => s.goodDecisions);
  const majorMistakes = useGameStore(s => s.majorMistakes);
  const lastDecisionDeltaEv = useGameStore(s => s.lastDecisionDeltaEv);
  const bb = useGameStore(s => s.settings.bigBlind);

  if (game.handNumber === 0) return null;

  const hasData = analysis && (analysis.outs || analysis.equity || analysis.potOdds || analysis.ev);
  const winRate = stats.handsPlayed > 0 ? (stats.handsWon / stats.handsPlayed) * 100 : 0;
  const vpip = stats.handsPlayed > 0 ? (stats.vpipHands / stats.handsPlayed) * 100 : 0;
  const pfr = stats.handsPlayed > 0 ? (stats.pfrHands / stats.handsPlayed) * 100 : 0;
  const bbPer100 = stats.handsPlayed > 0 ? (stats.netChips / bb) * (100 / stats.handsPlayed) : 0;
  const ctx = analysis?.context;
  const stackZone =
    ctx?.stackZone === 'comfort' ||
    ctx?.stackZone === 'pressure' ||
    ctx?.stackZone === 'push-fold' ||
    ctx?.stackZone === 'critical'
      ? ctx.stackZone
      : null;
  const showTournamentMetrics =
    ctx?.heroStackBb != null &&
    ctx.averageStackBb != null &&
    ctx.playersRemaining != null &&
    ctx.heroRankByStack != null &&
    ctx.mRatio != null &&
    stackZone != null;
  return (
    <div className="hero-hud">
      <div className="hud-cell hud-cell-session">
        <span className="hud-label">Session</span>
        <span className="hud-value">{stats.handsPlayed} Hands</span>
        <span className={`hud-detail hud-detail-session ${stats.netChips >= 0 ? 'hud-good' : 'hud-bad'}`}>
          {stats.netChips >= 0 ? '+' : ''}{stats.netChips} chips ({bbPer100 >= 0 ? '+' : ''}{bbPer100.toFixed(1)} bb/100)
        </span>
        <span className="hud-detail">
          WR {winRate.toFixed(0)}% | VPIP {vpip.toFixed(0)} | PFR {pfr.toFixed(0)}
        </span>
      </div>

      <div className="hud-cell">
        <span className="hud-label">Coach</span>
        <span className="hud-value">{coachingScore.toFixed(0)}</span>
        <span className="hud-detail">Good {goodDecisions} | Mistakes {majorMistakes}</span>
        {lastDecisionDeltaEv != null && (
          <span className={`hud-detail ${lastDecisionDeltaEv <= 0.1 ? 'hud-good' : 'hud-bad'}`}>
            Last ΔEV {lastDecisionDeltaEv > 0 ? '-' : '+'}${Math.abs(lastDecisionDeltaEv).toFixed(1)}
          </span>
        )}
      </div>

      {showTournamentMetrics && ctx && (
        <div className={`hud-cell hud-cell-stack-zone hud-zone-${ctx.stackZone}`}>
          <span className="hud-label">Tournament Pressure</span>
          <span className="hud-value">
            {STACK_ZONE_LABELS[stackZone]}
          </span>
          <span className="hud-detail">
            {ctx.heroStackBb!.toFixed(1)}BB ({ctx.heroRankByStack}/{ctx.playersRemaining})
          </span>
          <span className="hud-detail">
            Avg {ctx.averageStackBb!.toFixed(1)}BB | M {ctx.mRatio!.toFixed(1)}
          </span>
        </div>
      )}

      {/* Hand Strength */}
      {analysis?.handStrength && (
        <div className="hud-cell hud-cell-hand">
          <span className="hud-label">Your Hand</span>
          <span className="hud-value hud-hand-category">{analysis?.handStrength.category}</span>
          <span className="hud-detail">{analysis?.handStrength.description}</span>
        </div>
      )}

      {analysis?.context?.preflopTier && (
        <div className="hud-cell">
          <span className="hud-label">Preflop</span>
          <span className="hud-value">{analysis.context.handNotation}</span>
          <span className="hud-detail">{analysis.context.preflopTier} Tier</span>
        </div>
      )}

      {/* Equity */}
      {analysis?.equity && (
        <div className="hud-cell">
          <span className="hud-label">Equity</span>
          <span className="hud-value hud-equity">{analysis?.equity.equity.toFixed(1)}%</span>
          <div className="hud-equity-bar">
            <div className="hud-equity-fill" style={{ width: `${analysis?.equity.equity}%` }} />
          </div>
        </div>
      )}

      {/* Outs */}
      {analysis?.outs && analysis.outs.totalOuts > 0 && (
        <div className="hud-cell">
          <span className="hud-label">Outs</span>
          <span className="hud-value">{analysis?.outs.totalOuts}</span>
          <span className="hud-detail">{analysis?.outs.draws[0]?.description}</span>
        </div>
      )}

      {/* Pot Odds */}
      {analysis?.potOdds && (
        <div className="hud-cell">
          <span className="hud-label">Pot Odds</span>
          <span className="hud-value">{analysis?.potOdds.ratio}</span>
          <span className={`hud-verdict ${analysis?.potOdds.isGettingOdds ? 'hud-good' : 'hud-bad'}`}>
            {analysis?.potOdds.isGettingOdds ? 'Good price' : 'Bad price'}
          </span>
        </div>
      )}

      {analysis?.context && (analysis.context.boardTexture || analysis.context.spr != null) && (
        <div className="hud-cell">
          <span className="hud-label">Context</span>
          {analysis.context.spr != null && (
            <span className="hud-value">SPR {analysis.context.spr.toFixed(1)}</span>
          )}
          {analysis.context.boardTexture && (
            <span className="hud-detail">{analysis.context.boardTexture}</span>
          )}
        </div>
      )}

      {/* EV / Best Action */}
      {analysis?.ev && hasData && (
        <div className="hud-cell hud-cell-action">
          <span className="hud-label">Best Play</span>
          <span className="hud-value hud-best-action">
            {ACTION_LABELS[analysis.ev.bestAction] || analysis?.ev.bestAction}
          </span>
          <span className={`hud-ev ${analysis?.ev.callEV >= 0 ? 'hud-good' : 'hud-bad'}`}>
            EV: {analysis?.ev.callEV >= 0 ? '+' : ''}{analysis?.ev.callEV.toFixed(0)}
          </span>
        </div>
      )}
    </div>
  );
};
