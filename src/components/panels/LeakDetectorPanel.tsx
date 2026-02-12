import React, { useMemo } from 'react';
import { useGameStore } from '../../store';
import './LeakDetectorPanel.css';

type LeakItem = {
  id: string;
  severity: 'high' | 'medium' | 'low';
  title: string;
  fix: string;
};

export const LeakDetectorPanel: React.FC = () => {
  const stats = useGameStore(s => s.sessionStats);
  const coachingScore = useGameStore(s => s.coachingScore);
  const majorMistakes = useGameStore(s => s.majorMistakes);
  const hands = stats.handsPlayed;

  const leaks = useMemo<LeakItem[]>(() => {
    if (hands < 8) {
      return [{
        id: 'sample',
        severity: 'low',
        title: 'Collect More Hands',
        fix: 'Need at least 8 hands for a reliable leak read. Keep playing and reviewing.',
      }];
    }

    const vpip = (stats.vpipHands / Math.max(1, hands)) * 100;
    const pfr = (stats.pfrHands / Math.max(1, hands)) * 100;
    const gap = vpip - pfr;
    const items: LeakItem[] = [];

    if (vpip > 36) {
      items.push({
        id: 'vpip-high',
        severity: 'high',
        title: 'Too Loose Preflop',
        fix: 'Tighten early-position opens and cut dominated offsuit broadways facing action.',
      });
    } else if (vpip < 16) {
      items.push({
        id: 'vpip-low',
        severity: 'medium',
        title: 'Too Tight Overall',
        fix: 'Open wider from CO/BTN and steal more when folded to in late position.',
      });
    }

    if (gap > 14) {
      items.push({
        id: 'passive-gap',
        severity: 'high',
        title: 'Call-Heavy / Passive Line',
        fix: 'Convert strong calls into raises. Target VPIP-PFR gap below 10 for balanced pressure.',
      });
    }

    if (majorMistakes >= 6 && coachingScore < 78) {
      items.push({
        id: 'ev-bleed',
        severity: 'high',
        title: 'High EV Bleed',
        fix: 'Slow down in big pots. Prioritize pot-odds + stack-zone checks before committing chips.',
      });
    } else if (majorMistakes >= 3) {
      items.push({
        id: 'mistakes-medium',
        severity: 'medium',
        title: 'Decision Quality Drift',
        fix: 'Use recommended sizing and avoid marginal hero calls against multi-street aggression.',
      });
    }

    if (items.length === 0) {
      items.push({
        id: 'clean',
        severity: 'low',
        title: 'Stable Fundamentals',
        fix: 'Keep current baseline. Focus next edge on exploit spots from Rival Spotlight.',
      });
    }

    return items.slice(0, 3);
  }, [coachingScore, hands, majorMistakes, stats.pfrHands, stats.vpipHands]);

  return (
    <div className="leak-panel">
      <div className="leak-header">
        <span className="leak-dot" />
        <h3>Leak Detector</h3>
      </div>
      <div className="leak-body">
        {leaks.map(leak => (
          <div key={leak.id} className={`leak-item leak-item-${leak.severity}`}>
            <div className="leak-title">{leak.title}</div>
            <div className="leak-fix">{leak.fix}</div>
          </div>
        ))}
      </div>
    </div>
  );
};
