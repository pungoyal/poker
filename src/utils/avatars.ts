/** Creative emoji avatars for each player at the table */

const PLAYER_AVATARS: Record<string, string> = {
  'You':     '🎯',
  'Doyle':   '🤠',
  'Dwan':    '🦊',
  'Phil':    '😤',
  'Gus':     '🎲',
  'Patrik':  '🧊',
  'Vanessa': '🌹',
  'Daniel':  '🍁',
  'Maria':   '🔮',
};

/** Default avatar for unknown players */
const DEFAULT_AVATAR = '♟';

export function getPlayerAvatar(name: string): string {
  return PLAYER_AVATARS[name] || DEFAULT_AVATAR;
}
