import React, { useMemo } from 'react';
import { useGameStore } from '../../store';
import { ActionType } from '../../types/game';
import { Seat } from './Seat';
import { CommunityCards } from './CommunityCards';
import { PotDisplay } from './PotDisplay';
import './PokerTable.css';

// Seat positions around the table (percentages of .poker-table)
const SEAT_POSITIONS_9: { left: string; top: string }[] = [
  { left: '50%', top: '84%' },    // Seat 0: Hero (bottom center)
  { left: '14%', top: '76%' },    // Seat 1: Left bottom
  { left: '2%',  top: '50%' },    // Seat 2: Left middle
  { left: '10%', top: '22%' },    // Seat 3: Left top
  { left: '32%', top: '3%' },     // Seat 4: Top left
  { left: '68%', top: '3%' },     // Seat 5: Top right
  { left: '90%', top: '22%' },    // Seat 6: Right top
  { left: '98%', top: '50%' },    // Seat 7: Right middle
  { left: '86%', top: '76%' },    // Seat 8: Right bottom
];

// Dealer button positions: offset toward table center from each seat
const DEALER_BTN_POSITIONS_9: { left: string; top: string }[] = [
  { left: '43%', top: '76%' },
  { left: '21%', top: '69%' },
  { left: '14%', top: '48%' },
  { left: '19%', top: '28%' },
  { left: '36%', top: '14%' },
  { left: '64%', top: '14%' },
  { left: '81%', top: '28%' },
  { left: '86%', top: '48%' },
  { left: '79%', top: '69%' },
];

export const PokerTable: React.FC = () => {
  const game = useGameStore(s => s.game);
  const isAIThinking = useGameStore(s => s.isAIThinking);
  const opponentStats = useGameStore(s => s.opponentStats);

  const seatPositions = SEAT_POSITIONS_9;
  const dealerPositions = DEALER_BTN_POSITIONS_9;
  const dealerPos = dealerPositions[game.dealerIndex] || dealerPositions[0];

  // Derive each player's last action on the current street
  const lastActions = useMemo(() => {
    const map = new Map<string, { type: ActionType; amount: number }>();
    for (const action of game.actions) {
      if (action.street === game.street && action.type !== ActionType.PostBlind) {
        map.set(action.playerId, { type: action.type, amount: action.amount });
      }
    }
    return map;
  }, [game.actions, game.street]);

  const recentActions = useMemo(() => {
    const streetActions = game.actions
      .filter(a => a.street === game.street && a.type !== ActionType.PostBlind)
      .slice(-5);

    return streetActions.map(a => {
      const player = game.players.find(p => p.id === a.playerId);
      const name = player?.isHuman ? 'You' : (player?.name ?? 'Player');
      const verb = a.type === ActionType.AllIn ? 'all-in' : a.type;
      const text =
        a.amount > 0 && a.type !== ActionType.Fold && a.type !== ActionType.Check
          ? `${name} ${verb} $${a.amount}`
          : `${name} ${verb}`;
      return text;
    });
  }, [game.actions, game.players, game.street]);

  const exploitTargetId = useMemo(() => {
    let best: { id: string; score: number } | null = null;
    for (const p of game.players) {
      if (p.isHuman || p.isFolded || p.stack <= 0) continue;
      const s = opponentStats[p.id];
      if (!s || s.hands < 6) continue;
      const vpip = (s.vpipHands / Math.max(1, s.hands)) * 100;
      const pfr = (s.pfrHands / Math.max(1, s.hands)) * 100;
      const af = s.passiveActions > 0 ? s.aggressiveActions / s.passiveActions : s.aggressiveActions;
      const score = vpip * 0.8 - pfr * 0.3 - af * 6;
      if (!best || score > best.score) best = { id: p.id, score };
    }
    return best?.id ?? null;
  }, [game.players, opponentStats]);

  return (
    <div className="poker-table-container">
      <div className="poker-table">
        {/* Felt surface */}
        <div className={`poker-table-felt ${game.isHandComplete ? 'poker-table-felt-showdown' : ''}`}>
          <div className="poker-table-rail" />
          <CommunityCards cards={game.communityCards} />
          <PotDisplay pots={game.pots} />
          {recentActions.length > 0 && (
            <div className="table-action-ticker">
              {recentActions.map((line, idx) => (
                <span key={`${line}-${idx}`} className="ticker-line">{line}</span>
              ))}
            </div>
          )}
        </div>

        {/* Dealer button - standalone on table */}
        {game.handNumber > 0 && (
          <div
            className="table-dealer-btn"
            style={{ left: dealerPos.left, top: dealerPos.top }}
          >
            D
          </div>
        )}

        {/* Seats */}
        {game.players.map((player, idx) => {
          const winnerEntry = game.winners.find(w => w.playerId === player.id);
          return (
          <Seat
            key={player.id}
            player={player}
            position={seatPositions[idx]}
            isActive={idx === game.activePlayerIndex && !game.isHandComplete}
            isHero={player.isHuman}
            showCards={player.isHuman || game.street === 'showdown'}
            isThinking={isAIThinking && idx === game.activePlayerIndex}
            isWinner={game.winners.some(w => w.playerId === player.id)}
            lastAction={lastActions.get(player.id) ?? null}
            winningHand={winnerEntry?.hand ?? null}
            isExploitTarget={exploitTargetId === player.id}
          />
          );
        })}
      </div>
    </div>
  );
};
