import React, { useMemo } from 'react';
import { useGameStore } from '../../store';
import './RivalSpotlightPanel.css';

export const RivalSpotlightPanel: React.FC = () => {
  const game = useGameStore(s => s.game);
  const opponentStats = useGameStore(s => s.opponentStats);

  const spotlight = useMemo(() => {
    const candidates = game.players
      .filter(p => !p.isHuman && p.stack > 0)
      .map(p => {
        const s = opponentStats[p.id];
        if (!s || s.hands < 5) return null;
        const vpip = (s.vpipHands / Math.max(1, s.hands)) * 100;
        const pfr = (s.pfrHands / Math.max(1, s.hands)) * 100;
        const af = s.passiveActions > 0 ? s.aggressiveActions / s.passiveActions : s.aggressiveActions;
        const score = vpip - pfr * 0.5 - af * 7;
        const confidence = Math.min(100, Math.round((s.hands / 24) * 100));
        const threeBetProxy = Math.max(0, pfr - 0.55 * vpip);
        return { id: p.id, name: p.name, vpip, pfr, af, score, confidence, hands: s.hands, threeBetProxy };
      })
      .filter(Boolean) as Array<{
        id: string;
        name: string;
        vpip: number;
        pfr: number;
        af: number;
        score: number;
        confidence: number;
        hands: number;
        threeBetProxy: number;
      }>;
    candidates.sort((a, b) => b.score - a.score);
    return candidates[0] ?? null;
  }, [game.players, opponentStats]);

  if (!spotlight) return null;

  let exploitLine = 'Play balanced, no clear leak yet.';
  let planLine = 'Collect more showdowns before deviating hard.';
  if (spotlight.vpip > 38 && spotlight.pfr < 16) {
    exploitLine = 'Loose-passive: isolate wider and value-bet bigger; bluff less.';
    planLine = 'Use larger value sizes on turn/river and cut pure bluffs versus this pool type.';
  } else if (spotlight.af > 2.8 && spotlight.pfr > 24) {
    exploitLine = 'Aggro reg: trap strong hands and bluff-catch thinner.';
    planLine = 'Tighten opening bottoms out of position and widen check-raise value traps.';
  } else if (spotlight.vpip < 18) {
    exploitLine = 'Nit profile: steal blinds more and overfold to 3-bets less.';
    planLine = 'Open wider from CO/BTN and apply turn pressure when they cap range.';
  } else if (spotlight.threeBetProxy > 9) {
    exploitLine = '3-bet heavy opponent: defend with stronger blockers and reduce marginal opens.';
    planLine = 'Favor 4-bet jam/value mixes with premium blockers and avoid dominated flats.';
  }

  return (
    <div className="rival-panel">
      <div className="rival-header">
        <span className="rival-dot" />
        <h3>Rival Spotlight</h3>
      </div>
      <div className="rival-body">
        <div className="rival-name">{spotlight.name}</div>
        <div className="rival-stats">
          VPIP {spotlight.vpip.toFixed(0)} / PFR {spotlight.pfr.toFixed(0)} / AF {spotlight.af.toFixed(1)} / Hands {spotlight.hands}
        </div>
        <div className="rival-confidence">Read Confidence: {spotlight.confidence}%</div>
        <div className="rival-advice">{exploitLine}</div>
        <div className="rival-plan">{planLine}</div>
      </div>
    </div>
  );
};
