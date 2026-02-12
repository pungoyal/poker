import { Card } from './card';
import { ActionType } from './game';
import { DrawInfo } from './hand';

export interface OutsInfo {
  totalOuts: number;
  draws: DrawInfo[];
  outCards: Card[];
  improvementDescription: string;
}

export interface PotOddsInfo {
  potSize: number;
  costToCall: number;
  ratio: string;         // e.g. "3.5:1"
  percentage: number;    // e.g. 22.2
  isGettingOdds: boolean;
}

export interface EquityInfo {
  equity: number;        // 0-100 percentage
  wins: number;
  ties: number;
  losses: number;
  iterations: number;
}

export interface EVInfo {
  foldEV: number;
  callEV: number;
  raiseEV: number;
  bestAction: ActionType;
  reasoning: string;
}

export interface HandStrengthInfo {
  category: string;      // e.g. "Two Pair", "Flush"
  description: string;   // e.g. "Two Pair, Aces and Kings"
}

export interface StrategicContextInfo {
  preflopTier?: string;
  handNotation?: string;
  boardTexture?: string;
  spr?: number;
  heroStackBb?: number;
  averageStackBb?: number;
  playersRemaining?: number;
  heroRankByStack?: number;
  mRatio?: number;
  stackZone?: 'comfort' | 'pressure' | 'push-fold' | 'critical';
  pressureStage?: 'early' | 'middle' | 'late' | 'final-table' | 'bubble';
  shortStackPushFoldHint?: string;
}

export interface MathAnalysis {
  handStrength: HandStrengthInfo | null;
  outs: OutsInfo | null;
  potOdds: PotOddsInfo | null;
  equity: EquityInfo | null;
  ev: EVInfo | null;
  context: StrategicContextInfo | null;
}
