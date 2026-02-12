import React from 'react';
import { Player, AIPersonalityType } from '../../types/player';
import { ActionType } from '../../types/game';
import { useGameStore } from '../../store';
import { CardComponent, CardBack } from '../cards/Card';
import { getPlayerAvatar } from '../../utils/avatars';
import './Seat.css';

const PERSONALITY_LABELS: Record<AIPersonalityType, string> = {
  [AIPersonalityType.TAG]: 'TAG',
  [AIPersonalityType.LAG]: 'LAG',
  [AIPersonalityType.Nit]: 'NIT',
  [AIPersonalityType.Maniac]: 'MANIAC',
  [AIPersonalityType.CallingStation]: 'STATION',
};

const PERSONALITY_COLORS: Record<AIPersonalityType, string> = {
  [AIPersonalityType.TAG]: '#5c9cf5',
  [AIPersonalityType.LAG]: '#f59e0b',
  [AIPersonalityType.Nit]: '#78909c',
  [AIPersonalityType.Maniac]: '#ef5350',
  [AIPersonalityType.CallingStation]: '#66bb6a',
};

const ACTION_DISPLAY: Record<string, { label: string; className: string }> = {
  [ActionType.Fold]: { label: 'Fold', className: 'action-tag-fold' },
  [ActionType.Check]: { label: 'Check', className: 'action-tag-check' },
  [ActionType.Call]: { label: 'Call', className: 'action-tag-call' },
  [ActionType.Bet]: { label: 'Bet', className: 'action-tag-bet' },
  [ActionType.Raise]: { label: 'Raise', className: 'action-tag-raise' },
  [ActionType.AllIn]: { label: 'All-In', className: 'action-tag-allin' },
};

interface SeatProps {
  player: Player;
  position: { left: string; top: string };
  isActive: boolean;
  isHero: boolean;
  showCards: boolean;
  isThinking: boolean;
  isWinner: boolean;
  lastAction: { type: ActionType; amount: number } | null;
  winningHand: string | null;
  isExploitTarget: boolean;
}

export const Seat: React.FC<SeatProps> = ({
  player,
  position,
  isActive,
  isHero,
  showCards,
  isThinking,
  isWinner,
  lastAction,
  winningHand,
  isExploitTarget,
}) => {
  const bb = useGameStore(s => s.settings.bigBlind);
  const oppStats = useGameStore(s => s.opponentStats[player.id]);
  const isBusted = player.stack <= 0 && player.isFolded && !player.holeCards;
  const stackBB = Math.round((player.stack / bb) * 10) / 10;
  const vpip = oppStats && oppStats.hands > 0 ? (oppStats.vpipHands / oppStats.hands) * 100 : 0;
  const pfr = oppStats && oppStats.hands > 0 ? (oppStats.pfrHands / oppStats.hands) * 100 : 0;
  const af = oppStats ? (oppStats.passiveActions > 0 ? (oppStats.aggressiveActions / oppStats.passiveActions) : oppStats.aggressiveActions) : 0;

  return (
    <div
      className={`seat ${isHero ? 'seat-hero' : ''} ${isActive ? 'seat-active' : ''} ${player.isFolded ? 'seat-folded' : ''} ${isWinner ? 'seat-winner' : ''} ${isBusted ? 'seat-busted' : ''}`}
      style={{ left: position.left, top: position.top }}
    >
      {/* Hole cards */}
      <div className="seat-cards">
        {player.holeCards && !player.isFolded ? (
          showCards ? (
            <>
              <CardComponent card={player.holeCards[0]} small={!isHero} />
              <CardComponent card={player.holeCards[1]} small={!isHero} />
            </>
          ) : (
            <>
              <CardBack small={!isHero} />
              <CardBack small={!isHero} />
            </>
          )
        ) : null}
      </div>

      {/* Player info */}
      <div className={`seat-info ${isHero ? 'seat-info-hero' : ''}`}>
        <div className="seat-name">
          <span className="seat-avatar">{getPlayerAvatar(player.name)}</span>
          {player.name}
          {isThinking && <span className="thinking-dots"> ...</span>}
        </div>
        <div className="seat-stack-row">
          <span className="seat-stack">{stackBB} BB</span>
          {!isBusted && (
            <span className={`seat-position seat-position-${player.position === 'BTN' || player.position === 'SB' || player.position === 'BB' ? 'key' : 'other'}`}>
              {player.position}
            </span>
          )}
        </div>
        {!isHero && oppStats && oppStats.hands > 0 && (
          <div className="seat-stats">
            VPIP {vpip.toFixed(0)} / PFR {pfr.toFixed(0)} / AF {af.toFixed(1)}
          </div>
        )}
        {player.currentBet > 0 && (
          <div className="seat-bet">
            <span className="bet-chip" />
            ${player.currentBet}
          </div>
        )}
        {player.isAllIn && <div className="seat-allin">ALL IN</div>}
      </div>

      {/* Last action tag */}
      {lastAction && !isActive && (() => {
        const display = ACTION_DISPLAY[lastAction.type];
        if (!display) return null;
        const showAmount = lastAction.amount > 0 && lastAction.type !== ActionType.Fold && lastAction.type !== ActionType.Check;
        return (
          <div className={`seat-action-tag ${display.className}`}>
            {display.label}{showAmount ? ` $${lastAction.amount}` : ''}
          </div>
        );
      })()}

      {/* Winner hand tag */}
      {isWinner && winningHand && (
        <div className="seat-winning-hand">
          {winningHand}
        </div>
      )}

      {isExploitTarget && !isHero && (
        <div className="seat-exploit-tag">EXPLOIT</div>
      )}

      {/* Personality badge for AI */}
      {player.personality && (
        <div
          className="seat-personality"
          style={{
            color: PERSONALITY_COLORS[player.personality],
            borderColor: PERSONALITY_COLORS[player.personality],
          }}
        >
          {PERSONALITY_LABELS[player.personality]}
        </div>
      )}
    </div>
  );
};
