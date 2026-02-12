import { AIPersonalityType } from '../../types/player';
import { AIProfile } from '../types';
import { profile as tagProfile } from './tag';
import { profile as lagProfile } from './lag';
import { profile as nitProfile } from './nit';
import { profile as maniacProfile } from './maniac';
import { profile as callingStationProfile } from './calling-station';

const PROFILES: Record<AIPersonalityType, AIProfile> = {
  [AIPersonalityType.TAG]: tagProfile,
  [AIPersonalityType.LAG]: lagProfile,
  [AIPersonalityType.Nit]: nitProfile,
  [AIPersonalityType.Maniac]: maniacProfile,
  [AIPersonalityType.CallingStation]: callingStationProfile,
};

export function getProfile(type: AIPersonalityType): AIProfile {
  return PROFILES[type];
}

export { tagProfile, lagProfile, nitProfile, maniacProfile, callingStationProfile };
