//! The conversation with the language-model coach. The poker engine works out
//! every number; the model turns those facts into teaching.

use crate::coach::{Spot, brief, fmt_bb};
use crate::game::Action;
use crate::session::Session;
use serde_json::{Value, json};

/// Who said a message in the conversation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

/// One message of the conversation with the language model.
#[derive(Clone, Debug)]
pub struct Message {
    pub role: Role,
    pub text: String,
}

pub const SYSTEM: &str = "\
You are the coach inside a heads-up No-Limit Hold'em trainer. The player is learning to make decisions \
themselves; teach the reasoning, not only the answer.

Each request contains FACTS worked out exactly by the poker engine: the player's hand, the board, the pot, \
the price of a call, the player's equity, what the opponent's actions say about its hand, and the average \
value of each option. Treat FACTS as ground truth. Never state a number that is not in FACTS or simple \
arithmetic on them, and never do your own equity calculations. Values assume the hand is checked down \
afterwards, apart from the opponent's reply to a check; mention that only when it changes the answer.

The player reads you mid-hand, so every word must earn its place. Each sentence carries a decision-relevant \
fact, number or principle; cut anything that does not. That means no greetings, praise, encouragement, \
hedging, apologies, restating what the player said or did, or closing remarks. Lead with the verdict, the \
correction or the question. Prefer a number from FACTS to an adjective. Short sentences, one to three in \
total unless the player asks for more. Plain text only: no markdown, lists or emoji. Amounts in big blinds \
(bb). Speak to the player as \"you\".

Teach, as the hand calls for it: ranges (which hands the opponent can hold after what it did), equity \
against that range, pot odds, fold equity, value betting versus bluffing, bet sizing, position and \
initiative. Tie each point to this hand.

The opponents: \"solid\" plays a sound, balanced strategy, so each of its actions narrows its range; \
\"random\" ignores its cards, so its actions say nothing about its hand; \"calling station\" never folds \
and never bets.";

/// What the coach is being asked to do.
pub enum Ask {
    /// The player acted: review the decision.
    Review,
    /// The player asked the coach something.
    Free(String),
    /// The session ended: sum up the leaks to work on.
    Summary,
}

impl Ask {
    fn task(&self) -> String {
        match self {
            Ask::Review => "The player has acted (hero_action in FACTS). First the verdict with the \
                deciding numbers (what they chose versus the best option), then the one principle that \
                decided it. If the grade is Good, confirm in one sentence and add only an insight the \
                player would not already know. At most three sentences."
                .to_string(),
            Ask::Free(question) => format!(
                "The player asks: \"{question}\"\nAnswer directly, with numbers from FACTS where they apply."
            ),
            Ask::Summary => "The session is over. FACTS hold the player's results and their costliest \
                leaks. Three sentences at most: the leak to fix first, how to fix it in this player's \
                spots, and the drill (in the menu) to practise it. Mention a strength only if FACTS show one."
                .to_string(),
        }
    }
}

/// The facts the language-model coach works from: the spot, and the
/// player's session so far.
pub fn facts(spot: &Spot, action: Option<Action>, session: Option<&Session>) -> Value {
    let mut facts = brief(spot, action);
    if let Some(session) = session {
        let big_blind = session.config().big_blind;
        let leaks: Vec<Value> = session
            .tally()
            .lines()
            .iter()
            .take(2)
            .map(|l| {
                json!(format!(
                    "{} on the {} ({} times)",
                    l.leak.label(),
                    l.street.name().to_lowercase(),
                    l.count
                ))
            })
            .collect();
        let seen = session.observed();
        let pct = |x: Option<f64>| x.map(|x| (x * 100.0).round());
        facts["opponent_seen_so_far"] = json!({
            "hands": seen.hands,
            "plays_pct_of_hands": pct(seen.plays()),
            "raises_preflop_pct_of_hands": pct(seen.raises()),
            "folds_to_postflop_bets_pct": pct(seen.folds_to_bets()),
            "postflop_bets_and_raises_pct_of_its_actions": pct(seen.aggression()),
        });
        if session.opponent_hidden() {
            // The player has to read the opponent: do not hand them its name.
            facts["opponent"] = json!(
                "unknown: the player is working out its style from how it plays; do not name the style, point to what it has been seen to do"
            );
            if let Some(map) = facts.as_object_mut() {
                map.remove("opponent_style");
                map.remove("how_to_beat_it");
            }
        }
        facts["session"] = json!({
            "hands_played": session.finished(),
            "result_bb": session.total() as f64 / big_blind as f64,
            "recurring_leaks": leaks,
        });
    }
    facts
}

/// The facts for an end-of-session summary.
pub fn session_facts(session: &Session) -> Value {
    let big_blind = session.config().big_blind;
    let to_bb = |chips: f64| (chips / big_blind as f64 * 100.0).round() / 100.0;
    let grades: Vec<Value> = session
        .grades()
        .iter()
        .map(|(grade, count)| json!({ "grade": grade.label(), "count": count }))
        .collect();
    let leaks: Vec<Value> = session
        .tally()
        .lines()
        .iter()
        .take(4)
        .map(|l| {
            json!({
                "leak": l.leak.label(),
                "street": l.street.name(),
                "times": l.count,
                "cost_bb": to_bb(l.loss),
                "principle": l.leak.tip(),
            })
        })
        .collect();
    json!({
        "opponent": session.bot_name(),
        "hands_played": session.finished(),
        "result_bb": to_bb(session.total() as f64),
        "estimated_bb_given_up": to_bb(session.given_up()),
        "grades": grades,
        "leaks_costliest_first": leaks,
        "result_note": format!("Results over {} hands swing a lot with the cards; the bb given up is the better measure of play.", session.finished()),
        "given_up_text": fmt_bb(session.given_up(), big_blind),
    })
}

/// The conversation for the current hand.
#[derive(Default)]
pub struct Chat {
    messages: Vec<Message>,
}

impl Chat {
    /// Starts afresh, e.g. for a new hand.
    pub fn reset(&mut self) {
        self.messages.clear();
    }

    /// Adds the player's side of the next exchange and returns the whole
    /// conversation to send. `facts` and `history` are included when given.
    pub fn request(
        &mut self,
        ask: &Ask,
        facts: Option<&Value>,
        history: &[String],
    ) -> Vec<Message> {
        let mut text = String::new();
        if let Some(facts) = facts {
            text.push_str("FACTS:\n");
            text.push_str(&serde_json::to_string_pretty(facts).unwrap_or_default());
            text.push_str("\n\n");
        }
        if !history.is_empty() {
            text.push_str("HAND SO FAR:\n");
            text.push_str(&history.join("\n"));
            text.push_str("\n\n");
        }
        text.push_str("TASK: ");
        text.push_str(&ask.task());
        // Keep the roles alternating even if an earlier reply was lost.
        if self.messages.last().is_some_and(|m| m.role == Role::User) {
            self.messages.pop();
        }
        self.messages.push(Message {
            role: Role::User,
            text,
        });
        self.messages.clone()
    }

    /// Records the coach's reply.
    pub fn reply(&mut self, text: &str) {
        if self.messages.last().is_some_and(|m| m.role == Role::User) {
            self.messages.push(Message {
                role: Role::Assistant,
                text: text.to_string(),
            });
        }
    }

    /// Drops a request whose reply failed.
    pub fn abandon(&mut self) {
        if self.messages.last().is_some_and(|m| m.role == Role::User) {
            self.messages.pop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversations_alternate() {
        let mut chat = Chat::default();
        let facts = serde_json::json!({"equity_pct": 40.0});
        let first = chat.request(&Ask::Review, Some(&facts), &["Preflop: Bot raises.".into()]);
        assert_eq!(first.len(), 1);
        assert!(first[0].text.contains("FACTS") && first[0].text.contains("Bot raises"));
        chat.reply("Calling was best: the raise is wide.");
        let second = chat.request(&Ask::Free("why is it wide?".into()), None, &[]);
        assert_eq!(second.len(), 3);
        assert_eq!(second[1].role, Role::Assistant);
        // A failed reply is dropped, and a new request replaces it.
        chat.abandon();
        let third = chat.request(&Ask::Review, Some(&facts), &[]);
        assert_eq!(third.len(), 3);
        assert_eq!(third[2].role, Role::User);
    }
}
