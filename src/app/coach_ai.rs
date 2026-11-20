//! The conversation with the language-model coach. The poker engine works out
//! every number; the model turns those facts into teaching.

use super::llm::{Message, Role};
use serde_json::Value;

pub const SYSTEM: &str = "\
You are the coach inside a heads-up No-Limit Hold'em trainer. The player is learning, and your job is \
to teach them how to make decisions themselves, not only to tell them answers.

Each request contains FACTS worked out exactly by the poker engine: the player's hand, the board, the pot, \
the price of a call, the player's equity, what the opponent's actions say about its hand, and the average \
value of each option. Treat FACTS as ground truth. Never state a number that is not in FACTS or simple \
arithmetic on them, and never do your own equity calculations. Values assume the hand is checked down \
afterwards, so they ignore later betting; mention that only when it matters.

How to talk: speak to the player as \"you\", warm and direct, like a good coach sitting beside them. \
Plain text only: no markdown, headings, bullet points or emoji. Be brief: two to four sentences unless \
the player asks something that needs more. Give amounts in big blinds (bb). Vary your wording; never \
open with the same phrase twice in a row.

What to teach, as the hand calls for it: ranges (which hands the opponent can hold after what it did), \
equity against that range rather than against any two cards, pot odds, fold equity, value betting versus \
bluffing, bet sizing, position and initiative. Tie each point to this hand.

The opponents: \"solid\" plays a sound, balanced strategy, so each of its actions narrows its range; \
\"random\" ignores its cards, so its actions say nothing about its hand; \"calling station\" never folds \
and never bets.";

/// What the coach is being asked to do.
pub enum Ask {
    /// Before the player decides: ask one guiding question.
    Question,
    /// The player answered the coach's question.
    Answer(String),
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
            Ask::Question => "It is the player's turn. Ask ONE short question that leads them to the key \
                consideration for this decision: for example what the bot's actions say about its hand, \
                how often they win, what the price is, or what a bet would achieve. Do not reveal the \
                answer or the best action."
                .to_string(),
            Ask::Answer(answer) => format!(
                "The player answers: \"{answer}\"\nTell them whether they are on track, correcting them \
                 with the FACTS. Then either ask one more short question if a key step is still missing, \
                 or tell them they are ready to decide. Do not reveal the best action unless they ask for it."
            ),
            Ask::Review => "The player has acted (hero_action in FACTS). Review it: say whether it was right, \
                what the best option was and why, and the one principle to take away. If the grade is \
                Good, say so briefly and add one insight about the spot."
                .to_string(),
            Ask::Free(question) => format!(
                "The player asks: \"{question}\"\nAnswer it, using the FACTS where they apply."
            ),
            Ask::Summary => "The session is over. FACTS hold the player's results and their costliest \
                leaks. In three to five sentences, tell them what they did well, the one leak to work on \
                first and how to fix it, and suggest running 'poker drill' to practise it."
                .to_string(),
        }
    }
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
        let first = chat.request(
            &Ask::Question,
            Some(&facts),
            &["Preflop: Bot raises.".into()],
        );
        assert_eq!(first.len(), 1);
        assert!(first[0].text.contains("FACTS") && first[0].text.contains("Bot raises"));
        chat.reply("What does that raise tell you?");
        let second = chat.request(&Ask::Answer("it is strong".into()), None, &[]);
        assert_eq!(second.len(), 3);
        assert_eq!(second[1].role, Role::Assistant);
        // A failed reply is dropped, and a new request replaces it.
        chat.abandon();
        let third = chat.request(&Ask::Review, Some(&facts), &[]);
        assert_eq!(third.len(), 3);
        assert_eq!(third[2].role, Role::User);
    }
}
