// The page: draws the view the engine sends, and does what the engine leaves
// to its host: timing, storage, keys and the call to the language model.

import { askClaude } from './claude.js';
import * as hands from './hands.js';

const $ = (id) => document.getElementById(id);
const app = $('app');

const RANKS = 'AKQJT98765432';
const SUITS = { s: '♠', h: '♥', d: '♦', c: '♣' };
const GRADE_ICON = { good: '✓', close: '≈', inaccuracy: '?', mistake: '✗', blunder: '✗✗', note: '·' };
const GRADE_NAMES = [
  ['good', 'good', 'good'], ['close', 'close', 'close'], ['inaccuracy', 'inaccurate', 'inaccurate'],
  ['mistake', 'mistake', 'mistakes'], ['blunder', 'blunder', 'blunders'],
];
// Finished hands live in IndexedDB (hands.js); poker.history is where older
// versions kept them in localStorage, read once to move them over.
const STORE = { save: 'poker.save', history: 'poker.history', settings: 'poker.settings' };
const HISTORY_KEPT = 3000;
const DRILL_FROM = 300;
const DRILL_SPOTS = 10;
const STREETS = ['Preflop', 'Flop', 'Turn', 'River'];

// ---- state ----------------------------------------------------------------

const S = {
  view: null,
  info: null,
  settings: { fourColour: true, fast: false, apiKey: '', model: '', ...load(STORE.settings, {}) },
  history: [],        // finished hands, oldest first, as stored by hands.js
  elsewhere: false,   // another tab has taken over the session
  showNumbers: false,
  bet: null,          // the raise amount chosen, in chips
  betFor: '',         // the decision it was chosen for
  toggled: new Map(), // reviews opened or closed by hand
  streams: new Map(), // replies from Claude as they arrive, by feed id
  busy: 0,
  drawn: { hand: null, board: [], shown: false, feed: new Map(), sections: {} },
  botTimer: null,
  summaryId: null,
};

function load(key, fallback) {
  try {
    const text = localStorage.getItem(key);
    return text ? JSON.parse(text) : fallback;
  } catch {
    return fallback;
  }
}

function store(key, value) {
  try {
    if (value === null) localStorage.removeItem(key);
    else localStorage.setItem(key, JSON.stringify(value));
  } catch {
    toast('This browser is not letting the page save: progress will be lost on reload.');
  }
}

// ---- words and numbers ----------------------------------------------------

const esc = (text) => String(text ?? '').replace(/[&<>"']/g, (c) => (
  { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

/** A number of big blinds as text: "2.5bb". */
function bb(x, signed = false) {
  let text = x.toFixed(2).replace(/\.?0+$/, '');
  if (text === '-0') text = '0';
  const sign = signed && x > 0 && text !== '0' ? '+' : '';
  return `${sign}${text.replace('-', '−')}bb`;
}
const chips = (n, signed) => bb(n / S.view.bb, signed);
const pct = (x) => `${Math.round(x * 100)}%`;
const plural = (n, word) => `${n} ${word}${n === 1 ? '' : 's'}`;
const minus = (text) => esc(text).replace('-', '−');
const sign = (x) => (x > 0 ? 'pos' : x < 0 ? 'neg' : 'zero');

function cardHTML(code, cls = '', i = 0) {
  const [rank, suit] = [code[0] === 'T' ? '10' : code[0], code[1]];
  return `<span class="card s-${suit} ${cls}" style="--i:${i}" role="img" aria-label="${rank}${SUITS[suit]}">` +
    `<b>${rank}</b><i>${SUITS[suit]}</i></span>`;
}
const miniCards = (codes) => codes.map((c) =>
  `<span class="mini s-${c[1]}">${c[0] === 'T' ? '10' : c[0]}${SUITS[c[1]]}</span>`).join('');

// ---- the engine -----------------------------------------------------------

const waiting = new Map();
let worker = null;
let sent = 0;
let engineReady;
let crashes = 0;

/** Starts the engine in a fresh worker. Commands still waiting on the old
 *  one are answered with an error. */
function startEngine() {
  worker?.terminate();
  for (const resolve of waiting.values()) resolve({ error: 'the engine was restarted', dropped: true });
  waiting.clear();
  const w = new Worker(new URL('./worker.js', import.meta.url), { type: 'module' });
  worker = w;
  engineReady = new Promise((resolve, reject) => {
    w.onmessage = ({ data }) => {
      if (data.ready) return resolve();
      if (data.fatal) return reject(new Error(data.fatal));
      const reply = data.crashed ? { ...data.reply, crashed: true } : data.reply;
      waiting.get(data.id)?.(reply);
      waiting.delete(data.id);
    };
    w.onerror = (event) => reject(new Error(event.message || 'the engine failed to load'));
  });
}

function send(cmd) {
  return new Promise((resolve) => {
    waiting.set(++sent, resolve);
    worker.postMessage({ id: sent, cmd });
  });
}

/** After the engine panicked, which leaves it unable to run anything more:
 *  a new one, back at the last save. A second crash in a row gives up on
 *  the save, a third on the table. */
async function restartEngine() {
  crashes++;
  clearTimeout(S.botTimer);
  startEngine();
  try {
    await engineReady;
  } catch (error) {
    toast(`The table could not be set up again (${error.message}). Reload the page.`);
    return;
  }
  if (crashes > 2) {
    toast('The table keeps failing. Reload the page to try again.');
    return;
  }
  if (crashes > 1) {
    store(STORE.save, null);
    toast('The table failed again on your saved session, so a new one has started.');
  } else {
    toast('The table hit an error and was set up again from your last save.');
  }
  await resume();
}

/** Runs a command and draws what comes back. */
async function run(cmd, { quiet = false } = {}) {
  if (S.elsewhere) return { error: 'This session is open in another tab.' };
  S.busy++;
  app.classList.add('busy');
  try {
    const reply = await send(cmd);
    if (reply.crashed) {
      await restartEngine();
      return reply;
    }
    if (!reply.dropped) crashes = 0;
    if (reply.error && !quiet && !reply.dropped) toast(reply.error);
    if (reply.entry) {
      S.history.push(reply.entry);
      S.history.splice(0, Math.max(0, S.history.length - HISTORY_KEPT));
      // The hand is stored before the save that counts it, so a save never
      // claims a hand that is not there.
      try {
        await hands.add([reply.entry], HISTORY_KEPT);
        tabs?.postMessage({ hands: true });
      } catch {
        toast('This browser is not letting the page keep finished hands.');
      }
    }
    if ('save' in reply) {
      store(STORE.save, reply.save);
      if (reply.save) tabs?.postMessage({ save: true });
    }
    if (reply.view) await show(reply.view);
    return reply;
  } finally {
    S.busy--;
    app.classList.toggle('busy', S.busy > 0);
  }
}

// ---- other tabs -----------------------------------------------------------

// Only one tab can play the saved session: when another saves it, this one
// steps aside instead of overwriting it with a stale copy.
const tabs = 'BroadcastChannel' in self ? new BroadcastChannel('poker') : null;
if (tabs) {
  tabs.onmessage = async ({ data }) => {
    if (data.hands) S.history = await hands.all();
    if (data.save && !S.elsewhere) {
      S.elsewhere = true;
      clearTimeout(S.botTimer);
      for (const dialog of document.querySelectorAll('dialog[open]')) dialog.close();
      $('dlg-elsewhere').showModal();
    }
  };
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const calm = matchMedia('(prefers-reduced-motion: reduce)');

/** Draws a view. When an all-in runs the board out at once, the streets are
 *  dealt one after another before the result is shown. */
async function show(view) {
  const before = S.drawn.hand === handKey(view) ? S.drawn.board.length : 0;
  const board = view.table.board;
  if (view.outcome && board.length > before && before < 5 && !calm.matches && S.view) {
    const stops = [3, 4, 5].filter((n) => n > before && n <= board.length);
    for (const n of stops) {
      draw({ ...view, phase: 'runout', outcome: null, legal: null,
        table: { ...view.table, board: board.slice(0, n), bot: { ...view.table.bot, cards: null, rank: null },
          hero: { ...view.table.hero, rank: null } } });
      await sleep(n === 3 ? 900 : 800);
    }
  }
  draw(view);
  afterDraw(view);
}

function afterDraw(view) {
  clearTimeout(S.botTimer);
  if (view.phase === 'bot') {
    S.botTimer = setTimeout(() => run({ cmd: 'bot' }), S.settings.fast || calm.matches ? 120 : 750);
  }
  if (view.phase === 'over' && view.report && !$('dlg-report').open) openReport();
}

const handKey = (view) => `${view.kind}:${view.seed ?? ''}:${view.hand_no}:${view.drill?.index ?? ''}`;
const decisionKey = (view) => `${handKey(view)}:${view.table.log.join('|').length}`;

// ---- drawing --------------------------------------------------------------

/** Replaces a section's markup only when it changed. */
function put(id, html) {
  if (S.drawn.sections[id] === html) return false;
  S.drawn.sections[id] = html;
  $(id).innerHTML = html;
  return true;
}

function draw(view) {
  const first = !S.view;
  const newDecision = !S.view || decisionKey(view) !== decisionKey(S.view) || view.phase !== S.view.phase;
  if (first || view.mode !== S.view.mode) S.showNumbers = view.mode === 'full';
  S.view = view;
  app.dataset.phase = view.phase;
  app.dataset.kind = view.kind;
  app.dataset.mode = view.mode;
  app.classList.toggle('two-colour', !S.settings.fourColour);
  if ($('mode').value !== view.mode) $('mode').value = view.mode;
  $('mode').disabled = view.kind !== 'play' || view.phase === 'over';

  if (view.legal?.raise && (newDecision || S.betFor !== decisionKey(view))) {
    const presets = view.legal.raise.presets;
    S.bet = presets[Math.min(1, presets.length - 1)].amount;
    S.betFor = decisionKey(view);
  }
  drawBar(view);
  drawFelt(view);
  drawDock(view);
  drawFeed(view);
  drawRange(view);
  drawOpponent(view);
  drawLog(view);
  drawSession(view);
}

function drawBar(view) {
  let html;
  if (view.kind === 'drill') {
    const d = view.drill;
    html = `<span class="stat"><b>Drill</b> spot ${d.index} of ${d.spots}</span>` +
      `<span class="stat hide-s">${d.correct} played well</span>`;
  } else {
    const s = view.session;
    html = `<span class="stat">Hand <b>${view.hand_no}</b></span>` +
      `<span class="stat hide-s">vs <b>${esc(view.opponent.label)}</b></span>` +
      `<span class="stat ${sign(s.result)}" title="Your result over finished hands"><b>${bb(s.result, true)}</b></span>`;
    if (s.reviewed > 0) {
      html += `<span class="stat hide-s" title="Chips given up to mistakes, by the coach's estimate">` +
        `mistakes cost <b class="cost">${bb(s.given_up)}</b>` +
        (s.per_100 != null ? ` <span class="hide-m">· ${Math.round(s.per_100)}bb per 100 hands</span>` : '') + `</span>`;
    }
  }
  put('bar-stats', html);
}

function drawFelt(view) {
  const t = view.table;
  const key = handKey(view);
  const fresh = S.drawn.hand !== key;
  const oldBoard = fresh ? [] : S.drawn.board;
  const o = view.outcome;
  const over = view.phase === 'over';
  const turn = view.phase === 'bot' ? 'bot' : view.phase === 'hero' ? 'hero' : null;
  const said = (who) => (t.last?.who === who && !o
    ? `<div class="say" key="${esc(t.last.text)}">${esc(t.last.text.replace(/^(You|Bot) /, '').replace(/\.$/, ''))}</div>` : '');
  const stack = (seat) => (seat.stack === 0 && !o ? '<b class="allin">all-in</b>' : `<b>${chips(seat.stack)}</b>`);
  const button = (seat) => (seat.button
    ? '<span class="seatpos is-btn" title="Button: posts the small blind, acts first before the flop and last after it">BTN</span>'
    : '<span class="seatpos" title="Big blind: acts last before the flop and first after it">BB</span>');
  const bet = (seat, who, extra = '') => `<div class="bet bet-${who}">` +
    (seat.bet > 0 && !o ? `<span class="chip"><i></i>${chips(seat.bet)}</span>` : '') + said(who) + extra + `</div>`;

  const shown = Boolean(t.bot.cards);
  const botCards = shown
    ? t.bot.cards.map((c, i) => cardHTML(c, S.drawn.shown && !fresh ? '' : 'flip', i)).join('')
    : (o && !o.showdown && o.winner === 'hero'
      ? '<span class="card back mucked"></span><span class="card back mucked"></span>'
      : `<span class="card back ${fresh ? 'deal' : ''}" style="--i:0"></span><span class="card back ${fresh ? 'deal' : ''}" style="--i:1"></span>`);
  const heroFolded = o && !o.showdown && o.winner === 'bot';
  const heroCards = t.hero.cards.map((c, i) => cardHTML(c, `${fresh ? 'deal' : ''} ${heroFolded ? 'mucked' : ''}`, i)).join('');

  const board = [0, 1, 2, 3, 4].map((i) => (t.board[i]
    ? cardHTML(t.board[i], oldBoard[i] === t.board[i] ? '' : 'deal', Math.max(0, i - oldBoard.length))
    : '<span class="card slot"></span>')).join('');

  let result = '';
  if (o) {
    const net = o.net / view.bb;
    const text = o.winner === 'split' ? 'Split pot' : o.winner === 'hero' ? `You win ${bb(net)}` : `You lose ${bb(-net)}`;
    result = `<div class="result ${sign(net)}">${text}</div>`;
  }
  const label = view.kind === 'drill' ? view.opponent.label : (view.opponent.hidden ? 'Mystery' : view.opponent.label);
  const name = esc(label[0].toUpperCase() + label.slice(1));
  const botWins = o && o.winner === 'bot';
  const heroWins = o && o.winner === 'hero';

  const html = `
    <div class="seat seat-bot ${turn === 'bot' ? 'is-turn' : ''} ${botWins ? 'is-winner' : ''}">
      <div class="seat-id">
        <span class="avatar" aria-hidden="true">${view.opponent.hidden ? '?' : name[0]}</span>
        <span class="who"><span class="name">${name}</span>${button(t.bot)}</span>
        <span class="stack">${stack(t.bot)}</span>
      </div>
      <div class="hole hole-bot">${botCards}</div>
    </div>
    ${bet(t.bot, 'bot', t.bot.rank ? `<span class="rank">${esc(t.bot.rank)}</span>`
      : (turn === 'bot' ? '<span class="thinking">thinking<i>.</i><i>.</i><i>.</i></span>' : ''))}
    <div class="middle">
      <div class="board" aria-label="Board">${board}</div>
      <div class="pot ${o ? 'is-over' : ''}">
        ${result || `<span class="pot-label">Pot</span><b key="${t.pot}">${chips(t.pot)}</b>`}
        <span class="street">${esc(t.street)}</span>
      </div>
    </div>
    ${bet(t.hero, 'hero')}
    <div class="seat seat-hero ${turn === 'hero' ? 'is-turn' : ''} ${heroWins ? 'is-winner' : ''}">
      <div class="hole hole-hero">${heroCards}</div>
      <div class="seat-id">
        <span class="who"><span class="name">You</span>${button(t.hero)}</span>
        <span class="stack">${stack(t.hero)}</span>
      </div>
      <div class="made">${esc(t.hero.rank ?? t.hero.hand)}</div>
    </div>` +
    (turn === 'hero' && view.mode !== 'off'
      ? `<button type="button" class="felt-peek" data-do="peek" aria-pressed="${S.showNumbers}" ` +
        `aria-label="${S.showNumbers ? 'Hide the numbers' : 'Peek at your equity and the best play'}">` +
        `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M2 12s3.6-6.5 10-6.5S22 12 22 12s-3.6 6.5-10 6.5S2 12 2 12z"/><circle cx="12" cy="12" r="2.8"/></svg></button>` : '');
  put('felt', html);
  S.drawn.hand = key;
  S.drawn.board = t.board.slice();
  S.drawn.shown = shown;
  void over;
}

/** The share of the pot a raise to `amount` bets. */
function share(legal, amount) {
  return (amount - legal.opp_commit) / (legal.pot + legal.to_call);
}

function raiseLabel(legal, amount) {
  const r = legal.raise;
  return amount >= r.max
    ? { main: `All-in ${chips(amount)}`, sub: '' }
    : { main: `${r.verb} ${chips(amount)}`, sub: `${pct(share(legal, amount))} pot` };
}

function drawDock(view) {
  const { legal, phase, outcome: o } = view;
  const coached = view.mode !== 'off';
  let strip = '';
  let sizing = '';
  let actions = '';
  const key = (k) => `<kbd>${k}</kbd>`;

  if (phase === 'hero' && legal) {
    const tiles = [];
    const shown = S.showNumbers;
    if (legal.to_call > 0) {
      tiles.push(['To call', chips(legal.to_call), `into ${chips(legal.pot)}`, 'basic']);
      // The price is a question the coach may be asking: teach mode keeps it back.
      if (view.mode !== 'teach' || shown) tiles.push(['Price', `${pct(legal.pot_odds)}+`, 'wins to call']);
    } else {
      tiles.push(['Pot', chips(legal.pot), 'free to check', 'basic']);
    }
    tiles.push(['Stacks', `${legal.stacks_to_pot.toFixed(1)}×`, 'the pot behind', 'basic']);
    if (coached && view.spot && shown) {
      const sp = view.spot;
      tiles.push(['Equity', pct(sp.equity), `vs ${sp.against}`]);
      if (sp.best) {
        const [, play, size] = sp.best.text.match(/^(.*?)(?: \((.*)\))?$/);
        tiles.push(['Best play', esc(play), `${size ? `${esc(size)} · ` : ''}${minus(sp.best.ev_text)}`, 'wide']);
      }
    }
    strip = tiles.map(([label, value, sub, cls]) =>
      `<div class="tile ${cls ?? ''}"><span class="tile-label">${label}</span><span class="tile-value">${value}</span>` +
      `<span class="tile-sub">${sub}</span></div>`).join('');
    if (coached) {
      strip += `<button type="button" class="tile tile-toggle" data-do="peek" aria-pressed="${shown}" ` +
        `title="Show or hide what the coach worked out (N)">` +
        (shown ? `<span class="tile-value">Hide</span>` : `<span class="tile-label">Equity · best play</span><span class="tile-value">Peek</span>`) +
        `${key('N')}</button>`;
    }

    const r = legal.raise;
    if (r) {
      const amount = Math.min(Math.max(S.bet ?? r.min, r.min), r.max);
      S.bet = amount;
      sizing = `<div class="presets" role="group" aria-label="Bet sizes">` +
        r.presets.map((p, i) => `<button type="button" class="preset ${p.amount === amount ? 'is-on' : ''}" ` +
          `data-amount="${p.amount}"><span>${p.label}</span><kbd>${i + 1}</kbd></button>`).join('') + `</div>` +
        `<input type="range" id="bet-slider" min="${r.min}" max="${r.max}" step="${r.step}" value="${amount}" ` +
        `aria-label="Bet size" aria-valuetext="${chips(amount)}" ${r.min === r.max ? 'disabled' : ''}>` +
        `<label class="amount"><input type="number" id="bet-input" inputmode="decimal" step="0.5" ` +
        `min="${r.min / view.bb}" max="${r.max / view.bb}" value="${amount / view.bb}" aria-label="Bet size in big blinds"><span>bb</span></label>`;
    }
    const calling = legal.to_call > 0;
    const label = r ? raiseLabel(legal, S.bet) : null;
    actions =
      `<button type="button" class="act act-fold" data-act="fold" ${calling ? '' : 'disabled'}>` +
        `<span class="act-main">Fold</span>${key('F')}</button>` +
      `<button type="button" class="act act-call" data-act="${calling ? 'call' : 'check'}">` +
        `<span class="act-main">${calling ? `Call ${chips(legal.to_call)}` : 'Check'}</span>` +
        (legal.call_all_in && calling ? '<span class="act-sub">all-in</span>' : '') + `${key('C')}</button>` +
      (r
        ? `<button type="button" class="act act-raise" data-act="raise" id="raise-button">` +
          `<span class="act-main" id="raise-main">${label.main}</span><span class="act-sub" id="raise-sub">${label.sub}</span>${key('R')}</button>`
        : `<button type="button" class="act act-raise" disabled><span class="act-main">Raise</span></button>`);
  } else if (phase === 'next') {
    if (view.kind === 'drill') {
      strip = `<div class="note">Compare your play with the options in the review.</div>`;
      actions = `<button type="button" class="act act-next" data-do="next"><span class="act-main">` +
        `${view.drill.index >= view.drill.spots ? 'Finish the drill' : 'Next spot'}</span>${key('Space')}</button>`;
    } else {
      const t = view.table;
      const net = o.net / view.bb;
      let text = o.winner === 'split' ? 'Split pot.'
        : o.winner === 'hero' ? `You win ${bb(net)}.` : `You lose ${bb(-net)}.`;
      if (o.showdown) text += ` You show ${esc(t.hero.rank)}; it shows ${esc(t.bot.rank)}.`;
      else text += o.winner === 'hero' ? ' The opponent folded.' : ' You folded.';
      if (!o.showdown && t.bot.cards) text += ` It had ${miniCards(t.bot.cards)}.`;
      strip = `<div class="note ${sign(net)}">${text}</div>`;
      actions = `<button type="button" class="act act-next" data-do="next"><span class="act-main">Next hand</span>${key('Space')}</button>`;
    }
  } else if (phase === 'over') {
    strip = `<div class="note">${view.kind === 'drill' ? 'The drill is over.' : 'The session is over.'}</div>`;
    actions = `<button type="button" class="act" data-do="report"><span class="act-main">Report</span></button>` +
      `<button type="button" class="act act-next" data-do="${view.kind === 'drill' ? 'back' : 'new'}"><span class="act-main">` +
      `${view.kind === 'drill' ? 'Back to the table' : 'New session'}</span></button>`;
  } else {
    strip = `<div class="note waiting">${phase === 'runout' ? 'All in: running out the board…' : 'The opponent is thinking…'}</div>`;
    actions = `<button type="button" class="act act-fold" disabled><span class="act-main">Fold</span></button>` +
      `<button type="button" class="act act-call" disabled><span class="act-main">Check</span></button>` +
      `<button type="button" class="act act-raise" disabled><span class="act-main">Bet</span></button>`;
  }
  const quiet = phase === 'hero' && !S.showNumbers;
  put('dock', `<div class="strip ${quiet ? 'is-quiet' : ''}">${strip}</div><div class="sizing ${sizing ? '' : 'is-empty'}">${sizing}</div>` +
    `<div class="actions">${actions}</div>`);
  syncSlider();
}

/** Moves the chosen bet without redrawing the dock, so a drag is not cut off. */
function setBet(amount, from) {
  const legal = S.view?.legal;
  if (!legal?.raise) return;
  const r = legal.raise;
  amount = Math.min(Math.max(Math.round(amount), r.min), r.max);
  S.bet = amount;
  const label = raiseLabel(legal, amount);
  $('raise-main').textContent = label.main;
  $('raise-sub').textContent = label.sub;
  if (from !== 'slider') $('bet-slider').value = amount;
  if (from !== 'input') $('bet-input').value = amount / S.view.bb;
  $('bet-slider').setAttribute('aria-valuetext', chips(amount));
  for (const b of document.querySelectorAll('.preset')) b.classList.toggle('is-on', Number(b.dataset.amount) === amount);
  S.drawn.sections.dock = null;
  syncSlider();
}

function syncSlider() {
  const slider = $('bet-slider');
  if (!slider) return;
  const [min, max] = [Number(slider.min), Number(slider.max)];
  slider.style.setProperty('--fill', max > min ? `${((slider.value - min) / (max - min)) * 100}%` : '100%');
}

// ---- the coach's feed -----------------------------------------------------

function reviewHTML(item, open) {
  const chosen = item.rows.find((r) => r.chosen);
  const other = item.rows.find((r) => !r.chosen);
  let line = esc(item.action) + (chosen && !item.note ? ` <b>${minus(chosen.ev_text)}</b>` : '');
  if (other && chosen && !item.note) {
    line += ` <span class="dim">· ${chosen.best ? 'next' : 'best'}: ${esc(other.name.toLowerCase())} ${minus(other.ev_text)}</span>`;
  }
  const evs = item.rows.map((r) => r.ev);
  const [lo, hi] = [Math.min(0, ...evs), Math.max(0, ...evs)];
  const span = hi - lo || 1;
  const zero = ((0 - lo) / span) * 100;
  let rows = item.rows.filter((r, i) => i < 5 || r.chosen).map((r) => {
    const at = ((r.ev - lo) / span) * 100;
    const tag = r.best && r.chosen ? '<span class="tag tag-best">★ you</span>'
      : r.best ? '<span class="tag tag-best">★ best</span>' : r.chosen ? '<span class="tag tag-you">you</span>' : '';
    const sub = r.folds != null
      ? `<span class="opt-sub">it folds ${pct(r.folds)} · you have ${pct(r.equity_called)} when called</span>` : '';
    return `<li class="opt ${r.chosen ? 'is-chosen' : ''} ${r.best ? 'is-best' : ''}">` +
      `<span class="opt-name">${esc(r.name)}${tag}</span>` +
      `<span class="opt-bar" aria-hidden="true"><i style="left:${Math.min(at, zero)}%;width:${Math.abs(at - zero)}%"></i>` +
      `<u style="left:${zero}%"></u></span>` +
      `<span class="opt-ev">${esc(r.ev_text.replace('-', '−'))}</span>${sub}</li>`;
  }).join('');
  if (item.note) rows = '';
  const body = `<p class="headline">${esc(item.headline)} · <b>${pct(item.equity)}</b> vs ${esc(item.against)}</p>` +
    (item.note ? `<p>${esc(item.note)}</p>` : `<ol class="opts" aria-label="Options, best first">${rows}</ol>`) +
    (item.points.length ? `<ul class="points">${item.points.map((p) => `<li>${esc(p)}</li>`).join('')}</ul>` : '');
  return `<article class="item review grade-${item.grade} ${open ? 'is-open' : ''}" data-id="${item.id}">
    <button type="button" class="review-head" data-toggle="${item.id}" aria-expanded="${open}">
      <span class="grade-icon" aria-hidden="true">${GRADE_ICON[item.grade]}</span>
      <span class="review-title"><span class="review-grade">${esc(item.label)}` +
        (item.run >= 3 ? ` <span class="streak" title="Sound decisions in a row">×${item.run}</span>` : '') +
        (item.loss > 0 ? ` <span class="review-cost">${esc(item.loss_text)} given up</span>` : '') + `</span>
        <span class="review-line">${line}</span></span>
      <svg class="chev" viewBox="0 0 24 24" aria-hidden="true"><path d="M6 9l6 6 6-6"/></svg>
    </button>
    <div class="review-body">${body}</div>
  </article>`;
}

function quizHTML(item) {
  const open = item.state === 'open';
  const answers = item.choices.map((choice, i) => {
    let cls = '';
    if (!open) {
      if (i === item.right) cls = 'is-right';
      else if (i === item.choice) cls = item.right == null ? 'is-picked' : 'is-wrong';
    }
    return `<button type="button" class="answer ${cls}" data-answer="${i}" ${open ? '' : 'disabled'}>` +
      `<kbd>${i + 1}</kbd><span>${esc(choice)}</span></button>`;
  }).join('');
  const verdict = { good: '✓ Right.', close: '≈ Close.', inaccuracy: '✗ Not quite.', note: '' }[item.grade] ?? '';
  return `<article class="item quiz ${open ? 'is-open' : `grade-${item.grade}`}" data-id="${item.id}">
    <header><span class="eyebrow">Coach asks</span><span class="skill">${esc(item.skill)}</span>` +
      (open ? `<button type="button" class="skip" data-do="skip" title="Skip (Esc)">Skip</button>` : '') + `</header>
    <p class="question">${esc(item.text)}</p>
    <div class="answers n${item.choices.length}">${answers}</div>` +
    (open ? '' : `<p class="verdict"><b>${verdict}</b> ${esc(item.note ?? '')}</p>`) + `
  </article>`;
}

function itemHTML(item) {
  switch (item.kind) {
    case 'review': {
      const open = S.toggled.has(item.id) ? S.toggled.get(item.id) : item.detail === 'full';
      return reviewHTML(item, open);
    }
    case 'quiz': return quizHTML(item);
    case 'hand': return `<div class="item hand-end" data-id="${item.id}"><span class="hand-text ${sign(item.net)}">${esc(item.text)}</span>` +
      `<span class="dots">${item.grades.map((g) => `<i class="dot grade-${g}" title="${g}"></i>`).join('')}</span>` +
      (item.grades.length ? `<span class="dim">${item.lost_text ? `${esc(item.lost_text)} given up` : 'nothing given up'}</span>` : '') + `</div>`;
    case 'divider': return `<div class="item hand-end" data-id="${item.id}"><span class="hand-text">${esc(item.text)}</span></div>`;
    case 'you': return `<div class="item bubble you" data-id="${item.id}">${esc(item.text)}</div>`;
    case 'coach': {
      const text = S.streams.get(item.id) ?? item.text;
      return `<div class="item bubble coach ${item.pending && !text ? 'is-pending' : ''}" data-id="${item.id}">` +
        `<span class="eyebrow">Claude</span>${text ? esc(text) : '<span class="thinking"><i>.</i><i>.</i><i>.</i></span>'}</div>`;
    }
    default: return `<p class="item system" data-id="${item.id}">${esc(item.text)}</p>`;
  }
}

function drawFeed(view) {
  const feed = $('feed');
  const drawn = S.drawn.feed;
  const pinned = feed.scrollHeight - feed.scrollTop - feed.clientHeight < 80;
  const ids = new Set(view.feed.map((i) => i.id));
  let added = null;
  for (const [id, entry] of drawn) {
    if (!ids.has(id)) {
      entry.el.remove();
      drawn.delete(id);
    }
  }
  for (const item of view.feed) {
    const html = itemHTML(item);
    const old = drawn.get(item.id);
    if (old?.html === html) continue;
    const holder = document.createElement('template');
    holder.innerHTML = html.trim();
    const el = holder.content.firstElementChild;
    if (old) {
      old.el.replaceWith(el);
    } else {
      el.classList.add('is-new');
      feed.append(el);
      added = item;
    }
    drawn.set(item.id, { html, el });
  }
  if (added || pinned) feed.scrollTop = feed.scrollHeight;
  if (added && app.dataset.tab !== 'coach' && narrow.matches) {
    const urgent = added.kind === 'quiz' || (added.kind === 'review' && added.detail === 'full');
    if (urgent) setTab('coach');
    else $('coach-dot').hidden = false;
  }
  $('ask').hidden = view.mode === 'off';
  $('ask').classList.toggle('no-key', !S.settings.apiKey);
  $('ask-input').placeholder = S.settings.apiKey ? 'Ask the coach…' : 'Ask the coach… (set up Claude first)';
}

// ---- panels ---------------------------------------------------------------

function profileBar(read) {
  if (!read) return '';
  const parts = [['strong', read.strong], ['medium', read.medium], ['draws', read.draws], ['weak', read.weak]];
  return `<div class="profile" role="img" aria-label="${parts.map(([n, x]) => `${n} ${pct(x)}`).join(', ')}">` +
    parts.map(([n, x]) => `<i class="p-${n}" style="flex:${Math.max(x, 0.0001)}"></i>`).join('') + `</div>` +
    `<div class="legend">${parts.map(([n, x]) => `<span><i class="p-${n}"></i>${n} <b>${pct(x)}</b></span>`).join('')}</div>`;
}

function drawRange(view) {
  const live = S.showNumbers || view.phase === 'next' || view.phase === 'over' || view.mode === 'off';
  const grid = live ? view.grid.now : view.grid.last;
  const read = live ? (view.spot?.read ?? view.last_read) : view.last_read;
  let caption;
  if (!grid) caption = view.mode === 'teach' ? 'Shown once you act, so it never answers a question for you.' : 'Shown once you act.';
  else if (view.phase === 'next' && view.kind === 'play') caption = 'What it could hold at the end of the hand.';
  else if (live) caption = 'What it can hold right now, after everything it has done.';
  else caption = 'As of your last decision. Peek shows it live.';
  const flat = !grid || grid.every((w) => w > 0.97);
  let cells = '';
  for (let r = 0; r < 13; r++) {
    for (let c = 0; c < 13; c++) {
      const name = r === c ? RANKS[r] + RANKS[c] : c > r ? `${RANKS[r]}${RANKS[c]}s` : `${RANKS[c]}${RANKS[r]}o`;
      const w = grid ? grid[r * 13 + c] : 0;
      const hero = r * 13 + c === view.grid.hero;
      cells += `<span class="cell ${hero ? 'is-hero' : ''} ${r === c ? 'is-pair' : ''}" style="--w:${grid ? w : 0}" ` +
        `title="${name}${grid ? `: ${w >= 0.995 ? 'among its likeliest hands' : `${pct(w)} as likely as its likeliest`}` : ''}${hero ? ' · your hand' : ''}">${name}</span>`;
    }
  }
  put('range', `<h3>Its likely hands <span class="h-note">${grid && flat ? 'any two cards so far' : ''}</span></h3>
    <div class="grid ${grid ? '' : 'is-off'}" role="img" aria-label="The 169 starting hands, lit by how likely the opponent is to hold each">${cells}</div>
    <div class="grid-key"><span>unlikely</span><i></i><span>likely</span><span class="key-hero"><i></i>your hand</span></div>
    ${profileBar(read)}
    <p class="caption">${caption}</p>`);
}

function gauge(label, share, note = '') {
  if (share == null) {
    return `<div class="gauge is-empty"><span class="gauge-label">${label}</span><span class="gauge-bar"></span><span class="gauge-value">–</span></div>`;
  }
  return `<div class="gauge"><span class="gauge-label">${label}</span>` +
    `<span class="gauge-bar"><i style="width:${share * 100}%"></i></span><span class="gauge-value">${note || pct(share)}</span></div>`;
}

function drawOpponent(view) {
  const o = view.opponent;
  const seen = o.seen;
  let html = `<h3>Opponent <span class="h-note">${esc(o.label)}</span></h3>`;
  if (o.hidden) {
    html += `<p class="lead">${seen.read ? `Looks ${esc(seen.read)}.`
      : `Unknown. A read needs about 12 hands (${seen.hands} so far).`}</p>`;
    if (o.guess) html += `<p class="kv"><span>Your guess</span><b class="gold">${esc(o.guess)}</b></p>`;
  } else if (o.blurb) {
    html += `<p class="lead">${esc(o.blurb)}</p><p class="counter"><b>Beat it:</b> ${esc(o.counter)}</p>`;
  }
  if (seen) {
    html += `<div class="gauges bot">` +
      gauge('Plays hands', seen.plays) + gauge('Raises preflop', seen.raises) +
      gauge('Folds to bets', seen.folds) + gauge('Bets, not calls', seen.aggression) + `</div>` +
      `<p class="caption">${seen.hands ? `Seen over ${plural(seen.hands, 'hand')}.` : 'Nothing seen yet.'}</p>`;
  }
  if (o.hidden && S.info) {
    html += `<h4>It is one of</h4><dl class="kinds">` + S.info.styles.map((s) => {
      const trait = (s.blurb.split(': ')[1] ?? s.blurb).replace(/\.$/, '');
      return `<dt>${esc(s.name)}</dt><dd>${esc(trait)}</dd>`;
    }).join('') + `</dl>`;
  }
  put('opponent', html);
}

function drawLog(view) {
  const lines = view.table.log.map((line) => {
    const at = line.indexOf(':');
    return `<li><b>${esc(line.slice(0, at))}</b>${esc(line.slice(at + 1)) || ' <span class="dim">…</span>'}</li>`;
  }).join('');
  put('hand-log', `<h3>This hand <span class="h-note">${view.kind === 'drill' ? `from hand ${view.hand_no}` : `hand ${view.hand_no}`}</span></h3><ol class="log">${lines}</ol>`);
}

function sparkline(totals) {
  const values = [0, ...totals];
  const [lo, hi] = [Math.min(...values), Math.max(...values)];
  const span = hi - lo || 1;
  const [w, h] = [300, 56];
  const x = (i) => (i / (values.length - 1)) * w;
  const y = (v) => 3 + (1 - (v - lo) / span) * (h - 6);
  const line = values.map((v, i) => `${x(i).toFixed(1)},${y(v).toFixed(1)}`).join(' ');
  const last = values[values.length - 1];
  return `<svg class="spark ${sign(last)}" viewBox="0 0 ${w} ${h}" preserveAspectRatio="none" role="img" ` +
    `aria-label="Your result hand by hand, from ${bb(lo)} to ${bb(hi)}">` +
    `<line class="zero" x1="0" x2="${w}" y1="${y(0).toFixed(1)}" y2="${y(0).toFixed(1)}"/>` +
    `<polygon class="area" points="0,${y(0).toFixed(1)} ${line} ${w},${y(0).toFixed(1)}"/>` +
    `<polyline class="line" points="${line}"/></svg>` +
    `<div class="spark-axis"><span>hand 1</span><span>low ${bb(lo)} · high ${bb(hi)}</span><span>hand ${totals.length}</span></div>`;
}

function gradeBar(grades) {
  const total = GRADE_NAMES.reduce((sum, [g]) => sum + grades[g], 0);
  if (!total) return '';
  return `<div class="gradebar" role="img" aria-label="Decisions by grade">` +
    GRADE_NAMES.map(([g]) => (grades[g] ? `<i class="grade-${g}" style="flex:${grades[g]}"></i>` : '')).join('') + `</div>` +
    `<div class="legend">${GRADE_NAMES.filter(([g]) => grades[g]).map(([g, one, many]) =>
      `<span><i class="grade-${g}"></i><b>${grades[g]}</b> ${grades[g] === 1 ? one : many}</span>`).join('')}</div>`;
}

function leaksHTML(leaks) {
  return `<ol class="leaks">${leaks.map((l) => `<li><span class="leak-name">${esc(l.label)} on the ${esc(l.street)}</span>` +
    `<span class="leak-cost">${l.count}× · ${bb(l.loss)}</span><p>${esc(l.tip)}</p></li>`).join('')}</ol>`;
}

function skillsHTML(skills) {
  return `<div class="gauges skill">${skills.map((s) => gauge(s.name, s.right / s.asked, `${s.right}/${s.asked}`)).join('')}</div>`;
}

function drawSession(view) {
  if (view.kind !== 'play') {
    const d = view.drill;
    put('session', `<h3>Drill</h3><p class="lead">Spot ${d.index} of ${d.spots} · ${d.correct} played well.</p>
      <p class="caption">These are the decisions that cost you most in your last ${DRILL_FROM} hands. A spot counts as played well within a tenth of a big blind of the best option.</p>`);
    put('past', '');
    return;
  }
  const s = view.session;
  let html = `<h3>Session</h3>
    <div class="headline-stat"><span class="big ${sign(s.result)}">${bb(s.result, true)}</span>
      <span class="dim">after ${plural(s.hands, 'hand')}</span></div>`;
  if (s.totals.length >= 2) html += sparkline(s.totals);
  if (s.reviewed > 0) {
    html += `<h4>Mistakes cost</h4><div class="headline-stat"><span class="big cost">${bb(s.given_up)}</span>` +
      `<span class="dim">${s.per_100 != null ? `${Math.round(s.per_100)}bb per 100 hands` : ''}</span></div>` +
      `<p class="caption">This, not the result, measures how you played.</p>` + gradeBar(s.grades);
    if (s.run >= 3) html += `<p class="kv"><span>Run</span><b class="pos">${s.run} sound decisions in a row</b></p>`;
    if (s.leaks.length) {
      const worst = Math.max(...s.by_street.map((x) => x.lost));
      html += `<h4>Where it goes</h4><div class="gauges cost">` +
        s.by_street.map((x) => gauge(x.street, worst ? x.lost / worst : 0, bb(x.lost))).join('') + `</div>` +
        `<h4>Work on</h4>${leaksHTML(s.leaks.slice(0, 2))}`;
    }
  } else if (view.mode !== 'off') {
    html += `<p class="caption">Once you have made a few decisions, this shows what your mistakes cost and where.</p>`;
  }
  if (s.skills.length) html += `<h4>Questions</h4>${skillsHTML(s.skills)}`;
  put('session', html);

  const rows = s.past.map((p) => `<li><span class="past-no">${p.no}</span>` +
    `<span class="past-cards">${miniCards(p.cards)}</span><span class="seatpos ${p.button ? 'is-btn' : ''}">${p.button ? 'BTN' : 'BB'}</span>` +
    `<span class="dots">${p.grades.map((g) => `<i class="dot grade-${g}" title="${g}"></i>`).join('')}</span>` +
    `<span class="past-shown">${p.shown ? `vs ${miniCards(p.shown)}` : ''}</span>` +
    `<span class="past-net ${sign(p.net)}">${bb(p.net, true)}</span></li>`).join('');
  put('past', `<h3>Earlier hands</h3>${rows ? `<ol class="past">${rows}</ol>`
    : '<p class="caption">Finished hands are listed here: your cards, your seat, a dot for each decision and the result.</p>'}`);
}

// ---- dialogs --------------------------------------------------------------

function toast(text) {
  const el = $('toast');
  el.textContent = text;
  el.hidden = false;
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => { el.hidden = true; }, 4200);
}

function openNew() {
  const kinds = [{ name: 'mystery', blurb: 'One of the four below, chosen in secret. Read it from how it plays; it is named when the session ends.' },
    ...S.info.styles];
  $('new-opponents').innerHTML = kinds.map((k, i) =>
    `<label class="choice ${i === 0 ? 'choice-wide' : ''}"><input type="radio" name="opponent" value="${k.name}" ${i === 0 ? 'checked autofocus' : ''}>` +
    `<span><b>${k.name[0].toUpperCase() + k.name.slice(1)}</b><small>${esc(k.blurb.replace(/^A [^:]+: /, '').replace(/^./, (c) => c.toUpperCase()))}</small></span></label>`).join('');
  const playing = S.view?.kind === 'play' && S.view.phase !== 'over' && S.view.session.hands > 0;
  $('new-note').hidden = !playing;
  if (S.view?.mode) $('new-form').elements.mode.value = S.view.mode;
  $('dlg-new').showModal();
}

function randomSeed() {
  const [a, b] = crypto.getRandomValues(new Uint32Array(2));
  return ((BigInt(a) << 21n) ^ BigInt(b)).toString();
}

async function newSession({ opponent = 'mystery', mode = 'teach', stack = 100, seed = '' } = {}) {
  const chosen = seed.trim() || randomSeed();
  S.toggled.clear();
  await run({ cmd: 'new', seed: chosen, sid: `${chosen}-${Date.now().toString(36)}`, opponent, mode, stack: Number(stack) });
}

function reportHTML(r) {
  if (r.kind === 'drill') {
    return `<div class="headline-stat"><span class="big">${r.correct} of ${r.spots}</span><span class="dim">spots played well</span></div>
      <p>Play more hands and the drill follows your newest mistakes.</p>`;
  }
  let html = `<div class="report-grid">
    <div><span class="tile-label">Result</span><span class="big ${sign(r.result)}">${esc(r.result_text.replace('-', '−'))}</span>
      <span class="dim">over ${plural(r.hands, 'hand')}</span></div>` +
    (r.coached ? `<div><span class="tile-label">Given up to mistakes</span><span class="big cost">${esc(r.given_up_text)}</span>
      <span class="dim">${r.per_100 != null ? `${Math.round(r.per_100)}bb per 100 hands` : ''}</span></div>` : '') + `</div>`;
  if (r.coached) html += `<p class="caption">The chips given up, not the result, measure how you played.</p>${gradeBar(r.grades)}`;
  if (r.skills.length) html += `<h4>Questions</h4>${skillsHTML(r.skills)}`;
  if (r.leaks.length) html += `<h4>Work on</h4>${leaksHTML(r.leaks)}`;
  if (r.opponent) {
    const o = r.opponent;
    html += `<div class="reveal"><span class="eyebrow">Your opponent was</span><div class="reveal-name">${esc(o.name)}</div>
      <p>${esc(o.blurb ?? '')} ${esc(o.counter ?? '')}</p>` +
      (o.guess ? `<p class="${o.right ? 'pos' : 'neg'}"><b>You took it for a ${esc(o.guess)}: ${o.right ? 'right.' : 'not this time.'}</b></p>` : '') +
      `<div class="gauges bot">${gauge('Plays hands', o.seen.plays)}${gauge('Raises preflop', o.seen.raises)}` +
      `${gauge('Folds to bets', o.seen.folds)}${gauge('Bets, not calls', o.seen.aggression)}</div></div>`;
  }
  html += `<p class="bubble coach" id="report-coach" hidden></p>`;
  return html;
}

function openReport() {
  const r = S.view?.report;
  if (!r) return;
  $('report-title').textContent = r.kind === 'drill' ? 'Drill over' : 'Session report';
  $('report-body').innerHTML = reportHTML(r);
  $('report-actions').innerHTML = r.kind === 'drill'
    ? `<button type="submit" value="back" class="btn btn-primary" autofocus>Back to the table</button>`
    : `<button type="submit" value="close" class="btn">Look around</button>
       <button type="submit" value="drill" class="btn">Drill my mistakes</button>
       <button type="submit" value="new" class="btn btn-primary" autofocus>New session</button>`;
  $('dlg-report').showModal();
  if (r.kind === 'play' && r.coached && S.settings.apiKey && S.summaryId == null) explain('summary');
}

/** Mistakes in the newest stored hands, costliest first, and the leaks they add up to. */
function lifetime() {
  const recent = S.history.slice(-DRILL_FROM);
  const bigBlind = S.info.big_blind;
  const leaks = new Map();
  const spots = [];
  let decisions = 0;
  let lost = 0;
  for (const entry of recent) {
    entry.dec.forEach((d, i) => {
      decisions++;
      lost += d.loss;
      if (d.leak == null) return;
      const key = `${d.street}:${d.leak}`;
      const line = leaks.get(key) ?? { street: d.street.toLowerCase(), ...S.info.leaks[d.leak], count: 0, loss: 0 };
      line.count++;
      line.loss += d.loss / bigBlind;
      leaks.set(key, line);
      // Only hands reviewed throughout can be replayed decision by decision.
      if (entry.dec.length === entry.mine) spots.push({ rec: entry.rec, i, loss: d.loss });
    });
  }
  spots.sort((a, b) => b.loss - a.loss);
  return {
    hands: recent.length, decisions, lost: lost / bigBlind,
    leaks: [...leaks.values()].sort((a, b) => b.loss - a.loss),
    spots: spots.slice(0, DRILL_SPOTS).map(({ rec, i }) => ({ rec, i })),
  };
}

function openDrill() {
  const all = lifetime();
  let html;
  if (!all.hands) {
    html = `<p>Nothing here yet. Every hand you finish is kept in this browser; your costliest recurring mistakes show up here, and the drill replays them as practice spots.</p>`;
  } else {
    html = `<p class="lead">Your last ${plural(all.hands, 'hand')}: ${plural(all.decisions, 'decision')} reviewed, ` +
      `<b class="cost">${bb(all.lost)}</b> given up.</p>` +
      (all.leaks.length ? `<h4>Leaks, costliest first</h4>${leaksHTML(all.leaks.slice(0, 5))}`
        : `<p>No recurring mistakes in them. Nice.</p>`);
  }
  $('drill-body').innerHTML = html;
  $('drill-start').hidden = !all.spots.length;
  $('drill-start').textContent = `Drill the ${all.spots.length === 1 ? 'costliest spot' : `${all.spots.length} costliest spots`}`;
  $('dlg-drill').showModal();
}

async function startDrill() {
  const { spots } = lifetime();
  if (!spots.length) return;
  toast('Setting up the spots…');
  S.toggled.clear();
  await run({ cmd: 'drill', spots });
}

/** Back from a drill (or a broken save) to the session in progress, or a new one. */
async function resume() {
  const save = load(STORE.save, null);
  if (save) {
    S.history = await hands.all();
    const mine = S.history.filter((e) => e.sid === save.sid).slice(-save.hands_done);
    const reply = await run({ cmd: 'restore', save, hands: mine }, { quiet: true });
    // A crash is handled by the restart, which resumes again itself.
    if (!reply.error || reply.crashed || S.elsewhere) return;
    store(STORE.save, null);
    toast(`Your saved session could not be resumed (${reply.error}), so a new one has started.`);
  }
  await newSession();
}

function openSettings() {
  const form = $('settings-form');
  form.elements.fourColour.checked = S.settings.fourColour;
  form.elements.fast.checked = S.settings.fast;
  form.elements.apiKey.value = S.settings.apiKey;
  form.elements.model.value = S.settings.model;
  $('data-note').textContent = `${plural(S.history.length, 'finished hand')} kept in this browser. ` +
    'Export them to keep a copy or move to another device.';
  $('dlg-settings').showModal();
}

function exportHands() {
  const blob = new Blob([S.history.map((e) => JSON.stringify(e)).join('\n') + '\n'], { type: 'application/x-ndjson' });
  const link = document.createElement('a');
  link.href = URL.createObjectURL(blob);
  link.download = 'poker-hands.jsonl';
  link.click();
  setTimeout(() => URL.revokeObjectURL(link.href), 1000);
}

async function importHands(file) {
  S.history = await hands.all();
  const seen = new Set(S.history.map((e) => `${e.sid}:${e.no}`));
  const added = [];
  for (const line of (await file.text()).split('\n')) {
    if (!line.trim()) continue;
    try {
      const entry = JSON.parse(line);
      const ok = typeof entry?.rec === 'string' && typeof entry.sid === 'string' &&
        Number.isInteger(entry.no) && Number.isInteger(entry.mine) && Array.isArray(entry.dec) &&
        entry.dec.every((d) => STREETS.includes(d?.street) && Number.isFinite(d.loss) &&
          Number.isInteger(d.grade) && (d.leak == null || S.info.leaks[d.leak]));
      if (ok && !seen.has(`${entry.sid}:${entry.no}`)) {
        seen.add(`${entry.sid}:${entry.no}`);
        added.push(entry);
      }
    } catch { /* not a hand: skip the line */ }
  }
  if (added.length) {
    try {
      await hands.add(added, HISTORY_KEPT);
      tabs?.postMessage({ hands: true });
    } catch {
      toast('This browser is not letting the page keep the hands.');
    }
    S.history = await hands.all();
  }
  toast(added.length ? `${plural(added.length, 'hand')} imported.` : 'No new hands in that file.');
  openSettings();
}

// ---- Claude ---------------------------------------------------------------

let talking = Promise.resolve();

/** Asks Claude for a review, a summary or an answer; replies stream into the feed. */
function explain(kind, text = '', review = undefined) {
  talking = talking.then(async () => {
    // A review names its item, so one asked for while an earlier reply was
    // still arriving is dropped if the table has moved past it.
    const reply = await run({ cmd: 'chat', kind, text, review });
    const chat = reply.chat;
    if (!chat) return;
    if (kind === 'summary') S.summaryId = chat.id;
    const onText = (soFar) => {
      S.streams.set(chat.id, soFar);
      const entry = S.drawn.feed.get(chat.id);
      if (entry) {
        const feed = $('feed');
        const pinned = feed.scrollHeight - feed.scrollTop - feed.clientHeight < 80;
        entry.el.classList.remove('is-pending');
        entry.el.innerHTML = `<span class="eyebrow">Claude</span>${esc(soFar)}`;
        entry.html = '';
        if (pinned) feed.scrollTop = feed.scrollHeight;
      }
      const slot = $('report-coach');
      if (kind === 'summary' && slot) {
        slot.hidden = false;
        slot.innerHTML = `<span class="eyebrow">Claude</span>${esc(soFar)}`;
      }
    };
    try {
      const full = await askClaude({ key: S.settings.apiKey, model: S.settings.model, ...chat }, onText);
      S.streams.delete(chat.id);
      await run({ cmd: 'chat_done', id: chat.id, text: full });
    } catch (error) {
      S.streams.delete(chat.id);
      await run({ cmd: 'chat_fail', id: chat.id, error: error.message });
    }
  }).catch((error) => toast(String(error)));
}

// ---- doing things ---------------------------------------------------------

async function act(action) {
  const view = S.view;
  if (S.busy || view?.phase !== 'hero') return;
  const cmd = { cmd: 'act', action };
  if (action === 'raise') {
    if (!view.legal.raise) return;
    cmd.amount = S.bet;
  }
  if ((action === 'fold' || action === 'call') && view.legal.to_call === 0) return;
  if (action === 'check' && view.legal.to_call > 0) return;
  const reply = await run(cmd);
  const last = reply.view?.feed.at(-1);
  // Claude explains the reviews shown in full; a sound play needs no lecture.
  const review = reply.view?.feed.findLast((i) => i.kind === 'review');
  if (!reply.error && S.settings.apiKey && review?.detail === 'full' && (last === review || reply.view.kind === 'drill' || last?.kind === 'hand')) {
    explain('review', '', review.id);
  }
}

async function doIt(what) {
  switch (what) {
    case 'next':
      if (!S.busy && S.view?.phase === 'next') await run({ cmd: 'next' });
      break;
    case 'peek':
      S.showNumbers = !S.showNumbers;
      draw(S.view);
      break;
    case 'skip':
      if (!S.busy) await run({ cmd: 'skip' });
      break;
    case 'new': openNew(); break;
    case 'drill': openDrill(); break;
    case 'settings': openSettings(); break;
    case 'help': $('dlg-help').showModal(); break;
    case 'report': openReport(); break;
    case 'back': await resume(); break;
    case 'end':
      if (S.view?.phase === 'over') openReport();
      else if (S.view) await run({ cmd: 'end' });
      break;
    default:
  }
}

async function answer(choice) {
  const quiz = S.view?.feed.find((i) => i.id === S.view.quiz);
  if (S.busy || !quiz || choice >= quiz.choices.length) return;
  await run({ cmd: 'answer', choice });
}

function toggleReview(id) {
  const item = S.view.feed.find((i) => i.id === id);
  if (!item) return;
  const open = S.toggled.has(id) ? S.toggled.get(id) : item.detail === 'full';
  S.toggled.set(id, !open);
  drawFeed(S.view);
  S.drawn.feed.get(id)?.el.scrollIntoView({ block: 'nearest', behavior: calm.matches ? 'auto' : 'smooth' });
}

const narrow = matchMedia('(max-width: 1279px)');

function setTab(tab) {
  app.dataset.tab = tab;
  for (const button of document.querySelectorAll('.tabs [role=tab]')) {
    button.setAttribute('aria-selected', String(button.dataset.tab === tab));
  }
  if (tab === 'coach') {
    $('coach-dot').hidden = true;
    $('feed').scrollTop = $('feed').scrollHeight;
  }
}

document.addEventListener('click', (event) => {
  const el = event.target.closest('[data-act],[data-do],[data-answer],[data-toggle],[data-amount],[data-tab],[data-close]');
  if (!el || el.disabled) return;
  if ('close' in el.dataset) return el.closest('dialog').close();
  if (el.closest('#menu')) $('menu').close();
  if (el.dataset.act) act(el.dataset.act);
  else if (el.dataset.do) doIt(el.dataset.do);
  else if (el.dataset.answer) answer(Number(el.dataset.answer));
  else if (el.dataset.toggle) toggleReview(Number(el.dataset.toggle));
  else if (el.dataset.amount) setBet(Number(el.dataset.amount));
  else if (el.dataset.tab) setTab(el.dataset.tab);
});

document.addEventListener('input', (event) => {
  if (event.target.id === 'bet-slider') setBet(Number(event.target.value), 'slider');
  if (event.target.id === 'bet-input' && event.target.value !== '') {
    setBet(Number(event.target.value) * S.view.bb, 'input');
  }
});
document.addEventListener('change', (event) => {
  if (event.target.id === 'bet-input') setBet(S.bet);
});

$('mode').addEventListener('change', (event) => run({ cmd: 'mode', mode: event.target.value }));

$('ask').addEventListener('submit', (event) => {
  event.preventDefault();
  const input = $('ask-input');
  const text = input.value.trim().replace(/^\//, '').trim();
  if (!text) return;
  if (!S.settings.apiKey) return openSettings();
  input.value = '';
  explain('free', text);
});
$('ask-input').addEventListener('focus', () => {
  if (!S.settings.apiKey) {
    $('ask-input').blur();
    openSettings();
  }
});

$('dlg-new').addEventListener('close', (event) => {
  const dialog = event.target;
  if (dialog.returnValue === 'start') {
    const data = new FormData($('new-form'));
    newSession(Object.fromEntries(data));
  }
  dialog.returnValue = '';
});
$('dlg-report').addEventListener('close', (event) => {
  const value = event.target.returnValue;
  event.target.returnValue = '';
  if (value === 'new') openNew();
  else if (value === 'drill') openDrill();
  else if (value === 'back') resume();
});
$('dlg-drill').addEventListener('close', (event) => {
  const value = event.target.returnValue;
  event.target.returnValue = '';
  if (value === 'drill') startDrill();
});
$('dlg-settings').addEventListener('close', () => {
  const form = $('settings-form');
  S.settings = {
    fourColour: form.elements.fourColour.checked,
    fast: form.elements.fast.checked,
    apiKey: form.elements.apiKey.value.trim(),
    model: form.elements.model.value.trim(),
  };
  store(STORE.settings, S.settings);
  if (S.view) draw(S.view);
});
// Taking the session back reloads it from the save the other tab left.
$('dlg-elsewhere').addEventListener('cancel', (event) => event.preventDefault());
$('dlg-elsewhere').addEventListener('close', () => location.reload());
$('export').addEventListener('click', exportHands);
$('import').addEventListener('click', () => $('import-file').click());
$('import-file').addEventListener('change', (event) => {
  const [file] = event.target.files;
  if (file) importHands(file);
  event.target.value = '';
});
$('clear').addEventListener('click', (event) => {
  const button = event.currentTarget;
  if (button.dataset.sure !== 'yes') {
    button.dataset.sure = 'yes';
    button.textContent = 'Really delete?';
    setTimeout(() => { button.dataset.sure = ''; button.textContent = 'Delete all'; }, 4000);
    return;
  }
  S.history = [];
  hands.clear().catch(() => {});
  tabs?.postMessage({ hands: true });
  store(STORE.save, null);
  $('dlg-settings').close();
  newSession();
});

// Closing a dialog hands focus back to the menu button, where the
// space bar would open the menu again instead of dealing the next hand.
const unfocus = () => requestAnimationFrame(() => {
  if (!document.querySelector('dialog[open]')) document.activeElement?.blur();
});
for (const dialog of document.querySelectorAll('dialog')) dialog.addEventListener('close', unfocus);
$('menu-button').addEventListener('click', () => $('menu').showModal());

// Clicking outside closes the dialogs that allow it, where the browser does not yet.
if (!('closedBy' in HTMLDialogElement.prototype)) {
  const outside = (e, dialog) => {
    const r = dialog.getBoundingClientRect();
    return e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom;
  };
  for (const dialog of document.querySelectorAll('dialog[closedby="any"]')) {
    let pressedOutside = false;
    dialog.addEventListener('pointerdown', (e) => { pressedOutside = outside(e, dialog); });
    dialog.addEventListener('pointerup', (e) => { if (pressedOutside && outside(e, dialog)) dialog.close(); });
  }
}

document.addEventListener('keydown', (event) => {
  if (event.ctrlKey || event.metaKey || event.altKey) return;
  const typing = event.target.closest('input:not([type=range]), textarea, select');
  if (typing) {
    if (event.key === 'Escape') event.target.blur();
    if (event.key === 'Enter' && event.target.id === 'bet-input') {
      event.target.blur();
      act('raise');
    }
    return;
  }
  if (document.querySelector('dialog[open]') || !S.view) return;
  const view = S.view;
  const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
  const quiz = view.quiz != null;
  const raise = view.phase === 'hero' ? view.legal?.raise : null;
  const done = () => event.preventDefault();

  if (/^[1-9]$/.test(key)) {
    const n = Number(key) - 1;
    if (quiz) answer(n);
    else if (raise && raise.presets[n]) setBet(raise.presets[n].amount);
    return done();
  }
  switch (key) {
    case 'f': act('fold'); break;
    case 'c': case 'x': case 'k': act(view.legal?.to_call > 0 ? 'call' : 'check'); break;
    case 'r': case 'b': act('raise'); break;
    case 'a': if (raise) setBet(raise.max); break;
    case 'ArrowLeft': case '-': if (raise) setBet(S.bet - raise.step); break;
    case 'ArrowRight': case '+': case '=': if (raise) setBet(S.bet + raise.step); break;
    case 'ArrowDown': if (raise) setBet(S.bet - raise.step * 4); break;
    case 'ArrowUp': if (raise) setBet(S.bet + raise.step * 4); break;
    case ' ': case 'Enter':
      if (event.target.closest('button, a, summary')) return;
      doIt('next');
      break;
    case 'Escape': if (quiz) doIt('skip'); break;
    case 'n': if (view.mode !== 'off') doIt('peek'); break;
    case 'd': {
      const review = view.feed.findLast((i) => i.kind === 'review');
      if (review) toggleReview(review.id);
      break;
    }
    case '/': case 't':
      setTab('coach');
      $('ask-input').focus();
      break;
    case '?': case 'h': doIt('help'); break;
    default: return;
  }
  done();
});

// ---- start ----------------------------------------------------------------

/** Moves hands kept in localStorage by older versions into IndexedDB. */
async function moveOldHands() {
  const old = load(STORE.history, null);
  if (!Array.isArray(old) || !(await hands.lasting())) return;
  try {
    await hands.add(old, HISTORY_KEPT);
    store(STORE.history, null);
  } catch { /* left where it was, to try again next time */ }
}

async function start() {
  startEngine();
  await moveOldHands();
  S.history = await hands.all();
  try {
    await engineReady;
  } catch (error) {
    $('loading').innerHTML = `<p><b>The table could not be set up.</b></p><p class="dim">${esc(error.message)}</p>` +
      `<p class="dim">It needs a browser with WebAssembly and module workers, served over http(s).</p>`;
    return;
  }
  S.info = (await send({ cmd: 'info' })).info;
  await resume();
  if ('serviceWorker' in navigator && location.protocol === 'https:') {
    navigator.serviceWorker.register('sw.js').catch(() => {});
  }
}

start();
