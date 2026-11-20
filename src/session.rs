//! A playing session against a bot: deals hands, runs the bot, follows its
//! likely hands for the coach, and keeps score. Pure state: the caller does
//! all input, output and timing.

use crate::analysis::{LeakTally, analyse_with};
use crate::bot::Bot;
use crate::cards::{Rng, shuffled_deck};
use crate::coach::{Feedback, Grade, Spot, review};
use crate::game::{Action, Config, HandState, IllegalAction};
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
        }
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
        let (bot, state, strengths) = (&self.bot, &self.state, &self.strengths);
        self.range
            .update(|hole| bot.tendencies(state, hole, strengths).chance_of(action));
        self.apply(action).expect("bots only make legal moves");
        action
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
        for r in &self.reviews {
            self.tally.add(&r.spot, r.action, &r.feedback);
            *self.grades.entry(r.feedback.grade).or_default() += 1;
            self.given_up += r.feedback.loss;
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
