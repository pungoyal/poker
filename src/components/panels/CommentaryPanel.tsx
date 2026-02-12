import React, { useRef, useEffect } from 'react';
import { useGameStore } from '../../store';
import { CommentaryType } from '../../types/commentary';
import { getPlayerAvatar } from '../../utils/avatars';
import './CommentaryPanel.css';

const TYPE_ICONS: Record<CommentaryType, string> = {
  [CommentaryType.Situation]: '\u2660',
  [CommentaryType.Recommendation]: '\u2605',
  [CommentaryType.AIAction]: '\u265F',
  [CommentaryType.MathInsight]: '\u03A3',
  [CommentaryType.Showdown]: '\u2654',
  [CommentaryType.HandReview]: '\u270E',
  [CommentaryType.HandDivider]: '',
};

const TYPE_COLORS: Record<CommentaryType, string> = {
  [CommentaryType.Situation]: '#64b5f6',
  [CommentaryType.Recommendation]: '#ffd740',
  [CommentaryType.AIAction]: '#b0bec5',
  [CommentaryType.MathInsight]: '#81c784',
  [CommentaryType.Showdown]: '#ff8a65',
  [CommentaryType.HandReview]: '#ce93d8',
  [CommentaryType.HandDivider]: '#555',
};

export const CommentaryPanel: React.FC = () => {
  const commentary = useGameStore(s => s.commentary);
  const showPanel = useGameStore(s => s.showCommentaryPanel);
  const togglePanel = useGameStore(s => s.toggleCommentaryPanel);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [commentary.length]);

  return (
    <div className={`commentary-panel ${showPanel ? 'commentary-panel-open' : 'commentary-panel-closed'}`}>
      <div className="panel-header" onClick={togglePanel}>
        <span className="panel-icon">\u270E</span>
        <h3>Commentary</h3>
        <span className="panel-toggle">{showPanel ? '\u25BC' : '\u25B6'}</span>
      </div>

      {showPanel && (
        <div className="commentary-content" ref={scrollRef}>
          {commentary.length === 0 ? (
            <div className="panel-empty">Deal a hand to start...</div>
          ) : (
            commentary.map(entry => {
              if (entry.type === CommentaryType.HandDivider) {
                return (
                  <div key={entry.id} className="commentary-divider">
                    <span className="divider-line" />
                    <span className="divider-text">{entry.text}</span>
                    <span className="divider-line" />
                  </div>
                );
              }

              const icon = entry.playerName
                ? getPlayerAvatar(entry.playerName)
                : TYPE_ICONS[entry.type];

              return (
                <div
                  key={entry.id}
                  className={`commentary-entry ${entry.highlight ? 'commentary-highlight' : ''}`}
                >
                  <span
                    className="commentary-icon"
                    style={{ color: entry.playerName ? undefined : TYPE_COLORS[entry.type] }}
                  >
                    {icon}
                  </span>
                  <p className="commentary-text">{entry.text}</p>
                </div>
              );
            })
          )}
        </div>
      )}
    </div>
  );
};
