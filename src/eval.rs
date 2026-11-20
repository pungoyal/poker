//! Hand evaluation: the best five-card poker hand out of five to seven cards.

use crate::cards::Card;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Category {
    HighCard,
    Pair,
    TwoPair,
    Trips,
    Straight,
    Flush,
    FullHouse,
    Quads,
    StraightFlush,
}

/// Strength of a hand. A higher value beats a lower one; equal values tie.
/// Layout: category in the high bits, then five 4-bit ranks, most significant first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HandRank(u32);

impl HandRank {
    /// Packs a category and up to five ranks; any further ranks are ignored.
    fn new(category: Category, ranks: impl IntoIterator<Item = u8>) -> HandRank {
        let mut ranks = ranks.into_iter();
        let mut value = category as u32;
        for _ in 0..5 {
            value = (value << 4) | ranks.next().unwrap_or(0) as u32;
        }
        HandRank(value)
    }

    /// The ranks that define the hand, most significant first: for a pair,
    /// the pair's rank and then the kickers. Unused slots are zero.
    pub fn ranks(self) -> [u8; 5] {
        std::array::from_fn(|i| ((self.0 >> (16 - 4 * i)) & 0xF) as u8)
    }

    /// Plain-English name, e.g. "a pair of fives".
    pub fn describe(self) -> String {
        let r = self.ranks();
        let (one, many) = (|i: usize| rank_name(r[i]), |i: usize| rank_plural(r[i]));
        match self.category() {
            Category::HighCard => format!("{} high", one(0)),
            Category::Pair => format!("a pair of {}", many(0)),
            Category::TwoPair => format!("two pair, {} and {}", many(0), many(1)),
            Category::Trips => format!("three {}", many(0)),
            Category::Straight => format!("a straight, {} high", one(0)),
            Category::Flush => format!("a flush, {} high", one(0)),
            Category::FullHouse => format!("a full house, {} full of {}", many(0), many(1)),
            Category::Quads => format!("four {}", many(0)),
            Category::StraightFlush if r[0] == 12 => "a royal flush".to_string(),
            Category::StraightFlush => format!("a straight flush, {} high", one(0)),
        }
    }

    pub fn category(self) -> Category {
        match self.0 >> 20 {
            0 => Category::HighCard,
            1 => Category::Pair,
            2 => Category::TwoPair,
            3 => Category::Trips,
            4 => Category::Straight,
            5 => Category::Flush,
            6 => Category::FullHouse,
            7 => Category::Quads,
            _ => Category::StraightFlush,
        }
    }
}

const RANK_NAMES: [&str; 13] = [
    "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "jack", "queen",
    "king", "ace",
];

/// "ace", "king", ... for a rank where 0 is a two and 12 an ace.
pub fn rank_name(rank: u8) -> &'static str {
    RANK_NAMES[rank as usize]
}

/// "aces", "kings", ..., "sixes".
pub fn rank_plural(rank: u8) -> String {
    let name = rank_name(rank);
    if name == "six" {
        "sixes".to_string()
    } else {
        format!("{name}s")
    }
}

/// Highest card of the best straight in a 13-bit rank mask, if any.
/// The wheel (A-2-3-4-5) counts as five-high.
pub(crate) fn straight_high(mask: u16) -> Option<u8> {
    for high in (4..=12u8).rev() {
        let window = 0b11111 << (high - 4);
        if mask & window == window {
            return Some(high);
        }
    }
    const WHEEL: u16 = (1 << 12) | 0b1111;
    (mask & WHEEL == WHEEL).then_some(3)
}

/// The set bits of a rank mask, highest first, limited to `n`.
fn top_ranks(mask: u16, n: usize) -> impl Iterator<Item = u8> {
    (0..13u8)
        .rev()
        .filter(move |r| mask & (1 << r) != 0)
        .take(n)
}

/// The highest set bit of a rank mask.
fn highest(mask: u16) -> Option<u8> {
    (mask != 0).then(|| 15 - mask.leading_zeros() as u8)
}

/// Evaluates the best five-card hand among `cards` (five to seven cards).
/// Allocation-free: the coach calls this about a million times per decision.
pub fn evaluate(cards: &[Card]) -> HandRank {
    assert!((5..=7).contains(&cards.len()), "need 5 to 7 cards");

    let mut counts = [0u8; 13];
    let mut suit_masks = [0u16; 4];
    let mut rank_mask = 0u16;
    for card in cards {
        counts[card.rank() as usize] += 1;
        suit_masks[card.suit() as usize] |= 1 << card.rank();
        rank_mask |= 1 << card.rank();
    }

    let flush_mask = suit_masks.iter().copied().find(|m| m.count_ones() >= 5);
    if let Some(mask) = flush_mask
        && let Some(high) = straight_high(mask)
    {
        return HandRank::new(Category::StraightFlush, [high]);
    }

    // Ranks held exactly once, twice, three or four times.
    let mut of_count = [0u16; 5];
    for (rank, &count) in counts.iter().enumerate() {
        of_count[count.min(4) as usize] |= 1 << rank;
    }
    let [_, _, pairs, trips, quads] = of_count;
    let without = |rank: u8| rank_mask & !(1 << rank);

    if let Some(quad) = highest(quads) {
        let kicker = highest(without(quad)).unwrap_or(0);
        return HandRank::new(Category::Quads, [quad, kicker]);
    }

    if let Some(trip) = highest(trips) {
        // The pair half may come from a second set of trips.
        if let Some(pair) = highest((trips & !(1 << trip)) | pairs) {
            return HandRank::new(Category::FullHouse, [trip, pair]);
        }
    }

    if let Some(mask) = flush_mask {
        return HandRank::new(Category::Flush, top_ranks(mask, 5));
    }

    if let Some(high) = straight_high(rank_mask) {
        return HandRank::new(Category::Straight, [high]);
    }

    if let Some(trip) = highest(trips) {
        let kickers = top_ranks(without(trip), 2);
        return HandRank::new(Category::Trips, std::iter::once(trip).chain(kickers));
    }

    if let Some(high_pair) = highest(pairs) {
        if let Some(low_pair) = highest(pairs & !(1 << high_pair)) {
            let kicker = top_ranks(without(high_pair) & !(1 << low_pair), 1);
            return HandRank::new(
                Category::TwoPair,
                [high_pair, low_pair].into_iter().chain(kicker),
            );
        }
        let kickers = top_ranks(without(high_pair), 3);
        return HandRank::new(Category::Pair, std::iter::once(high_pair).chain(kickers));
    }

    HandRank::new(Category::HighCard, top_ranks(rank_mask, 5))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{parse_cards, shuffled_deck};

    fn rank(s: &str) -> HandRank {
        evaluate(&parse_cards(s).unwrap())
    }

    #[test]
    fn categories_are_recognised_and_ordered() {
        let ladder = [
            ("Ah Kd 9c 7s 3h", Category::HighCard),
            ("2h 2d 9c 7s 3h", Category::Pair),
            ("2h 2d 3c 3s 4h", Category::TwoPair),
            ("2h 2d 2c 7s 3h", Category::Trips),
            ("Ah 2d 3c 4s 5h", Category::Straight),
            ("2h 4h 6h 8h Th", Category::Flush),
            ("2h 2d 2c 3s 3h", Category::FullHouse),
            ("2h 2d 2c 2s 3h", Category::Quads),
            ("Ah 2h 3h 4h 5h", Category::StraightFlush),
        ];
        let mut previous = None;
        for (hand, category) in ladder {
            let value = rank(hand);
            assert_eq!(value.category(), category, "{hand}");
            assert!(
                Some(value) > previous,
                "{hand} should beat the hand before it"
            );
            previous = Some(value);
        }
    }

    #[test]
    fn descriptions() {
        assert_eq!(rank("Ah Kd 9c 7s 3h").describe(), "ace high");
        assert_eq!(rank("6h 6d 9c 7s 3h").describe(), "a pair of sixes");
        assert_eq!(
            rank("Kh Kd 3c 3s 4h").describe(),
            "two pair, kings and threes"
        );
        assert_eq!(rank("Ah 2d 3c 4s 5h").describe(), "a straight, five high");
        assert_eq!(
            rank("2h 2d 2c 3s 3h").describe(),
            "a full house, twos full of threes"
        );
        assert_eq!(rank("Th Jh Qh Kh Ah").describe(), "a royal flush");
    }

    #[test]
    fn straights_edge_cases() {
        assert!(rank("Ah 2d 3c 4s 5h") < rank("2d 3c 4s 5h 6h"));
        assert!(rank("Th Jd Qc Ks Ah") > rank("9h Td Jc Qs Kh"));
        // K-A-2-3-4 does not wrap around.
        assert_eq!(rank("Kh Ad 2c 3s 4h").category(), Category::HighCard);
        // With six in a row, the highest straight is used.
        assert_eq!(rank("4h 5d 6c 7s 8h 9d 2c"), rank("5d 6c 7s 8h 9d"));
    }

    #[test]
    fn kickers_break_ties() {
        assert!(rank("Ah Ad Kc 7s 3h") > rank("As Ac Qc 7d 3d"));
        assert!(rank("Ah Ad Kc 7s 3h") > rank("As Ac Kd 6d 3d"));
        assert_eq!(rank("Ah Ad Kc 7s 3h"), rank("As Ac Kd 7d 3d"));
        assert!(rank("Kh Kd 4c 4s Ah") > rank("Ks Kc 4d 4h Qh"));
    }

    #[test]
    fn seven_card_hands_use_the_best_five() {
        // Three pairs: best two pair plus the best remaining kicker.
        assert_eq!(rank("Ah Ad Kc Ks Qh Qd 2c"), rank("Ah Ad Kc Ks Qh"));
        // Two sets of trips make a full house.
        assert_eq!(rank("Ah Ad Ac Ks Kh Kd 2c"), rank("Ah Ad Ac Ks Kh"));
        // A flush beats the straight that is also present.
        assert_eq!(rank("2h 3h 4h 5d 6c 9h Kh").category(), Category::Flush);
        // The sixth card of a suit does not change which five play.
        assert_eq!(rank("2h 4h 6h 8h Th Qh Ah"), rank("6h 8h Th Qh Ah"));
        // Playing the board is a tie whatever the hole cards.
        assert_eq!(rank("Ts Js Qs Ks As 2c 2d"), rank("Ts Js Qs Ks As 3h 4h"));
    }

    #[test]
    fn five_card_category_counts_match_known_totals() {
        let deck: Vec<Card> = (0..52).map(Card::from_index).collect();
        let mut counts = [0u32; 9];
        for a in 0..52 {
            for b in a + 1..52 {
                for c in b + 1..52 {
                    for d in c + 1..52 {
                        for e in d + 1..52 {
                            let hand = [deck[a], deck[b], deck[c], deck[d], deck[e]];
                            counts[evaluate(&hand).category() as usize] += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(
            counts,
            [
                1_302_540, 1_098_240, 123_552, 54_912, 10_200, 5_108, 3_744, 624, 40
            ]
        );
    }

    #[test]
    fn seven_card_result_equals_best_of_all_five_card_subsets() {
        for seed in 0..3000 {
            let deck = shuffled_deck(seed);
            let seven = &deck[..7];
            let mut best = None;
            for skip_a in 0..7 {
                for skip_b in skip_a + 1..7 {
                    let five: Vec<Card> = (0..7)
                        .filter(|&i| i != skip_a && i != skip_b)
                        .map(|i| seven[i])
                        .collect();
                    best = best.max(Some(evaluate(&five)));
                }
            }
            assert_eq!(Some(evaluate(seven)), best, "cards {seven:?}");
        }
    }
}
