import React from 'react';
import { useGameStore } from './store';
import { useGameFlow } from './hooks/useGameFlow';
import { useOddsCalculation } from './hooks/useOddsCalculation';
import { useAudioCues } from './hooks/useAudioCues';
import { Card, RANK_SYMBOLS, SUIT_SYMBOLS } from './types/card';
import { PokerTable } from './components/table/PokerTable';
import { ActionBar } from './components/controls/ActionBar';
import { HeroHUD } from './components/controls/HeroHUD';
import { CommentaryPanel } from './components/panels/CommentaryPanel';
import { HandReplayPanel } from './components/panels/HandReplayPanel';
import { RivalSpotlightPanel } from './components/panels/RivalSpotlightPanel';
import { LeakDetectorPanel } from './components/panels/LeakDetectorPanel';
import './App.css';

const App: React.FC = () => {
  const game = useGameStore(s => s.game);
  const settings = useGameStore(s => s.settings);
  const handHistory = useGameStore(s => s.handHistory);
  const resetSession = useGameStore(s => s.resetSession);
  const { startGame } = useGameFlow();
  useOddsCalculation();
  useAudioCues();

  const hasStarted = game.handNumber > 0;
  const [startingStack, setStartingStack] = React.useState(settings.startingStack);
  const [smallBlind, setSmallBlind] = React.useState(settings.smallBlind);
  const [bigBlind, setBigBlind] = React.useState(settings.bigBlind);
  const [numOpponents, setNumOpponents] = React.useState(settings.numOpponents);
  const [gameSpeed, setGameSpeed] = React.useState(settings.gameSpeed);
  const [aiSkill, setAiSkill] = React.useState(settings.aiSkill);
  const [gameMode, setGameMode] = React.useState(settings.gameMode);
  const [blindIncreaseEveryHands, setBlindIncreaseEveryHands] = React.useState(settings.blindIncreaseEveryHands);
  const [blindIncreaseFactor, setBlindIncreaseFactor] = React.useState(settings.blindIncreaseFactor);
  const [anteEnabled, setAnteEnabled] = React.useState(settings.anteEnabled);
  const [anteBb, setAnteBb] = React.useState(settings.anteBb);

  const tournamentOk = gameMode === 'cash' || (blindIncreaseEveryHands >= 2 && blindIncreaseFactor > 1);
  const anteOk = !anteEnabled || (anteBb >= 0 && anteBb <= 1);
  const canStart = smallBlind > 0 && bigBlind > smallBlind && numOpponents >= 1 && numOpponents <= 8 && startingStack > bigBlind * 10 && tournamentOk && anteOk;

  const exportHistory = React.useCallback(() => {
    if (handHistory.length === 0) return;
    const cardStr = (c: Card) => `${RANK_SYMBOLS[c.rank]}${SUIT_SYMBOLS[c.suit]}`;
    const payload = handHistory.map(h => ({
      handNumber: h.handNumber,
      endedAtISO: new Date(h.endedAt).toISOString(),
      heroCards: h.heroCards?.map(cardStr) ?? null,
      board: h.board.map(cardStr),
      pot: h.pot,
      playerNames: h.playerNames,
      heroNetChips: h.heroNetChips,
      winners: h.winners,
      actions: h.actions,
      marked: h.marked,
      coach: h.coach,
    }));
    const blob = new Blob([JSON.stringify(payload, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `poker-lab-history-${new Date().toISOString().slice(0, 19).replace(/:/g, '-')}.json`;
    a.click();
    URL.revokeObjectURL(url);
  }, [handHistory]);

  return (
    <div className="app">
      <header className="app-header">
        <h1>Poker Lab</h1>
        <div className="header-right">
          <span className="hand-counter">
            {hasStarted ? `Hand #${game.handNumber}` : 'Ready to deal'}
          </span>
          {hasStarted && (
            <>
              <button className="header-btn" onClick={exportHistory} disabled={handHistory.length === 0}>
                Export Hands
              </button>
              <button className="header-btn header-btn-danger" onClick={resetSession}>
                Reset Session
              </button>
            </>
          )}
        </div>
      </header>

      <main className="app-main">
        <div className="center-area">
          {!hasStarted ? (
            <div className="start-screen">
              <div className="start-content">
                <h2>No-Limit Texas Hold'em</h2>
                <p>{gameMode === 'cash' ? 'Cash Game' : 'Tournament'} | 9-Max</p>
                <p className="start-desc">
                  Play against 8 AI opponents with distinct personalities.
                  Every decision point shows you the math: outs, pot odds, equity, and EV.
                </p>

                <div className="setup-grid">
                  <label className="setup-field">
                    <span>Starting Stack</span>
                    <input
                      type="number"
                      min={200}
                      step={50}
                      value={startingStack}
                      onChange={e => setStartingStack(Number(e.target.value) || 0)}
                    />
                  </label>
                  <label className="setup-field">
                    <span>Small Blind</span>
                    <input
                      type="number"
                      min={1}
                      step={1}
                      value={smallBlind}
                      onChange={e => setSmallBlind(Number(e.target.value) || 0)}
                    />
                  </label>
                  <label className="setup-field">
                    <span>Big Blind</span>
                    <input
                      type="number"
                      min={2}
                      step={1}
                      value={bigBlind}
                      onChange={e => setBigBlind(Number(e.target.value) || 0)}
                    />
                  </label>
                  <label className="setup-field">
                    <span>Opponents</span>
                    <input
                      type="number"
                      min={1}
                      max={8}
                      step={1}
                      value={numOpponents}
                      onChange={e => setNumOpponents(Number(e.target.value) || 0)}
                    />
                  </label>
                  <label className="setup-field setup-field-wide">
                    <span>Game Mode</span>
                    <select value={gameMode} onChange={e => setGameMode(e.target.value as typeof gameMode)}>
                      <option value="cash">Cash</option>
                      <option value="tournament">Tournament</option>
                    </select>
                  </label>
                  {gameMode === 'tournament' && (
                    <>
                      <label className="setup-field">
                        <span>Blind Level (Hands)</span>
                        <input
                          type="number"
                          min={2}
                          max={50}
                          step={1}
                          value={blindIncreaseEveryHands}
                          onChange={e => setBlindIncreaseEveryHands(Number(e.target.value) || 2)}
                        />
                      </label>
                      <label className="setup-field">
                        <span>Blind Multiplier</span>
                        <input
                          type="number"
                          min={1.1}
                          max={3}
                          step={0.1}
                          value={blindIncreaseFactor}
                          onChange={e => setBlindIncreaseFactor(Number(e.target.value) || 1.5)}
                        />
                      </label>
                    </>
                  )}
                  <label className="setup-field setup-field-wide">
                    <span>Game Speed</span>
                    <select value={gameSpeed} onChange={e => setGameSpeed(e.target.value as typeof gameSpeed)}>
                      <option value="slow">Slow</option>
                      <option value="normal">Normal</option>
                      <option value="fast">Fast</option>
                    </select>
                  </label>
                  <label className="setup-field setup-field-wide">
                    <span>AI Skill</span>
                    <select value={aiSkill} onChange={e => setAiSkill(e.target.value as typeof aiSkill)}>
                      <option value="novice">Novice</option>
                      <option value="standard">Standard</option>
                      <option value="elite">Elite</option>
                    </select>
                  </label>
                  {gameMode === 'tournament' && (
                    <>
                      <label className="setup-field">
                        <span>Antes</span>
                        <select value={anteEnabled ? 'on' : 'off'} onChange={e => setAnteEnabled(e.target.value === 'on')}>
                          <option value="on">On</option>
                          <option value="off">Off</option>
                        </select>
                      </label>
                      <label className="setup-field">
                        <span>Ante (BB)</span>
                        <input
                          type="number"
                          min={0}
                          max={1}
                          step={0.05}
                          value={anteBb}
                          onChange={e => setAnteBb(Number(e.target.value) || 0)}
                        />
                      </label>
                    </>
                  )}
                </div>

                <button
                  className="start-btn"
                  disabled={!canStart}
                  onClick={() =>
                    startGame({
                      startingStack,
                      smallBlind,
                      bigBlind,
                      numOpponents,
                      gameSpeed,
                      aiSkill,
                      gameMode,
                      blindIncreaseEveryHands,
                      blindIncreaseFactor,
                      anteEnabled,
                      anteBb,
                    })
                  }
                >
                  Deal Me In
                </button>
                {!canStart && (
                  <div className="start-error">
                    Set valid blinds/stacks and tournament parameters before starting.
                  </div>
                )}
                <div className="shortcut-help">
                  Keyboard: <kbd>F</kbd> Fold <kbd>C</kbd> Call/Check <kbd>R</kbd> Raise <kbd>Space</kbd> Pause
                </div>
              </div>
            </div>
          ) : (
            <>
              <PokerTable />
              <HeroHUD />
              <ActionBar />
            </>
          )}
        </div>

        <div className="right-panel">
          <RivalSpotlightPanel />
          <LeakDetectorPanel />
          <CommentaryPanel />
          <HandReplayPanel />
        </div>
      </main>
    </div>
  );
};

export default App;
