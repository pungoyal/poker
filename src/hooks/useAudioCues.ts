import { useCallback, useEffect, useRef } from 'react';
import { useGameStore } from '../store';
import { ActionType, Street } from '../types/game';

function useBeep() {
  const ctxRef = useRef<AudioContext | null>(null);

  const getCtx = useCallback(() => {
    if (typeof window === 'undefined') return null;
    if (!ctxRef.current) {
      const Ctx = window.AudioContext || (window as typeof window & { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
      if (!Ctx) return null;
      ctxRef.current = new Ctx();
    }
    return ctxRef.current;
  }, []);

  const beep = useCallback((freq: number, durationMs: number, gainValue: number = 0.03, type: OscillatorType = 'sine') => {
    const ctx = getCtx();
    if (!ctx) return;
    const now = ctx.currentTime;
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();
    osc.type = type;
    osc.frequency.setValueAtTime(freq, now);
    gain.gain.setValueAtTime(0.0001, now);
    gain.gain.exponentialRampToValueAtTime(gainValue, now + 0.01);
    gain.gain.exponentialRampToValueAtTime(0.0001, now + durationMs / 1000);
    osc.connect(gain).connect(ctx.destination);
    osc.start(now);
    osc.stop(now + durationMs / 1000 + 0.03);
  }, [getCtx]);

  useEffect(() => {
    const wake = () => {
      const ctx = getCtx();
      if (ctx && ctx.state === 'suspended') {
        void ctx.resume();
      }
    };
    window.addEventListener('pointerdown', wake, { once: true });
    return () => window.removeEventListener('pointerdown', wake);
  }, [getCtx]);

  return beep;
}

export function useAudioCues() {
  const game = useGameStore(s => s.game);
  const isPaused = useGameStore(s => s.isPaused);
  const soundEnabled = useGameStore(s => s.soundEnabled);
  const beep = useBeep();

  const prevActionsLen = useRef(0);
  const prevStreet = useRef<Street | null>(null);
  const prevHandComplete = useRef(false);

  useEffect(() => {
    if (!soundEnabled || isPaused) return;
    if (game.actions.length <= prevActionsLen.current) return;
    prevActionsLen.current = game.actions.length;

    const last = game.actions[game.actions.length - 1];
    if (!last) return;
    if (last.type === ActionType.PostBlind) return;

    if (last.type === ActionType.Fold) beep(180, 60, 0.02, 'triangle');
    else if (last.type === ActionType.Check) beep(420, 50, 0.015);
    else if (last.type === ActionType.Call) beep(520, 70, 0.02);
    else if (last.type === ActionType.Bet || last.type === ActionType.Raise) {
      beep(700, 70, 0.025, 'square');
    } else if (last.type === ActionType.AllIn) {
      beep(860, 110, 0.04, 'sawtooth');
    }
  }, [game.actions, game.actions.length, soundEnabled, isPaused, beep]);

  useEffect(() => {
    if (!soundEnabled) return;
    if (game.street === prevStreet.current || game.handNumber === 0) return;
    prevStreet.current = game.street;

    if (game.street === Street.Flop) {
      beep(520, 90, 0.03);
      setTimeout(() => beep(620, 90, 0.03), 90);
    } else if (game.street === Street.Turn) {
      beep(640, 90, 0.03);
    } else if (game.street === Street.River) {
      beep(740, 110, 0.035);
    }
  }, [game.street, game.handNumber, soundEnabled, beep]);

  useEffect(() => {
    if (!soundEnabled) return;
    if (!game.isHandComplete || prevHandComplete.current) return;
    prevHandComplete.current = true;
    beep(760, 100, 0.03);
    setTimeout(() => beep(920, 140, 0.03), 130);
  }, [game.isHandComplete, soundEnabled, beep]);

  useEffect(() => {
    if (!game.isHandComplete) {
      prevHandComplete.current = false;
    }
  }, [game.isHandComplete, game.handNumber]);

}
