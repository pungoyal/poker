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
}

/// The bot with this name, if there is one.
pub fn by_name(name: &str, seed: u64) -> Option<Box<dyn Bot>> {
    match name {
        "solid" => Some(Box::new(SolidBot::new(seed))),
        "random" => Some(Box::new(RandomBot::new(seed))),
        "calling station" => Some(Box::new(CallingStation)),
        _ => None,
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

/// A sound, straightforward regular. Before the flop it plays a range of
/// starting hands that narrows as the bets get bigger; after it, it bets and
/// calls by hand strength and pot odds, semi-bluffs draws, continuation-bets
/// with the initiative and bluffs now and then. Bets are one size for value
/// and bluffs alike, so the size gives nothing away.
pub struct SolidBot {
    rng: Rng,
    strengths: Option<Strengths>,
}

impl SolidBot {
    pub fn new(seed: u64) -> SolidBot {
        SolidBot {
            rng: Rng::new(seed),
            strengths: None,
        }
    }
}

/// Smooth step from 0 to 1 as `x` passes zero; `width` sets how gradual.
fn soft(x: f64, width: f64) -> f64 {
    1.0 / (1.0 + (-x / width).exp())
}

impl Bot for SolidBot {
    fn name(&self) -> &'static str {
        "solid"
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
            Kind::Aggressive => Action::RaiseTo(solid_raise_size(state)),
        }
    }

    fn tendencies(&self, state: &HandState, hole: [Card; 2], strengths: &Strengths) -> Mix {
        if state.street() == Street::Preflop {
            solid_preflop(state, hole, strengths)
        } else {
            solid_postflop(state, hole, strengths)
        }
    }
}

fn solid_preflop(state: &HandState, hole: [Card; 2], strengths: &Strengths) -> Mix {
    let legal = state.legal().expect("hand is over");
    let me = state.to_act().unwrap();
    // How far from the top: 0 for aces, 1 for seven-deuce.
    let p = 1.0 - strengths.of(hole);
    let bb = state.config().big_blind as f64;
    if state.raises() == 0 {
        return if legal.to_call > 0 {
            // First in on the button: raise most hands, fold the rest, never limp.
            Mix::legal(state, soft(0.8 - p, 0.015), 0.0, 0.0)
        } else {
            // The button limped: raise for value, check the rest.
            Mix::legal(state, soft(0.3 - p, 0.015), 0.0, 1.0)
        };
    }
    // Facing a raise: the bigger it is, the fewer hands continue.
    let size = state.street_commit(1 - me) as f64 / bb;
    // Facing a re-raise it already showed strength, so it defends more of
    // what it has left rather than folding to every 4-bet.
    let defend = match state.raises() {
        1 => 1.6,
        2 => 3.8,
        _ => 2.6,
    };
    let width = (defend / size).clamp(0.04, 0.68);
    // Edges blur in proportion to the range: tight ranges have sharp edges.
    let blur = |w: f64| (0.03 * w).max(0.001);
    let continuing = soft(width - p, blur(width));
    let value = soft(0.22 * width - p, blur(0.22 * width));
    // A few re-raise bluffs from the hands just too weak to call an open.
    let bluff = if state.raises() == 1 {
        0.3 * soft(p - width, blur(width)) * soft(width + 0.12 - p, blur(width))
    } else {
        0.0
    };
    Mix::legal(state, value, bluff, (continuing - value).max(0.0))
}

fn solid_postflop(state: &HandState, hole: [Card; 2], strengths: &Strengths) -> Mix {
    let legal = state.legal().expect("hand is over");
    let me = state.to_act().unwrap();
    let street = state.street();
    let strength = strengths.of(hole);
    let outs = strengths.outs(hole);
    let drawing = street != Street::River && outs >= 4;

    if legal.to_call == 0 {
        let value = 0.85 * soft(strength - 0.8, 0.03);
        let thin = 0.35 * soft(strength - 0.62, 0.03);
        let semi_bluff = match outs {
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
        let bet = value.max(thin).max(semi_bluff).max(bluff);
        return Mix::legal(state, bet, 0.0, 1.0);
    }

    // Facing a bet: continue with hands that beat what the bet represents.
    let pot = state.pot() as f64;
    let to_call = legal.to_call as f64;
    let bet_size = to_call / (pot - to_call).max(1.0);
    let river = if street == Street::River { 0.05 } else { 0.0 };
    let threshold = (0.44 + 0.2 * bet_size.min(1.5) + river).min(0.92);
    let mut continuing = soft(strength - threshold, 0.03);
    if drawing {
        // Draws continue when the price is close to their chance of hitting.
        let need = to_call / (pot + to_call);
        if strengths.draw_chance(hole) + 0.06 >= need {
            continuing = continuing.max(0.9);
        }
    }
    let value = 0.5 * soft(strength - (threshold + 0.28).min(0.95), 0.02);
    let semi_bluff = if drawing && outs >= 8 { 0.2 } else { 0.0 };
    let bluff = 0.04 * soft(0.3 - strength, 0.03);
    Mix::legal(
        state,
        value + semi_bluff,
        bluff,
        (continuing - value - semi_bluff).max(0.0),
    )
}

/// The solid bot's bet or raise size in `state`, within the legal range.
pub fn solid_raise_size(state: &HandState) -> u32 {
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
        0.66 * state.pot() as f64
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
            _ => Box::new(CallingStation),
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
        for a in 0..3 {
            for b in 0..3 {
                match_up(a, b, 150);
            }
        }
    }

    #[test]
    fn tendencies_are_probabilities() {
        let bots: Vec<Box<dyn Bot>> = (0..3).map(|k| new_bot(k, 0)).collect();
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

    #[test]
    fn solid_beats_the_placeholder_bots() {
        let hands = 1_000;
        let vs_random = match_up(0, 1, hands);
        let vs_station = match_up(0, 2, hands);
        assert!(vs_random > 0.0, "solid vs random: {vs_random}bb");
        assert!(vs_station > 0.0, "solid vs station: {vs_station}bb");
    }
}
