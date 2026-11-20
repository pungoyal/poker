//! Computer opponents.
//!
//! Every bot also describes its strategy as [`Bot::tendencies`]: how likely it
//! is to fold, check or call, and bet or raise with any given hand. The coach
//! uses that to work out which hands the bot can hold after its actions.

use crate::cards::{Card, Rng};
use crate::game::{Action, HandState, Street};
use crate::range::Strengths;

/// Chances of the three kinds of response. They sum to one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mix {
    pub fold: f64,
    /// Check, or call.
    pub passive: f64,
    /// Bet, or raise.
    pub aggressive: f64,
}

impl Mix {
    /// The chance of the kind of action `action` is.
    pub fn chance_of(&self, action: Action) -> f64 {
        match action {
            Action::Fold => self.fold,
            Action::Check | Action::Call => self.passive,
            Action::RaiseTo(_) => self.aggressive,
        }
    }

    /// Builds a mix from the chances of raising for value, raising as a bluff
    /// and checking or calling, folding the rest. When raising is not legal,
    /// value raises call instead and bluffs give up.
    fn legal(state: &HandState, value: f64, bluff: f64, passive: f64) -> Mix {
        let legal = state.legal().expect("no tendencies once the hand is over");
        let (mut aggressive, mut passive) = (value + bluff, passive);
        if legal.raise.is_none() {
            passive += value;
            aggressive = 0.0;
        }
        let aggressive = aggressive.clamp(0.0, 1.0);
        let mut passive = passive.clamp(0.0, 1.0 - aggressive);
        if legal.to_call == 0 {
            // Folding when checking is free is never needed.
            passive = 1.0 - aggressive;
        }
        Mix {
            fold: (1.0 - aggressive - passive).max(0.0),
            passive,
            aggressive,
        }
    }

    fn sample(&self, roll: f64) -> Kind {
        if roll < self.fold {
            Kind::Fold
        } else if roll < self.fold + self.passive {
            Kind::Passive
        } else {
            Kind::Aggressive
        }
    }
}

enum Kind {
    Fold,
    Passive,
    Aggressive,
}

fn passive_action(state: &HandState) -> Action {
    if state.legal().is_some_and(|l| l.to_call > 0) {
        Action::Call
    } else {
        Action::Check
    }
}

pub trait Bot {
    fn name(&self) -> &'static str;

    /// Chooses an action for `seat`, which is the player to act.
    /// A bot may only look at its own hole cards and the visible board.
    fn act(&mut self, state: &HandState, seat: usize) -> Action;

    /// How this bot plays the player to act in `state` when holding `hole`.
    /// `strengths` must be for the current board.
    fn tendencies(&self, state: &HandState, hole: [Card; 2], strengths: &Strengths) -> Mix;

    /// What it bets or raises to in `state`, when it does.
    fn raise_to(&self, state: &HandState) -> u32 {
        solid_raise_size(state)
    }

    /// Its style of play, if it has a describable one.
    fn style(&self) -> Option<Style> {
        None
    }
}

/// The bot with this name, if there is one.
pub fn by_name(name: &str, seed: u64) -> Option<Box<dyn Bot>> {
    match name {
        "random" => Some(Box::new(RandomBot::new(seed))),
        "calling station" => Some(Box::new(CallingStation)),
        name => Some(Box::new(StyleBot::new(style_named(name)?, seed))),
    }
}

/// Never folds, never bets: checks when it can and calls otherwise.
pub struct CallingStation;

impl Bot for CallingStation {
    fn name(&self) -> &'static str {
        "calling station"
    }

    fn act(&mut self, state: &HandState, _seat: usize) -> Action {
        passive_action(state)
    }

    fn tendencies(&self, state: &HandState, _hole: [Card; 2], _: &Strengths) -> Mix {
        Mix::legal(state, 0.0, 0.0, 1.0)
    }
}

/// Folds to a bet this often, whatever its cards or the bet size.
const RANDOM_FOLD: f64 = 0.2;
const RANDOM_RAISE: f64 = 0.3;

/// Plays without looking at its cards: mostly passive, with random bets,
/// raises and folds mixed in.
pub struct RandomBot {
    rng: Rng,
}

impl RandomBot {
    pub fn new(seed: u64) -> RandomBot {
        RandomBot {
            rng: Rng::new(seed),
        }
    }
}

impl Bot for RandomBot {
    fn name(&self) -> &'static str {
        "random"
    }

    fn act(&mut self, state: &HandState, _seat: usize) -> Action {
        let legal = state.legal().expect("bot asked to act in a finished hand");
        let roll = self.rng.unit();

        if let Some((min, max)) = legal.raise
            && roll < RANDOM_RAISE
        {
            // Bet between a third of the pot and the full pot, within the legal range.
            let pot_after_call = state.pot() + legal.to_call;
            let fraction = 1.0 / 3.0 + self.rng.unit() * 2.0 / 3.0;
            let current_bet = state.street_commit(1 - state.to_act().unwrap());
            let target = current_bet + (pot_after_call as f64 * fraction) as u32;
            return Action::RaiseTo(target.clamp(min, max));
        }
        if legal.to_call == 0 {
            Action::Check
        } else if roll > 1.0 - RANDOM_FOLD {
            Action::Fold
        } else {
            Action::Call
        }
    }

    fn tendencies(&self, state: &HandState, _hole: [Card; 2], _: &Strengths) -> Mix {
        let can_raise = state.legal().is_some_and(|l| l.raise.is_some());
        let aggressive = if can_raise { RANDOM_RAISE } else { 0.0 };
        Mix::legal(state, 0.0, aggressive, 1.0 - aggressive - RANDOM_FOLD)
    }
}

/// How a [`StyleBot`] plays: one set of dials covers a sound regular and the
/// usual kinds of weaker player.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Style {
    pub name: &'static str,
    /// What kind of player this is, in a few words.
    pub blurb: &'static str,
    /// How to beat it.
    pub counter: &'static str,
    /// First in on the button: the share of hands it raises.
    open: f64,
    /// The share of hands below those that it limps.
    limp: f64,
    /// The share of hands it raises after a limp.
    iso: f64,
    /// How wide it continues against raises; 1 is sound.
    defend: f64,
    /// The share of its continuing hands that re-raise for value.
    reraise: f64,
    /// How often the hands just below its calls re-raise as a bluff.
    reraise_bluff: f64,
    /// The strength from which it bets for value.
    value: f64,
    /// How often it bets medium hands for thin value.
    thin: f64,
    /// Its bluffs and semi-bluffs; 1 is sound.
    bluff: f64,
    /// Shift of the strength it needs to call a bet: negative calls more.
    call: f64,
    /// Its raises after the flop; 1 is sound.
    raise: f64,
    /// Its bet, as a share of the pot.
    bet_size: f64,
}

/// A sound, straightforward regular: plays a range of starting hands that
/// narrows as the bets get bigger, bets and calls by hand strength and pot
/// odds, semi-bluffs draws, continuation-bets and bluffs now and then. It
/// defends enough against every bet size that betting any two cards at it
/// does not pay.
pub const SOLID: Style = Style {
    name: "solid",
    blurb: "A sound regular: bets its good hands, bluffs in balance, defends enough.",
    counter: "No big hole. Play your equity and the price; bluff only with a reason.",
    open: 0.8,
    limp: 0.0,
    iso: 0.3,
    defend: 1.0,
    reraise: 0.22,
    reraise_bluff: 0.3,
    value: 0.8,
    thin: 0.35,
    bluff: 1.0,
    call: 0.0,
    raise: 1.0,
    bet_size: 0.66,
};

/// Loose and passive: the most common player in a home game.
pub const STATION: Style = Style {
    name: "station",
    blurb: "A calling station: plays most hands, limps, calls far too much, rarely raises.",
    counter: "Never bluff. Bet your good hands big and often; its raises mean strength.",
    open: 0.15,
    limp: 0.7,
    iso: 0.08,
    defend: 1.5,
    reraise: 0.08,
    reraise_bluff: 0.0,
    value: 0.85,
    thin: 0.15,
    bluff: 0.15,
    call: -0.22,
    raise: 0.4,
    bet_size: 0.5,
};

/// Tight and timid.
pub const NIT: Style = Style {
    name: "nit",
    blurb: "A nit: plays few hands, folds to pressure, bets only when it has it.",
    counter: "Steal its blinds and bet when it checks. When it bets or raises, believe it and fold.",
    open: 0.3,
    limp: 0.1,
    iso: 0.12,
    defend: 0.55,
    reraise: 0.25,
    reraise_bluff: 0.0,
    value: 0.82,
    thin: 0.15,
    bluff: 0.25,
    call: 0.18,
    raise: 0.8,
    bet_size: 0.66,
};

/// Loose and aggressive.
pub const MANIAC: Style = Style {
    name: "maniac",
    blurb: "A maniac: raises most hands, bets big and bluffs far too often.",
    counter: "Do not bluff it. Call down lighter with made hands and let it bet for you.",
    open: 0.92,
    limp: 0.0,
    iso: 0.6,
    defend: 1.3,
    reraise: 0.4,
    reraise_bluff: 1.0,
    value: 0.7,
    thin: 0.6,
    bluff: 2.2,
    call: -0.08,
    raise: 1.8,
    bet_size: 1.0,
};

/// Every style, the sound one first.
pub const STYLES: [Style; 4] = [SOLID, STATION, NIT, MANIAC];

/// The style with this name, if there is one.
pub fn style_named(name: &str) -> Option<Style> {
    STYLES.into_iter().find(|s| s.name == name)
}

/// A bot that plays a [`Style`]. Bets are one size for value and bluffs
/// alike, so the size gives nothing away.
pub struct StyleBot {
    style: Style,
    rng: Rng,
    strengths: Option<Strengths>,
}

impl StyleBot {
    pub fn new(style: Style, seed: u64) -> StyleBot {
        StyleBot {
            style,
            rng: Rng::new(seed),
            strengths: None,
        }
    }

    pub fn style(&self) -> Style {
        self.style
    }
}

/// The sound regular.
pub struct SolidBot;

impl SolidBot {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(seed: u64) -> StyleBot {
        StyleBot::new(SOLID, seed)
    }
}

/// The share of its strong hands a style checks rather than bets.
const TRAPS: f64 = 0.3;

/// Smooth step from 0 to 1 as `x` passes zero; `width` sets how gradual.
fn soft(x: f64, width: f64) -> f64 {
    1.0 / (1.0 + (-x / width).exp())
}

impl Bot for StyleBot {
    fn name(&self) -> &'static str {
        self.style.name
    }

    fn act(&mut self, state: &HandState, seat: usize) -> Action {
        if self
            .strengths
            .as_ref()
            .is_none_or(|s| s.board() != state.board())
        {
            self.strengths = Some(Strengths::new(state.board()));
        }
        let strengths = self.strengths.as_ref().unwrap();
        let mix = self.tendencies(state, state.hole(seat), strengths);
        match mix.sample(self.rng.unit()) {
            Kind::Fold => Action::Fold,
            Kind::Passive => passive_action(state),
            Kind::Aggressive => Action::RaiseTo(self.raise_to(state)),
        }
    }

    fn tendencies(&self, state: &HandState, hole: [Card; 2], strengths: &Strengths) -> Mix {
        if state.street() == Street::Preflop {
            style_preflop(&self.style, state, hole, strengths)
        } else {
            style_postflop(&self.style, state, hole, strengths)
        }
    }

    fn raise_to(&self, state: &HandState) -> u32 {
        raise_size(state, self.style.bet_size)
    }

    fn style(&self) -> Option<Style> {
        Some(self.style)
    }
}

fn style_preflop(style: &Style, state: &HandState, hole: [Card; 2], strengths: &Strengths) -> Mix {
    let legal = state.legal().expect("hand is over");
    let me = state.to_act().unwrap();
    // How far from the top: 0 for aces, 1 for seven-deuce.
    let p = 1.0 - strengths.of(hole);
    let bb = state.config().big_blind as f64;
    if state.raises() == 0 {
        return if legal.to_call > 0 {
            // First in on the button: raise the best hands, limp the next, fold the rest.
            let raise = soft(style.open - p, 0.015);
            let play = soft(style.open + style.limp - p, 0.015);
            Mix::legal(state, raise, 0.0, (play - raise).max(0.0))
        } else {
            // The button limped: raise for value, check the rest.
            Mix::legal(state, soft(style.iso - p, 0.015), 0.0, 1.0)
        };
    }
    // Facing a raise: the bigger it is, the fewer hands continue.
    let size = state.street_commit(1 - me) as f64 / bb;
    // Facing a re-raise it already showed strength, so it defends more of
    // what it has left rather than folding to every 4-bet.
    // However big the raise, the best hands still continue: enough of them
    // that shoving any two cards at it does not pay.
    let (defend, floor) = match state.raises() {
        1 => (1.6, 0.06),
        2 => (3.8, 0.1),
        _ => (2.6, 0.05),
    };
    let cap = (0.68 * style.defend).min(0.95);
    let width = (defend * style.defend / size).clamp(floor * style.defend, cap);
    // After limping, its best hands are gone (they raised): it continues
    // with the top of what it limped rather than the top of every hand.
    let limped = style.limp > 0.0
        && state
            .actions()
            .iter()
            .find(|&&(seat, _)| seat == me)
            .is_some_and(|&(_, first)| first == Action::Call && state.button() == me);
    let p = if limped {
        ((p - style.open) / style.limp).max(0.0)
    } else {
        p
    };
    // Edges blur in proportion to the range: tight ranges have sharp edges.
    let blur = |w: f64| (0.03 * w).max(0.001);
    let continuing = soft(width - p, blur(width));
    let value_width = style.reraise * width;
    let value = soft(value_width - p, blur(value_width));
    // A few re-raise bluffs from the hands just too weak to call an open.
    let bluff = if state.raises() == 1 {
        (0.3 * style.reraise_bluff).min(0.9)
            * soft(p - width, blur(width))
            * soft(width + 0.12 - p, blur(width))
    } else {
        0.0
    };
    Mix::legal(state, value, bluff, (continuing - value).max(0.0))
}

fn style_postflop(style: &Style, state: &HandState, hole: [Card; 2], strengths: &Strengths) -> Mix {
    let legal = state.legal().expect("hand is over");
    let me = state.to_act().unwrap();
    let street = state.street();
    let strength = strengths.of(hole);
    let outs = strengths.outs(hole);
    let drawing = street != Street::River && outs >= 4;

    if legal.to_call == 0 {
        // Some strong hands check, so a check does not rule them out.
        let value = (1.0 - TRAPS) * soft(strength - style.value, 0.03);
        let thin = style.thin * soft(strength - 0.62, 0.03);
        let semi_bluff: f64 = match outs {
            _ if !drawing => 0.0,
            8.. => 0.55,
            _ => 0.25,
        };
        let initiative = state.aggressor() == Some(me);
        let bluff_rate = if initiative && street == Street::Flop {
            0.45
        } else if initiative {
            0.25
        } else {
            0.12
        };
        let bluff = bluff_rate * soft(0.42 - strength, 0.03);
        let bluffs = (semi_bluff.max(bluff) * style.bluff).min(0.9);
        let bet = value.max(thin).max(bluffs);
        return Mix::legal(state, bet, 0.0, 1.0);
    }

    // Facing a bet: continue with enough hands that the bet cannot profit
    // with any two cards. A bet of b pots must fold b / (1 + b) to do that.
    let pot = state.pot() as f64;
    let to_call = legal.to_call as f64;
    let bet_size = to_call / (pot - to_call).max(1.0);
    let river = if street == Street::River { 0.05 } else { 0.0 };
    let mut folds = bet_size / (1.0 + bet_size);
    // After checking, most of its strong hands are gone (they bet), so the
    // same share of what is left sits lower: it calls with weaker hands
    // rather than folding nearly everything to a big bet.
    let checked = state.button() != me && state.raises() == 1 && state.street_commit(me) == 0;
    if checked {
        let strong = (1.0 - style.value) * TRAPS;
        let defend = (1.0 - folds) * (style.value + strong);
        folds = if defend > strong {
            style.value - (defend - strong)
        } else {
            1.0 - defend / TRAPS
        };
    }
    // A loose or tight caller leans that way less the bigger the bet: even
    // a calling station needs a real hand to call several pots.
    let lean = style.call / (1.0 + bet_size / 4.0);
    // A little tighter than that against ordinary bets, which are mostly for
    // value; not against overbets, where every extra fold is costly.
    let caution = (0.05 + river) / (1.0 + bet_size);
    let threshold = (folds + caution + lean).clamp(0.02, 0.97);
    let mut continuing = soft(strength - threshold, 0.03);
    if drawing {
        // Draws continue when the price is close to their chance of hitting.
        let need = to_call / (pot + to_call);
        if strengths.draw_chance(hole) + 0.06 >= need {
            continuing = continuing.max(0.9);
        }
    }
    let value_from = (threshold + 0.28).clamp(0.82, 0.93);
    let value = (0.5 * style.raise).min(0.9) * soft(strength - value_from, 0.02);
    let semi_bluff = if drawing && outs >= 8 {
        (0.2 * style.bluff).min(0.5)
    } else {
        0.0
    };
    let bluff = (0.04 * style.bluff).min(0.3) * soft(0.3 - strength, 0.03);
    Mix::legal(
        state,
        value + semi_bluff,
        bluff,
        (continuing - value - semi_bluff).max(0.0),
    )
}

/// The sound regular's bet or raise size in `state`, within the legal range.
pub fn solid_raise_size(state: &HandState) -> u32 {
    raise_size(state, SOLID.bet_size)
}

/// A bet of `share` of the pot, or a standard raise, within the legal range.
fn raise_size(state: &HandState, share: f64) -> u32 {
    let legal = state.legal().expect("hand is over");
    let (min, max) = legal.raise.expect("no raise is possible");
    let me = state.to_act().unwrap();
    let bb = state.config().big_blind as f64;
    let facing = state.street_commit(1 - me) as f64;
    let target = if state.street() == Street::Preflop {
        match state.raises() {
            0 if legal.to_call > 0 => 2.5 * bb,
            0 => 4.0 * bb,
            1 => 3.5 * facing,
            _ => 2.3 * facing,
        }
    } else if facing == 0.0 {
        share * state.pot() as f64
    } else {
        3.0 * facing
    };
    // Round to a tenth of a big blind; commit fully rather than leave a sliver behind.
    let step = (bb / 10.0).max(1.0);
    let target = ((target / step).round() * step) as u32;
    let total = state.street_commit(me) + state.stack(me);
    if target as f64 >= 0.5 * total as f64 {
        max
    } else {
        target.clamp(min, max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{parse_cards, shuffled_deck};
    use crate::game::Config;

    fn new_bot(kind: u64, seed: u64) -> Box<dyn Bot> {
        match kind {
            0 => Box::new(SolidBot::new(seed)),
            1 => Box::new(RandomBot::new(seed)),
            2 => Box::new(CallingStation),
            n => Box::new(StyleBot::new(STYLES[(n - 2) as usize], seed)),
        }
    }

    /// Plays `hands` hands and returns seat 0's net result in big blinds.
    fn match_up(a: u64, b: u64, hands: u64) -> f64 {
        let mut total = 0i64;
        let mut bots = [new_bot(a, 1), new_bot(b, 2)];
        for seed in 0..hands {
            let button = (seed % 2) as usize;
            let mut state = HandState::deal(Config::default(), button, &shuffled_deck(seed));
            while let Some(seat) = state.to_act() {
                let action = bots[seat].act(&state, seat);
                state
                    .apply(action)
                    .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
            }
            total += state.outcome().unwrap().net[0];
        }
        total as f64 / 100.0
    }

    #[test]
    fn bots_only_make_legal_moves() {
        for a in 0..6 {
            for b in 0..6 {
                match_up(a, b, 40);
            }
        }
    }

    #[test]
    fn tendencies_are_probabilities() {
        let bots: Vec<Box<dyn Bot>> = (0..6).map(|k| new_bot(k, 0)).collect();
        for seed in 0..40u64 {
            let mut state = HandState::deal(Config::default(), 0, &shuffled_deck(seed));
            let mut driver = SolidBot::new(seed);
            while let Some(seat) = state.to_act() {
                let strengths = Strengths::new(state.board());
                for bot in &bots {
                    for i in (0..crate::range::COMBOS).step_by(37) {
                        let mix = bot.tendencies(&state, crate::range::combo(i), &strengths);
                        let sum = mix.fold + mix.passive + mix.aggressive;
                        assert!((sum - 1.0).abs() < 1e-9, "{mix:?}");
                        assert!(mix.fold >= 0.0 && mix.passive >= 0.0 && mix.aggressive >= 0.0);
                        if state.legal().unwrap().to_call == 0 {
                            assert_eq!(mix.fold, 0.0);
                        }
                    }
                }
                state.apply(driver.act(&state, seat)).unwrap();
            }
        }
    }

    #[test]
    fn solid_plays_sensible_preflop_ranges() {
        let state = HandState::deal(Config::default(), 0, &shuffled_deck(0));
        let strengths = Strengths::new(&[]);
        let bot = SolidBot::new(0);
        let hole = |s: &str| {
            let c = parse_cards(s).unwrap();
            [c[0], c[1]]
        };
        // First in on the button.
        assert!(bot.tendencies(&state, hole("As Ks"), &strengths).aggressive > 0.99);
        assert!(bot.tendencies(&state, hole("7c 2d"), &strengths).fold > 0.99);
        // Facing an all-in, only premium hands continue.
        let mut shove = state.clone();
        shove.apply(Action::RaiseTo(10_000)).unwrap();
        assert!(bot.tendencies(&shove, hole("Ah Ad"), &strengths).passive > 0.99);
        assert!(bot.tendencies(&shove, hole("9h 8h"), &strengths).fold > 0.99);
    }

    /// The share of all hands `style` folds in `state`.
    fn fold_rate(style: Style, state: &HandState) -> f64 {
        let strengths = Strengths::new(state.board());
        let bot = StyleBot::new(style, 0);
        let live: Vec<usize> = (0..crate::range::COMBOS)
            .filter(|&i| {
                crate::range::combo(i)
                    .iter()
                    .all(|c| !state.board().contains(c))
            })
            .collect();
        let folds: f64 = live
            .iter()
            .map(|&i| {
                bot.tendencies(state, crate::range::combo(i), &strengths)
                    .fold
            })
            .sum();
        folds / live.len() as f64
    }

    #[test]
    fn styles_answer_bets_differently() {
        // Seat 0 opens, seat 1 calls; seat 1 checks the flop and seat 0 bets half the pot.
        let mut state = HandState::deal(Config::default(), 0, &shuffled_deck(4));
        for action in [Action::RaiseTo(250), Action::Call, Action::Check] {
            state.apply(action).unwrap();
        }
        state.apply(Action::RaiseTo(250)).unwrap();
        let (station, solid, nit) = (
            fold_rate(STATION, &state),
            fold_rate(SOLID, &state),
            fold_rate(NIT, &state),
        );
        assert!(station < solid && solid < nit, "{station} {solid} {nit}");
        // The sound regular folds no more than half the time to a half-pot
        // bet, so betting any two cards at it does not pay.
        assert!(solid < 0.5, "{solid}");
        assert!(station < 0.2, "{station}");
    }

    #[test]
    fn nobody_folds_everything_to_a_shove() {
        let hole = |s: &str| {
            let c = parse_cards(s).unwrap();
            [c[0], c[1]]
        };
        let strengths = Strengths::new(&[]);
        // The station limps the button, then faces an all-in: the top of
        // what it limped with still calls, though its best hands raised.
        let mut state = HandState::deal(Config::default(), 0, &shuffled_deck(0));
        state.apply(Action::Call).unwrap();
        state.apply(Action::RaiseTo(10_000)).unwrap();
        let station = StyleBot::new(STATION, 0);
        assert!(
            station
                .tendencies(&state, hole("Ah Td"), &strengths)
                .passive
                > 0.9
        );
        assert!(station.tendencies(&state, hole("7c 2d"), &strengths).fold > 0.9);
    }

    #[test]
    fn every_style_has_a_name_to_replay_by() {
        for style in STYLES {
            assert_eq!(by_name(style.name, 0).unwrap().name(), style.name);
        }
        assert!(
            by_name("calling station", 0).is_some(),
            "old records still load"
        );
        assert!(by_name("nobody", 0).is_none());
    }

    #[test]
    fn solid_beats_the_placeholder_bots() {
        let hands = 1_000;
        let vs_random = match_up(0, 1, hands);
        let vs_station = match_up(0, 2, hands);
        assert!(vs_random > 0.0, "solid vs random: {vs_random}bb");
        assert!(vs_station > 0.0, "solid vs station: {vs_station}bb");
    }
}
