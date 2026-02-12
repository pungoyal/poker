import React, { useMemo, useState } from 'react';
import { useGameStore } from '../../store';
import { ActionType } from '../../types/game';
import './HandReplayPanel.css';

function actionLabel(type: ActionType): string {
  switch (type) {
    case ActionType.Fold:
      return 'folds';
    case ActionType.Check:
      return 'checks';
    case ActionType.Call:
      return 'calls';
    case ActionType.Bet:
      return 'bets';
    case ActionType.Raise:
      return 'raises';
    case ActionType.AllIn:
      return 'shoves';
    case ActionType.PostBlind:
      return 'posts blind';
    default:
      return type;
  }
}

export const HandReplayPanel: React.FC = () => {
  const handHistory = useGameStore(s => s.handHistory);
  const [open, setOpen] = useState(true);
  const [selectedHandNumber, setSelectedHandNumber] = useState<number | null>(null);
  const [step, setStep] = useState(0);

  const selected = useMemo(() => {
    if (handHistory.length === 0) return null;
    if (selectedHandNumber == null) return handHistory[handHistory.length - 1];
    return handHistory.find(h => h.handNumber === selectedHandNumber) ?? handHistory[handHistory.length - 1];
  }, [handHistory, selectedHandNumber]);

  const replayLines = useMemo(() => {
    if (!selected) return [];
    return selected.actions.map(action => {
      const name = selected.playerNames[action.playerId] ?? action.playerId;
      const amount = action.amount > 0 ? ` $${action.amount}` : '';
      return `${name} ${actionLabel(action.type)}${amount}`;
    });
  }, [selected]);

  React.useEffect(() => {
    if (!selected) return;
    setStep(s => Math.min(s, selected.actions.length));
  }, [selected]);

  if (handHistory.length === 0) return null;

  const maxStep = selected?.actions.length ?? 0;
  const currentStep = Math.min(step, maxStep);
  const visibleLines = replayLines.slice(0, currentStep);
  const lastLine = visibleLines[visibleLines.length - 1];

  return (
    <div className={`replay-panel ${open ? 'replay-panel-open' : 'replay-panel-closed'}`}>
      <div className="replay-header" onClick={() => setOpen(v => !v)}>
        <span className="replay-icon">▶</span>
        <h3>Hand Replay</h3>
        <span className="replay-toggle">{open ? '▼' : '▶'}</span>
      </div>

      {open && selected && (
        <div className="replay-content">
          <div className="replay-controls-top">
            <label>
              Hand
              <select
                value={selected.handNumber}
                onChange={e => {
                  setSelectedHandNumber(Number(e.target.value));
                  setStep(0);
                }}
              >
                {[...handHistory].reverse().slice(0, 30).map(h => (
                  <option key={h.handNumber} value={h.handNumber}>
                    #{h.handNumber}
                  </option>
                ))}
              </select>
            </label>
            <span className={`replay-net ${selected.heroNetChips >= 0 ? 'replay-net-plus' : 'replay-net-minus'}`}>
              Hero {selected.heroNetChips >= 0 ? '+' : ''}{selected.heroNetChips}
            </span>
          </div>

          <div className="replay-coach">
            <span className="replay-coach-title">Coach</span>
            <span className="replay-coach-line">
              Good {selected.coach.goodDecisions} | Mistakes {selected.coach.mistakes} | Worst ΔEV {selected.coach.worstDeltaEv.toFixed(1)}
            </span>
            {selected.coach.worstSpot && (
              <span className="replay-coach-line replay-coach-spot">Leak spot: {selected.coach.worstSpot}</span>
            )}
          </div>

          <div className="replay-stepper">
            <button onClick={() => setStep(0)}>Start</button>
            <button onClick={() => setStep(s => Math.max(0, s - 1))}>Prev</button>
            <button onClick={() => setStep(s => Math.min(maxStep, s + 1))}>Next</button>
            <button onClick={() => setStep(maxStep)}>End</button>
          </div>

          <div className="replay-progress">
            Step {currentStep}/{maxStep}
          </div>

          <div className="replay-last-line">
            {lastLine ?? 'Step through this hand to review action flow.'}
          </div>

          <div className="replay-log">
            {visibleLines.length === 0 ? (
              <div className="replay-empty">No actions shown yet.</div>
            ) : (
              visibleLines.map((line, i) => (
                <div key={`${line}-${i}`} className="replay-line">{i + 1}. {line}</div>
              ))
            )}
          </div>
        </div>
      )}
    </div>
  );
};
