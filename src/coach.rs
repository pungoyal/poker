//! The coach: facts about a spot (hand strength, draws, equity, pot odds)
//! and a review of the decision taken.
//!
//! Equity here is measured against a random opponent hand. That is exact for
//! the placeholder bots, whose actions do not depend on their cards, and it
//! will be replaced by solver ranges for the real bot. Decision values treat
//! the hand as checked down afterwards: they ignore betting on later streets.

use crate::cards::{Card, Rng};
use crate::eval::{Category, HandRank, evaluate, rank_name, rank_plural, straight_high};
use crate::game::Action;

/// Formats a chip amount in big blinds, e.g. "2.5bb".
pub fn fmt_bb(chips: f64, big_blind: u32) -> String {
    let text = format!("{:.2}", chips / big_blind as f64);
    let text = text.trim_end_matches('0').trim_end_matches('.');
    let text = if text == "-0" { "0" } else { text };
    format!("{text}bb")
}

/// Share of the final pot a call must win to break even.
pub fn pot_odds(pot: u32, to_call: u32) -> f64 {
    to_call as f64 / (pot + to_call) as f64
}

/// Plain-English description of the player's hand so far, including draws.
pub fn describe_hand(hole: [Card; 2], board: &[Card]) -> String {
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
    let mut parts = vec![evaluate(&cards).describe()];
    parts.extend(draws(hole, board));
    parts.join(" · ")
}

/// Draws to a straight or flush with cards still to come.
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
        let mask = cards.iter().fold(0u16, |mask, c| mask | 1 << c.rank());
        let completing = (0..13)
            .filter(|r| straight_high(mask | 1 << r).is_some())
            .count();
        match completing {
            0 => {}
            1 => found.push("gutshot straight draw (4 outs)".to_string()),
            _ => found.push("open-ended straight draw (8 outs)".to_string()),
        }
    }
    found
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

fn enumerate(hole: [Card; 2], board: &[Card], rest: &[Card]) -> f64 {
    let n = rest.len();
    // Every way to complete the board, as indices into `rest`.
    let completions: Vec<Vec<usize>> = match 5 - board.len() {
        0 => vec![vec![]],
        1 => (0..n).map(|i| vec![i]).collect(),
        _ => (0..n)
            .flat_map(|i| (i + 1..n).map(move |j| vec![i, j]))
            .collect(),
    };
    let mut total = 0.0;
    let mut count = 0u64;
    for completion in &completions {
        let mut cards = [hole[0]; 7];
        cards[..board.len()].copy_from_slice(board);
        for (slot, &i) in completion.iter().enumerate() {
            cards[board.len() + slot] = rest[i];
        }
        cards[5] = hole[0];
        cards[6] = hole[1];
        let hero = evaluate(&cards);
        for i in (0..n).filter(|i| !completion.contains(i)) {
            for j in (i + 1..n).filter(|j| !completion.contains(j)) {
                cards[5] = rest[i];
                cards[6] = rest[j];
                total += score(hero, evaluate(&cards));
                count += 1;
            }
        }
    }
    total / count as f64
}

fn simulate_preflop(hole: [Card; 2], rest: &[Card]) -> f64 {
    const SAMPLES: u32 = 40_000;
    // Seeded by the cards so the same hand always reports the same number.
    let mut rng = Rng::new(hole[0].index() as u64 * 52 + hole[1].index() as u64);
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
#[derive(Clone, Copy, Debug)]
pub struct Spot {
    pub big_blind: u32,
    /// Chips in the middle before acting, including the bet being faced.
    pub pot: u32,
    pub to_call: u32,
    /// Chips the player has already put in on this street.
    pub street_commit: u32,
    /// Chance of winning at showdown, from [`equity_vs_random`].
    pub equity: f64,
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

/// Reviews the action the player took in `spot`.
pub fn review(spot: &Spot, action: Action) -> Feedback {
    let bb = |chips: f64| fmt_bb(chips, spot.big_blind);
    let equity = spot.equity;
    let pot = spot.pot as f64;
    let have = percent(equity);

    // A bet or raise wins the pot outright when the opponent folds.
    let aggression = |amount: u32| -> Feedback {
        let risk = (amount - spot.street_commit) as f64;
        let text = if equity >= 0.5 {
            format!(
                "A value bet: you win about {have} of the time at showdown, so you want calls from worse hands."
            )
        } else {
            format!(
                "A bluff: you win only about {have} at showdown. Risking {} to win {}, it needs a fold more than {} of the time to profit on its own.",
                bb(risk),
                bb(pot),
                percent(risk / (risk + pot)),
            )
        };
        Feedback {
            grade: Grade::Note,
            loss: 0.0,
            text,
        }
    };

    if spot.to_call == 0 {
        return match action {
            Action::RaiseTo(amount) => aggression(amount),
            _ if equity >= 0.65 => Feedback {
                grade: Grade::Note,
                loss: 0.0,
                text: format!(
                    "You checked with about {have} equity. When you are a clear favourite, betting usually earns more: worse hands can pay you."
                ),
            },
            _ => Feedback {
                grade: Grade::Good,
                loss: 0.0,
                text: format!("Checking with about {have} equity is reasonable."),
            },
        };
    }

    let call = spot.to_call as f64;
    let need = percent(pot_odds(spot.pot, spot.to_call));
    // Value of calling compared with folding, if no more chips go in.
    let call_value = equity * (pot + call) - call;
    let odds = format!(
        "You needed {need} equity to call {} into {} and had about {have}",
        bb(call),
        bb(pot)
    );

    match action {
        Action::RaiseTo(amount) => aggression(amount),
        Action::Fold if call_value > 0.0 => Feedback {
            grade: Grade::from_loss(call_value / spot.big_blind as f64),
            loss: call_value,
            text: format!("{odds}. Folding gave up about {}.", bb(call_value)),
        },
        Action::Fold => Feedback {
            grade: Grade::Good,
            loss: 0.0,
            text: format!("{odds}. Folding saves about {}.", bb(-call_value)),
        },
        _ if call_value < 0.0 => Feedback {
            grade: Grade::from_loss(-call_value / spot.big_blind as f64),
            loss: -call_value,
            text: format!(
                "{odds}. Calling costs about {} on average.",
                bb(-call_value)
            ),
        },
        _ => Feedback {
            grade: Grade::Good,
            loss: 0.0,
            text: format!(
                "{odds}. Calling earns about {} more than folding.",
                bb(call_value)
            ),
        },
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

    fn spot(pot: u32, to_call: u32, equity: f64) -> Spot {
        Spot {
            big_blind: 100,
            pot,
            to_call,
            street_commit: 0,
            equity,
        }
    }

    #[test]
    fn calls_and_folds_are_priced_against_pot_odds() {
        // Call 100 into 300: needs 25%.
        close(pot_odds(300, 100), 0.25, 1e-9);

        let good_call = review(&spot(300, 100, 0.40), Action::Call);
        assert_eq!(good_call.grade, Grade::Good);

        // Folding 40% equity gives up 0.4 * 400 - 100 = 60 chips.
        let bad_fold = review(&spot(300, 100, 0.40), Action::Fold);
        assert_eq!(bad_fold.grade, Grade::Inaccuracy);
        close(bad_fold.loss, 60.0, 1e-9);
        assert!(bad_fold.text.contains("needed 25%"), "{}", bad_fold.text);

        // Calling a pot-sized 2000 with 10% loses 2000 - 0.1 * 6000 = 1400.
        let bad_call = review(&spot(4000, 2000, 0.10), Action::Call);
        assert_eq!(bad_call.grade, Grade::Blunder);
        close(bad_call.loss, 1400.0, 1e-9);

        let good_fold = review(&spot(4000, 2000, 0.10), Action::Fold);
        assert_eq!(good_fold.grade, Grade::Good);
        assert_eq!(good_fold.loss, 0.0);

        // Exactly at the odds, either choice is fine.
        assert_eq!(
            review(&spot(300, 100, 0.2501), Action::Fold).grade,
            Grade::Close
        );
    }

    #[test]
    fn bets_are_described_as_value_or_bluff() {
        let value = review(&spot(400, 0, 0.8), Action::RaiseTo(300));
        assert!(value.text.contains("value bet"), "{}", value.text);

        // Betting 200 into 400 as a bluff needs folds 200 / 600 = 33% of the time.
        let bluff = review(&spot(400, 0, 0.2), Action::RaiseTo(200));
        assert!(bluff.text.contains("33%"), "{}", bluff.text);
        assert_eq!(bluff.loss, 0.0);

        let slowplay = review(&spot(400, 0, 0.9), Action::Check);
        assert_eq!(slowplay.grade, Grade::Note);
        assert_eq!(review(&spot(400, 0, 0.4), Action::Check).grade, Grade::Good);
    }

    #[test]
    fn big_blind_formatting() {
        assert_eq!(fmt_bb(250.0, 100), "2.5bb");
        assert_eq!(fmt_bb(100.0, 100), "1bb");
        assert_eq!(fmt_bb(239.0, 100), "2.39bb");
        assert_eq!(fmt_bb(-0.1, 100), "0bb");
    }
}
