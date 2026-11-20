//! Cards, and a small self-contained seeded shuffle.
//!
//! The shuffle uses our own PRNG rather than a crate so that a seed always
//! reproduces the same deck, regardless of dependency versions.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

const RANK_CHARS: &[u8; 13] = b"23456789TJQKA";
const SUIT_CHARS: &[u8; 4] = b"cdhs";
const SUIT_SYMBOLS: [&str; 4] = ["♣", "♦", "♥", "♠"];

/// A playing card, stored as `rank * 4 + suit`.
/// Rank 0 is a deuce and 12 an ace; suits are clubs, diamonds, hearts, spades.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Card(u8);

impl Card {
    pub fn new(rank: u8, suit: u8) -> Card {
        assert!(rank < 13 && suit < 4);
        Card(rank * 4 + suit)
    }

    pub fn from_index(index: u8) -> Card {
        assert!(index < 52);
        Card(index)
    }

    pub fn rank(self) -> u8 {
        self.0 / 4
    }

    pub fn suit(self) -> u8 {
        self.0 % 4
    }

    /// Display form with a suit symbol, optionally ANSI-coloured.
    pub fn pretty(self, color: bool) -> String {
        let rank = RANK_CHARS[self.rank() as usize] as char;
        let symbol = SUIT_SYMBOLS[self.suit() as usize];
        if !color {
            return format!("{rank}{symbol}");
        }
        // Four-colour deck: green clubs, blue diamonds, red hearts, white spades.
        let code = ["32", "34", "31", "37"][self.suit() as usize];
        format!("\x1b[1;{code}m{rank}{symbol}\x1b[0m")
    }
}

impl fmt::Display for Card {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let rank = RANK_CHARS[self.rank() as usize] as char;
        let suit = SUIT_CHARS[self.suit() as usize] as char;
        write!(f, "{rank}{suit}")
    }
}

impl fmt::Debug for Card {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}

impl FromStr for Card {
    type Err = String;

    fn from_str(s: &str) -> Result<Card, String> {
        let bytes = s.as_bytes();
        if bytes.len() != 2 {
            return Err(format!("invalid card '{s}'"));
        }
        let rank = RANK_CHARS
            .iter()
            .position(|&c| c == bytes[0].to_ascii_uppercase())
            .ok_or_else(|| format!("invalid rank in card '{s}'"))?;
        let suit = SUIT_CHARS
            .iter()
            .position(|&c| c == bytes[1].to_ascii_lowercase())
            .ok_or_else(|| format!("invalid suit in card '{s}'"))?;
        Ok(Card::new(rank as u8, suit as u8))
    }
}

impl TryFrom<String> for Card {
    type Error = String;

    fn try_from(s: String) -> Result<Card, String> {
        s.parse()
    }
}

impl From<Card> for String {
    fn from(card: Card) -> String {
        card.to_string()
    }
}

/// Parses a whitespace-separated list of cards, e.g. `"As Kd 7c"`.
pub fn parse_cards(s: &str) -> Result<Vec<Card>, String> {
    s.split_whitespace().map(str::parse).collect()
}

/// SplitMix64: tiny, fast, and good enough for shuffling and dumb bots.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform integer in `0..n`, without modulo bias.
    pub fn below(&mut self, n: u64) -> u64 {
        assert!(n > 0);
        let zone = u64::MAX - (u64::MAX % n);
        loop {
            let x = self.next_u64();
            if x < zone {
                return x % n;
            }
        }
    }

    /// Uniform float in `[0, 1)`.
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// A full 52-card deck in an order fully determined by `seed`.
pub fn shuffled_deck(seed: u64) -> [Card; 52] {
    let mut deck: [Card; 52] = std::array::from_fn(|i| Card(i as u8));
    let mut rng = Rng::new(seed);
    for i in (1..52).rev() {
        let j = rng.below(i as u64 + 1) as usize;
        deck.swap(i, j);
    }
    deck
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn card_text_round_trips() {
        for index in 0..52 {
            let card = Card::from_index(index);
            assert_eq!(card.to_string().parse::<Card>().unwrap(), card);
        }
        assert_eq!("as".parse::<Card>().unwrap(), Card::new(12, 3));
        assert!("1s".parse::<Card>().is_err());
        assert!("Asx".parse::<Card>().is_err());
    }

    #[test]
    fn shuffle_is_a_seeded_permutation() {
        let a = shuffled_deck(42);
        assert_eq!(a, shuffled_deck(42));
        assert_ne!(a, shuffled_deck(43));
        assert_eq!(a.iter().collect::<HashSet<_>>().len(), 52);
    }

    #[test]
    fn shuffle_is_roughly_uniform() {
        // The ace of spades should land in each position about equally often.
        let trials = 52_000;
        let mut counts = [0u32; 52];
        for seed in 0..trials {
            let deck = shuffled_deck(seed);
            let pos = deck.iter().position(|&c| c == Card::new(12, 3)).unwrap();
            counts[pos] += 1;
        }
        for &count in &counts {
            assert!((800..1200).contains(&count), "position count {count}");
        }
    }
}
