//! A playing session against a bot: deals hands, runs the bot, follows its
//! likely hands for the coach, and keeps score. Pure state: the caller does
//! all input, output and timing.

use crate::analysis::{Leak, LeakTally, analyse_with};
use crate::bot::Bot;
use crate::cards::{Rng, shuffled_deck};
use crate::coach::{Feedback, Grade, Spot, review};
use crate::game::{Action, Config, HandState, IllegalAction, Street};
use crate::history::HandRecord;
use crate::range::{Range, Strengths};
use std::collections::HashMap;

/// The human player's seat.
pub const HERO: usize = 0;

/// A decision the player made, and the coach's view of it.
#[derive(Clone, Debug)]
pub struct Review {
    pub spot: Spot,
    pub action: Action,
    pub feedback: Feedback,
}

/// What the opponent has been seen to do over finished and current hands:
/// the raw material for reading what kind of player it is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Observed {
    /// Hands finished.
    pub hands: u32,
    /// Hands in which it put chips in by choice before the flop.
    pub played: u32,
    /// Hands in which it raised before the flop.
    pub raised: u32,
    /// Bets and raises after the flop.
    pub bets: u32,
    /// Calls after the flop.
    pub calls: u32,
    /// Bets it faced after the flop.
    pub faced: u32,
    /// Folds to those bets.
    pub folds: u32,
}

impl Observed {
    fn share(part: u32, whole: u32) -> Option<f64> {
        (whole > 0).then(|| part as f64 / whole as f64)
    }

    /// Share of hands it plays.
    pub fn plays(&self) -> Option<f64> {
        Observed::share(self.played, self.hands)
    }

    /// Share of hands it raises before the flop.
    pub fn raises(&self) -> Option<f64> {
        Observed::share(self.raised, self.hands)
    }

    /// Share of bets after the flop that it folds to.
    pub fn folds_to_bets(&self) -> Option<f64> {
        Observed::share(self.folds, self.faced)
    }

    /// Bets and raises as a share of its bets, raises and calls after the flop.
    pub fn aggression(&self) -> Option<f64> {
        Observed::share(self.bets, self.bets + self.calls)
    }

    /// What the numbers suggest, once there are enough hands to say.
    pub fn read(&self) -> Option<&'static str> {
        if self.hands < 12 {
            return None;
        }
        let (plays, raises) = (self.plays()?, self.raises()?);
        let folds = self.folds_to_bets().unwrap_or(0.4);
        let aggression = self.aggression().unwrap_or(0.4);
        Some(if plays < 0.5 {
            "tight: plays few hands"
        } else if raises > 0.6 && aggression > 0.55 {
            "loose and aggressive: raises and bets a lot"
        } else if raises < 0.3 && (folds < 0.3 || aggression < 0.3) {
            "loose and passive: calls a lot, rarely raises"
        } else {
            "balanced: no clear lean yet"
        })
    }
}

pub struct Session {
    cfg: Config,
    seed: u64,
    deals: Rng,
    bot: Box<dyn Bot>,
    hand_no: u64,
    hand_seed: u64,
    state: HandState,
    range: Range,
    strengths: Strengths,
    reviews: Vec<Review>,
    recorded: bool,
    total: i64,
    finished: u64,
    tally: LeakTally,
    grades: HashMap<Grade, u32>,
    given_up: f64,
    reviewed: u32,
    observed: Observed,
    hand_played: bool,
    hand_raised: bool,
    hidden: bool,
}

impl Session {
    /// Starts a session and deals its first hand. The same seed deals the same cards.
    pub fn new(cfg: Config, bot: Box<dyn Bot>, seed: u64) -> Session {
        let mut deals = Rng::new(seed);
        let hand_seed = deals.next_u64();
        let state = HandState::deal(cfg, 0, &shuffled_deck(hand_seed));
        let strengths = Strengths::new(state.board());
        Session {
            cfg,
            seed,
            deals,
            bot,
            hand_no: 1,
            hand_seed,
            state,
            range: Range::uniform(),
            strengths,
            reviews: Vec::new(),
            recorded: false,
            total: 0,
            finished: 0,
            tally: LeakTally::default(),
            grades: HashMap::new(),
            given_up: 0.0,
            reviewed: 0,
            observed: Observed::default(),
            hand_played: false,
            hand_raised: false,
            hidden: false,
        }
    }

    /// Keeps the opponent's name and style from the player until the session
    /// ends: what kind of player it is has to be read from how it plays.
    pub fn hide_opponent(&mut self) {
        self.hidden = true;
    }

    pub fn opponent_hidden(&self) -> bool {
        self.hidden
    }

    /// What the opponent has been seen to do.
    pub fn observed(&self) -> Observed {
        self.observed
    }

    /// Decisions reviewed in finished hands.
    pub fn reviewed(&self) -> u32 {
        self.reviewed
    }

    /// Deals the next hand; the button alternates.
    pub fn next_hand(&mut self) {
        self.hand_no += 1;
        self.hand_seed = self.deals.next_u64();
        let button = ((self.hand_no - 1) % 2) as usize;
        self.state = HandState::deal(self.cfg, button, &shuffled_deck(self.hand_seed));
        self.range = Range::uniform();
        self.strengths = Strengths::new(self.state.board());
        self.reviews.clear();
        self.recorded = false;
        self.hand_played = false;
        self.hand_raised = false;
    }

    pub fn state(&self) -> &HandState {
        &self.state
    }

    pub fn config(&self) -> Config {
        self.cfg
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    pub fn hand_no(&self) -> u64 {
        self.hand_no
    }

    pub fn bot_name(&self) -> &'static str {
        self.bot.name()
    }

    /// The opponent as the player may know it: its name, or "mystery".
    pub fn opponent_label(&self) -> &'static str {
        if self.hidden {
            "mystery"
        } else {
            self.bot.name()
        }
    }

    pub fn bot(&self) -> &dyn Bot {
        self.bot.as_ref()
    }

    /// The bot's likely hands, as far as the player can tell.
    pub fn range(&self) -> &Range {
        &self.range
    }

    pub fn strengths(&self) -> &Strengths {
        &self.strengths
    }

    /// The player's result over finished hands, in chips.
    pub fn total(&self) -> i64 {
        self.total
    }

    pub fn finished(&self) -> u64 {
        self.finished
    }

    /// Mistakes in finished hands, grouped.
    pub fn tally(&self) -> &LeakTally {
        &self.tally
    }

    /// Reviews in finished hands, by grade.
    pub fn grades(&self) -> &HashMap<Grade, u32> {
        &self.grades
    }

    /// Chips given up in finished hands, by the coach's estimate.
    pub fn given_up(&self) -> f64 {
        self.given_up
    }

    /// Reviews of the player's decisions in the current hand.
    pub fn reviews(&self) -> &[Review] {
        &self.reviews
    }

    pub fn hero_to_act(&self) -> bool {
        self.state.to_act() == Some(HERO)
    }

    pub fn bot_to_act(&self) -> bool {
        self.state.to_act() == Some(1 - HERO)
    }

    /// Lets the bot act; it must be its turn. Narrows its likely hands by
    /// what it did. Returns its action.
    pub fn bot_act(&mut self) -> Action {
        assert!(self.bot_to_act(), "not the bot's turn");
        let seat = 1 - HERO;
        let action = self.bot.act(&self.state, seat);
        self.observe(action);
        let (bot, state, strengths) = (&self.bot, &self.state, &self.strengths);
        self.range
            .update(|hole| bot.tendencies(state, hole, strengths).chance_of(action));
        self.apply(action).expect("bots only make legal moves");
        action
    }

    fn observe(&mut self, action: Action) {
        let facing = self.state.legal().is_some_and(|l| l.to_call > 0);
        let o = &mut self.observed;
        if self.state.street() == Street::Preflop {
            self.hand_played |= matches!(action, Action::Call | Action::RaiseTo(_));
            self.hand_raised |= matches!(action, Action::RaiseTo(_));
            return;
        }
        if facing {
            o.faced += 1;
        }
        match action {
            Action::Fold => o.folds += 1,
            Action::Call => o.calls += 1,
            Action::RaiseTo(_) => o.bets += 1,
            Action::Check => {}
        }
    }

    /// The coach's analysis of the player's current decision.
    pub fn analyse(&self) -> Spot {
        Spot::analyse(&self.state, self.bot(), &self.range, &self.strengths, None)
    }

    /// Plays the player's action. With `coach`, it is reviewed against
    /// `spot` (or a fresh analysis when `spot` is `None` or did not price it).
    pub fn hero_act(
        &mut self,
        action: Action,
        coach: bool,
        spot: Option<Spot>,
    ) -> Result<Option<Review>, IllegalAction> {
        if !self.hero_to_act() {
            return Err(IllegalAction {
                action,
                legal: None,
            });
        }
        if let Some(legal) = self.state.legal()
            && !legal.allows(action)
        {
            return Err(IllegalAction {
                action,
                legal: Some(legal),
            });
        }
        let review = coach.then(|| {
            let spot = match spot {
                Some(spot) if spot.ev(action).is_some() => spot,
                _ => analyse_with(
                    &self.state,
                    self.bot(),
                    &self.range,
                    &self.strengths,
                    action,
                ),
            };
            let feedback = review(&spot, action);
            Review {
                spot,
                action,
                feedback,
            }
        });
        self.apply(action)?;
        if let Some(r) = &review {
            self.reviews.push(r.clone());
        }
        Ok(review)
    }

    fn apply(&mut self, action: Action) -> Result<(), IllegalAction> {
        self.state.apply(action)?;
        if self.strengths.board() != self.state.board() {
            self.strengths = Strengths::new(self.state.board());
        }
        Ok(())
    }

    /// Counts a decision reviewed before this session object existed, so a
    /// saved session can be resumed with its score intact: its grade, the
    /// chips it gave up and the kind of mistake it was, if any.
    pub fn credit(&mut self, street: Street, leak: Option<Leak>, grade: Grade, loss: f64) {
        if let Some(leak) = leak {
            self.tally.record(street, leak, loss);
        }
        *self.grades.entry(grade).or_default() += 1;
        self.given_up += loss;
        self.reviewed += 1;
    }

    /// Once the hand is over: scores it and returns its record, the first
    /// time only. Reviews count towards the session only for finished hands.
    pub fn finish_hand(&mut self) -> Option<HandRecord> {
        let outcome = self.state.outcome()?;
        if self.recorded {
            return None;
        }
        self.recorded = true;
        self.total += outcome.net[HERO];
        self.finished += 1;
        self.observed.hands += 1;
        self.observed.played += u32::from(self.hand_played);
        self.observed.raised += u32::from(self.hand_raised);
        for r in &self.reviews {
            self.tally.add(&r.spot, r.action, &r.feedback);
            *self.grades.entry(r.feedback.grade).or_default() += 1;
            self.given_up += r.feedback.loss;
            self.reviewed += 1;
        }
        Some(HandRecord::from_state(
            &self.state,
            self.hand_no,
            self.hand_seed,
            HERO,
            self.bot.name(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::SolidBot;

    #[test]
    fn a_session_plays_scores_and_records_hands() {
        let mut session = Session::new(Config::default(), Box::new(SolidBot::new(3)), 3);
        let mut hero = SolidBot::new(99);
        for _ in 0..6 {
            while let Some(seat) = session.state().to_act() {
                if seat == HERO {
                    let action = hero.act(session.state(), HERO);
                    let review = session.hero_act(action, true, None).unwrap();
                    assert!(review.is_some());
                } else {
                    session.bot_act();
                }
            }
            let record = session.finish_hand().expect("hand over");
            assert!(session.finish_hand().is_none(), "only recorded once");
            record.replay().unwrap();
            session.next_hand();
        }
        assert_eq!(session.finished(), 6);
        assert_eq!(session.hand_no(), 7);
    }

    #[test]
    fn same_seed_same_cards() {
        let a = Session::new(Config::default(), Box::new(SolidBot::new(0)), 42);
        let b = Session::new(Config::default(), Box::new(SolidBot::new(0)), 42);
        assert_eq!(a.state().hole(HERO), b.state().hole(HERO));
    }
}
