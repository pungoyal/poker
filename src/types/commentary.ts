import { Street } from './game';

export enum CommentaryType {
  Situation = 'situation',
  Recommendation = 'recommendation',
  AIAction = 'ai-action',
  MathInsight = 'math-insight',
  Showdown = 'showdown',
  HandReview = 'hand-review',
  HandDivider = 'hand-divider',
}

export interface CommentaryEntry {
  id: string;
  type: CommentaryType;
  street: Street;
  text: string;
  timestamp: number;
  highlight?: boolean;
  playerId?: string;
  playerName?: string;
}
