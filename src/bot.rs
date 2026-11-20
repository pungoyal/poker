//! Computer opponents. These are placeholders for exercising the engine;
//! the solver-backed bot will implement the same trait.

use crate::cards::Rng;
use crate::game::{Action, HandState};

pub trait Bot {
    fn name(&self) -> &'static str;

    /// Chooses an action for `seat`, which is the player to act.
    /// A bot may only look at its own hole cards and the visible board.
    fn act(&mut self, state: &HandState, seat: usize) -> Action;
}

/// Never folds, never bets: checks when it can and calls otherwise.
pub struct CallingStation;

impl Bot for CallingStation {
    fn name(&self) -> &'static str {
        "calling station"
    }

    fn act(&mut self, state: &HandState, _seat: usize) -> Action {
        let legal = state.legal().expect("bot asked to act in a finished hand");
        if legal.to_call > 0 {
            Action::Call
        } else {
            Action::Check
        }
    }
}

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
            && roll < 0.3
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
        } else if roll > 0.8 {
            Action::Fold
        } else {
            Action::Call
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::shuffled_deck;
    use crate::game::Config;

    #[test]
    fn bots_only_make_legal_moves() {
        for seed in 0..2_000u64 {
            let mut bots: [Box<dyn Bot>; 2] = [
                Box::new(RandomBot::new(seed)),
                Box::new(RandomBot::new(!seed)),
            ];
            if seed % 3 == 0 {
                bots[1] = Box::new(CallingStation);
            }
            let mut state = HandState::deal(Config::default(), 0, &shuffled_deck(seed));
            while let Some(seat) = state.to_act() {
                let action = bots[seat].act(&state, seat);
                state
                    .apply(action)
                    .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
            }
        }
    }
}
