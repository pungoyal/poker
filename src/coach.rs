//! The coach: facts about a spot (hand strength, draws, equity, pot odds)
//! and a review of the decision taken.
//!
//! Equity here is measured against a random opponent hand. That is exact for
//! the placeholder bots, whose actions do not depend on their cards, and it
//! will be replaced by solver ranges for the real bot. Decision values treat
//! the hand as checked down afterwards: they ignore betting on later streets.
//! Bets are priced only against opponents whose folding frequency is known.

use crate::cards::{Card, Rng};
use crate::eval::{Category, HandRank, evaluate, rank_name, rank_plural, straight_high};
use crate::game::{Action, HandState, Street};

/// Formats a chip amount in big blinds, e.g. "2.5bb".
pub fn fmt_bb(chips: f64, big_blind: u32) -> String {
    let text = format!("{:.2}", chips / big_blind as f64);
    let text = text.trim_end_matches('0').trim_end_matches('.');
    let text = if text == "-0" { "0" } else { text };
    format!("{text}bb")
}

/// Like [`fmt_bb`], with a leading "+" for gains.
pub fn fmt_bb_signed(chips: f64, big_blind: u32) -> String {
    let text = fmt_bb(chips, big_blind);
    if text.starts_with('-') || text == "0bb" {
        text
    } else {
        format!("+{text}")
    }
}

/// Share of the final pot a call must win to break even.
pub fn pot_odds(pot: u32, to_call: u32) -> f64 {
    to_call as f64 / (pot + to_call) as f64
}

/// What the player holds right now: the starting hand before the flop,
/// the made hand after it. Draws are not included.
pub fn made_hand(hole: [Card; 2], board: &[Card]) -> String {
    if board.is_empty() {
        let (high, low) = (
            hole[0].rank().max(hole[1].rank()),
            hole[0].rank().min(hole[1].rank()),
        );
        if high == low {
            return format!("pocket {}", rank_plural(high));
        }
        let suited = if hole[0].suit() == hole[1].suit() {
            "suited"
        } else {
            "offsuit"
        };
        return format!("{}-{} {suited}", rank_name(high), rank_name(low));
    }
    let mut cards = board.to_vec();
    cards.extend(hole);
    evaluate(&cards).describe()
}

/// Plain-English description of the player's hand so far, including draws.
pub fn describe_hand(hole: [Card; 2], board: &[Card]) -> String {
    let mut parts = vec![made_hand(hole, board)];
    parts.extend(draws(hole, board));
    parts.join(" · ")
}

/// Draws to a straight or flush with cards still to come. Only draws that
/// use a hole card count: a draw on the board is everyone's draw.
pub fn draws(hole: [Card; 2], board: &[Card]) -> Vec<String> {
    let mut found = Vec::new();
    if !(3..=4).contains(&board.len()) {
        return found;
    }
    let mut cards = board.to_vec();
    cards.extend(hole);
    let category = evaluate(&cards).category();

    if category < Category::Flush {
        for suit in 0..4 {
            let count = cards.iter().filter(|c| c.suit() == suit).count();
            if count == 4 && hole.iter().any(|c| c.suit() == suit) {
                found.push("flush draw (9 outs)".to_string());
            }
        }
    }
    if category < Category::Straight {
        let board_mask = rank_mask(board);
        let mask = board_mask | rank_mask(&hole);
        // Ranks that give us a straight better than the one the board alone would make.
        let completing = (0..13)
            .filter(|r| straight_high(mask | 1 << r) > straight_high(board_mask | 1 << r))
            .fold(0u16, |m, r| m | 1 << r);
        match completing.count_ones() {
            0 => {}
            1 => found.push("gutshot straight draw (4 outs)".to_string()),
            _ if open_ended(mask, completing) => {
                found.push("open-ended straight draw (8 outs)".to_string())
            }
            _ => found.push("double gutshot straight draw (8 outs)".to_string()),
        }
    }
    found
}

fn rank_mask(cards: &[Card]) -> u16 {
    cards.iter().fold(0u16, |mask, c| mask | 1 << c.rank())
}

/// Four ranks in a row with a completing card at each end. Rank -1 is a low ace.
fn open_ended(mask: u16, completing: u16) -> bool {
    let bit = |r: i32| -> u16 { if r < 0 { 1 << 12 } else { 1 << r } };
    (0..=8).any(|low: i32| {
        (low..low + 4).all(|r| mask & bit(r) != 0)
            && completing & bit(low - 1) != 0
            && completing & bit(low + 4) != 0
    })
}

/// Chance of winning at showdown against a random hand, counting ties as half.
/// Exact from the flop onwards; a fixed-seed simulation before the flop.
pub fn equity_vs_random(hole: [Card; 2], board: &[Card]) -> f64 {
    let rest: Vec<Card> = (0..52)
        .map(Card::from_index)
        .filter(|c| !hole.contains(c) && !board.contains(c))
        .collect();
    match board.len() {
        0 => simulate_preflop(hole, &rest),
        3..=5 => enumerate(hole, board, &rest),
        n => panic!("a board cannot have {n} cards"),
    }
}

fn score(hero: HandRank, villain: HandRank) -> f64 {
    match hero.cmp(&villain) {
        std::cmp::Ordering::Greater => 1.0,
        std::cmp::Ordering::Equal => 0.5,
        std::cmp::Ordering::Less => 0.0,
    }
}

/// Every board completion against every opponent hand from `rest`.
fn enumerate(hole: [Card; 2], board: &[Card], rest: &[Card]) -> f64 {
    let n = rest.len();
    let mut cards = [hole[0]; 7];
    cards[..board.len()].copy_from_slice(board);
    let (mut total, mut count) = (0.0, 0u64);
    let mut add = |(t, c): (f64, u64)| {
        total += t;
        count += c;
    };
    match board.len() {
        5 => add(against_all(&mut cards, hole, rest, 0)),
        4 => {
            for i in 0..n {
                cards[4] = rest[i];
                add(against_all(&mut cards, hole, rest, 1 << i));
            }
        }
        _ => {
            for i in 0..n {
                for j in i + 1..n {
                    cards[3] = rest[i];
                    cards[4] = rest[j];
                    add(against_all(&mut cards, hole, rest, 1 << i | 1 << j));
                }
            }
        }
    }
    total / count as f64
}

/// Scores the complete board in `cards[..5]` against every opponent hand
/// from `rest` that avoids the cards marked in `used`. Returns (total, count).
fn against_all(cards: &mut [Card; 7], hole: [Card; 2], rest: &[Card], used: u64) -> (f64, u64) {
    cards[5] = hole[0];
    cards[6] = hole[1];
    let hero = evaluate(cards);
    let (mut total, mut count) = (0.0, 0u64);
    for i in (0..rest.len()).filter(|i| used & 1 << i == 0) {
        for j in (i + 1..rest.len()).filter(|j| used & 1 << j == 0) {
            cards[5] = rest[i];
            cards[6] = rest[j];
            total += score(hero, evaluate(cards));
            count += 1;
        }
    }
    (total, count)
}

fn simulate_preflop(hole: [Card; 2], rest: &[Card]) -> f64 {
    const SAMPLES: u32 = 40_000;
    // Seeded by the cards, in either order, so the same hand always reports the same number.
    let (a, b) = (hole[0].index(), hole[1].index());
    let mut rng = Rng::new(a.min(b) as u64 * 52 + a.max(b) as u64);
    let mut deck = rest.to_vec();
    let mut total = 0.0;
    for _ in 0..SAMPLES {
        // Partial shuffle: draw five board cards and two opponent cards.
        for i in 0..7 {
            let j = i + rng.below((deck.len() - i) as u64) as usize;
            deck.swap(i, j);
        }
        let mut cards = [hole[0]; 7];
        cards[..5].copy_from_slice(&deck[..5]);
        cards[6] = hole[1];
        let hero = evaluate(&cards);
        cards[5] = deck[5];
        cards[6] = deck[6];
        total += score(hero, evaluate(&cards));
    }
    total / SAMPLES as f64
}

/// The situation the player faced when choosing an action.
#[derive(Clone, Debug)]
pub struct Spot {
    pub big_blind: u32,
    pub street: Street,
    /// Chips in the middle before acting, including the bet being faced.
    pub pot: u32,
    pub to_call: u32,
    /// Chips the player has already put in on this street.
    pub street_commit: u32,
    /// Chips the opponent has put in on this street.
    pub opp_commit: u32,
    /// Smallest and largest legal `RaiseTo`, if raising is possible.
    pub raise: Option<(u32, u32)>,
    /// Chance of winning at showdown, from [`equity_vs_random`].
    pub equity: f64,
    /// The player's hand, from [`made_hand`].
    pub made: String,
    /// The player's draws, from [`draws`].
    pub draws: Vec<String>,
    /// The opponent's name, e.g. "calling station".
    pub opponent: &'static str,
    /// How often the opponent folds to a bet or raise, if known.
    pub fold_to_bet: Option<f64>,
}

impl Spot {
    /// The spot for the player to act in `state`.
    pub fn from_state(
        state: &HandState,
        equity: f64,
        opponent: &'static str,
        fold_to_bet: Option<f64>,
    ) -> Spot {
        let me = state.to_act().expect("no spot once the hand is over");
        let legal = state.legal().expect("no spot once the hand is over");
        let (hole, board) = (state.hole(me), state.board());
        Spot {
            big_blind: state.config().big_blind,
            street: state.street(),
            pot: state.pot(),
            to_call: legal.to_call,
            street_commit: state.street_commit(me),
            opp_commit: state.street_commit(1 - me),
            raise: legal.raise,
            equity,
            made: made_hand(hole, board),
            draws: draws(hole, board),
            opponent,
            fold_to_bet,
        }
    }

    /// Average chips won from here on by taking `action`, if the hand is then
    /// checked down. `None` for a bet when the opponent's fold rate is unknown.
    pub fn ev(&self, action: Action) -> Option<f64> {
        let (equity, pot) = (self.equity, self.pot as f64);
        match action {
            Action::Fold => Some(0.0),
            Action::Check => Some(equity * pot),
            Action::Call => {
                let call = self.to_call as f64;
                Some(equity * (pot + call) - call)
            }
            Action::RaiseTo(amount) => {
                let fold = self.fold_to_bet?;
                let risk = (amount - self.street_commit) as f64;
                let called = (amount - self.opp_commit) as f64;
                Some(fold * pot + (1.0 - fold) * (equity * (pot + risk + called) - risk))
            }
        }
    }

    /// The legal actions worth comparing: fold or check, call, and bets of a
    /// third, half, three quarters and all of the pot, plus all-in.
    pub fn options(&self) -> Vec<Action> {
        let mut options = if self.to_call > 0 {
            vec![Action::Fold, Action::Call]
        } else {
            vec![Action::Check]
        };
        if let Some((min, max)) = self.raise {
            let base = (self.pot + self.to_call) as f64;
            for share in [1.0 / 3.0, 0.5, 0.75, 1.0] {
                let target = (self.opp_commit as f64 + base * share).round() as u32;
                options.push(Action::RaiseTo(target.clamp(min, max)));
            }
            options.push(Action::RaiseTo(max));
            options.dedup();
        }
        options
    }

    /// The priced options, best first. `extra` is included even if it is not
    /// one of the standard [`options`](Spot::options).
    pub fn ranked(&self, extra: Option<Action>) -> Vec<(Action, f64)> {
        let mut ranked: Vec<(Action, f64)> = Vec::new();
        for action in self.options().into_iter().chain(extra) {
            if let Some(ev) = self.ev(action)
                && !ranked.iter().any(|&(a, _)| a == action)
            {
                ranked.push((action, ev));
            }
        }
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
        ranked
    }

    /// The action in words, e.g. "betting 3bb (75% pot)".
    pub fn phrase(&self, action: Action) -> String {
        let bb = |chips: u32| fmt_bb(chips as f64, self.big_blind);
        match action {
            Action::Fold => "folding".to_string(),
            Action::Check => "checking".to_string(),
            Action::Call => format!("calling {}", bb(self.to_call)),
            Action::RaiseTo(amount) => {
                let verb = if self.opp_commit == 0 {
                    format!("betting {}", bb(amount))
                } else {
                    format!("raising to {}", bb(amount))
                };
                if self.raise.is_some_and(|(_, max)| amount == max) {
                    format!("{verb} all-in")
                } else {
                    let share =
                        (amount - self.opp_commit) as f64 / (self.pot + self.to_call) as f64;
                    format!("{verb} ({} pot)", percent(share))
                }
            }
        }
    }

    fn bb(&self, chips: f64) -> String {
        fmt_bb(chips, self.big_blind)
    }

    fn signed(&self, chips: f64) -> String {
        fmt_bb_signed(chips, self.big_blind)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Grade {
    Good,
    /// Wrong by less than a tenth of a big blind.
    Close,
    Inaccuracy,
    Mistake,
    Blunder,
    /// An observation with no price attached.
    Note,
}

impl Grade {
    fn from_loss(loss_bb: f64) -> Grade {
        match loss_bb {
            x if x < 0.1 => Grade::Close,
            x if x < 1.0 => Grade::Inaccuracy,
            x if x < 5.0 => Grade::Mistake,
            _ => Grade::Blunder,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Grade::Good => "Good",
            Grade::Close => "Close",
            Grade::Inaccuracy => "Inaccuracy",
            Grade::Mistake => "Mistake",
            Grade::Blunder => "Blunder",
            Grade::Note => "Note",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Feedback {
    pub grade: Grade,
    /// Estimated chips given up compared with the better option; zero if none.
    pub loss: f64,
    pub text: String,
}

fn percent(x: f64) -> String {
    format!("{:.0}%", x * 100.0)
}

fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

fn is_bet(action: Action) -> bool {
    matches!(action, Action::RaiseTo(_))
}

/// Reviews the action the player took in `spot`: what it was worth, what the
/// best option was, and the reasons that decided it.
pub fn review(spot: &Spot, action: Action) -> Feedback {
    let ranked = spot.ranked(Some(action));
    let Some(chosen) = spot.ev(action) else {
        return Feedback {
            grade: Grade::Note,
            loss: 0.0,
            text: unpriced_bet(spot, action),
        };
    };
    let (best, best_ev) = ranked[0];
    let loss = (best_ev - chosen).max(0.0);
    let grade = if loss < 1e-6 {
        Grade::Good
    } else {
        Grade::from_loss(loss / spot.big_blind as f64)
    };

    let mut text = vec![situation(spot)];
    if loss < 1e-6 {
        let runner_up = ranked.iter().find(|&&(a, _)| a != action);
        text.push(match runner_up {
            Some(&(alt, alt_ev)) => format!(
                "{} was the best option, worth about {} on average; next best was {} at {}.",
                capitalise(&spot.phrase(action)),
                spot.signed(chosen),
                spot.phrase(alt),
                spot.signed(alt_ev),
            ),
            None => format!(
                "{} was the only sensible option ({}).",
                capitalise(&spot.phrase(action)),
                spot.signed(chosen)
            ),
        });
    } else {
        text.push(format!(
            "{} is worth about {} on average, but {} was worth {}: about {} given up.",
            capitalise(&spot.phrase(action)),
            spot.signed(chosen),
            spot.phrase(best),
            spot.signed(best_ev),
            spot.bb(loss),
        ));
    }
    text.extend(reasons(spot, action, best));
    Feedback {
        grade,
        loss,
        text: text.join(" "),
    }
}

/// "With a pair of fives and a flush draw on the turn (41% against a random hand)."
fn situation(spot: &Spot) -> String {
    let mut hand = spot.made.clone();
    if !spot.draws.is_empty() {
        let draws: Vec<String> = spot
            .draws
            .iter()
            .map(|d| {
                let name = d.split(" (").next().unwrap_or(d);
                let article = if name.starts_with(['a', 'e', 'i', 'o', 'u']) {
                    "an"
                } else {
                    "a"
                };
                format!("{article} {name}")
            })
            .collect();
        hand = format!("{hand} and {}", draws.join(" and "));
    }
    let when = match spot.street {
        Street::Preflop => "before the flop".to_string(),
        street => format!("on the {}", street.name().to_lowercase()),
    };
    format!(
        "With {hand} {when} you win {} against a random hand.",
        percent(spot.equity)
    )
}

/// The facts that decided this spot, chosen by what was done and what was best.
fn reasons(spot: &Spot, action: Action, best: Action) -> Vec<String> {
    let mut why = Vec::new();
    let equity = spot.equity;
    let pot = spot.pot as f64;
    let involves = |pred: fn(Action) -> bool| pred(action) || pred(best);

    // Fold or call: pot odds decide it.
    if spot.to_call > 0 && involves(|a| matches!(a, Action::Fold | Action::Call)) {
        let need = pot_odds(spot.pot, spot.to_call);
        let margin = equity - need;
        let verdict = if margin.abs() < 0.03 {
            ": a near coin-flip, so neither choice costs much"
        } else if margin > 0.15 {
            ": an easy call on price alone"
        } else if margin > 0.0 {
            ": enough to continue"
        } else if margin > -0.15 {
            ": just short"
        } else {
            ": well short"
        };
        why.push(format!(
            "Calling {} into {} needs {} equity and you had {}{verdict}.",
            spot.bb(spot.to_call as f64),
            spot.bb(pot),
            percent(need),
            percent(equity),
        ));
        if !spot.draws.is_empty() && matches!(spot.street, Street::Flop | Street::Turn) {
            let cards = if spot.street == Street::Flop {
                "both the turn and the river"
            } else {
                "the river"
            };
            why.push(format!(
                "Your draw is part of that number: it counts {cards}, but assumes no more betting."
            ));
        }
    }

    // Betting: how the opponent responds decides it.
    if involves(is_bet) {
        let bet = if is_bet(action) { action } else { best };
        let Action::RaiseTo(amount) = bet else {
            unreachable!()
        };
        let risk = (amount - spot.street_commit) as f64;
        let needed = risk / (risk + pot);
        match spot.fold_to_bet {
            Some(0.0) => why.push(if equity >= 0.5 {
                format!(
                    "The {} bot never folds and calls with any two cards, so while you are ahead every extra chip in the pot is profit: bigger is better.",
                    spot.opponent
                )
            } else {
                format!(
                    "The {} bot never folds, so a bet cannot win the pot by itself; called, you win only {} of the time.",
                    spot.opponent,
                    percent(equity)
                )
            }),
            Some(fold) => {
                let have = percent(equity);
                let support = match (fold >= needed, equity >= 0.5) {
                    (true, true) => "it profits from the folds alone, and you are ahead when called too.".to_string(),
                    (true, false) => "it profits from the folds alone, before your cards even count.".to_string(),
                    (false, true) => format!("folds won't pay for it, but called you win {have} of the time, so the calls are where the money is."),
                    (false, false) => format!("folds alone won't pay for it, so the rest must come from the {have} of the time you win when called."),
                };
                why.push(format!(
                    "The {} bot folds about {} of the time whatever the size, and otherwise plays on with any two cards. {} risks {} to win {}, which needs folds {} of the time on its own; {support}",
                    spot.opponent,
                    percent(fold),
                    capitalise(&spot.phrase(bet)),
                    spot.bb(risk),
                    spot.bb(pot),
                    percent(needed),
                ));
            }
            None => {}
        }
    }

    // Wrong bet size.
    if let (Action::RaiseTo(chosen), Action::RaiseTo(better)) = (action, best)
        && chosen != better
    {
        why.push(if better > chosen {
            "Size up: you are ahead when called, so the bigger the pot the better.".to_string()
        } else {
            "Size down: a small bet gets the same folds while risking less when called by better hands.".to_string()
        });
    }

    // Checked when betting was better.
    if action == Action::Check && is_bet(best) {
        why.push("Checking lets a worse hand see the showdown for free.".to_string());
    }

    if spot.street == Street::River && spot.to_call > 0 {
        why.push(
            "On the river nothing is left to come: the equity is simply how often your hand is best now."
                .to_string(),
        );
    }
    why
}

/// A bet against an opponent whose fold rate is unknown: describe, don't price.
fn unpriced_bet(spot: &Spot, action: Action) -> String {
    let Action::RaiseTo(amount) = action else {
        unreachable!("only bets go unpriced")
    };
    let risk = (amount - spot.street_commit) as f64;
    let pot = spot.pot as f64;
    let have = percent(spot.equity);
    if spot.equity >= 0.5 {
        format!(
            "{} A value bet: you are ahead {have} of the time, so you want calls from worse hands.",
            situation(spot)
        )
    } else {
        format!(
            "{} A bluff: risking {} to win {}, it needs a fold more than {} of the time to profit on its own.",
            situation(spot),
            spot.bb(risk),
            spot.bb(pot),
            percent(risk / (risk + pot)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::parse_cards;

    fn hole(s: &str) -> [Card; 2] {
        let c = parse_cards(s).unwrap();
        [c[0], c[1]]
    }

    fn equity(hand: &str, board: &str) -> f64 {
        equity_vs_random(hole(hand), &parse_cards(board).unwrap())
    }

    fn close(actual: f64, expected: f64, tolerance: f64) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "got {actual}, expected about {expected}"
        );
    }

    #[test]
    fn preflop_equities_match_known_values() {
        close(equity("As Ah", ""), 0.852, 0.01);
        close(equity("7s 2h", ""), 0.346, 0.01);
        close(equity("As Ks", ""), 0.670, 0.01);
        // Deterministic: the same hand gives the same number.
        assert_eq!(equity("Qs Jh", ""), equity("Qs Jh", ""));
    }

    #[test]
    fn postflop_equities_are_exact() {
        // The nuts on the river always wins.
        assert_eq!(equity("As Ks", "Qs Js Ts 2c 2d"), 1.0);
        // A royal flush on the board is always a split.
        assert_eq!(equity("2c 2d", "Ts Js Qs Ks As"), 0.5);
        // Top set on a dry flop is a huge favourite; seven high is not.
        assert!(equity("As Ah", "Ad 7c 2h") > 0.95);
        assert!(equity("7s 2h", "Ad Kc 9h") < 0.25);
        // Top two pair on a paired river board is strong, but trips and full houses beat it.
        let e = equity("Ah Kh", "Ac Kd 7s 7h 2c");
        assert!(e > 0.85 && e < 0.97, "{e}");
    }

    #[test]
    fn hand_descriptions() {
        let describe =
            |hand: &str, board: &str| describe_hand(hole(hand), &parse_cards(board).unwrap());
        assert_eq!(describe("5d 5h", ""), "pocket fives");
        assert_eq!(describe("Kd As", ""), "ace-king offsuit");
        assert_eq!(describe("7h 6h", ""), "seven-six suited");
        assert_eq!(describe("5d 5h", "Ac 4h Kd"), "a pair of fives");
        assert_eq!(
            describe("Ah 9h", "Kh 4h 2c"),
            "ace high · flush draw (9 outs)"
        );
        assert_eq!(
            describe("9s 8d", "7c 6h Kd"),
            "king high · open-ended straight draw (8 outs)"
        );
        assert_eq!(
            describe("9s 8d", "6c 5h Kd"),
            "king high · gutshot straight draw (4 outs)"
        );
        // No draws are reported once all the cards are out, or when already made.
        assert_eq!(describe("Ah 9h", "Kh 4h 2c 3s 8d"), "ace high");
        assert_eq!(describe("Ah 9h", "Kh 4h 2h"), "a flush, ace high");
        // Four of a suit on the board alone is not our flush draw.
        assert_eq!(describe("As Ad", "Kh 4h 2h 9h"), "a pair of aces");
    }

    #[test]
    fn straight_draws_need_a_hole_card() {
        let describe =
            |hand: &str, board: &str| describe_hand(hole(hand), &parse_cards(board).unwrap());
        // The board is four to a straight: any 4 or 9 makes it for everyone.
        assert_eq!(describe("Ah 2c", "5d 6s 7h 8c"), "ace high");
        // A 9 makes our hand a better straight than the board's, so it is a draw.
        assert_eq!(
            describe("Ts 2c", "5d 6s 7h 8c"),
            "ten high · gutshot straight draw (4 outs)"
        );
        // Two different inside cards complete it: a double gutshot, not open-ended.
        assert_eq!(
            describe("9s 7d", "Jc 8h 5d"),
            "jack high · double gutshot straight draw (8 outs)"
        );
        // Two-three-four-five is open at both ends: an ace or a six.
        assert_eq!(
            describe("2s 3d", "4c 5h Kd"),
            "king high · open-ended straight draw (8 outs)"
        );
        // Jack-queen-king-ace can only be completed by a ten.
        assert_eq!(
            describe("As Kd", "Qc Jh 3d"),
            "ace high · gutshot straight draw (4 outs)"
        );
    }

    #[test]
    fn preflop_equity_ignores_card_order() {
        assert_eq!(equity("As Ks", ""), equity("Ks As", ""));
    }

    fn spot(pot: u32, to_call: u32, equity: f64) -> Spot {
        Spot {
            big_blind: 100,
            street: Street::Flop,
            pot,
            to_call,
            street_commit: 0,
            opp_commit: to_call,
            raise: None,
            equity,
            made: "a pair of fives".to_string(),
            draws: Vec::new(),
            opponent: "random",
            fold_to_bet: None,
        }
    }

    fn betting(pot: u32, equity: f64, fold_to_bet: Option<f64>) -> Spot {
        Spot {
            raise: Some((100, 10_000)),
            fold_to_bet,
            ..spot(pot, 0, equity)
        }
    }

    #[test]
    fn calls_and_folds_are_priced_against_pot_odds() {
        // Call 100 into 300: needs 25%.
        close(pot_odds(300, 100), 0.25, 1e-9);

        let good_call = review(&spot(300, 100, 0.40), Action::Call);
        assert_eq!(good_call.grade, Grade::Good);
        assert!(good_call.text.contains("needs 25%"), "{}", good_call.text);

        // Folding 40% equity gives up 0.4 * 400 - 100 = 60 chips.
        let bad_fold = review(&spot(300, 100, 0.40), Action::Fold);
        assert_eq!(bad_fold.grade, Grade::Inaccuracy);
        close(bad_fold.loss, 60.0, 1e-9);
        assert!(
            bad_fold.text.contains("calling 1bb was worth +0.6bb"),
            "{}",
            bad_fold.text
        );

        // Calling a pot-sized 2000 with 10% loses 2000 - 0.1 * 6000 = 1400.
        let bad_call = review(&spot(4000, 2000, 0.10), Action::Call);
        assert_eq!(bad_call.grade, Grade::Blunder);
        close(bad_call.loss, 1400.0, 1e-9);
        assert!(bad_call.text.contains("well short"), "{}", bad_call.text);

        let good_fold = review(&spot(4000, 2000, 0.10), Action::Fold);
        assert_eq!(good_fold.grade, Grade::Good);
        assert_eq!(good_fold.loss, 0.0);

        // Exactly at the odds, either choice is fine.
        let coin_flip = review(&spot(300, 100, 0.2501), Action::Fold);
        assert_eq!(coin_flip.grade, Grade::Close);
        assert!(coin_flip.text.contains("coin-flip"), "{}", coin_flip.text);
    }

    #[test]
    fn bets_are_priced_against_the_opponents_fold_rate() {
        // Against a calling station, a strong hand should bet as big as it can.
        let station = betting(400, 0.8, Some(0.0));
        let (best, _) = station.ranked(None)[0];
        assert_eq!(best, Action::RaiseTo(10_000));
        let small = review(&station, Action::RaiseTo(100));
        assert!(small.loss > 0.0);
        assert!(small.text.contains("Size up"), "{}", small.text);
        let slowplay = review(&station, Action::Check);
        assert!(slowplay.grade != Grade::Good);
        assert!(slowplay.text.contains("never folds"), "{}", slowplay.text);

        // A weak hand should not bluff a player who never folds.
        let weak = betting(400, 0.2, Some(0.0));
        assert_eq!(review(&weak, Action::Check).grade, Grade::Good);
        let bluff = review(&weak, Action::RaiseTo(400));
        assert!(bluff.grade != Grade::Good);
        assert!(bluff.text.contains("cannot win the pot"), "{}", bluff.text);

        // A bot that folds 20% to any size: a small bluff beats a big one.
        // Betting 200 into 400 needs folds 200 / 600 = 33% of the time.
        let folder = betting(400, 0.2, Some(0.2));
        let half = review(&folder, Action::RaiseTo(200));
        assert!(half.text.contains("33%"), "{}", half.text);
        assert!(half.text.contains("Size down"), "{}", half.text);
    }

    #[test]
    fn unknown_opponents_get_unpriced_notes() {
        let value = review(&betting(400, 0.8, None), Action::RaiseTo(300));
        assert_eq!(value.grade, Grade::Note);
        assert!(value.text.contains("value bet"), "{}", value.text);
        let bluff = review(&betting(400, 0.2, None), Action::RaiseTo(200));
        assert!(bluff.text.contains("33%"), "{}", bluff.text);
        assert_eq!(bluff.loss, 0.0);
    }

    #[test]
    fn options_cover_standard_sizes_within_the_legal_range() {
        let spot = Spot {
            raise: Some((200, 1_000)),
            ..spot(1_200, 0, 0.5)
        };
        // A third, half and three-quarters of 1200 are 400, 600 and 900; pot is capped by all-in.
        assert_eq!(
            spot.options(),
            vec![
                Action::Check,
                Action::RaiseTo(400),
                Action::RaiseTo(600),
                Action::RaiseTo(900),
                Action::RaiseTo(1_000),
            ]
        );
        assert_eq!(spot.phrase(Action::RaiseTo(600)), "betting 6bb (50% pot)");
        assert_eq!(spot.phrase(Action::RaiseTo(1_000)), "betting 10bb all-in");
    }

    #[test]
    fn big_blind_formatting() {
        assert_eq!(fmt_bb(250.0, 100), "2.5bb");
        assert_eq!(fmt_bb(100.0, 100), "1bb");
        assert_eq!(fmt_bb(239.0, 100), "2.39bb");
        assert_eq!(fmt_bb(-0.1, 100), "0bb");
        assert_eq!(fmt_bb_signed(60.0, 100), "+0.6bb");
        assert_eq!(fmt_bb_signed(-60.0, 100), "-0.6bb");
        assert_eq!(fmt_bb_signed(0.0, 100), "0bb");
    }
}
