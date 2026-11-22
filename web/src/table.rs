//! The table behind the page: a session or a drill, the coach's questions and
//! reviews, and the view the page draws. Pure state, like the library: the
//! page does the timing, the storage and the network.

use poker::analysis::{Decision, Leak, analyse_with, decisions};
use poker::bot::{Bot, STYLES, StyleBot, by_name};
use poker::cards::{Card, Rng};
use poker::chat::{Ask, Chat, Role, SYSTEM, facts, session_facts};
use poker::coach::{
    Grade, Question, Skill, Spot, describe_hand, drills, fmt_bb, fmt_bb_signed, fmt_pot, fmt_share,
    headline, pot_odds, review, standard_sizes, why,
};
use poker::game::{Action, Config, HandState, Street};
use poker::history::HandRecord;
use poker::range::{Range, Strengths, combo};
use poker::session::{HERO, Observed, Review, Session};
use poker::text::{describe, narrate};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

const BOT: usize = 1 - HERO;
/// The newest coach items sent to the page; older ones have scrolled away.
const FEED_SHOWN: usize = 120;
/// Earlier hands listed in the session panel.
const PAST_SHOWN: usize = 40;

/// How the coach helps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// No coaching.
    Off,
    /// A review after each decision.
    After,
    /// Reviews, plus the numbers before you act.
    Full,
    /// A question before each decision, then a review.
    #[default]
    Teach,
}

/// One reviewed decision, as stored with its hand.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Dec {
    street: Street,
    grade: u8,
    loss: f64,
    leak: Option<u8>,
}

/// A finished hand as the page stores it: the record, the coach's marks and
/// the session it came from.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Entry {
    sid: String,
    /// The hand's number in its session.
    no: u64,
    /// How many decisions the player made in it.
    mine: usize,
    /// The record as one line of JSON text, exactly as a hand history file
    /// holds it. Text, because the page must hand it back unchanged and a
    /// JavaScript number cannot hold the record's 64-bit seed.
    #[serde(with = "as_text")]
    rec: HandRecord,
    #[serde(default)]
    dec: Vec<Dec>,
}

/// Everything needed to resume a session, apart from its finished hands
/// (which the page keeps as [`Entry`]s): replaying them rebuilds the rest.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Save {
    v: u32,
    /// Names the session among the page's stored hands.
    sid: String,
    /// The session seed, as text: it does not fit a JavaScript number.
    seed: String,
    /// What was asked for: "mystery" or a style.
    opponent: String,
    /// The style actually played.
    bot: String,
    mode: Mode,
    stack_bb: u32,
    hands_done: u64,
    /// The player's actions in the hand in progress.
    current: Vec<Action>,
    skills: [(u32, u32); 5],
    streaks: [u32; 5],
    guess: Option<usize>,
    guess_asked: Option<u64>,
    run: u32,
    reviews_shown: u32,
}

/// Stores a value as its JSON text.
mod as_text {
    use serde::de::{DeserializeOwned, Error};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<T: Serialize, S: Serializer>(value: &T, s: S) -> Result<S::Ok, S::Error> {
        let text = serde_json::to_string(value).map_err(serde::ser::Error::custom)?;
        s.serialize_str(&text)
    }

    pub fn deserialize<'de, T: DeserializeOwned, D: Deserializer<'de>>(
        d: D,
    ) -> Result<T, D::Error> {
        let text = String::deserialize(d)?;
        serde_json::from_str(&text).map_err(D::Error::custom)
    }
}

enum Game {
    Play(Box<Session>),
    Drill {
        spots: Vec<Decision>,
        index: usize,
        correct: u32,
    },
}

struct Quiz {
    /// About the spot; `None` asks which kind of opponent this is.
    question: Option<Question>,
    /// Its item in the feed.
    id: u64,
}

const GRADES: [Grade; 6] = [
    Grade::Good,
    Grade::Close,
    Grade::Inaccuracy,
    Grade::Mistake,
    Grade::Blunder,
    Grade::Note,
];

const LEAKS: [Leak; 7] = [
    Leak::FoldedTooMuch,
    Leak::CalledTooLight,
    Leak::MissedRaise,
    Leak::MissedBet,
    Leak::WastedBet,
    Leak::BetTooSmall,
    Leak::BetTooBig,
];

const STREETS: [Street; 4] = [Street::Preflop, Street::Flop, Street::Turn, Street::River];

fn grade_index(grade: Grade) -> u8 {
    GRADES.iter().position(|&g| g == grade).unwrap_or(5) as u8
}

fn grade_key(grade: Grade) -> &'static str {
    match grade {
        Grade::Good => "good",
        Grade::Close => "close",
        Grade::Inaccuracy => "inaccuracy",
        Grade::Mistake => "mistake",
        Grade::Blunder => "blunder",
        Grade::Note => "note",
    }
}

fn leak_index(leak: Leak) -> u8 {
    LEAKS.iter().position(|&l| l == leak).unwrap_or(0) as u8
}

fn cards(cards: &[Card]) -> Vec<String> {
    cards.iter().map(|c| c.to_string()).collect()
}

fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// The row and column of a starting hand in the 13×13 grid: pairs on the
/// diagonal, suited hands above it, offsuit below, aces first.
fn grid_cell(hole: [Card; 2]) -> (usize, usize) {
    let (a, b) = (hole[0].rank() as usize, hole[1].rank() as usize);
    let (high, low) = (12 - a.max(b), 12 - a.min(b));
    if hole[0].suit() == hole[1].suit() {
        (high, low)
    } else {
        (low, high)
    }
}

/// How likely each starting hand is in `range`, against the likeliest,
/// leaving out combinations that use a `dead` card. Row by row.
fn range_grid(range: &Range, dead: &[Card]) -> Vec<f64> {
    let mut sum = [0.0f64; 169];
    let mut count = [0u32; 169];
    for (i, &weight) in range.weights().iter().enumerate() {
        let hole = combo(i);
        if dead.iter().any(|d| hole.contains(d)) {
            continue;
        }
        let (row, col) = grid_cell(hole);
        sum[row * 13 + col] += weight;
        count[row * 13 + col] += 1;
    }
    let mut top = 0.0f64;
    for i in 0..169 {
        if count[i] > 0 {
            sum[i] /= count[i] as f64;
            top = top.max(sum[i]);
        }
    }
    sum.iter()
        .map(|&x| {
            if top > 0.0 {
                (x / top * 100.0).round() / 100.0
            } else {
                0.0
            }
        })
        .collect()
}

/// A reviewed decision for the page: the verdict, every priced option and
/// the facts that decided it.
fn review_json(review: &Review, detail: &str, run: u32) -> Value {
    let (spot, fb) = (&review.spot, &review.feedback);
    // The coach measures in pots, not big blinds: values, costs and sizes
    // are all shares of the pot in front of the player.
    let pot = spot.pot.max(1) as f64;
    let ranked = spot.ranked(Some(review.action));
    let (best, top) = ranked.first().map_or((None, 0.0), |r| (Some(r.0), r.1));
    let rows: Vec<Value> = ranked
        .iter()
        .map(|&(action, ev)| {
            let mut row = json!({
                "name": capitalise(&spot.phrase(action)),
                "ev": ev / pot,
                // Each option against the best: what you learn from is the
                // gap, not the total, which can run to several pots.
                "gap_text": (Some(action) != best).then(|| fmt_pot(ev - top, spot.pot)),
                "chosen": action == review.action,
                "best": Some(action) == best,
            });
            if let Action::RaiseTo(amount) = action
                && let Some(r) = spot.response(amount)
            {
                row["folds"] = json!(r.fold);
                row["equity_called"] = json!(r.equity_called);
            }
            row
        })
        .collect();
    let priced = spot.ev(review.action).is_some();
    json!({
        "kind": "review",
        "detail": detail,
        "grade": grade_key(fb.grade),
        "label": fb.grade.label(),
        "loss": fb.loss / pot,
        "loss_text": format!("{} of the pot", fmt_share(fb.loss / pot)),
        "run": run,
        "street": spot.street.name(),
        "action": capitalise(&spot.phrase(review.action)),
        "headline": headline(spot),
        "equity": spot.equity,
        "against": if spot.read.is_some() { "its likely hands" } else { "a random hand" },
        "rows": rows,
        "points": why(spot, review.action),
        "note": if priced { Value::Null } else { json!(fb.text) },
    })
}

fn observed_json(seen: &Observed) -> Value {
    json!({
        "hands": seen.hands,
        "plays": seen.plays(),
        "raises": seen.raises(),
        "folds": seen.folds_to_bets(),
        "faced": seen.faced,
        "aggression": seen.aggression(),
        "read": seen.read(),
    })
}

/// The table: one session or drill, and the coach beside it.
#[derive(Default)]
pub struct Table {
    game: Option<Game>,
    save: Option<Save>,
    /// Finished hands of this session, oldest first.
    past: Vec<Entry>,
    mode: Mode,
    chat: Chat,
    feed: Vec<Value>,
    next_id: u64,
    /// The coach's analysis of the decision in front of the player.
    spot: Option<Spot>,
    quiz: Option<Quiz>,
    /// Skills already asked about in this hand.
    asked: Vec<Skill>,
    /// Questions asked and answered well, by skill.
    skills: [(u32, u32); 5],
    /// Answers right in a row, by skill.
    streaks: [u32; 5],
    last_skill: Option<Skill>,
    rng: Option<Rng>,
    /// Sound decisions in a row.
    run: u32,
    /// Decisions reviewed so far; the first few are shown in full.
    reviews_shown: u32,
    /// The opponent's likely hands when the player last acted.
    last_grid: Option<Vec<f64>>,
    guess: Option<usize>,
    guess_asked: Option<u64>,
    last_review: Option<Review>,
    /// The feed item that shows `last_review`.
    last_review_id: Option<u64>,
    /// The latest action at the table: who, and in words.
    last_action: Option<(usize, String)>,
    hand_done: bool,
    over: bool,
    report: Option<Value>,
    /// A hand finished by the last command, for the page to store.
    finished: Option<Entry>,
    /// Whether the save changed in the last command.
    dirty: bool,
}

impl Table {
    /// Runs a command given as JSON and returns the reply as JSON.
    pub fn command_json(&mut self, json: &str) -> String {
        let reply = match serde_json::from_str::<Value>(json) {
            Ok(command) => self.command(&command),
            Err(e) => json!({ "error": format!("bad command: {e}") }),
        };
        reply.to_string()
    }

    /// Runs one command. The reply holds the `view`; a `save` when the saved
    /// session changed (`null` once it has ended); an `entry` when a hand
    /// finished; and `error` if the command could not be run.
    pub fn command(&mut self, command: &Value) -> Value {
        self.finished = None;
        self.dirty = false;
        let mut extra = Map::new();
        let result = self.run(command, &mut extra);
        if result.is_ok() {
            self.advance();
        }
        let mut reply = Map::new();
        if let Err(error) = result {
            reply.insert("error".into(), json!(error));
        }
        reply.insert("view".into(), self.view());
        if let Some(entry) = self.finished.take() {
            reply.insert("entry".into(), json!(entry));
        }
        if self.dirty && matches!(self.game, Some(Game::Play(_))) {
            let save = if self.over {
                Value::Null
            } else {
                self.sync_save();
                json!(self.save)
            };
            reply.insert("save".into(), save);
        }
        reply.extend(extra);
        Value::Object(reply)
    }

    fn run(&mut self, command: &Value, extra: &mut Map<String, Value>) -> Result<(), String> {
        let text = |key: &str| command[key].as_str().unwrap_or_default().to_string();
        match command["cmd"].as_str().unwrap_or_default() {
            "info" => {
                extra.insert("info".into(), info());
                Ok(())
            }
            "view" => Ok(()),
            "new" => self.new_session(command),
            "restore" => self.restore(command),
            "act" => {
                let action = match command["action"].as_str().unwrap_or_default() {
                    "fold" => Action::Fold,
                    "check" => Action::Check,
                    "call" => Action::Call,
                    "raise" => Action::RaiseTo(
                        command["amount"]
                            .as_u64()
                            .and_then(|a| u32::try_from(a).ok())
                            .ok_or("a raise needs an amount")?,
                    ),
                    other => return Err(format!("unknown action '{other}'")),
                };
                self.hero_act(action)
            }
            "bot" => {
                self.bot_act();
                Ok(())
            }
            "next" => {
                self.next();
                Ok(())
            }
            "answer" => {
                let choice = command["choice"].as_u64().ok_or("which answer?")? as usize;
                self.answer_question(choice);
                Ok(())
            }
            "skip" => {
                self.skip_question();
                Ok(())
            }
            "mode" => {
                self.mode = serde_json::from_value(command["mode"].clone())
                    .map_err(|_| "unknown coach mode".to_string())?;
                if self.mode != Mode::Teach {
                    self.skip_question();
                }
                self.dirty = true;
                Ok(())
            }
            "end" => {
                if !self.over && self.game.is_some() {
                    self.end_session();
                }
                Ok(())
            }
            "drill" => self.start_drill(command),
            "chat" => {
                // A review asked for while an earlier reply was still coming
                // in may be about a decision the table has moved past.
                if let Some(review) = command["review"].as_u64()
                    && self.last_review_id != Some(review)
                {
                    return Ok(());
                }
                let request = self.chat_request(&text("kind"), &text("text"))?;
                extra.insert("chat".into(), request);
                Ok(())
            }
            "chat_done" => {
                let (id, reply) = (command["id"].as_u64().unwrap_or(0), text("text"));
                self.chat.reply(&reply);
                self.set_item(id, |item| {
                    item["text"] = json!(reply);
                    item["pending"] = json!(false);
                });
                Ok(())
            }
            "chat_fail" => {
                let (id, error) = (command["id"].as_u64().unwrap_or(0), text("error"));
                self.chat.abandon();
                self.set_item(id, |item| {
                    let fallback = item["fallback"].as_str().map(str::to_string);
                    item["pending"] = json!(false);
                    match fallback {
                        // The engine's own reasons stand in for the model's.
                        Some(fallback) => item["text"] = json!(fallback),
                        None => {
                            item["kind"] = json!("system");
                            item["text"] = json!(format!("The coach could not answer: {error}"));
                        }
                    }
                });
                Ok(())
            }
            other => Err(format!("unknown command '{other}'")),
        }
    }

    // ---- sessions --------------------------------------------------------

    fn reset(&mut self) {
        *self = Table::default();
    }

    fn new_session(&mut self, command: &Value) -> Result<(), String> {
        let seed: u64 = match &command["seed"] {
            Value::String(s) => s.trim().parse().map_err(|_| "the seed must be a number")?,
            Value::Number(n) => n.as_u64().ok_or("the seed must be a whole number")?,
            _ => return Err("a session needs a seed".into()),
        };
        let opponent = command["opponent"]
            .as_str()
            .unwrap_or("mystery")
            .to_string();
        let bot = match opponent.as_str() {
            "mystery" => {
                let pick = Rng::new(seed ^ 0x5717E).below(STYLES.len() as u64);
                STYLES[pick as usize].name.to_string()
            }
            name if STYLES.iter().any(|s| s.name == name) => name.to_string(),
            other => return Err(format!("unknown opponent '{other}'")),
        };
        let mode = match &command["mode"] {
            Value::Null => Mode::default(),
            mode => serde_json::from_value(mode.clone()).map_err(|_| "unknown coach mode")?,
        };
        let save = Save {
            v: 1,
            sid: match command["sid"].as_str() {
                Some(sid) if !sid.is_empty() => sid.to_string(),
                _ => seed.to_string(),
            },
            seed: seed.to_string(),
            opponent,
            bot,
            mode,
            stack_bb: match &command["stack"] {
                Value::Null => 100,
                stack => stack
                    .as_u64()
                    .and_then(|s| u32::try_from(s).ok())
                    .ok_or("the stack is out of range")?,
            },
            hands_done: 0,
            current: Vec::new(),
            skills: [(0, 0); 5],
            streaks: [0; 5],
            guess: None,
            guess_asked: None,
            run: 0,
            reviews_shown: 0,
        };
        self.begin(save, Vec::new())
    }

    fn restore(&mut self, command: &Value) -> Result<(), String> {
        let save: Save = serde_json::from_value(command["save"].clone())
            .map_err(|e| format!("the saved session cannot be read: {e}"))?;
        let hands: Vec<Entry> = match &command["hands"] {
            Value::Null => Vec::new(),
            hands => serde_json::from_value(hands.clone())
                .map_err(|e| format!("the saved hands cannot be read: {e}"))?,
        };
        self.begin(save, hands)
    }

    /// Starts the session `save` describes, replaying its finished `hands`
    /// and the actions of the hand in progress. Leaves the table as it was
    /// if the save does not replay.
    fn begin(&mut self, save: Save, hands: Vec<Entry>) -> Result<(), String> {
        let before = std::mem::take(self);
        let started = self.begin_afresh(save, hands);
        if started.is_err() {
            *self = before;
        }
        started
    }

    fn begin_afresh(&mut self, save: Save, hands: Vec<Entry>) -> Result<(), String> {
        if save.v != 1 {
            return Err("the saved session is from another version".into());
        }
        if hands.len() as u64 != save.hands_done {
            return Err("the saved session is missing hands".into());
        }
        let seed: u64 = save.seed.parse().map_err(|_| "bad seed")?;
        let default = Config::default();
        if !(1..=Config::MAX_STACK / default.big_blind).contains(&save.stack_bb) {
            return Err("the stack is out of range".into());
        }
        let cfg = Config {
            stack: save.stack_bb * default.big_blind,
            ..default
        };
        let bot: Box<dyn Bot> = Box::new(StyleBot::new(
            STYLES
                .into_iter()
                .find(|s| s.name == save.bot)
                .ok_or("unknown opponent")?,
            seed ^ 0xB07,
        ));
        let mut session = Session::new(cfg, bot, seed);
        if save.opponent == "mystery" {
            session.hide_opponent();
        }
        for entry in &hands {
            let mut mine = entry
                .rec
                .actions
                .iter()
                .filter(|(seat, _)| *seat == HERO)
                .map(|&(_, action)| action);
            while let Some(seat) = session.state().to_act() {
                if seat == HERO {
                    let action = mine.next().ok_or("a saved hand stops short")?;
                    session
                        .hero_act(action, false, None)
                        .map_err(|e| format!("a saved hand does not replay: {e}"))?;
                } else {
                    session.bot_act();
                }
            }
            let record = session.finish_hand().ok_or("a saved hand does not end")?;
            if record.net != entry.rec.net {
                return Err("a saved hand replays differently".into());
            }
            for d in &entry.dec {
                session.credit(
                    d.street,
                    d.leak.and_then(|i| LEAKS.get(i as usize).copied()),
                    GRADES.get(d.grade as usize).copied().unwrap_or(Grade::Note),
                    d.loss,
                );
            }
            session.next_hand();
        }

        self.reset();
        let resumed = !hands.is_empty() || !save.current.is_empty();
        self.mode = save.mode;
        self.skills = save.skills;
        self.streaks = save.streaks;
        self.guess = save.guess;
        self.guess_asked = save.guess_asked;
        self.run = save.run;
        self.reviews_shown = save.reviews_shown;
        self.rng = Some(Rng::new(seed ^ 0xC0AC4 ^ (hands.len() as u64) << 20));
        let hidden = session.opponent_hidden();
        let current = save.current.clone();
        self.save = Some(Save {
            current: Vec::new(),
            ..save
        });
        self.past = hands;
        self.game = Some(Game::Play(Box::new(session)));
        self.dirty = true;

        if resumed {
            self.say("system", "Welcome back. Your session is as you left it.");
        } else {
            self.say(
                "system",
                match self.mode {
                    Mode::Off => "The coach is off. Change that any time in the menu.",
                    Mode::Teach => {
                        "I watch every decision. Before some I ask a question; after each I show \
                         what your play was worth next to the alternatives."
                    }
                    _ => {
                        "I watch every decision and show what your play was worth next to the \
                         alternatives."
                    }
                },
            );
            if hidden && self.mode != Mode::Off {
                self.say(
                    "system",
                    "Your opponent is a mystery: a solid regular, a calling station, a nit or a \
                     maniac. Watch what it does, work out which, and adjust. I name it when the \
                     session ends.",
                );
            }
        }
        // The hand in progress: the player's actions again, the bot's as
        // they fall. The reviews come back with them.
        let reviews_shown = self.reviews_shown;
        let run = self.run;
        for action in current {
            while self.bot_to_act() {
                self.bot_act();
            }
            if !self.hero_to_act() {
                break;
            }
            self.spot = Some(self.analyse());
            self.hero_act(action)?;
        }
        self.reviews_shown = reviews_shown;
        self.run = run;
        Ok(())
    }

    /// Brings the saved head up to date with the table.
    fn sync_save(&mut self) {
        if let Some(save) = &mut self.save {
            save.mode = self.mode;
            save.hands_done = self.past.len() as u64;
            save.skills = self.skills;
            save.streaks = self.streaks;
            save.guess = self.guess;
            save.guess_asked = self.guess_asked;
            save.run = self.run;
            save.reviews_shown = self.reviews_shown;
        }
    }

    fn start_drill(&mut self, command: &Value) -> Result<(), String> {
        let picks = command["spots"].as_array().ok_or("no spots to drill")?;
        let mut spots = Vec::new();
        for pick in picks {
            let record: HandRecord = serde_json::from_str(pick["rec"].as_str().unwrap_or_default())
                .map_err(|e| format!("a stored hand cannot be read: {e}"))?;
            let index = pick["i"].as_u64().unwrap_or(0) as usize;
            // A hand that no longer replays is left out, not fatal.
            if let Ok(mut found) = decisions(&record)
                && index < found.len()
            {
                spots.push(found.swap_remove(index));
            }
        }
        if spots.is_empty() {
            return Err("there is nothing to drill yet".into());
        }
        self.reset();
        self.mode = Mode::After;
        self.rng = Some(Rng::new(0));
        self.say(
            "system",
            "Your costliest past decisions, dearest first. Play each one again; I show what \
             every option was worth.",
        );
        self.game = Some(Game::Drill {
            spots,
            index: 0,
            correct: 0,
        });
        Ok(())
    }

    // ---- game flow -------------------------------------------------------

    fn state(&self) -> Option<&HandState> {
        match self.game.as_ref()? {
            Game::Play(session) => Some(session.state()),
            Game::Drill { spots, index, .. } => Some(&spots[*index].state),
        }
    }

    fn session(&self) -> Option<&Session> {
        match self.game.as_ref()? {
            Game::Play(session) => Some(session),
            Game::Drill { .. } => None,
        }
    }

    fn hero_to_act(&self) -> bool {
        match &self.game {
            Some(Game::Play(session)) => session.hero_to_act() && !self.over,
            Some(Game::Drill { .. }) => self.last_review.is_none() && !self.over,
            None => false,
        }
    }

    fn bot_to_act(&self) -> bool {
        !self.over && self.session().is_some_and(|s| s.bot_to_act())
    }

    /// Whatever follows without the player: scoring a finished hand, or the
    /// coach looking at the next decision. The bot waits for the page's cue.
    fn advance(&mut self) {
        if self.over || self.bot_to_act() {
            return;
        }
        if let Some(Game::Play(session)) = &self.game
            && session.state().outcome().is_some()
            && !self.hand_done
        {
            self.finish_hand();
            return;
        }
        if self.hero_to_act() && self.spot.is_none() {
            self.begin_decision();
        }
    }

    fn start_hand(&mut self) {
        self.chat.reset();
        self.spot = None;
        self.last_review = None;
        self.last_review_id = None;
        self.last_action = None;
        self.hand_done = false;
        self.quiz = None;
        self.asked.clear();
        self.last_grid = None;
    }

    fn bot_act(&mut self) {
        if !self.bot_to_act() {
            return;
        }
        let Some(Game::Play(session)) = &mut self.game else {
            return;
        };
        let before = session.state().clone();
        let legal = before.legal().expect("the bot acts in a live hand");
        let action = session.bot_act();
        self.last_action = Some((BOT, describe("Bot", action, legal, &before)));
    }

    fn analyse(&self) -> Spot {
        match self.game.as_ref().expect("a game is on") {
            Game::Play(session) => session.analyse(),
            Game::Drill { spots, index, .. } => spots[*index].spot.clone(),
        }
    }

    fn begin_decision(&mut self) {
        let spot = self.analyse();
        if self.guess_due() {
            self.ask_for_guess();
        } else if self.mode == Mode::Teach {
            self.ask_question(&spot);
        }
        self.spot = Some(spot);
    }

    /// Whether to ask which kind of opponent this is: once there is enough
    /// to go on, and again every so often in case the player's view changed.
    fn guess_due(&self) -> bool {
        let Some(session) = self.session() else {
            return false;
        };
        let finished = session.finished();
        session.opponent_hidden()
            && self.mode != Mode::Off
            && self.quiz.is_none()
            && finished >= 12
            && self.guess_asked.is_none_or(|at| finished >= at + 15)
    }

    fn ask_for_guess(&mut self) {
        let Some(session) = self.session() else {
            return;
        };
        self.guess_asked = Some(session.finished());
        self.dirty = true;
        let text = match self.guess {
            None => "You have seen enough to judge. What kind of player is this?".to_string(),
            Some(guess) => format!(
                "You took this for a {}. Still think so?",
                STYLES[guess].name
            ),
        };
        let choices = STYLES.iter().map(|s| s.name.to_string()).collect();
        self.open_quiz(None, "Read", text, choices);
    }

    fn open_quiz(
        &mut self,
        question: Option<Question>,
        skill: &str,
        text: String,
        choices: Vec<String>,
    ) {
        let id = self.push(json!({
            "kind": "quiz",
            "skill": skill,
            "text": text,
            "choices": choices,
            "state": "open",
        }));
        self.quiz = Some(Quiz { question, id });
    }

    /// Asks one question about the spot, or none. Skills answered wrong or
    /// never tried come up most; a skill answered right several times running
    /// is asked about less and less, and never twice in a hand or twice in
    /// succession if there is another to ask.
    fn ask_question(&mut self, spot: &Spot) {
        let answered: u32 = self.skills.iter().map(|s| s.0).sum();
        // Once the player knows the routine, two questions a hand at most.
        if answered >= 6 && self.asked.len() >= 2 {
            return;
        }
        let candidates: Vec<Question> = drills(spot)
            .into_iter()
            .filter(|q| !self.asked.contains(&q.skill()))
            .collect();
        let weights: Vec<u64> = candidates
            .iter()
            .map(|q| {
                let skill = q.skill();
                if Some(skill) == self.last_skill {
                    1
                } else if self.skills[skill as usize].0 == 0 {
                    10
                } else {
                    match self.streaks[skill as usize] {
                        0 => 8,
                        1 => 5,
                        2 => 3,
                        _ => 2,
                    }
                }
            })
            .collect();
        let total: u64 = weights.iter().sum();
        if total == 0 {
            return;
        }
        let rng = self.rng.get_or_insert_with(|| Rng::new(0));
        let mut roll = rng.below(total);
        let mut pick = 0;
        for (i, &weight) in weights.iter().enumerate() {
            if roll < weight {
                pick = i;
                break;
            }
            roll -= weight;
        }
        let question = candidates[pick].clone();
        let skill = question.skill();
        // The better the player knows it, the more decisions pass in peace;
        // big pots are worth a question more often.
        let mut chance = match self.streaks[skill as usize] {
            0 | 1 => 100,
            2 => 70,
            3 => 45,
            4 => 30,
            _ => 20,
        };
        if spot.pot >= 12 * spot.big_blind {
            chance += 20;
        }
        if answered >= 3 && rng.below(100) >= chance {
            return;
        }
        self.asked.push(skill);
        self.last_skill = Some(skill);
        let (choices, _) = question.choices(spot);
        let text = question.ask(spot);
        self.open_quiz(Some(question), skill.name(), text, choices);
    }

    fn answer_question(&mut self, choice: usize) {
        let Some(quiz) = self.quiz.take() else {
            return;
        };
        self.dirty = true;
        let Some(question) = quiz.question else {
            if choice >= STYLES.len() {
                self.quiz = Some(Quiz {
                    question: None,
                    id: quiz.id,
                });
                return;
            }
            self.guess = Some(choice);
            self.set_item(quiz.id, |item| {
                item["state"] = json!("answered");
                item["choice"] = json!(choice);
                item["grade"] = json!("note");
                item["note"] = json!(format!(
                    "Noted: {}. Play against that; I say whether you were right when the \
                     session ends.",
                    STYLES[choice].name
                ));
            });
            return;
        };
        let Some(spot) = self.spot.clone() else {
            return;
        };
        let (choices, right) = question.choices(&spot);
        if choice >= choices.len() {
            self.quiz = Some(Quiz {
                question: Some(question),
                id: quiz.id,
            });
            return;
        }
        let feedback = question.answer(&spot, choice);
        let skill = question.skill() as usize;
        self.skills[skill].0 += 1;
        if feedback.grade == Grade::Inaccuracy {
            self.streaks[skill] = 0;
        } else {
            self.skills[skill].1 += 1;
            self.streaks[skill] += 1;
        }
        self.set_item(quiz.id, |item| {
            item["state"] = json!("answered");
            item["choice"] = json!(choice);
            item["right"] = json!(right);
            item["grade"] = json!(grade_key(feedback.grade));
            item["note"] = json!(feedback.text);
        });
    }

    /// Drops the open question unanswered.
    fn skip_question(&mut self) {
        if let Some(quiz) = self.quiz.take() {
            self.feed
                .retain(|item| item["id"].as_u64() != Some(quiz.id));
        }
    }

    fn hero_act(&mut self, action: Action) -> Result<(), String> {
        if !self.hero_to_act() {
            return Err("it is not your turn".into());
        }
        let before = self.state().expect("a game is on").clone();
        let legal = before.legal().ok_or("the hand is over")?;
        if !legal.allows(action) {
            return Err("that is not a legal action here".into());
        }
        self.skip_question();
        let coached = self.mode != Mode::Off;
        self.last_grid = self.grid_now();
        let spot = self.spot.take();
        let review = match self.game.as_mut().expect("a game is on") {
            Game::Play(session) => session
                .hero_act(action, coached, spot)
                .map_err(|e| e.to_string())?,
            Game::Drill {
                spots,
                index,
                correct,
            } => {
                let d = &spots[*index];
                let bot = by_name(&d.opponent, 0).ok_or("unknown opponent")?;
                let strengths = Strengths::new(d.state.board());
                let spot = analyse_with(&d.state, bot.as_ref(), &d.range, &strengths, action);
                let feedback = review(&spot, action);
                if matches!(feedback.grade, Grade::Good | Grade::Close) {
                    *correct += 1;
                }
                Some(Review {
                    spot,
                    action,
                    feedback,
                })
            }
        };
        self.last_action = Some((HERO, describe("You", action, legal, &before)));
        if let (Some(Game::Play(_)), Some(save)) = (&self.game, &mut self.save) {
            save.current.push(action);
            self.dirty = true;
        }
        if let Some(review) = &review {
            self.show_review(review);
        }
        self.last_review = review;
        Ok(())
    }

    fn show_review(&mut self, review: &Review) {
        let sound = matches!(review.feedback.grade, Grade::Good | Grade::Close);
        self.run = if sound { self.run + 1 } else { 0 };
        self.reviews_shown += 1;
        // A sound play gets a line, not a lecture, once the player has seen
        // what a full review looks like. Drills are always shown in full.
        let play = matches!(self.game, Some(Game::Play(_)));
        let detail = if sound && self.reviews_shown > 2 && play {
            "brief"
        } else {
            "full"
        };
        self.last_review_id = Some(self.push(review_json(review, detail, self.run)));
        if let Some(Game::Drill { spots, index, .. }) = &self.game {
            let d = &spots[*index];
            let text = format!(
                "In the game you chose {} ({}).",
                d.spot.phrase(d.action),
                d.feedback.grade.label().to_lowercase()
            );
            self.say("system", &text);
        }
    }

    fn finish_hand(&mut self) {
        self.hand_done = true;
        let Some(Game::Play(session)) = &mut self.game else {
            return;
        };
        let Some(record) = session.finish_hand() else {
            return;
        };
        let big_blind = record.config.big_blind;
        let net = record.net[HERO];
        let summary = match net {
            0 => "split pot".to_string(),
            n if n > 0 => format!("you won {}", fmt_bb(n as f64, big_blind)),
            n => format!("you lost {}", fmt_bb(-n as f64, big_blind)),
        };
        let dec: Vec<Dec> = session
            .reviews()
            .iter()
            .map(|r| Dec {
                street: r.spot.street,
                grade: grade_index(r.feedback.grade),
                loss: r.feedback.loss,
                leak: Leak::of(&r.spot, r.action, &r.feedback).map(leak_index),
            })
            .collect();
        let grades: Vec<&str> = session
            .reviews()
            .iter()
            .map(|r| grade_key(r.feedback.grade))
            .collect();
        let lost: f64 = dec.iter().map(|d| d.loss).sum();
        let item = json!({
            "kind": "hand",
            "text": format!("Hand {} · {summary}", record.hand_no),
            "net": net as f64 / big_blind as f64,
            "grades": grades,
            "lost_text": if lost > 0.0 { Some(fmt_bb(lost, big_blind)) } else { None },
        });
        let entry = Entry {
            sid: self
                .save
                .as_ref()
                .map(|s| s.sid.clone())
                .unwrap_or_default(),
            no: record.hand_no,
            mine: record
                .actions
                .iter()
                .filter(|(seat, _)| *seat == HERO)
                .count(),
            rec: record,
            dec,
        };
        self.push(item);
        self.past.push(entry.clone());
        self.finished = Some(entry);
        if let Some(save) = &mut self.save {
            save.current.clear();
        }
        self.dirty = true;
    }

    fn next(&mut self) {
        if self.over {
            return;
        }
        match self.game.as_mut() {
            Some(Game::Play(session)) => {
                if !self.hand_done {
                    return;
                }
                session.next_hand();
            }
            Some(Game::Drill { spots, index, .. }) => {
                if self.last_review.is_none() {
                    return;
                }
                if *index + 1 >= spots.len() {
                    self.end_session();
                    return;
                }
                *index += 1;
                let text = format!("Spot {} of {}", *index + 1, spots.len());
                self.push(json!({ "kind": "divider", "text": text }));
            }
            None => return,
        }
        self.start_hand();
    }

    fn end_session(&mut self) {
        self.over = true;
        self.dirty = true;
        self.skip_question();
        let skills = self.skills_json();
        let report = match self.game.as_ref() {
            Some(Game::Play(session)) => {
                let big_blind = session.config().big_blind;
                let bb = big_blind as f64;
                let hands = session.finished();
                let mut report = json!({
                    "kind": "play",
                    "hands": hands,
                    "result": session.total() as f64 / bb,
                    "result_text": fmt_bb_signed(session.total() as f64, big_blind),
                    "coached": self.mode != Mode::Off && session.reviewed() > 0,
                    "given_up_text": fmt_bb(session.given_up(), big_blind),
                    "per_100": (hands > 0).then(|| session.given_up() / hands as f64 * 100.0 / bb),
                    "grades": self.grades_json(session),
                    "skills": skills,
                    "leaks": leaks_json(session, 3),
                    "seed": session.seed().to_string(),
                });
                if session.opponent_hidden() {
                    let style = session.bot().style();
                    report["opponent"] = json!({
                        "name": session.bot_name(),
                        "blurb": style.map(|s| s.blurb),
                        "counter": style.map(|s| s.counter),
                        "seen": observed_json(&session.observed()),
                        "guess": self.guess.map(|g| STYLES[g].name),
                        "right": self.guess.map(|g| STYLES[g].name == session.bot_name()),
                    });
                }
                report
            }
            Some(Game::Drill { spots, correct, .. }) => json!({
                "kind": "drill",
                "correct": correct,
                "spots": spots.len(),
            }),
            None => Value::Null,
        };
        self.report = Some(report);
    }

    // ---- the coach's feed -------------------------------------------------

    fn push(&mut self, mut item: Value) -> u64 {
        self.next_id += 1;
        item["id"] = json!(self.next_id);
        self.feed.push(item);
        self.next_id
    }

    fn say(&mut self, kind: &str, text: &str) -> u64 {
        self.push(json!({ "kind": kind, "text": text }))
    }

    fn set_item(&mut self, id: u64, change: impl FnOnce(&mut Value)) {
        if let Some(item) = self
            .feed
            .iter_mut()
            .rev()
            .find(|i| i["id"].as_u64() == Some(id))
        {
            change(item);
        }
    }

    /// The opponent's likely hands right now, for the grid.
    fn grid_now(&self) -> Option<Vec<f64>> {
        let state = self.state()?;
        let mut dead = state.hole(HERO).to_vec();
        dead.extend_from_slice(state.board());
        let range = match self.game.as_ref()? {
            Game::Play(session) => session.range(),
            Game::Drill { spots, index, .. } => &spots[*index].range,
        };
        Some(range_grid(range, &dead))
    }

    /// Builds the request for the language-model coach and adds the item its
    /// reply will fill. The page makes the call and reports back.
    fn chat_request(&mut self, kind: &str, text: &str) -> Result<Value, String> {
        let session = self.session();
        let spot_facts = |spot: &Spot, action: Option<Action>| facts(spot, action, session);
        let (ask, facts, fallback) = match kind {
            "review" => {
                let review = self.last_review.as_ref().ok_or("nothing to review")?;
                (
                    Ask::Review,
                    Some(spot_facts(&review.spot, Some(review.action))),
                    Some(why(&review.spot, review.action).join(" ")),
                )
            }
            "free" => {
                // In teach mode the spot's numbers stay hidden until the player acts.
                let current = self.spot.as_ref().filter(|_| self.mode != Mode::Teach);
                let facts = match (current, self.last_review.as_ref()) {
                    (Some(spot), _) => Some(spot_facts(spot, None)),
                    (None, Some(r)) => Some(spot_facts(&r.spot, Some(r.action))),
                    (None, None) => None,
                };
                (Ask::Free(text.to_string()), facts, None)
            }
            "summary" => (
                Ask::Summary,
                Some(session_facts(session.ok_or("no session to sum up")?)),
                None,
            ),
            other => return Err(format!("unknown request '{other}'")),
        };
        // The summary is about the session, not the hand on the table.
        let history = match kind {
            "summary" => Vec::new(),
            _ => self.state().map(|s| narrate(s, HERO)).unwrap_or_default(),
        };
        let messages: Vec<Value> = self
            .chat
            .request(&ask, facts.as_ref(), &history)
            .iter()
            .map(|m| {
                json!({
                    "role": match m.role { Role::User => "user", Role::Assistant => "assistant" },
                    "content": m.text,
                })
            })
            .collect();
        if kind == "free" {
            self.say("you", text);
        }
        let id = self.push(json!({
            "kind": "coach",
            "text": "",
            "pending": true,
            "fallback": fallback,
        }));
        Ok(json!({ "id": id, "system": SYSTEM, "messages": messages }))
    }

    // ---- the view ---------------------------------------------------------

    fn skills_json(&self) -> Vec<Value> {
        Skill::ALL
            .iter()
            .filter(|&&skill| self.skills[skill as usize].0 > 0)
            .map(|&skill| {
                let (asked, right) = self.skills[skill as usize];
                json!({ "name": skill.name(), "asked": asked, "right": right })
            })
            .collect()
    }

    fn grades_json(&self, session: &Session) -> Value {
        let count = |g: Grade| session.grades().get(&g).copied().unwrap_or(0);
        json!({
            "good": count(Grade::Good),
            "close": count(Grade::Close),
            "inaccuracy": count(Grade::Inaccuracy),
            "mistake": count(Grade::Mistake),
            "blunder": count(Grade::Blunder),
        })
    }

    /// Everything the page draws.
    pub fn view(&self) -> Value {
        let (Some(game), Some(state)) = (&self.game, self.state()) else {
            return Value::Null;
        };
        let cfg = state.config();
        let bb = cfg.big_blind as f64;
        let outcome = state.outcome();
        let showdown = outcome.is_some_and(|o| o.showdown);
        // After an all-in the bot's cards are shown even if someone folded;
        // its hand rank is not, since that would give away undealt cards.
        let revealed = state.hands_revealed();
        let hole = state.hole(HERO);
        let hero_turn = self.hero_to_act();
        let phase = if self.over {
            "over"
        } else if self.bot_to_act() {
            "bot"
        } else if hero_turn {
            "hero"
        } else {
            "next"
        };

        let table = json!({
            "street": state.street().name(),
            "board": cards(state.board()),
            "pot": state.pot(),
            "hero": {
                "cards": cards(&hole),
                "stack": state.stack(HERO),
                "bet": state.street_commit(HERO),
                "button": state.button() == HERO,
                "hand": describe_hand(hole, state.board()),
                "rank": showdown.then(|| state.showdown_rank(HERO).describe()),
            },
            "bot": {
                "cards": revealed.then(|| cards(&state.hole(BOT))),
                "stack": state.stack(BOT),
                "bet": state.street_commit(BOT),
                "button": state.button() == BOT,
                "rank": showdown.then(|| state.showdown_rank(BOT).describe()),
            },
            "last": self.last_action.as_ref().map(|(seat, text)| json!({
                "who": if *seat == HERO { "hero" } else { "bot" },
                "text": text,
            })),
            "log": narrate(state, HERO),
        });

        let legal = state.legal().filter(|_| hero_turn).map(|legal| {
            let opp_commit = state.street_commit(BOT);
            let mut value = json!({
                "to_call": legal.to_call,
                "call_all_in": legal.to_call >= state.stack(HERO),
                "pot": state.pot(),
                "pot_odds": (legal.to_call > 0).then(|| pot_odds(state.pot(), legal.to_call)),
                "stacks_to_pot": state.stack(HERO).min(state.stack(BOT)) as f64
                    / state.pot().max(1) as f64,
                "opp_commit": opp_commit,
                "raise": Value::Null,
            });
            if let Some((min, max)) = legal.raise {
                let sizes =
                    standard_sizes(state.pot(), legal.to_call, opp_commit, cfg.big_blind, (min, max));
                let labels = ["⅓", "½", "¾", "Pot", "All-in"];
                let mut presets: Vec<Value> = Vec::new();
                for (i, &amount) in sizes.iter().enumerate() {
                    let all_in = amount == max;
                    // Sizes that the stack or the minimum squeezes together
                    // are offered once; the biggest is always the all-in.
                    if presets.iter().any(|p| p["amount"] == json!(amount)) {
                        continue;
                    }
                    presets.push(json!({
                        "amount": amount,
                        "label": if all_in { "All-in" } else if amount == min { "Min" } else { labels[i] },
                    }));
                }
                value["raise"] = json!({
                    "min": min,
                    "max": max,
                    "step": (cfg.big_blind / 2).max(1),
                    "verb": if opp_commit == 0 { "Bet" } else { "Raise to" },
                    "presets": presets,
                });
            }
            value
        });

        // What the coach worked out; the page decides when to show it.
        let spot = self.spot.as_ref().filter(|_| self.mode != Mode::Off).map(|spot| {
            let ranked = spot.ranked(None);
            let best = ranked.first().map(|&(action, ev)| {
                json!({
                    "text": capitalise(&spot.phrase(action)),
                    // How clear-cut it is: the lead over the next option.
                    "margin_text": ranked.get(1).map(|&(_, next)| fmt_pot(ev - next, spot.pot)),
                })
            });
            json!({
                "equity": spot.equity,
                "against": if spot.read.is_some() { "its likely hands" } else { "a random hand" },
                "outs": spot.outs,
                "best": best,
                "read": spot.read.map(|r| json!({
                    "strong": r.strong, "medium": r.medium, "draws": r.draws, "weak": r.weak,
                })),
            })
        });
        let last_read = self.last_review.as_ref().map(|r| {
            r.spot.read.map(|r| {
                json!({ "strong": r.strong, "medium": r.medium, "draws": r.draws, "weak": r.weak })
            })
        });

        let (row, col) = grid_cell(hole);
        let grid = json!({
            "now": self.grid_now(),
            "last": self.last_grid,
            "hero": row * 13 + col,
        });

        let quiz = self.quiz.as_ref().map(|q| q.id);
        let skip = self.feed.len().saturating_sub(FEED_SHOWN);

        let mut view = json!({
            "kind": match game { Game::Play(_) => "play", Game::Drill { .. } => "drill" },
            "phase": phase,
            "mode": self.mode,
            "bb": cfg.big_blind,
            "stack": cfg.stack,
            "table": table,
            "legal": legal,
            "spot": spot,
            "last_read": last_read,
            "grid": grid,
            "quiz": quiz,
            "feed": &self.feed[skip..],
            "outcome": outcome.map(|o| json!({
                "winner": match o.winner { None => "split", Some(HERO) => "hero", Some(_) => "bot" },
                "showdown": o.showdown,
                "net": o.net[HERO],
            })),
            "report": self.report,
        });

        match game {
            Game::Play(session) => {
                let hidden = session.opponent_hidden() && !self.over;
                let style = session.bot().style().filter(|_| !hidden);
                view["hand_no"] = json!(session.hand_no());
                view["seed"] = json!(session.seed().to_string());
                view["opponent"] = json!({
                    "label": if hidden { "mystery" } else { session.bot_name() },
                    "hidden": hidden,
                    "blurb": style.map(|s| s.blurb),
                    "counter": style.map(|s| s.counter),
                    "seen": observed_json(&session.observed()),
                    "guess": self.guess.map(|g| STYLES[g].name),
                });
                let mut total = 0i64;
                let totals: Vec<f64> = self
                    .past
                    .iter()
                    .map(|e| {
                        total += e.rec.net[HERO];
                        total as f64 / bb
                    })
                    .collect();
                let lines = session.tally().lines();
                let by_street: Vec<Value> = STREETS
                    .iter()
                    .map(|&street| {
                        let lost: f64 = lines
                            .iter()
                            .filter(|l| l.street == street)
                            .map(|l| l.loss)
                            .sum();
                        json!({ "street": street.name(), "lost": lost / bb })
                    })
                    .collect();
                let from = self.past.len().saturating_sub(PAST_SHOWN);
                let past: Vec<Value> = self.past[from..]
                    .iter()
                    .rev()
                    .map(|e| {
                        let rec = &e.rec;
                        json!({
                            "no": rec.hand_no,
                            "cards": cards(&rec.hole[HERO]),
                            "button": rec.button == HERO,
                            "net": rec.net[HERO] as f64 / bb,
                            "grades": e.dec.iter()
                                .map(|d| grade_key(GRADES[(d.grade as usize).min(5)]))
                                .collect::<Vec<_>>(),
                            "shown": bot_shown(rec).then(|| cards(&rec.hole[BOT])),
                        })
                    })
                    .collect();
                let hands = session.finished();
                view["session"] = json!({
                    "hands": hands,
                    "result": session.total() as f64 / bb,
                    "totals": totals,
                    "reviewed": session.reviewed(),
                    "given_up": session.given_up() / bb,
                    "per_100": (hands > 0).then(|| session.given_up() / hands as f64 * 100.0 / bb),
                    "grades": self.grades_json(session),
                    "run": self.run,
                    "by_street": by_street,
                    "leaks": leaks_json(session, 3),
                    "skills": self.skills_json(),
                    "past": past,
                });
            }
            Game::Drill {
                spots,
                index,
                correct,
            } => {
                let d = &spots[*index];
                view["hand_no"] = json!(d.hand_no);
                view["opponent"] = json!({
                    "label": d.opponent,
                    "hidden": false,
                    "blurb": d.spot.style.map(|s| s.blurb),
                    "counter": d.spot.style.map(|s| s.counter),
                    "seen": Value::Null,
                    "guess": Value::Null,
                });
                view["drill"] = json!({
                    "index": index + 1,
                    "spots": spots.len(),
                    "correct": correct,
                    "answered": self.last_review.is_some(),
                });
            }
        }
        view
    }
}

/// Whether the bot's cards were face up at the end of the hand in `record`:
/// at a showdown, or after an all-in even if it was folded to.
fn bot_shown(record: &HandRecord) -> bool {
    match record.replay() {
        Ok(state) => state.hands_revealed(),
        Err(_) => !record
            .actions
            .iter()
            .any(|&(_, action)| action == Action::Fold),
    }
}

fn leaks_json(session: &Session, limit: usize) -> Vec<Value> {
    let big_blind = session.config().big_blind;
    session
        .tally()
        .lines()
        .iter()
        .take(limit)
        .map(|l| {
            json!({
                "label": capitalise(l.leak.label()),
                "street": l.street.name().to_lowercase(),
                "count": l.count,
                "loss": l.loss / big_blind as f64,
                "tip": l.leak.tip(),
            })
        })
        .collect()
}

/// What never changes: the kinds of opponent and of mistake, for the page's
/// menus and for summing up stored hands.
fn info() -> Value {
    json!({
        "styles": STYLES.iter().map(|s| json!({
            "name": s.name,
            "blurb": s.blurb,
            "counter": s.counter,
        })).collect::<Vec<_>>(),
        "leaks": LEAKS.iter().map(|l| json!({
            "label": capitalise(l.label()),
            "tip": l.tip(),
        })).collect::<Vec<_>>(),
        "grades": GRADES.iter().map(|&g| grade_key(g)).collect::<Vec<_>>(),
        "big_blind": Config::default().big_blind,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn send(table: &mut Table, command: Value) -> Value {
        let reply = table.command(&command);
        assert!(reply["error"].is_null(), "{}", reply["error"]);
        reply
    }

    /// Plays on, always checking or calling, until `hands` hands are done.
    /// Returns the last reply, every save and every finished hand.
    fn play(table: &mut Table, hands: usize) -> (Value, Value, Vec<Value>) {
        let mut reply = send(table, json!({ "cmd": "view" }));
        let (mut save, mut entries) = (Value::Null, Vec::new());
        while entries.len() < hands {
            let view = &reply["view"];
            let command = match view["phase"].as_str().unwrap() {
                "bot" => json!({ "cmd": "bot" }),
                "next" => json!({ "cmd": "next" }),
                "hero" => {
                    if view["quiz"].is_u64() {
                        reply = send(table, json!({ "cmd": "answer", "choice": 0 }));
                        continue;
                    }
                    let call = view["legal"]["to_call"].as_u64().unwrap() > 0;
                    json!({ "cmd": "act", "action": if call { "call" } else { "check" } })
                }
                phase => panic!("unexpected phase {phase}"),
            };
            reply = send(table, command);
            if !reply["save"].is_null() {
                save = reply["save"].clone();
            }
            if !reply["entry"].is_null() {
                entries.push(reply["entry"].clone());
            }
        }
        (reply, save, entries)
    }

    fn new_table(opponent: &str) -> Table {
        let mut table = Table::default();
        send(
            &mut table,
            json!({ "cmd": "new", "seed": "7", "opponent": opponent, "mode": "teach" }),
        );
        table
    }

    #[test]
    fn plays_hands_and_reports_them() {
        let mut table = new_table("mystery");
        let (reply, save, entries) = play(&mut table, 4);
        let view = &reply["view"];
        assert_eq!(view["session"]["hands"], 4);
        assert_eq!(view["session"]["past"].as_array().unwrap().len(), 4);
        assert_eq!(view["opponent"]["label"], "mystery");
        assert!(view["opponent"]["blurb"].is_null(), "a mystery stays one");
        assert_eq!(save["hands_done"], 4);
        assert_eq!(entries.len(), 4);
        assert!(
            view["feed"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i["kind"] == "review")
        );
    }

    #[test]
    fn a_saved_session_resumes_where_it_stopped() {
        let mut table = new_table("mystery");
        let (_, _, mut entries) = play(&mut table, 5);
        // Into the next hand, until the player has acted at least once.
        send(&mut table, json!({ "cmd": "next" }));
        let save = loop {
            let view = table.view();
            let command = match view["phase"].as_str().unwrap() {
                "bot" => json!({ "cmd": "bot" }),
                "hero" => json!({ "cmd": "act", "action":
                    if view["legal"]["to_call"].as_u64().unwrap() > 0 { "call" } else { "check" } }),
                // The bot folded before the player could act: try the next hand.
                _ => json!({ "cmd": "next" }),
            };
            let reply = send(&mut table, command);
            if !reply["entry"].is_null() {
                entries.push(reply["entry"].clone());
            }
            let save = &reply["save"];
            if save["current"].as_array().is_some_and(|c| !c.is_empty()) {
                break save.clone();
            }
        };
        let before = table.view();
        let mut resumed = Table::default();
        send(
            &mut resumed,
            json!({ "cmd": "restore", "save": save, "hands": entries }),
        );
        let after = resumed.view();
        for key in ["table", "legal", "session", "opponent", "hand_no", "phase"] {
            assert_eq!(before[key], after[key], "{key} differs after resuming");
        }
    }

    #[test]
    fn a_tampered_save_is_refused_and_changes_nothing() {
        let mut table = new_table("solid");
        let (_, save, mut entries) = play(&mut table, 2);
        let mut record: Value = serde_json::from_str(entries[1]["rec"].as_str().unwrap()).unwrap();
        record["net"] = json!([123, -123]);
        entries[1]["rec"] = json!(record.to_string());
        let reply = table.command(&json!({ "cmd": "restore", "save": save, "hands": entries }));
        assert!(reply["error"].is_string());
        assert_eq!(
            reply["view"]["session"]["hands"], 2,
            "the table is untouched"
        );
    }

    #[test]
    fn a_save_whose_hand_in_progress_does_not_replay_changes_nothing() {
        let mut table = new_table("solid");
        let (_, mut save, entries) = play(&mut table, 2);
        // No raise to a single chip is ever legal.
        save["current"] = json!([{ "RaiseTo": 1 }]);
        let mut other = new_table("station");
        let reply = other.command(&json!({ "cmd": "restore", "save": save, "hands": entries }));
        assert!(reply["error"].is_string());
        assert!(reply.get("save").is_none(), "nothing for the page to store");
        assert_eq!(reply["view"]["opponent"]["label"], "station");
        assert_eq!(
            reply["view"]["session"]["hands"], 0,
            "the table is untouched"
        );
    }

    #[test]
    fn out_of_range_stacks_are_refused() {
        let mut table = Table::default();
        let reply = table.command(&json!({ "cmd": "new", "seed": "1", "stack": 4_294_967_396u64 }));
        assert!(reply["error"].is_string());
        assert!(reply["view"].is_null());
    }

    #[test]
    fn questions_are_asked_and_marked() {
        let mut table = new_table("solid");
        let mut reply = send(&mut table, json!({ "cmd": "view" }));
        while reply["view"]["phase"] == "bot" {
            reply = send(&mut table, json!({ "cmd": "bot" }));
        }
        let id = reply["view"]["quiz"]
            .as_u64()
            .expect("the first decision gets a question");
        let reply = send(&mut table, json!({ "cmd": "answer", "choice": 1 }));
        let view = &reply["view"];
        assert!(view["quiz"].is_null());
        let item = view["feed"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["id"] == id)
            .unwrap();
        assert_eq!(item["state"], "answered");
        assert!(item["right"].is_u64() && item["note"].is_string());
        assert_eq!(view["session"]["skills"][0]["asked"], 1);
    }

    #[test]
    fn ending_a_session_names_the_mystery_opponent() {
        let mut table = new_table("mystery");
        play(&mut table, 3);
        let reply = send(&mut table, json!({ "cmd": "end" }));
        assert!(reply["save"].is_null() && reply.get("save").is_some());
        let view = &reply["view"];
        assert_eq!(view["phase"], "over");
        let name = view["report"]["opponent"]["name"].as_str().unwrap();
        assert!(STYLES.iter().any(|s| s.name == name));
        assert_eq!(view["opponent"]["label"], name);
    }

    #[test]
    fn mistakes_come_back_as_drills() {
        // Fold everything: plenty of mistakes to drill.
        let mut table = new_table("station");
        let mut entries = Vec::new();
        let mut reply = send(&mut table, json!({ "cmd": "view" }));
        while entries.len() < 12 {
            let view = &reply["view"];
            let command = match view["phase"].as_str().unwrap() {
                "bot" => json!({ "cmd": "bot" }),
                "next" => json!({ "cmd": "next" }),
                _ if view["legal"]["to_call"].as_u64().unwrap() > 0 => {
                    json!({ "cmd": "act", "action": "fold" })
                }
                _ => json!({ "cmd": "act", "action": "check" }),
            };
            reply = send(&mut table, command);
            if !reply["entry"].is_null() {
                entries.push(reply["entry"].clone());
            }
        }
        let spots: Vec<Value> = entries
            .iter()
            .flat_map(|e| {
                e["dec"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .enumerate()
                    .filter(|(_, d)| !d["leak"].is_null())
                    .map(|(i, _)| json!({ "rec": e["rec"], "i": i }))
                    .collect::<Vec<_>>()
            })
            .take(3)
            .collect();
        assert!(!spots.is_empty(), "folding everything is a leak");
        let count = spots.len();
        let reply = send(&mut table, json!({ "cmd": "drill", "spots": spots }));
        assert_eq!(reply["view"]["kind"], "drill");
        assert_eq!(reply["view"]["drill"]["spots"], count);
        for _ in 0..count {
            let call = table.view()["legal"]["to_call"].as_u64().unwrap() > 0;
            let action = if call { "call" } else { "check" };
            let reply = send(&mut table, json!({ "cmd": "act", "action": action }));
            assert_eq!(reply["view"]["phase"], "next");
            send(&mut table, json!({ "cmd": "next" }));
        }
        let view = table.view();
        assert_eq!(view["phase"], "over");
        assert_eq!(view["report"]["kind"], "drill");
    }

    #[test]
    fn the_coach_request_carries_the_facts() {
        let mut table = new_table("solid");
        play(&mut table, 1);
        let reply = send(&mut table, json!({ "cmd": "chat", "kind": "review" }));
        let chat = &reply["chat"];
        assert!(chat["system"].as_str().unwrap().contains("coach"));
        // A request for a review the table has moved past gets nothing.
        let stale = send(
            &mut table,
            json!({ "cmd": "chat", "kind": "review", "review": 999_999 }),
        );
        assert!(stale.get("chat").is_none());
        let content = chat["messages"][0]["content"].as_str().unwrap();
        assert!(content.contains("FACTS") && content.contains("hero_action"));
        let id = chat["id"].as_u64().unwrap();
        let reply = send(
            &mut table,
            json!({ "cmd": "chat_fail", "id": id, "error": "offline" }),
        );
        let feed = reply["view"]["feed"].as_array().unwrap();
        let item = feed.iter().find(|i| i["id"] == id).unwrap();
        assert_eq!(item["pending"], false);
        assert!(
            !item["text"].as_str().unwrap().is_empty(),
            "falls back to the engine's reasons"
        );
    }
}
