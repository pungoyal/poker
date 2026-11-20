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

impl Category {
    pub fn name(self) -> &'static str {
        match self {
            Category::HighCard => "high card",
            Category::Pair => "a pair",
            Category::TwoPair => "two pair",
            Category::Trips => "three of a kind",
            Category::Straight => "a straight",
            Category::Flush => "a flush",
            Category::FullHouse => "a full house",
            Category::Quads => "four of a kind",
            Category::StraightFlush => "a straight flush",
        }
    }
}

/// Strength of a hand. A higher value beats a lower one; equal values tie.
/// Layout: category in the high bits, then five 4-bit ranks, most significant first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HandRank(u32);

impl HandRank {
    fn new(category: Category, ranks: &[u8]) -> HandRank {
        debug_assert!(ranks.len() <= 5);
        let mut value = category as u32;
        for i in 0..5 {
            value = (value << 4) | ranks.get(i).copied().unwrap_or(0) as u32;
        }
        HandRank(value)
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

/// Highest card of the best straight in a 13-bit rank mask, if any.
/// The wheel (A-2-3-4-5) counts as five-high.
fn straight_high(mask: u16) -> Option<u8> {
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
fn top_ranks(mask: u16, n: usize) -> Vec<u8> {
    (0..13u8).rev().filter(|r| mask & (1 << r) != 0).take(n).collect()
}

/// Evaluates the best five-card hand among `cards` (five to seven cards).
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
        return HandRank::new(Category::StraightFlush, &[high]);
    }

    // Ranks grouped by how many copies we hold, each highest first.
    let of_count = |n: u8| -> Vec<u8> { (0..13u8).rev().filter(|&r| counts[r as usize] == n).collect() };
    let quads = of_count(4);
    let trips = of_count(3);
    let pairs = of_count(2);
    let without = |exclude: &[u8]| -> u16 {
        exclude.iter().fold(rank_mask, |mask, r| mask & !(1 << r))
    };

    if let Some(&quad) = quads.first() {
        let kicker = top_ranks(without(&[quad]), 1);
        return HandRank::new(Category::Quads, &[quad, kicker[0]]);
    }

    if let Some(&trip) = trips.first() {
        // The pair half may come from a second set of trips.
        let pair = trips.get(1).copied().max(pairs.first().copied());
        if let Some(pair) = pair {
            return HandRank::new(Category::FullHouse, &[trip, pair]);
        }
    }

    if let Some(mask) = flush_mask {
        return HandRank::new(Category::Flush, &top_ranks(mask, 5));
    }

    if let Some(high) = straight_high(rank_mask) {
        return HandRank::new(Category::Straight, &[high]);
    }

    if let Some(&trip) = trips.first() {
        let mut ranks = vec![trip];
        ranks.extend(top_ranks(without(&[trip]), 2));
        return HandRank::new(Category::Trips, &ranks);
    }

    if pairs.len() >= 2 {
        let mut ranks = vec![pairs[0], pairs[1]];
        ranks.extend(top_ranks(without(&[pairs[0], pairs[1]]), 1));
        return HandRank::new(Category::TwoPair, &ranks);
    }

    if let Some(&pair) = pairs.first() {
        let mut ranks = vec![pair];
        ranks.extend(top_ranks(without(&[pair]), 3));
        return HandRank::new(Category::Pair, &ranks);
    }

    HandRank::new(Category::HighCard, &top_ranks(rank_mask, 5))
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
            assert!(Some(value) > previous, "{hand} should beat the hand before it");
            previous = Some(value);
        }
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
        assert_eq!(counts, [1_302_540, 1_098_240, 123_552, 54_912, 10_200, 5_108, 3_744, 624, 40]);
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
