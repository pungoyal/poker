import React from 'react';
import { Pot } from '../../types/game';
import './PotDisplay.css';

interface PotDisplayProps {
  pots: Pot[];
}

export const PotDisplay: React.FC<PotDisplayProps> = ({ pots }) => {
  const total = pots.reduce((sum, p) => sum + p.amount, 0);
  const [pulse, setPulse] = React.useState(false);
  const lastTotalRef = React.useRef(0);

  React.useEffect(() => {
    if (total !== lastTotalRef.current && total > 0) {
      lastTotalRef.current = total;
      setPulse(true);
      const t = setTimeout(() => setPulse(false), 260);
      return () => clearTimeout(t);
    }
  }, [total]);

  if (total === 0) return null;

  return (
    <div className={`pot-display ${pulse ? 'pot-display-pulse' : ''}`}>
      <div className="pot-chips">
        {[...Array(Math.min(5, Math.ceil(total / 100)))].map((_, i) => (
          <div key={i} className="pot-chip" style={{ transform: `translateX(${i * 3}px)` }} />
        ))}
      </div>
      <div className="pot-amount">
        Pot: ${total}
        {pots.length > 1 && (
          <span className="side-pots">
            {pots.map((p, i) => (
              <span key={i} className="side-pot">
                {i === 0 ? 'Main' : `Side ${i}`}: ${p.amount}
              </span>
            ))}
          </span>
        )}
      </div>
    </div>
  );
};
