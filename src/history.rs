//! Hand records: everything needed to replay a finished hand exactly.
//! Records are stored one JSON object per line.

use crate::cards::{Card, shuffled_deck};
use crate::game::{Action, Config, HandState};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HandRecord {
    pub hand_no: u64,
    /// Seed the deck was shuffled from.
    pub seed: u64,
    pub config: Config,
    pub button: usize,
    /// Seat of the human player.
    pub hero: usize,
    pub opponent: String,
    pub hole: [[Card; 2]; 2],
    pub board: [Card; 5],
    pub actions: Vec<(usize, Action)>,
    pub net: [i64; 2],
}

impl HandRecord {
    /// Captures a finished hand.
    pub fn from_state(
        state: &HandState,
        hand_no: u64,
        seed: u64,
        hero: usize,
        opponent: &str,
    ) -> HandRecord {
        let outcome = state.outcome().expect("cannot record an unfinished hand");
        HandRecord {
            hand_no,
            seed,
            config: state.config(),
            button: state.button(),
            hero,
            opponent: opponent.to_string(),
            hole: [state.hole(0), state.hole(1)],
            board: state.full_board(),
            actions: state.actions().to_vec(),
            net: outcome.net,
        }
    }

    /// Checks the record describes a real deal: valid stakes and seats, and the
    /// cards the recorded seed shuffles to. Anything that passes is safe to replay.
    pub fn validate(&self) -> Result<(), String> {
        self.config.validate()?;
        if self.button > 1 || self.hero > 1 {
            return Err(format!(
                "seats must be 0 or 1 (button {}, hero {})",
                self.button, self.hero
            ));
        }
        let deck = shuffled_deck(self.seed);
        let hole = [[deck[0], deck[1]], [deck[2], deck[3]]];
        if self.hole != hole || self.board[..] != deck[4..9] {
            return Err(format!("cards do not match seed {}", self.seed));
        }
        Ok(())
    }

    /// Validates the record, replays the recorded actions and checks they
    /// reproduce the recorded result.
    pub fn replay(&self) -> Result<HandState, String> {
        self.validate()?;
        let mut state = HandState::new(self.config, self.button, self.hole, self.board);
        for (i, &(seat, action)) in self.actions.iter().enumerate() {
            if state.to_act() != Some(seat) {
                return Err(format!("action {i}: seat {seat} acted out of turn"));
            }
            state
                .apply(action)
                .map_err(|e| format!("action {i}: {e}"))?;
        }
        match state.outcome() {
            None => Err("recorded actions do not finish the hand".into()),
            Some(outcome) if outcome.net != self.net => Err(format!(
                "replay result {:?} differs from recorded {:?}",
                outcome.net, self.net
            )),
            Some(_) => Ok(state),
        }
    }
}

pub fn append(path: &Path, record: &HandRecord) -> io::Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let line = serde_json::to_string(record).map_err(io::Error::other)?;
    writeln!(file, "{line}")
}

pub fn load(path: &Path) -> io::Result<Vec<HandRecord>> {
    let file = std::fs::File::open(path)?;
    let mut records = Vec::new();
    for (i, line) in BufReader::new(file).lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let record = serde_json::from_str(&line).map_err(|e| {
            io::Error::new(io::ErrorKind::InvalidData, format!("line {}: {e}", i + 1))
        })?;
        records.push(record);
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::{Bot, RandomBot};
    use crate::cards::shuffled_deck;

    fn random_hand(seed: u64) -> HandRecord {
        let mut bots = [RandomBot::new(seed), RandomBot::new(seed + 1_000_000)];
        let button = (seed % 2) as usize;
        let mut state = HandState::deal(Config::default(), button, &shuffled_deck(seed));
        while let Some(seat) = state.to_act() {
            let action = bots[seat].act(&state, seat);
            state.apply(action).unwrap();
        }
        HandRecord::from_state(&state, seed, seed, 0, "random")
    }

    #[test]
    fn records_survive_json_and_replay_to_the_same_result() {
        for seed in 0..500 {
            let record = random_hand(seed);
            let json = serde_json::to_string(&record).unwrap();
            let parsed: HandRecord = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, record);
            parsed
                .replay()
                .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        }
    }

    #[test]
    fn replay_detects_tampering() {
        let mut record = random_hand(7);
        record.net = [record.net[0] + 1, record.net[1] - 1];
        assert!(record.replay().is_err());

        let mut record = random_hand(7);
        record.actions.pop();
        assert!(record.replay().is_err());

        // Swapped or duplicated cards no longer match the seed.
        let mut record = random_hand(7);
        record.hole[0].swap(0, 1);
        record.hole[1][0] = record.hole[0][0];
        assert!(record.replay().unwrap_err().contains("seed"));

        // Malformed fields are errors, not panics.
        let mut record = random_hand(7);
        record.hero = 2;
        assert!(record.replay().is_err());
        let mut record = random_hand(7);
        record.button = 5;
        assert!(record.replay().is_err());
        let mut record = random_hand(7);
        record.config.small_blind = 0;
        assert!(record.replay().is_err());
    }

    #[test]
    fn file_round_trip() {
        let path =
            std::env::temp_dir().join(format!("poker-history-test-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let records: Vec<HandRecord> = (0..5).map(random_hand).collect();
        for record in &records {
            append(&path, record).unwrap();
        }
        assert_eq!(load(&path).unwrap(), records);
        std::fs::remove_file(&path).unwrap();
    }
}
