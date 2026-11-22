//! Looking back over played hands: every decision re-run through the coach,
//! and the mistakes grouped into leaks to work on.

use crate::bot::{Bot, by_name};
use crate::coach::{Feedback, Grade, Spot, review};
use crate::game::{Action, HandState, Street};
use crate::history::HandRecord;
use crate::range::{Range, Strengths};
use std::collections::BTreeMap;

/// One decision the player made, with the coach's view of it.
#[derive(Clone, Debug)]
pub struct Decision {
    pub hand_no: u64,
    pub opponent: String,
    /// The hand just before the decision.
    pub state: HandState,
    /// The opponent's likely hands at that point.
    pub range: Range,
    pub spot: Spot,
    pub action: Action,
    pub feedback: Feedback,
}

/// Prices a spot, including the raise size actually chosen if it is not a standard one.
pub fn analyse_with(
    state: &HandState,
    bot: &dyn Bot,
    range: &Range,
    strengths: &Strengths,
    action: Action,
) -> Spot {
    let spot = Spot::analyse(state, bot, range, strengths, None);
    match action {
        Action::RaiseTo(amount) if spot.response(amount).is_none() => {
            Spot::analyse(state, bot, range, strengths, Some(amount))
        }
        _ => spot,
    }
}

/// Re-runs the coach over every decision the player made in `record`.
pub fn decisions(record: &HandRecord) -> Result<Vec<Decision>, String> {
    let bot = by_name(&record.opponent, 0).ok_or_else(|| {
        format!(
            "hand {}: unknown opponent '{}'",
            record.hand_no, record.opponent
        )
    })?;
    record.validate()?;
    let mut state = HandState::new(record.config, record.button, record.hole, record.board);
    let mut range = Range::uniform();
    let mut strengths = Strengths::new(state.board());
    let mut found = Vec::new();
    for &(seat, action) in &record.actions {
        if seat == record.hero {
            let spot = analyse_with(&state, bot.as_ref(), &range, &strengths, action);
            let feedback = review(&spot, action);
            found.push(Decision {
                hand_no: record.hand_no,
                opponent: record.opponent.clone(),
                state: state.clone(),
                range: range.clone(),
                spot,
                action,
                feedback,
            });
        } else {
            range.update(|hole| bot.tendencies(&state, hole, &strengths).chance_of(action));
        }
        state
            .apply(action)
            .map_err(|e| format!("hand {}: {e}", record.hand_no))?;
        if strengths.board() != state.board() {
            strengths = Strengths::new(state.board());
        }
    }
    Ok(found)
}

/// A recurring kind of mistake.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Leak {
    /// Folded when continuing was better.
    FoldedTooMuch,
    /// Called when folding was better.
    CalledTooLight,
    /// Called when raising was better.
    MissedRaise,
    /// Checked when betting was better.
    MissedBet,
    /// Bet or raised when checking, calling or folding was better.
    WastedBet,
    /// Bet too small.
    BetTooSmall,
    /// Bet too big.
    BetTooBig,
}

impl Leak {
    /// The kind of mistake taking `action` was, if it was a real mistake.
    pub fn of(spot: &Spot, action: Action, feedback: &Feedback) -> Option<Leak> {
        if !matches!(
            feedback.grade,
            Grade::Inaccuracy | Grade::Mistake | Grade::Blunder
        ) {
            return None;
        }
        let (best, _) = *spot.ranked(Some(action)).first()?;
        Some(match (action, best) {
            (Action::Fold, _) => Leak::FoldedTooMuch,
            (Action::Call, Action::Fold) => Leak::CalledTooLight,
            (Action::Call, _) => Leak::MissedRaise,
            (Action::Check, _) => Leak::MissedBet,
            (Action::RaiseTo(chosen), Action::RaiseTo(better)) if better > chosen => {
                Leak::BetTooSmall
            }
            (Action::RaiseTo(_), Action::RaiseTo(_)) => Leak::BetTooBig,
            (Action::RaiseTo(_), _) => Leak::WastedBet,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Leak::FoldedTooMuch => "folding when continuing was better",
            Leak::CalledTooLight => "calling when folding was better",
            Leak::MissedRaise => "calling when raising was better",
            Leak::MissedBet => "checking when betting was better",
            Leak::WastedBet => "betting when it did not pay",
            Leak::BetTooSmall => "betting too small",
            Leak::BetTooBig => "betting too big",
        }
    }

    /// The principle that fixes it.
    pub fn tip(self) -> &'static str {
        match self {
            Leak::FoldedTooMuch => {
                "Before folding, compare your equity with the pot odds: a cheap call only needs to win a small share of the time."
            }
            Leak::CalledTooLight => {
                "A bet narrows the bettor's hands towards strong ones. Judge your equity against what they bet with, not against any two cards."
            }
            Leak::MissedRaise => {
                "When you are well ahead of the hands that bet, raise: they will often call, and more chips go in while you are ahead."
            }
            Leak::MissedBet => {
                "Bet your strong hands and some of your draws: checking lets worse hands see cards for free and wins smaller pots."
            }
            Leak::WastedBet => {
                "Ask who calls and who folds. A bet that only folds worse hands and gets called by better ones loses money."
            }
            Leak::BetTooSmall => {
                "When worse hands call, bigger bets win more. Size up with strong hands."
            }
            Leak::BetTooBig => {
                "A bluff only needs enough size to get folds; anything more is lost when it is called."
            }
        }
    }
}

/// Mistakes grouped by street and kind, with what they cost.
#[derive(Clone, Debug, Default)]
pub struct LeakTally {
    by_kind: BTreeMap<(Street, Leak), (u32, f64)>,
}

/// One row of a [`LeakTally`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LeakLine {
    pub street: Street,
    pub leak: Leak,
    pub count: u32,
    /// Chips given up in total.
    pub loss: f64,
}

impl LeakTally {
    /// Counts one mistake of a known kind and cost, e.g. from a saved session.
    pub fn record(&mut self, street: Street, leak: Leak, loss: f64) {
        let entry = self.by_kind.entry((street, leak)).or_default();
        entry.0 += 1;
        entry.1 += loss;
    }

    pub fn add(&mut self, spot: &Spot, action: Action, feedback: &Feedback) {
        if let Some(leak) = Leak::of(spot, action, feedback) {
            self.record(spot.street, leak, feedback.loss);
        }
    }

    /// Leaks, costliest first.
    pub fn lines(&self) -> Vec<LeakLine> {
        let mut lines: Vec<LeakLine> = self
            .by_kind
            .iter()
            .map(|(&(street, leak), &(count, loss))| LeakLine {
                street,
                leak,
                count,
                loss,
            })
            .collect();
        lines.sort_by(|a, b| b.loss.total_cmp(&a.loss));
        lines
    }

    pub fn is_empty(&self) -> bool {
        self.by_kind.is_empty()
    }
}

/// The decisions worth drilling: real mistakes, costliest first.
pub fn drill_spots(decisions: Vec<Decision>, limit: usize) -> Vec<Decision> {
    let mut mistakes: Vec<Decision> = decisions
        .into_iter()
        .filter(|d| Leak::of(&d.spot, d.action, &d.feedback).is_some())
        .collect();
    mistakes.sort_by(|a, b| b.feedback.loss.total_cmp(&a.feedback.loss));
    mistakes.truncate(limit);
    mistakes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::SolidBot;
    use crate::cards::shuffled_deck;
    use crate::game::Config;

    /// A hand where the player always folds to a bet and calls otherwise.
    fn timid_hand(seed: u64) -> HandRecord {
        let mut bot = SolidBot::new(seed);
        let mut state =
            HandState::deal(Config::default(), (seed % 2) as usize, &shuffled_deck(seed));
        while let Some(seat) = state.to_act() {
            let action = if seat == 0 {
                if state.legal().unwrap().to_call > 0 {
                    Action::Fold
                } else {
                    Action::Check
                }
            } else {
                bot.act(&state, seat)
            };
            state.apply(action).unwrap();
        }
        HandRecord::from_state(&state, seed, seed, 0, "solid")
    }

    #[test]
    fn folding_everything_shows_up_as_a_leak() {
        let mut tally = LeakTally::default();
        let mut all = Vec::new();
        for seed in 0..12 {
            for d in decisions(&timid_hand(seed)).unwrap() {
                tally.add(&d.spot, d.action, &d.feedback);
                all.push(d);
            }
        }
        let lines = tally.lines();
        assert!(!lines.is_empty());
        assert!(
            lines.iter().any(|l| l.leak == Leak::FoldedTooMuch),
            "{lines:?}"
        );
        let drills = drill_spots(all, 3);
        assert!(!drills.is_empty() && drills.len() <= 3);
        assert!(
            drills
                .windows(2)
                .all(|w| w[0].feedback.loss >= w[1].feedback.loss)
        );
    }

    #[test]
    fn unknown_opponents_are_an_error() {
        let mut record = timid_hand(1);
        record.opponent = "mystery".into();
        assert!(decisions(&record).is_err());
    }
}
