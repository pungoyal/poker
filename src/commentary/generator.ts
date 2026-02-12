import { Card, RANK_SYMBOLS, SUIT_SYMBOLS, Rank } from '../types/card';
import { GameState, ActionType, Street } from '../types/game';
import { Player, Position, AIPersonalityType } from '../types/player';
import { CommentaryEntry, CommentaryType } from '../types/commentary';
import { HandEvalResult } from '../types/hand';
import { MathAnalysis } from '../types/odds';
import { SessionStats } from '../types/stats';

let commentaryId = 0;
function nextId(): string {
  return `c-${++commentaryId}`;
}

function cardStr(card: Card): string {
  return `${RANK_SYMBOLS[card.rank]}${SUIT_SYMBOLS[card.suit]}`;
}

function cardsStr(cards: Card[]): string {
  return cards.map(cardStr).join(' ');
}

function positionName(pos: Position): string {
  switch (pos) {
    case Position.Button: return 'the button';
    case Position.SmallBlind: return 'the small blind';
    case Position.BigBlind: return 'the big blind';
    case Position.UTG: return 'under the gun';
    case Position.UTG1: return 'UTG+1';
    case Position.MP: return 'middle position';
    case Position.LJ: return 'the lojack';
    case Position.HJ: return 'the hijack';
    case Position.CO: return 'the cutoff';
    default: return pos;
  }
}

function personalityStyle(type?: AIPersonalityType): string {
  switch (type) {
    case AIPersonalityType.TAG: return 'tight-aggressive';
    case AIPersonalityType.LAG: return 'loose-aggressive';
    case AIPersonalityType.Nit: return 'nitty';
    case AIPersonalityType.Maniac: return 'maniacal';
    case AIPersonalityType.CallingStation: return 'calling station';
    default: return '';
  }
}

// --- Hand Divider ---

export function generateHandDivider(handNumber: number): CommentaryEntry {
  return {
    id: nextId(),
    type: CommentaryType.HandDivider,
    street: Street.Preflop,
    text: `Hand #${handNumber}`,
    timestamp: Date.now(),
  };
}

// --- Situation Commentary ---

export function generateDealCommentary(
  heroCards: [Card, Card],
  position: Position,
  street: Street
): CommentaryEntry {
  const hand = cardsStr(heroCards);
  const pos = positionName(position);

  const isPocket = heroCards[0].rank === heroCards[1].rank;
  const isSuited = heroCards[0].suit === heroCards[1].suit;
  const highCard = Math.max(heroCards[0].rank, heroCards[1].rank);
  const lowCard = Math.min(heroCards[0].rank, heroCards[1].rank);
  const gap = highCard - lowCard;

  let handDescription = '';
  if (isPocket) {
    if (highCard >= Rank.Jack) handDescription = 'a premium pocket pair';
    else if (highCard >= Rank.Seven) handDescription = 'a middle pocket pair';
    else handDescription = 'a small pocket pair';
  } else if (highCard === Rank.Ace && lowCard >= Rank.King) {
    handDescription = isSuited ? 'Big Slick suited — a premium holding' : 'Big Slick offsuit — a strong hand';
  } else if (highCard === Rank.Ace) {
    handDescription = isSuited ? 'a suited ace — playable with position' : 'an ace with a weak kicker';
  } else if (isSuited && gap <= 2 && highCard >= Rank.Nine) {
    handDescription = 'suited connectors — great implied odds potential';
  } else if (isSuited && gap <= 2) {
    handDescription = 'small suited connectors — speculative but playable';
  } else if (highCard >= Rank.Jack && lowCard >= Rank.Ten) {
    handDescription = 'broadway cards — solid playability';
  } else {
    handDescription = 'a marginal holding';
  }

  const text = `You're dealt [${hand}] in ${pos}. You're looking at ${handDescription}.`;

  return {
    id: nextId(),
    type: CommentaryType.Situation,
    street,
    text,
    timestamp: Date.now(),
  };
}

export function generateFlopCommentary(
  _heroCards: [Card, Card],
  communityCards: Card[],
  handResult: HandEvalResult,
  street: Street
): CommentaryEntry {
  const board = cardsStr(communityCards);
  const texts: string[] = [];

  texts.push(`Flop comes [${board}].`);

  // Board texture
  const suits = communityCards.map(c => c.suit);
  const isMonotone = suits[0] === suits[1] && suits[1] === suits[2];
  const isTwoTone = new Set(suits).size === 2;
  const isRainbow = new Set(suits).size === 3;

  const ranks = communityCards.map(c => c.rank).sort((a, b) => b - a);
  const isConnected = ranks[0] - ranks[2] <= 4;

  let texture = '';
  if (isMonotone) texture = 'Monotone board — flush draws are live.';
  else if (isTwoTone && isConnected) texture = 'Wet, two-tone board — draws galore.';
  else if (isRainbow && !isConnected) texture = 'Dry, rainbow board — made hands dominate here.';
  else if (isTwoTone) texture = 'Two-tone board — watch for flush draws.';
  else texture = 'Rainbow board — draws are limited.';

  texts.push(texture);
  texts.push(`You've flopped ${handResult.description.toLowerCase()}.`);

  return {
    id: nextId(),
    type: CommentaryType.Situation,
    street,
    text: texts.join(' '),
    timestamp: Date.now(),
  };
}

export function generateStreetCommentary(
  newCard: Card,
  handResult: HandEvalResult,
  street: Street
): CommentaryEntry {
  const streetName = street === Street.Turn ? 'Turn' : 'River';
  const text = `${streetName} is the [${cardStr(newCard)}]. Your hand is now ${handResult.description.toLowerCase()}.`;

  return {
    id: nextId(),
    type: CommentaryType.Situation,
    street,
    text,
    timestamp: Date.now(),
  };
}

// --- Hero Action Commentary ---

export function generateHeroActionCommentary(
  action: ActionType,
  amount: number,
  street: Street,
): CommentaryEntry {
  let text = '';
  switch (action) {
    case ActionType.Fold:
      text = 'You fold.';
      break;
    case ActionType.Check:
      text = 'You check.';
      break;
    case ActionType.Call:
      text = `You call $${amount}.`;
      break;
    case ActionType.Bet:
      text = `You bet $${amount}.`;
      break;
    case ActionType.Raise:
      text = `You raise to $${amount}.`;
      break;
    case ActionType.AllIn:
      text = `You go all-in for $${amount}!`;
      break;
  }

  return {
    id: nextId(),
    type: CommentaryType.Situation,
    street,
    text,
    timestamp: Date.now(),
    playerId: 'hero',
    playerName: 'You',
  };
}

// --- AI Action Commentary ---

export function generateAIActionCommentary(
  player: Player,
  action: ActionType,
  amount: number,
  street: Street,
): CommentaryEntry {
  const name = player.name;
  const pos = positionName(player.position);
  const style = personalityStyle(player.personality);

  let actionText = '';
  switch (action) {
    case ActionType.Fold:
      actionText = `${name} folds from ${pos}.`;
      break;
    case ActionType.Check:
      actionText = `${name} checks from ${pos}.`;
      break;
    case ActionType.Call:
      actionText = `${name} calls $${amount} from ${pos}.`;
      break;
    case ActionType.Bet:
      actionText = `${name} leads out for $${amount} from ${pos}.`;
      break;
    case ActionType.Raise:
      actionText = `${name} raises to $${amount} from ${pos}.`;
      break;
    case ActionType.AllIn:
      actionText = `${name} shoves all-in for $${amount}! ${style ? `Classic ${style} move.` : ''}`;
      break;
  }

  const flavorTexts: Record<string, string[]> = {
    [AIPersonalityType.TAG]: [
      'This is consistent with a strong, considered range.',
      'Selective and purposeful — respect this action.',
      'A disciplined play from a solid player.',
    ],
    [AIPersonalityType.LAG]: [
      'Could be anything — wide range territory.',
      'Applying maximum pressure as always.',
      'The range is polarized here — nuts or air.',
    ],
    [AIPersonalityType.Nit]: [
      'When this player bets, you\'d better have the goods.',
      'Alarm bells — this player only shows up with premiums.',
      'Proceed with extreme caution.',
    ],
    [AIPersonalityType.Maniac]: [
      'Take this with a grain of salt — this player fires at everything.',
      'Unhinged aggression — exploit with patience.',
      'Could literally be any two cards.',
    ],
    [AIPersonalityType.CallingStation]: [
      'Don\'t try to bluff this player — they\'re calling.',
      'Sticky as glue — bet for value only.',
      'This player will call you down with bottom pair.',
    ],
  };

  let flavor = '';
  if (player.personality && action !== ActionType.Fold && action !== ActionType.Check) {
    const options = flavorTexts[player.personality] || [];
    if (options.length > 0) {
      flavor = ' ' + options[Math.floor(Math.random() * options.length)];
    }
  }

  return {
    id: nextId(),
    type: CommentaryType.AIAction,
    street,
    text: actionText + flavor,
    timestamp: Date.now(),
    playerId: player.id,
    playerName: player.name,
  };
}

// --- Math Insight Commentary ---

export function generateMathCommentary(
  analysis: MathAnalysis,
  street: Street
): CommentaryEntry | null {
  const parts: string[] = [];

  if (analysis.context?.preflopTier && analysis.context.handNotation && street === Street.Preflop) {
    parts.push(`Preflop read: ${analysis.context.handNotation} is a ${analysis.context.preflopTier.toLowerCase()}-tier holding.`);
  }

  if (analysis.context?.spr != null && street !== Street.Preflop) {
    parts.push(`SPR is ${analysis.context.spr.toFixed(1)}.${analysis.context.boardTexture ? ` ${analysis.context.boardTexture}.` : ''}`);
  }

  if (analysis.outs && analysis.outs.totalOuts > 0) {
    const outsDesc = analysis.outs.draws.map(d => d.description).join(', ');
    parts.push(`You have ${analysis.outs.totalOuts} outs: ${outsDesc}.`);

    // Rule of 2 and 4
    const multiplier = street === Street.Flop ? 4 : 2;
    const approxEquity = analysis.outs.totalOuts * multiplier;
    parts.push(`Rule of ${multiplier}: ~${approxEquity}% chance to improve.`);
  }

  if (analysis.potOdds) {
    const po = analysis.potOdds;
    parts.push(`Pot odds: ${po.ratio} (${po.percentage.toFixed(1)}%).`);

    if (analysis.equity) {
      if (analysis.equity.equity > po.percentage) {
        parts.push(`Your equity (${analysis.equity.equity.toFixed(1)}%) exceeds pot odds — mathematically a call is profitable.`);
      } else {
        parts.push(`Your equity (${analysis.equity.equity.toFixed(1)}%) is below pot odds — calling is -EV long-term.`);
      }
    }
  }

  if (analysis.ev) {
    const ev = analysis.ev;
    parts.push(`EV: Fold = $${ev.foldEV.toFixed(0)}, Call = $${ev.callEV.toFixed(0)}, Raise = $${ev.raiseEV.toFixed(0)}. ${ev.reasoning}`);
  }

  if (parts.length === 0) return null;

  return {
    id: nextId(),
    type: CommentaryType.MathInsight,
    street,
    text: parts.join(' '),
    timestamp: Date.now(),
    highlight: true,
  };
}

// --- Recommendation Commentary ---

export function generateRecommendation(
  analysis: MathAnalysis,
  street: Street
): CommentaryEntry | null {
  if (!analysis.ev) return null;

  const ev = analysis.ev;
  let text = '';

  switch (ev.bestAction) {
    case ActionType.Fold:
      text = `The math says fold. You're not getting the right price to continue here. Save your chips for a better spot.`;
      break;
    case ActionType.Call:
      text = `Calling is the correct play. Your pot odds justify a call, and you have enough equity to make this profitable long-term.`;
      break;
    case ActionType.Raise:
      text = `Consider raising here. You have enough equity to build the pot, and aggression applies pressure on marginal holdings.`;
      break;
    case ActionType.Check:
      text = `Checking is optimal here. You can pot-control with a marginal hand and avoid bloating the pot out of position.`;
      break;
    default:
      text = `${ev.reasoning}`;
  }

  return {
    id: nextId(),
    type: CommentaryType.Recommendation,
    street,
    text,
    timestamp: Date.now(),
    highlight: true,
  };
}

// --- Showdown Commentary ---

export function generateShowdownCommentary(
  winners: { playerId: string; amount: number; hand?: string }[],
  players: Player[],
  street: Street
): CommentaryEntry {
  const parts: string[] = [];

  for (const winner of winners) {
    const player = players.find(p => p.id === winner.playerId);
    const name = player?.isHuman ? 'You' : (player?.name || 'Unknown');
    const verb = player?.isHuman ? 'take' : 'takes';

    if (winner.hand === 'Everyone folded') {
      parts.push(`${name} ${verb} down the pot ($${winner.amount}) uncontested. No showdown needed.`);
    } else {
      parts.push(`${name} ${verb} the pot ($${winner.amount}) with ${winner.hand?.toLowerCase()}.`);
    }
  }

  return {
    id: nextId(),
    type: CommentaryType.Showdown,
    street,
    text: parts.join(' '),
    timestamp: Date.now(),
    highlight: true,
  };
}

// --- Hand Review ---

export function generateHandReview(
  game: GameState,
  analysis: MathAnalysis | null,
  sessionStats?: SessionStats,
): CommentaryEntry {
  const hero = game.players.find(p => p.isHuman);
  const heroWon = game.winners.some(w => w.playerId === hero?.id);
  const totalPot = game.winners.reduce((sum, w) => sum + w.amount, 0);

  let review = '';
  if (heroWon) {
    review = `Nice hand! You scooped a $${totalPot} pot.`;
    if (analysis?.ev && analysis.ev.bestAction === ActionType.Fold) {
      review += ` Interesting — the math suggested folding earlier, but the result worked out. Remember: don't let results-oriented thinking override correct process.`;
    }
  } else {
    review = `You lost this hand.`;
    if (hero?.isFolded) {
      review += ` Good discipline on the fold — protecting your stack is key to long-term profitability.`;
    } else if (analysis?.ev && analysis.ev.callEV > 0) {
      review += ` Your call was mathematically correct even though you lost. That's poker — focus on the process, not the outcome. Over thousands of hands, +EV decisions print money.`;
    } else if (analysis?.ev && analysis.ev.callEV < 0) {
      review += ` The math was against you on this one. Consider tightening up in similar spots — your equity didn't justify the investment.`;
    }
  }

  if (sessionStats && sessionStats.handsPlayed >= 15) {
    const vpip = (sessionStats.vpipHands / Math.max(1, sessionStats.handsPlayed)) * 100;
    const pfrOverVpip = sessionStats.vpipHands > 0 ? sessionStats.pfrHands / sessionStats.vpipHands : 0;
    const sdWin = sessionStats.showdownsSeen > 0
      ? (sessionStats.showdownsWon / sessionStats.showdownsSeen) * 100
      : 0;

    if (sessionStats.netChips < 0 && vpip > 38) {
      review += ' Leak alert: your VPIP is very high for current results. Tighten marginal opens/calls.';
    } else if (sessionStats.netChips < 0 && pfrOverVpip < 0.45) {
      review += ' Leak alert: you are too passive preflop. Raise more of your playable range.';
    } else if (sessionStats.showdownsSeen >= 8 && sdWin < 35) {
      review += ' Leak alert: showdown win rate is low. Fold thinner bluff-catchers and value-bet stronger.';
    }
  }

  return {
    id: nextId(),
    type: CommentaryType.HandReview,
    street: Street.Showdown,
    text: review,
    timestamp: Date.now(),
    highlight: true,
  };
}
