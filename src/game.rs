//! Heads-up No-Limit Hold'em rules for a single hand.
//!
//! The hand is a deterministic state machine: given the cards and the
//! sequence of actions, the result is always the same. Chips are integers.
//! The button posts the small blind, acts first before the flop and last
//! on every later street.

use crate::cards::Card;
use crate::eval::{HandRank, evaluate};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    pub small_blind: u32,
    pub big_blind: u32,
    /// Each player's stack at the start of the hand.
    pub stack: u32,
}

impl Config {
    /// Largest stack the engine accepts. Leaves headroom so that the pot plus a
    /// call or bet (at most four stacks' worth of chips) always fits in a `u32`.
    pub const MAX_STACK: u32 = u32::MAX / 4;

    /// Checks that the blinds and stack make a playable hand.
    pub fn validate(&self) -> Result<(), String> {
        if self.small_blind == 0 || self.small_blind > self.big_blind {
            Err("the small blind must be positive and no bigger than the big blind".into())
        } else if self.stack < self.big_blind {
            Err("the stack must cover the big blind".into())
        } else if self.stack > Config::MAX_STACK {
            Err(format!(
                "the stack may be at most {} chips",
                Config::MAX_STACK
            ))
        } else {
            Ok(())
        }
    }
}

impl Default for Config {
    /// 100 big blinds deep, with chips fine enough for 0.01bb bet sizes.
    fn default() -> Config {
        Config {
            small_blind: 50,
            big_blind: 100,
            stack: 10_000,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Street {
    Preflop,
    Flop,
    Turn,
    River,
}

impl Street {
    pub fn name(self) -> &'static str {
        match self {
            Street::Preflop => "Preflop",
            Street::Flop => "Flop",
            Street::Turn => "Turn",
            Street::River => "River",
        }
    }

    fn board_cards(self) -> usize {
        match self {
            Street::Preflop => 0,
            Street::Flop => 3,
            Street::Turn => 4,
            Street::River => 5,
        }
    }

    fn next(self) -> Option<Street> {
        match self {
            Street::Preflop => Some(Street::Flop),
            Street::Flop => Some(Street::Turn),
            Street::Turn => Some(Street::River),
            Street::River => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    Fold,
    Check,
    Call,
    /// A bet or raise, given as the player's total chips in on this street.
    RaiseTo(u32),
}

/// What the player to act may do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Legal {
    /// Chips needed to call; zero means checking is allowed (and folding is not).
    pub to_call: u32,
    /// Smallest and largest legal `RaiseTo` amounts, if betting or raising is possible.
    pub raise: Option<(u32, u32)>,
}

impl Legal {
    pub fn allows(&self, action: Action) -> bool {
        match action {
            Action::Fold | Action::Call => self.to_call > 0,
            Action::Check => self.to_call == 0,
            Action::RaiseTo(amount) => self
                .raise
                .is_some_and(|(min, max)| (min..=max).contains(&amount)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Outcome {
    /// `None` is a split pot.
    pub winner: Option<usize>,
    pub showdown: bool,
    /// Chips won or lost by each seat; always sums to zero.
    pub net: [i64; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IllegalAction {
    pub action: Action,
    pub legal: Option<Legal>,
}

impl fmt::Display for IllegalAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.legal {
            None => write!(f, "{:?} is not possible: the hand is over", self.action),
            Some(legal) => write!(f, "{:?} is not legal here ({legal:?})", self.action),
        }
    }
}

impl std::error::Error for IllegalAction {}

#[derive(Clone, Debug)]
pub struct HandState {
    cfg: Config,
    button: usize,
    hole: [[Card; 2]; 2],
    full_board: [Card; 5],
    street: Street,
    /// How many board cards are face up (five after an all-in runout).
    board_len: usize,
    /// Chips behind for each seat.
    stacks: [u32; 2],
    /// Chips put in on the current street.
    street_commit: [u32; 2],
    /// Chips put in over the whole hand.
    total_commit: [u32; 2],
    to_act: usize,
    /// Size of the last bet or raise on this street; the next raise must be at least as big.
    last_raise: u32,
    acted: [bool; 2],
    actions: Vec<(usize, Action)>,
    outcome: Option<Outcome>,
}

impl HandState {
    /// Starts a hand with the blinds posted. `button` is the seat (0 or 1)
    /// that has the button and the small blind.
    pub fn new(cfg: Config, button: usize, hole: [[Card; 2]; 2], board: [Card; 5]) -> HandState {
        assert!(button < 2);
        if let Err(error) = cfg.validate() {
            panic!("invalid config: {error}");
        }
        let mut state = HandState {
            cfg,
            button,
            hole,
            full_board: board,
            street: Street::Preflop,
            board_len: 0,
            stacks: [cfg.stack; 2],
            street_commit: [0; 2],
            total_commit: [0; 2],
            to_act: button,
            last_raise: cfg.big_blind,
            acted: [false; 2],
            actions: Vec::new(),
            outcome: None,
        };
        state.put_in(button, cfg.small_blind);
        state.put_in(1 - button, cfg.big_blind);
        state
    }

    /// Deals a hand from a shuffled deck: two cards each, then the board.
    pub fn deal(cfg: Config, button: usize, deck: &[Card; 52]) -> HandState {
        let hole = [[deck[0], deck[1]], [deck[2], deck[3]]];
        let board = [deck[4], deck[5], deck[6], deck[7], deck[8]];
        HandState::new(cfg, button, hole, board)
    }

    pub fn config(&self) -> Config {
        self.cfg
    }

    pub fn button(&self) -> usize {
        self.button
    }

    pub fn street(&self) -> Street {
        self.street
    }

    /// The board cards currently face up.
    pub fn board(&self) -> &[Card] {
        &self.full_board[..self.board_len]
    }

    /// All five board cards, including ones not yet dealt. For records and replay only.
    pub fn full_board(&self) -> [Card; 5] {
        self.full_board
    }

    pub fn hole(&self, seat: usize) -> [Card; 2] {
        self.hole[seat]
    }

    pub fn stack(&self, seat: usize) -> u32 {
        self.stacks[seat]
    }

    pub fn street_commit(&self, seat: usize) -> u32 {
        self.street_commit[seat]
    }

    /// All chips in the middle, including bets on the current street.
    pub fn pot(&self) -> u32 {
        self.total_commit[0] + self.total_commit[1]
    }

    pub fn actions(&self) -> &[(usize, Action)] {
        &self.actions
    }

    pub fn outcome(&self) -> Option<Outcome> {
        self.outcome
    }

    /// The seat whose turn it is, or `None` once the hand is over.
    pub fn to_act(&self) -> Option<usize> {
        self.outcome.is_none().then_some(self.to_act)
    }

    /// Legal actions for the player to act, or `None` once the hand is over.
    pub fn legal(&self) -> Option<Legal> {
        if self.outcome.is_some() {
            return None;
        }
        let me = self.to_act;
        let opp = 1 - me;
        let current_bet = self.street_commit[opp];
        let to_call = current_bet.saturating_sub(self.street_commit[me]);
        // Nobody can put in more than the smaller stack can match.
        let max = (self.street_commit[me] + self.stacks[me])
            .min(self.street_commit[opp] + self.stacks[opp]);
        let raise = (max > current_bet).then(|| ((current_bet + self.last_raise).min(max), max));
        Some(Legal { to_call, raise })
    }

    pub fn apply(&mut self, action: Action) -> Result<(), IllegalAction> {
        let legal = self.legal();
        let Some(legal) = legal.filter(|l| l.allows(action)) else {
            return Err(IllegalAction { action, legal });
        };
        let me = self.to_act;
        let opp = 1 - me;
        self.actions.push((me, action));
        self.acted[me] = true;

        match action {
            Action::Fold => {
                // The winner collects what the folder put in; any unmatched bet comes back.
                let lost = self.total_commit[me] as i64;
                let mut net = [0i64; 2];
                net[me] = -lost;
                net[opp] = lost;
                self.outcome = Some(Outcome {
                    winner: Some(opp),
                    showdown: false,
                    net,
                });
                return Ok(());
            }
            Action::Check => {}
            Action::Call => self.put_in(me, legal.to_call),
            Action::RaiseTo(amount) => {
                let raise_size = amount - self.street_commit[opp];
                self.last_raise = self.last_raise.max(raise_size);
                self.put_in(me, amount - self.street_commit[me]);
            }
        }

        let matched = self.street_commit[0] == self.street_commit[1];
        if matched && self.acted[0] && self.acted[1] {
            self.close_street();
        } else {
            self.to_act = opp;
        }
        Ok(())
    }

    fn put_in(&mut self, seat: usize, chips: u32) {
        self.stacks[seat] -= chips;
        self.street_commit[seat] += chips;
        self.total_commit[seat] += chips;
    }

    fn close_street(&mut self) {
        let all_in = self.stacks[0] == 0 || self.stacks[1] == 0;
        match self.street.next() {
            Some(next) if !all_in => {
                self.street = next;
                self.board_len = next.board_cards();
                self.street_commit = [0; 2];
                self.acted = [false; 2];
                self.last_raise = self.cfg.big_blind;
                self.to_act = 1 - self.button;
            }
            _ => {
                // River betting is done, or someone is all-in: run out the board.
                self.board_len = 5;
                self.showdown();
            }
        }
    }

    fn showdown(&mut self) {
        let ranks = [self.showdown_rank(0), self.showdown_rank(1)];
        let stake = self.total_commit[0] as i64;
        debug_assert_eq!(self.total_commit[0], self.total_commit[1]);
        let (winner, net) = match ranks[0].cmp(&ranks[1]) {
            std::cmp::Ordering::Greater => (Some(0), [stake, -stake]),
            std::cmp::Ordering::Less => (Some(1), [-stake, stake]),
            std::cmp::Ordering::Equal => (None, [0, 0]),
        };
        self.outcome = Some(Outcome {
            winner,
            showdown: true,
            net,
        });
    }

    /// A seat's hand strength using all five board cards.
    pub fn showdown_rank(&self, seat: usize) -> HandRank {
        let mut cards = self.full_board.to_vec();
        cards.extend(self.hole[seat]);
        evaluate(&cards)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{Rng, parse_cards, shuffled_deck};
    use Action::*;

    /// Seat 0 holds aces, seat 1 holds kings, on a board that helps neither.
    fn hand(button: usize) -> HandState {
        hand_with(Config::default(), button)
    }

    fn hand_with(cfg: Config, button: usize) -> HandState {
        let c = parse_cards("As Ah Ks Kh 2c 7d 9h Jc 3s").unwrap();
        HandState::new(
            cfg,
            button,
            [[c[0], c[1]], [c[2], c[3]]],
            [c[4], c[5], c[6], c[7], c[8]],
        )
    }

    fn play(state: &mut HandState, actions: &[Action]) {
        for &action in actions {
            state.apply(action).unwrap_or_else(|e| panic!("{e}"));
        }
    }

    #[test]
    fn blinds_and_first_action() {
        let state = hand(0);
        assert_eq!(state.pot(), 150);
        assert_eq!(state.stack(0), 9_950);
        assert_eq!(state.stack(1), 9_900);
        assert_eq!(state.to_act(), Some(0));
        // The button must complete 50 or raise to at least 200.
        assert_eq!(
            state.legal(),
            Some(Legal {
                to_call: 50,
                raise: Some((200, 10_000))
            })
        );
    }

    #[test]
    fn limp_gives_the_big_blind_an_option() {
        let mut state = hand(0);
        play(&mut state, &[Call]);
        assert_eq!(state.street(), Street::Preflop);
        assert_eq!(state.to_act(), Some(1));
        assert_eq!(
            state.legal(),
            Some(Legal {
                to_call: 0,
                raise: Some((200, 10_000))
            })
        );
        play(&mut state, &[Check]);
        assert_eq!(state.street(), Street::Flop);
        assert_eq!(state.board().len(), 3);
    }

    #[test]
    fn big_blind_acts_first_after_the_flop() {
        let mut state = hand(0);
        play(&mut state, &[RaiseTo(250), Call]);
        assert_eq!(state.street(), Street::Flop);
        assert_eq!(state.pot(), 500);
        assert_eq!(state.to_act(), Some(1));
        // A check from the first player does not end the street.
        play(&mut state, &[Check]);
        assert_eq!(state.street(), Street::Flop);
        assert_eq!(state.to_act(), Some(0));
        play(&mut state, &[Check]);
        assert_eq!(state.street(), Street::Turn);
        assert_eq!(state.to_act(), Some(1));
    }

    #[test]
    fn minimum_raise_matches_the_previous_raise() {
        let mut state = hand(0);
        play(&mut state, &[RaiseTo(300)]); // a raise of 200 over the big blind
        assert_eq!(state.legal().unwrap().raise, Some((500, 10_000)));
        assert!(state.apply(RaiseTo(499)).is_err());
        play(&mut state, &[RaiseTo(900)]); // a raise of 600
        assert_eq!(state.legal().unwrap().raise, Some((1_500, 10_000)));
        play(&mut state, &[Call]);
        // A new street starts with a minimum bet of one big blind.
        assert_eq!(
            state.legal(),
            Some(Legal {
                to_call: 0,
                raise: Some((100, 9_100))
            })
        );
    }

    #[test]
    fn illegal_actions_are_rejected_without_changing_state() {
        let mut state = hand(0);
        assert!(state.apply(Check).is_err());
        assert!(state.apply(RaiseTo(150)).is_err());
        assert!(state.apply(RaiseTo(10_001)).is_err());
        play(&mut state, &[Call]);
        assert!(
            state.apply(Fold).is_err(),
            "no folding when checking is free"
        );
        assert!(state.apply(Call).is_err());
        assert_eq!(state.actions().len(), 1);
        assert_eq!(state.pot(), 200);
    }

    #[test]
    fn fold_awards_the_pot_and_returns_the_unmatched_bet() {
        let mut state = hand(0);
        play(&mut state, &[RaiseTo(300), RaiseTo(1_000), Fold]);
        let outcome = state.outcome().unwrap();
        assert_eq!(
            outcome,
            Outcome {
                winner: Some(1),
                showdown: false,
                net: [-300, 300]
            }
        );
        assert_eq!(state.to_act(), None);
        assert_eq!(state.legal(), None);
        assert!(state.apply(Check).is_err());
    }

    #[test]
    fn showdown_after_river_betting() {
        let mut state = hand(1);
        play(&mut state, &[RaiseTo(250), Call]); // seat 1 opens, seat 0 calls
        play(&mut state, &[Check, Check, Check, Check]); // flop, turn
        assert_eq!(state.street(), Street::River);
        play(&mut state, &[RaiseTo(400), Call]);
        let outcome = state.outcome().unwrap();
        assert_eq!(
            outcome,
            Outcome {
                winner: Some(0),
                showdown: true,
                net: [650, -650]
            }
        );
    }

    #[test]
    fn all_in_call_runs_out_the_board() {
        let mut state = hand(0);
        play(&mut state, &[RaiseTo(10_000)]);
        // Facing an all-in there is nothing left to raise.
        assert_eq!(
            state.legal(),
            Some(Legal {
                to_call: 9_900,
                raise: None
            })
        );
        play(&mut state, &[Call]);
        assert_eq!(state.board().len(), 5);
        let outcome = state.outcome().unwrap();
        assert_eq!(
            outcome,
            Outcome {
                winner: Some(0),
                showdown: true,
                net: [10_000, -10_000]
            }
        );
    }

    #[test]
    fn short_all_in_below_the_minimum_raise_is_allowed() {
        let cfg = Config {
            small_blind: 50,
            big_blind: 100,
            stack: 350,
        };
        let mut state = hand_with(cfg, 0);
        play(&mut state, &[RaiseTo(300)]);
        // A full raise would be to 500, but only 350 is available.
        assert_eq!(state.legal().unwrap().raise, Some((350, 350)));
        play(&mut state, &[RaiseTo(350)]);
        assert_eq!(
            state.legal(),
            Some(Legal {
                to_call: 50,
                raise: None
            })
        );
        play(&mut state, &[Call]);
        assert_eq!(state.outcome().unwrap().net, [350, -350]);
    }

    #[test]
    fn identical_hands_split_the_pot() {
        // Both players play the broadway straight on the board.
        let c = parse_cards("2c 3d 2h 3s Ts Jd Qh Kc As").unwrap();
        let mut state = HandState::new(
            Config::default(),
            0,
            [[c[0], c[1]], [c[2], c[3]]],
            [c[4], c[5], c[6], c[7], c[8]],
        );
        play(&mut state, &[RaiseTo(10_000), Call]);
        assert_eq!(
            state.outcome().unwrap(),
            Outcome {
                winner: None,
                showdown: true,
                net: [0, 0]
            }
        );
    }

    #[test]
    fn random_playouts_keep_chips_consistent() {
        let cfg = Config::default();
        for seed in 0..5_000u64 {
            let mut rng = Rng::new(seed);
            let button = (seed % 2) as usize;
            let mut state = HandState::deal(cfg, button, &shuffled_deck(seed));
            let mut steps = 0;
            while let Some(legal) = state.legal() {
                let mut options = Vec::new();
                if legal.to_call > 0 {
                    options.extend([Fold, Call, Call]);
                } else {
                    options.extend([Check, Check]);
                }
                if let Some((min, max)) = legal.raise {
                    options.extend([RaiseTo(min), RaiseTo(max)]);
                    options.push(RaiseTo(min + rng.below((max - min) as u64 + 1) as u32));
                }
                let action = options[rng.below(options.len() as u64) as usize];
                state
                    .apply(action)
                    .unwrap_or_else(|e| panic!("seed {seed}: {e}"));

                assert_eq!(state.pot() + state.stack(0) + state.stack(1), 2 * cfg.stack);
                steps += 1;
                assert!(steps < 500, "seed {seed}: hand did not finish");
            }
            let outcome = state.outcome().unwrap();
            assert_eq!(outcome.net[0] + outcome.net[1], 0, "seed {seed}");
            assert!(
                outcome.net[0].unsigned_abs() <= cfg.stack as u64,
                "seed {seed}"
            );
            if outcome.showdown {
                assert_eq!(state.board().len(), 5, "seed {seed}");
            }
        }
    }
}
