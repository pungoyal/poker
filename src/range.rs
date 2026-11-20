//! Hand ranges: weights over the 1326 two-card combinations, the hand
//! strengths that bots and the coach read them with, and equity against them.

use crate::cards::{Card, Rng};
use crate::eval::{Draws, HandRank, evaluate};
use std::sync::OnceLock;

/// Number of two-card combinations in a deck.
pub const COMBOS: usize = 1326;

const COMBO_TABLE: [(u8, u8); COMBOS] = {
    let mut table = [(0u8, 0u8); COMBOS];
    let mut high = 1;
    while high < 52 {
        let mut low = 0;
        while low < high {
            table[high * (high - 1) / 2 + low] = (low as u8, high as u8);
            low += 1;
        }
        high += 1;
    }
    table
};

/// The two cards of combination `index`.
pub fn combo(index: usize) -> [Card; 2] {
    let (low, high) = COMBO_TABLE[index];
    [Card::from_index(low), Card::from_index(high)]
}

/// The index of a two-card combination, in either card order.
pub fn combo_index(hole: [Card; 2]) -> usize {
    let (a, b) = (hole[0].index() as usize, hole[1].index() as usize);
    let (low, high) = (a.min(b), a.max(b));
    high * (high - 1) / 2 + low
}

fn card_mask(cards: &[Card]) -> u64 {
    cards.iter().fold(0, |mask, c| mask | 1 << c.index())
}

fn combo_mask(index: usize) -> u64 {
    let (low, high) = COMBO_TABLE[index];
    1 << low | 1 << high
}

/// The 169 starting-hand classes, strongest first by equity against a random hand.
const PREFLOP_ORDER: &str = "AA KK QQ JJ TT 99 88 AKs 77 AQs AJs AKo AQo ATs AJo KQs ATo 66 A9s \
    KJs A8s KTs KQo A7s KJo 55 A9o A8o K9s KTo QJs A6s QTs A5s A4s A7o K8s QJo A3s JTs K9o Q9s \
    K7s A5o A6o A2s QTo A4o 44 Q8s K6s A3o K8o K5s J9s K7o K4s Q9o JTo A2o T9s K6o J8s Q7s Q6s \
    K3s K2s Q8o 33 K5o J9o Q5s K4o T9o J7s Q7o T8s Q4s K3o Q6o 98s J8o Q3s T7s 22 J6s Q5o Q2s \
    K2o J7o J5s T8o T6s J4s 97s Q4o J3s Q3o 87s T7o 96s 98o J6o J2s T5s Q2o J5o T4s 97o 86s J4o \
    T6o T3s 95s J3o 76s 87o T2s 85s 96o J2o T5o 75s 94s 84s 65s 86o 93s T3o T4o 95o 74s 92s 76o \
    T2o 64s 54s 85o 83s 94o 75o 82s 73s 93o 53s 65o 63s 84o 43s 92o 74o 54o 72s 64o 52s 62s \
    83o 82o 73o 42s 63o 32s 53o 43o 52o 72o 62o 42o 32o";

/// Starting-hand strengths in a 13x13 grid: `[high][low]` for suited hands,
/// `[low][high]` for offsuit ones and `[rank][rank]` for pairs.
fn preflop_grid() -> &'static [[f64; 13]; 13] {
    static GRID: OnceLock<[[f64; 13]; 13]> = OnceLock::new();
    GRID.get_or_init(|| {
        let rank = |c: u8| b"23456789TJQKA".iter().position(|&r| r == c).unwrap();
        let mut grid = [[0.0; 13]; 13];
        let mut stronger = 0.0;
        for class in PREFLOP_ORDER.split_whitespace() {
            let b = class.as_bytes();
            let (high, low) = (rank(b[0]), rank(b[1]));
            let (combos, cell) = match b.get(2) {
                None => (6.0, (high, high)),
                Some(b's') => (4.0, (high, low)),
                Some(_) => (12.0, (low, high)),
            };
            // The share of all combinations this class beats, counting its own as half.
            grid[cell.0][cell.1] = 1.0 - (stronger + combos / 2.0) / COMBOS as f64;
            stronger += combos;
        }
        grid
    })
}

fn preflop_cell(hole: [Card; 2]) -> (usize, usize) {
    let (a, b) = (hole[0].rank() as usize, hole[1].rank() as usize);
    let (high, low) = (a.max(b), a.min(b));
    if hole[0].suit() == hole[1].suit() {
        (high, low)
    } else {
        (low, high)
    }
}

/// Before the flop: the share of starting hands this one beats, from 0 to 1.
pub fn preflop_strength(hole: [Card; 2]) -> f64 {
    let (row, col) = preflop_cell(hole);
    preflop_grid()[row][col]
}

/// Short name of a starting hand, e.g. "AKs", "T9o", "77".
pub fn hand_class(hole: [Card; 2]) -> String {
    const RANKS: &[u8; 13] = b"23456789TJQKA";
    let (a, b) = (hole[0].rank(), hole[1].rank());
    let (high, low) = (a.max(b) as usize, a.min(b) as usize);
    let mut name = format!("{}{}", RANKS[high] as char, RANKS[low] as char);
    if high != low {
        name.push(if hole[0].suit() == hole[1].suit() {
            's'
        } else {
            'o'
        });
    }
    name
}

/// How strong every combination is on one board.
#[derive(Clone, Debug)]
pub struct Strengths {
    board: Vec<Card>,
    /// Share of other combinations beaten right now, by combination.
    made: Vec<f32>,
    outs: Vec<u8>,
}

impl Strengths {
    pub fn new(board: &[Card]) -> Strengths {
        let mut made = vec![0.0f32; COMBOS];
        let mut outs = vec![0u8; COMBOS];
        if board.is_empty() {
            for (i, strength) in made.iter_mut().enumerate() {
                *strength = preflop_strength(combo(i)) as f32;
            }
        } else {
            let dead = card_mask(board);
            let mut cards = [board[0]; 7];
            cards[..board.len()].copy_from_slice(board);
            let n = board.len() + 2;
            let mut ranked: Vec<(HandRank, usize)> = (0..COMBOS)
                .filter(|&i| combo_mask(i) & dead == 0)
                .map(|i| {
                    let hole = combo(i);
                    cards[n - 2] = hole[0];
                    cards[n - 1] = hole[1];
                    (evaluate(&cards[..n]), i)
                })
                .collect();
            ranked.sort_unstable();
            let total = (ranked.len() - 1) as f32;
            let mut start = 0;
            while start < ranked.len() {
                let mut end = start;
                while end < ranked.len() && ranked[end].0 == ranked[start].0 {
                    end += 1;
                }
                // Beats everything below, ties the rest of its group.
                let share = (start as f32 + (end - start - 1) as f32 / 2.0) / total;
                for &(_, i) in &ranked[start..end] {
                    made[i] = share;
                    if board.len() < 5 {
                        outs[i] = Draws::find(combo(i), board).outs() as u8;
                    }
                }
                start = end;
            }
        }
        Strengths {
            board: board.to_vec(),
            made,
            outs,
        }
    }

    pub fn board(&self) -> &[Card] {
        &self.board
    }

    /// Share of other hands this one beats right now, from 0 to 1.
    pub fn made(&self, hole: [Card; 2]) -> f64 {
        self.made[combo_index(hole)] as f64
    }

    /// Cards that complete a straight or flush draw.
    pub fn outs(&self, hole: [Card; 2]) -> u32 {
        self.outs[combo_index(hole)] as u32
    }

    /// Chance a draw completes by the river, by the rule of two and four.
    pub fn draw_chance(&self, hole: [Card; 2]) -> f64 {
        let per_card = match self.board.len() {
            3 => 0.04,
            4 => 0.022,
            _ => 0.0,
        };
        (self.outs(hole) as f64 * per_card).min(0.6)
    }

    /// Overall strength from 0 to 1: the made hand, lifted by any draw.
    pub fn of(&self, hole: [Card; 2]) -> f64 {
        let made = self.made(hole);
        made + (1.0 - made) * self.draw_chance(hole)
    }
}

/// A probability weight for each two-card combination a player might hold.
#[derive(Clone, Debug, PartialEq)]
pub struct Range {
    weights: Vec<f64>,
}

impl Default for Range {
    fn default() -> Range {
        Range::uniform()
    }
}

impl Range {
    /// Any two cards, equally likely.
    pub fn uniform() -> Range {
        Range {
            weights: vec![1.0; COMBOS],
        }
    }

    pub fn weights(&self) -> &[f64] {
        &self.weights
    }

    pub fn weight(&self, hole: [Card; 2]) -> f64 {
        self.weights[combo_index(hole)]
    }

    /// Bayesian update: weights each combination by the chance that a player
    /// holding it would have done what was seen.
    pub fn update(&mut self, mut likelihood: impl FnMut([Card; 2]) -> f64) {
        let before = self.weights.clone();
        for (i, weight) in self.weights.iter_mut().enumerate() {
            if *weight > 0.0 {
                *weight *= likelihood(combo(i)).clamp(0.0, 1.0);
            }
        }
        // An action the model calls impossible leaves the range as it was.
        if self.weights.iter().all(|&w| w <= 0.0) {
            self.weights = before;
        }
    }

    /// True while the range is still any two cards.
    pub fn is_uniform(&self) -> bool {
        self.weights
            .iter()
            .all(|&w| (w - self.weights[0]).abs() < 1e-12)
    }

    /// The range's make-up on `strengths`' board, ignoring combinations that use a `dead` card.
    pub fn profile(&self, strengths: &Strengths, dead: &[Card]) -> Profile {
        let dead = card_mask(dead) | card_mask(strengths.board());
        let mut shares = [0.0; 4];
        let mut total = 0.0;
        for (i, &weight) in self.weights.iter().enumerate() {
            if weight <= 0.0 || combo_mask(i) & dead != 0 {
                continue;
            }
            let hole = combo(i);
            let made = strengths.made(hole);
            let slot = if made >= 0.85 {
                0
            } else if made >= 0.6 {
                1
            } else if strengths.outs(hole) >= 8 {
                2
            } else {
                3
            };
            shares[slot] += weight;
            total += weight;
        }
        let share = |x: f64| if total > 0.0 { x / total } else { 0.0 };
        Profile {
            strong: share(shares[0]),
            medium: share(shares[1]),
            draws: share(shares[2]),
            weak: share(shares[3]),
        }
    }
}

/// What a range is made of: shares of strong hands (beating 85% of hands or
/// more), medium hands (60-85%), draws and weak hands. They sum to one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Profile {
    pub strong: f64,
    pub medium: f64,
    pub draws: f64,
    pub weak: f64,
}

impl std::fmt::Display for Profile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let pct = |x: f64| (x * 100.0).round();
        write!(
            f,
            "strong {}% · medium {}% · draws {}% · weak {}%",
            pct(self.strong),
            pct(self.medium),
            pct(self.draws),
            pct(self.weak)
        )
    }
}

/// Chance that `hole` wins at showdown against each range, counting ties as
/// half. Exact after the flop; before it, a fixed-seed sample of boards.
/// Computing several ranges together shares the work of evaluating each board.
pub fn equity_vs_ranges(hole: [Card; 2], board: &[Card], ranges: &[&[f64]]) -> Vec<f64> {
    assert!(matches!(board.len(), 0 | 3 | 4 | 5), "bad board size");
    let dead = card_mask(&hole) | card_mask(board);
    let live: Vec<usize> = (0..COMBOS)
        .filter(|&i| combo_mask(i) & dead == 0 && ranges.iter().any(|r| r[i] > 0.0))
        .collect();
    let rest: Vec<Card> = (0..52)
        .map(Card::from_index)
        .filter(|c| dead & 1 << c.index() == 0)
        .collect();

    let mut wins = vec![0.0; ranges.len()];
    let mut totals = vec![0.0; ranges.len()];
    let mut cards = [hole[0]; 7];
    cards[..board.len()].copy_from_slice(board);
    let mut showdown = |cards: &mut [Card; 7]| {
        let runout = card_mask(&cards[..5]);
        cards[5] = hole[0];
        cards[6] = hole[1];
        let hero = evaluate(cards);
        for &i in &live {
            if combo_mask(i) & runout != 0 {
                continue;
            }
            [cards[5], cards[6]] = combo(i);
            let score = match hero.cmp(&evaluate(cards)) {
                std::cmp::Ordering::Greater => 1.0,
                std::cmp::Ordering::Equal => 0.5,
                std::cmp::Ordering::Less => 0.0,
            };
            for (k, range) in ranges.iter().enumerate() {
                wins[k] += range[i] * score;
                totals[k] += range[i];
            }
        }
    };

    let n = rest.len();
    match board.len() {
        5 => showdown(&mut cards),
        4 => {
            for &river in &rest {
                cards[4] = river;
                showdown(&mut cards);
            }
        }
        3 => {
            for i in 0..n {
                for j in i + 1..n {
                    cards[3] = rest[i];
                    cards[4] = rest[j];
                    showdown(&mut cards);
                }
            }
        }
        _ => {
            const BOARDS: usize = 600;
            let (a, b) = (hole[0].index() as u64, hole[1].index() as u64);
            let mut rng = Rng::new(0x5EED ^ (a.min(b) * 52 + a.max(b)));
            let mut deck = rest.clone();
            for _ in 0..BOARDS {
                for i in 0..5 {
                    let j = i + rng.below((n - i) as u64) as usize;
                    deck.swap(i, j);
                }
                cards[..5].copy_from_slice(&deck[..5]);
                showdown(&mut cards);
            }
        }
    }
    wins.iter()
        .zip(&totals)
        .map(|(&w, &t)| if t > 0.0 { w / t } else { 0.5 })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::parse_cards;

    fn hole(s: &str) -> [Card; 2] {
        let c = parse_cards(s).unwrap();
        [c[0], c[1]]
    }

    #[test]
    fn combo_indices_round_trip() {
        for i in 0..COMBOS {
            let [a, b] = combo(i);
            assert!(a.index() < b.index());
            assert_eq!(combo_index([a, b]), i);
            assert_eq!(combo_index([b, a]), i);
        }
    }

    #[test]
    fn preflop_strength_orders_classic_hands() {
        let s = |h: &str| preflop_strength(hole(h));
        assert!(s("As Ah") > 0.99);
        assert!(s("3c 2d") < 0.01);
        assert!(s("7c 2d") < 0.05);
        assert!(s("Ah Kh") > s("Ah Kd"));
        assert!(s("Ah Kd") > s("Td 9d"));
        assert_eq!(s("Ah Kh"), s("Ks As"));
        assert_eq!(hand_class(hole("Kd As")), "AKo");
        assert_eq!(hand_class(hole("9h Th")), "T9s");
        assert_eq!(hand_class(hole("7h 7c")), "77");
        // Every class appears once, and the classes cover all 1326 combinations.
        assert_eq!(PREFLOP_ORDER.split_whitespace().count(), 169);
    }

    #[test]
    fn board_strengths() {
        let board = parse_cards("Ah 7c 2d").unwrap();
        let s = Strengths::new(&board);
        assert!(s.made(hole("As Ad")) > 0.99, "top set");
        assert!(s.made(hole("Ks Kd")) > s.made(hole("7s 6s")));
        assert!(s.made(hole("6c 4s")) < 0.3, "six high");
        // A flush draw adds potential, not made strength.
        let flush = Strengths::new(&parse_cards("Kh 7h 2d").unwrap());
        let draw = hole("Ah 4h");
        assert_eq!(flush.outs(draw), 9);
        assert!(flush.of(draw) > flush.made(draw) + 0.15);
    }

    #[test]
    fn uniform_range_equity_matches_equity_vs_random() {
        let uniform = Range::uniform();
        let board = parse_cards("Ac Kd 7s 7h 2c").unwrap();
        let e = equity_vs_ranges(hole("Ah Kh"), &board, &[uniform.weights()])[0];
        let random = crate::coach::equity_vs_random(hole("Ah Kh"), &board);
        assert!((e - random).abs() < 1e-9, "{e} vs {random}");
    }

    #[test]
    fn narrow_ranges_change_equity() {
        let board = parse_cards("Ac 8d 7s 3h 2c").unwrap();
        let mut aces = Range::uniform();
        aces.update(|h| {
            if h.iter().any(|c| c.rank() == 12) {
                1.0
            } else {
                0.0
            }
        });
        let [all, only_aces] = equity_vs_ranges(
            hole("Kh Kd"),
            &board,
            &[Range::uniform().weights(), aces.weights()],
        )[..] else {
            unreachable!()
        };
        assert!(all > 0.6, "{all}");
        assert!(only_aces < 0.05, "{only_aces}");
    }

    #[test]
    fn profiles_sum_to_one_and_reflect_updates() {
        let board = parse_cards("Ah 7c 2d").unwrap();
        let strengths = Strengths::new(&board);
        let mut range = Range::uniform();
        let p = range.profile(&strengths, &[]);
        close(p.strong + p.medium + p.draws + p.weak, 1.0);
        range.update(|h| strengths.made(h));
        assert!(range.profile(&strengths, &[]).strong > p.strong);
        assert!(!range.is_uniform());
    }

    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} vs {b}");
    }
}
