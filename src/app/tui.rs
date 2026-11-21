//! The full-screen table: cards, stacks and pot on a felt, an action log, a
//! coach that talks with you, and one-key actions.

use super::coach_ai::{Ask, Chat, SYSTEM};
use super::llm::{Llm, Message};
use super::text::{bb, bb_signed, describe, narrate, position};
use super::{CoachMode, facts, session_facts};
use poker::analysis::{Decision, analyse_with};
use poker::bot::{STYLES, by_name};
use poker::cards::{Card, Rng};
use poker::coach::{
    Grade, Question, Skill, Spot, drills, fmt_bb, fmt_bb_signed, headline, review, why,
};
use poker::game::{Action, HandState, Legal, Street};
use poker::history;
use poker::range::{Profile, Range, Strengths, combo};
use poker::session::{HERO, Observed, Review, Session};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};
use std::collections::VecDeque;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

const FELT: Color = Color::Rgb(22, 92, 58);
const FELT_EDGE: Color = Color::Rgb(120, 82, 40);
const GOLD: Color = Color::Rgb(232, 190, 90);
const BOT_DELAY: Duration = Duration::from_millis(700);

/// What the table is for: live play, or drilling past mistakes.
pub enum Game {
    Play {
        session: Box<Session>,
        log_file: PathBuf,
        hands: Option<u64>,
    },
    Drill {
        spots: Vec<Decision>,
        index: usize,
        correct: u32,
    },
}

#[derive(Clone, Copy, PartialEq)]
enum Speaker {
    Coach,
    You,
    System,
    /// A rule across the panel, e.g. at the end of a hand.
    Divider,
}

struct CoachLine {
    who: Speaker,
    text: String,
    grade: Option<Grade>,
    extra: Extra,
}

/// What a line in the coach panel shows besides its text.
enum Extra {
    None,
    /// The answers to a question still open, chosen with one key each.
    Choices(Vec<String>),
    /// A reviewed decision: the verdict and, in full, the options compared.
    Review(Box<Review>, Detail),
    /// The grades of the decisions in a finished hand, and the chips given up.
    Hand(Vec<Grade>, f64),
}

/// How much of a reviewed decision is shown.
#[derive(Clone, Copy, PartialEq)]
enum Detail {
    /// One line: the verdict and what the play was worth. Carries the run
    /// of sound decisions it extends.
    Brief(u32),
    /// The options compared; the language model, if any, explains.
    Full,
    /// Opened by the player: the options and the facts that decided it.
    Opened,
}

/// How likely each of the 169 starting hands is in a range, against the
/// likeliest: pairs on the diagonal, suited hands above it, offsuit below.
type Grid = [[f32; 13]; 13];

/// A finished hand, for the list of recent hands.
struct PastHand {
    number: u64,
    hole: [Card; 2],
    position: &'static str,
    /// The opponent's cards, if they were shown.
    shown: Option<[Card; 2]>,
    net: i64,
    grades: Vec<Grade>,
}

/// A question to the coach being typed.
struct Input {
    text: String,
}

/// The question in front of the player, answered with one key.
struct Quiz {
    /// About the spot; `None` asks which kind of opponent this is.
    question: Option<Question>,
    /// Its line in the coach panel.
    line: usize,
    choices: usize,
}

struct BetSlider {
    amount: u32,
    typed: String,
}

enum CoachEvent {
    Text(String),
    Done(Result<String, String>),
}

struct Inflight {
    rx: Receiver<CoachEvent>,
    line: usize,
    /// Shown instead if the request fails.
    fallback: Option<String>,
}

pub struct App {
    game: Game,
    mode: CoachMode,
    llm: Option<Arc<Llm>>,
    chat: Chat,
    log: Vec<Line<'static>>,
    coach: Vec<CoachLine>,
    coach_scroll: u16,
    input: Option<Input>,
    bet: Option<BetSlider>,
    /// The coach's analysis of the decision in front of the player.
    spot: Option<Spot>,
    quiz: Option<Quiz>,
    /// Skills already asked about in this hand.
    asked: Vec<Skill>,
    /// Questions asked and answered well, by skill.
    skills: [(u32, u32); 5],
    /// Answers right in a row, by skill: the longer the run, the less
    /// often that skill is asked about.
    streaks: [u32; 5],
    last_skill: Option<Skill>,
    /// Chooses which question to ask, and whether to ask at all.
    rng: Rng,
    /// Sound decisions in a row.
    run: u32,
    /// Decisions reviewed so far; the first few are shown in full.
    reviews_shown: u32,
    /// The result after each finished hand.
    totals: Vec<i64>,
    past: Vec<PastHand>,
    /// The opponent's likely hands when the player last acted.
    last_grid: Option<Grid>,
    /// The player's guess at a mystery opponent (an index into `STYLES`).
    guess: Option<usize>,
    /// Hands finished when the guess was last asked for.
    guess_asked: Option<u64>,
    last_review: Option<Review>,
    queue: VecDeque<(Ask, Option<serde_json::Value>, Option<String>)>,
    inflight: Option<Inflight>,
    bot_due: Option<Instant>,
    hand_done: bool,
    show_numbers: bool,
    show_help: bool,
    confirm_quit: bool,
    over: bool,
    flash: Option<(String, Instant)>,
    /// A divider to add once the coach has finished speaking, with the
    /// grades of the hand it closes.
    pending_divider: Option<(String, Vec<Grade>, f64)>,
}

impl App {
    pub fn new(game: Game, mode: CoachMode, llm: Option<Llm>) -> App {
        let mut app = App {
            game,
            mode,
            llm: llm.map(Arc::new),
            chat: Chat::default(),
            log: Vec::new(),
            coach: Vec::new(),
            coach_scroll: 0,
            input: None,
            bet: None,
            spot: None,
            quiz: None,
            asked: Vec::new(),
            skills: [(0, 0); 5],
            streaks: [0; 5],
            last_skill: None,
            rng: Rng::new(0),
            run: 0,
            reviews_shown: 0,
            totals: Vec::new(),
            past: Vec::new(),
            last_grid: None,
            guess: None,
            guess_asked: None,
            last_review: None,
            queue: VecDeque::new(),
            inflight: None,
            bot_due: None,
            hand_done: false,
            show_numbers: mode == CoachMode::Full,
            show_help: false,
            confirm_quit: false,
            over: false,
            flash: None,
            pending_divider: None,
        };
        let intro = match (&app.llm, mode) {
            (_, CoachMode::Off) => "Coach is off. Press ? for keys.".to_string(),
            (Some(llm), _) => format!(
                "I'm watching every decision ({}). Press / to ask me anything, ? for keys.",
                llm.model()
            ),
            (None, _) => "I'm watching every decision and will price each one. Set \
                 ANTHROPIC_API_KEY to talk it through with Claude. ? for keys."
                .to_string(),
        };
        app.say(Speaker::System, intro, None);
        if let Game::Play { session, .. } = &app.game {
            app.rng = Rng::new(session.seed() ^ 0xC0AC4);
        }
        if let Game::Play { session, .. } = &app.game
            && session.opponent_hidden()
            && mode != CoachMode::Off
        {
            app.say(
                Speaker::System,
                "Your opponent is a mystery: a solid regular, a calling station, a nit or a \
                 maniac. Watch what it does (top right), work out which, and adjust. I'll \
                 tell you who it was when you leave."
                    .to_string(),
                None,
            );
        }
        app.start_hand();
        app
    }

    pub fn run(mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        loop {
            self.tick()?;
            terminal.draw(|frame| self.draw(frame))?;
            if event::poll(Duration::from_millis(40))?
                && let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                self.key(key)?;
            }
            if self.over && self.confirm_quit {
                return Ok(());
            }
        }
    }

    // ---- game flow -------------------------------------------------------

    fn state(&self) -> &HandState {
        match &self.game {
            Game::Play { session, .. } => session.state(),
            Game::Drill { spots, index, .. } => &spots[*index].state,
        }
    }

    fn hero_to_act(&self) -> bool {
        match &self.game {
            Game::Play { session, .. } => session.hero_to_act() && !self.over,
            Game::Drill { .. } => self.last_review.is_none() && !self.over,
        }
    }

    fn start_hand(&mut self) {
        self.chat.reset();
        self.log.clear();
        self.spot = None;
        self.last_review = None;
        self.hand_done = false;
        self.quiz = None;
        self.asked.clear();
        self.last_grid = None;
        match &self.game {
            Game::Play { session, .. } => {
                let state = session.state();
                let head = format!(
                    "Hand {} · you are on the {}",
                    session.hand_no(),
                    position(state, HERO)
                );
                self.log.push(Line::from(head.bold().fg(GOLD)));
                if session.bot_to_act() {
                    self.bot_due = Some(Instant::now() + BOT_DELAY);
                }
            }
            Game::Drill { spots, index, .. } => {
                let d = &spots[*index];
                self.log.push(Line::from(
                    format!(
                        "Drill {} of {} · from hand {} · you are on the {}",
                        index + 1,
                        spots.len(),
                        d.hand_no,
                        position(&d.state, HERO)
                    )
                    .bold()
                    .fg(GOLD),
                ));
                for line in narrate(&d.state, HERO) {
                    self.log.push(Line::from(line));
                }
            }
        }
    }

    fn tick(&mut self) -> io::Result<()> {
        self.pump_coach();
        if self.over {
            return Ok(());
        }
        if let Game::Play { session, .. } = &self.game {
            if session.bot_to_act() {
                let due = *self.bot_due.get_or_insert(Instant::now() + BOT_DELAY);
                if Instant::now() >= due {
                    self.bot_due = None;
                    self.bot_act();
                }
                return Ok(());
            }
            if session.state().outcome().is_some() && !self.hand_done {
                return self.finish_hand();
            }
        }
        if self.hero_to_act() && self.spot.is_none() {
            self.begin_decision();
        }
        Ok(())
    }

    fn bot_act(&mut self) {
        let Game::Play { session, .. } = &mut self.game else {
            return;
        };
        let before = session.state().clone();
        let legal = before.legal().expect("bot acts in a live hand");
        let action = session.bot_act();
        let line = describe("Bot", action, legal, &before);
        self.last_review = None;
        self.log_action(&before, Line::from(line.fg(Color::LightRed)));
    }

    /// Logs an action, plus the new street if it brought one.
    fn log_action(&mut self, before: &HandState, line: Line<'static>) {
        self.log.push(line);
        let after = self.state().clone();
        if after.outcome().is_none() && after.street() != before.street() {
            let board = cards_text(after.board());
            self.log.push(Line::from(vec![
                format!("── {} ", after.street().name()).bold().fg(GOLD),
                Span::raw(board),
            ]));
        }
    }

    fn begin_decision(&mut self) {
        let spot = self.analyse_quietly();
        if self.guess_due() {
            self.ask_for_guess();
        } else if self.mode == CoachMode::Teach {
            self.ask_question(&spot);
        }
        self.spot = Some(spot);
    }

    fn analyse_quietly(&self) -> Spot {
        match &self.game {
            Game::Play { session, .. } => session.analyse(),
            Game::Drill { spots, index, .. } => spots[*index].spot.clone(),
        }
    }

    /// Whether to ask which kind of opponent this is: once there is enough
    /// to go on, and again every so often in case the player's view changed.
    fn guess_due(&self) -> bool {
        let Game::Play { session, .. } = &self.game else {
            return false;
        };
        let finished = session.finished();
        session.opponent_hidden()
            && self.mode != CoachMode::Off
            && self.quiz.is_none()
            && finished >= 12
            && self.guess_asked.is_none_or(|at| finished >= at + 15)
    }

    fn ask_for_guess(&mut self) {
        let Game::Play { session, .. } = &self.game else {
            return;
        };
        self.guess_asked = Some(session.finished());
        let text = match self.guess {
            None => "You have seen enough to judge. What kind of player is this?".to_string(),
            Some(guess) => format!(
                "You took this for a {}. Still think so?",
                STYLES[guess].name
            ),
        };
        self.open_quiz(
            None,
            text,
            STYLES.iter().map(|s| s.name.to_string()).collect(),
        );
    }

    fn open_quiz(&mut self, question: Option<Question>, text: String, choices: Vec<String>) {
        let count = choices.len();
        self.coach.push(CoachLine {
            who: Speaker::Coach,
            text,
            grade: None,
            extra: Extra::Choices(choices),
        });
        self.coach_scroll = 0;
        self.quiz = Some(Quiz {
            question,
            line: self.coach.len() - 1,
            choices: count,
        });
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
        let mut roll = self.rng.below(total);
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
        if answered >= 3 && self.rng.below(100) >= chance {
            return;
        }
        self.asked.push(skill);
        self.last_skill = Some(skill);
        let (choices, _) = question.choices(spot);
        let text = question.ask(spot);
        self.open_quiz(Some(question), text, choices);
    }

    fn answer_question(&mut self, choice: usize) {
        let (Some(quiz), Some(spot)) = (self.quiz.take(), self.spot.clone()) else {
            return;
        };
        self.coach[quiz.line].extra = Extra::None;
        let Some(question) = quiz.question else {
            self.guess = Some(choice);
            self.say(
                Speaker::System,
                format!(
                    "Noted: {}. Play against that. I'll say whether you were right when you leave.",
                    STYLES[choice].name
                ),
                Some(Grade::Note),
            );
            return;
        };
        let (choices, right) = question.choices(&spot);
        let feedback = question.answer(&spot, choice);
        let skill = question.skill() as usize;
        self.skills[skill].0 += 1;
        if feedback.grade == Grade::Inaccuracy {
            self.streaks[skill] = 0;
        } else {
            self.skills[skill].1 += 1;
            self.streaks[skill] += 1;
        }
        let said = &choices[choice];
        let text = match feedback.grade {
            Grade::Good => format!("✓ {}: right. {}", capitalise(said), feedback.text),
            Grade::Close => format!("≈ {}: close. {}", capitalise(said), feedback.text),
            _ => format!(
                "✗ {}? No: {}. {}",
                capitalise(said),
                choices[right],
                feedback.text
            ),
        };
        self.say(Speaker::System, text, Some(feedback.grade));
    }

    /// Drops the open question unanswered.
    fn skip_question(&mut self) {
        if let Some(quiz) = self.quiz.take() {
            // Its line stays, empty and unseen, so later lines keep their places.
            self.coach[quiz.line].extra = Extra::None;
            self.coach[quiz.line].text.clear();
        }
    }

    fn facts(&self, spot: &Spot, action: Option<Action>) -> serde_json::Value {
        let session = match &self.game {
            Game::Play { session, .. } => Some(session),
            Game::Drill { .. } => None,
        };
        facts(spot, action, session.map(|s| s.as_ref()))
    }

    fn history(&self) -> Vec<String> {
        narrate(self.state(), HERO)
    }

    fn hero_act(&mut self, action: Action) -> io::Result<()> {
        let Some(legal) = self.state().legal() else {
            return Ok(());
        };
        if !legal.allows(action) {
            self.flash(format!("{action:?} is not legal here"));
            return Ok(());
        }
        self.input = None;
        self.bet = None;
        self.skip_question();
        let before = self.state().clone();
        let coached = self.mode != CoachMode::Off;
        self.last_grid = self.grid_now();
        let spot = self.spot.take();
        let review = match &mut self.game {
            Game::Play { session, .. } => session
                .hero_act(action, coached, spot)
                .map_err(io::Error::other)?,
            Game::Drill {
                spots,
                index,
                correct,
            } => {
                let d = &spots[*index];
                let bot = by_name(&d.opponent, 0).expect("drills come from known bots");
                let strengths = Strengths::new(d.state.board());
                let spot = analyse_with(&d.state, bot.as_ref(), &d.range, &strengths, action);
                let feedback = review(&spot, action);
                if feedback.loss < 0.1 * d.state.config().big_blind as f64 {
                    *correct += 1;
                }
                Some(Review {
                    spot,
                    action,
                    feedback,
                })
            }
        };
        let line = describe("You", action, legal, &before);
        self.log_action(&before, Line::from(line.fg(Color::LightCyan)));
        if let Some(review) = review {
            self.show_review(&review, &before);
            self.last_review = Some(review);
        } else {
            self.last_review = None;
        }
        if let Game::Play { session, .. } = &self.game
            && session.bot_to_act()
        {
            self.bot_due = Some(Instant::now() + BOT_DELAY);
        }
        Ok(())
    }

    fn show_review(&mut self, review: &Review, _before: &HandState) {
        let fb = &review.feedback;
        let sound = matches!(fb.grade, Grade::Good | Grade::Close);
        self.run = if sound { self.run + 1 } else { 0 };
        self.reviews_shown += 1;
        // A sound play gets a line, not a lecture, once the player has seen
        // what a full review looks like. Drills are always shown in full.
        let detail = if sound && self.reviews_shown > 2 && matches!(self.game, Game::Play { .. }) {
            Detail::Brief(self.run)
        } else {
            Detail::Full
        };
        self.coach.push(CoachLine {
            who: Speaker::System,
            text: String::new(),
            grade: Some(fb.grade),
            extra: Extra::Review(Box::new(review.clone()), detail),
        });
        self.coach_scroll = 0;
        if self.llm.is_some() && detail == Detail::Full {
            let facts = self.facts(&review.spot, Some(review.action));
            let fallback = why(&review.spot, review.action).join(" ");
            self.queue
                .push_back((Ask::Review, Some(facts), Some(fallback)));
        }
        if let Game::Drill { spots, index, .. } = &self.game {
            let d = &spots[*index];
            let original = format!(
                "In the game you chose {} ({}). Press space for the next spot.",
                d.spot.phrase(d.action),
                d.feedback.grade.label().to_lowercase()
            );
            self.say(Speaker::System, original, None);
        }
    }

    fn finish_hand(&mut self) -> io::Result<()> {
        self.hand_done = true;
        let Game::Play {
            session,
            log_file,
            hands,
        } = &mut self.game
        else {
            return Ok(());
        };
        let record = session.finish_hand().expect("hand is over");
        history::append(log_file, &record)?;
        self.totals.push(session.total());
        let state = session.state();
        let outcome = state.outcome().unwrap();
        let big_blind = state.config().big_blind;
        let mut lines = Vec::new();
        if outcome.showdown {
            lines.push(Line::from(vec![
                "Showdown: ".bold(),
                Span::raw(format!(
                    "you show {} ({}), the bot shows {} ({}).",
                    cards_text(&state.hole(HERO)),
                    state.showdown_rank(HERO).describe(),
                    cards_text(&state.hole(1 - HERO)),
                    state.showdown_rank(1 - HERO).describe()
                )),
            ]));
        }
        let net = outcome.net[HERO];
        let result = match outcome.winner {
            None => "Split pot.".to_string(),
            Some(HERO) => format!("You win {}.", bb(net as u32, big_blind)),
            Some(_) => format!("The bot wins; you lose {}.", bb((-net) as u32, big_blind)),
        };
        let color = if net > 0 {
            Color::LightGreen
        } else if net < 0 {
            Color::LightRed
        } else {
            Color::White
        };
        lines.push(Line::from(result.bold().fg(color)));
        lines.push(Line::from("Press space for the next hand.".dim()));
        self.log.extend(lines);
        let summary = match outcome.winner {
            None => "split pot".to_string(),
            Some(HERO) => format!("you won {}", bb(net as u32, big_blind)),
            Some(_) => format!("you lost {}", bb((-net) as u32, big_blind)),
        };
        let grades: Vec<Grade> = session.reviews().iter().map(|r| r.feedback.grade).collect();
        self.past.push(PastHand {
            number: session.hand_no(),
            hole: state.hole(HERO),
            position: if state.button() == HERO { "BTN" } else { "BB" },
            shown: outcome.showdown.then(|| state.hole(1 - HERO)),
            net,
            grades: grades.clone(),
        });
        let lost: f64 = session.reviews().iter().map(|r| r.feedback.loss).sum();
        self.pending_divider = Some((
            format!("End of hand {} · {summary}", session.hand_no()),
            grades,
            lost,
        ));
        let done = hands.is_some_and(|limit| session.hand_no() >= limit);
        if done {
            self.end_session();
        }
        Ok(())
    }

    fn next(&mut self) {
        match &mut self.game {
            Game::Play { session, .. } => {
                if !self.hand_done {
                    return;
                }
                session.next_hand();
            }
            Game::Drill { spots, index, .. } => {
                if self.last_review.is_none() {
                    return;
                }
                self.pending_divider =
                    Some((format!("End of spot {}", *index + 1), Vec::new(), 0.0));
                if *index + 1 >= spots.len() {
                    self.end_session();
                    return;
                }
                *index += 1;
            }
        }
        self.start_hand();
    }

    fn end_session(&mut self) {
        self.over = true;
        self.input = None;
        self.bet = None;
        self.skip_question();
        let summary = match &self.game {
            Game::Play { session, .. } => {
                let big_blind = session.config().big_blind;
                let hands = session.finished();
                let mut text = vec![format!(
                    "SESSION REPORT\nResult: {} over {} hand{}.",
                    bb_signed(session.total(), big_blind),
                    hands,
                    if hands == 1 { "" } else { "s" }
                )];
                if self.mode != CoachMode::Off && session.reviewed() > 0 {
                    text.push(format!(
                        "Given up to mistakes: {}{}. This, not the result, measures how you played.",
                        fmt_bb(session.given_up(), big_blind),
                        per_hundred(session.given_up(), hands, big_blind)
                            .map(|x| format!(" ({x} per 100 hands)"))
                            .unwrap_or_default(),
                    ));
                    let count = |g: Grade| session.grades().get(&g).copied().unwrap_or(0);
                    text.push(format!(
                        "Decisions: good or close {} · inaccurate {} · mistakes {} · blunders {}.",
                        count(Grade::Good) + count(Grade::Close),
                        count(Grade::Inaccuracy),
                        count(Grade::Mistake),
                        count(Grade::Blunder),
                    ));
                }
                let asked: u32 = self.skills.iter().map(|s| s.0).sum();
                if asked > 0 {
                    text.push(format!("Questions: {}.", skills_text(&self.skills)));
                }
                for (i, leak) in session.tally().lines().iter().take(3).enumerate() {
                    if i == 0 {
                        text.push("\nWORK ON".to_string());
                    }
                    text.push(format!(
                        "{}. {} on the {} ({}×, {}). {}",
                        i + 1,
                        capitalise(leak.leak.label()),
                        leak.street.name().to_lowercase(),
                        leak.count,
                        fmt_bb(leak.loss, big_blind),
                        leak.leak.tip()
                    ));
                }
                if session.opponent_hidden() {
                    text.push(format!(
                        "\nYOUR OPPONENT WAS: {}",
                        session.bot_name().to_uppercase()
                    ));
                    if let Some(style) = session.bot().style() {
                        text.push(format!("{} {}", style.blurb, style.counter));
                    }
                    text.push(format!(
                        "What you saw: {}.",
                        observed_text(&session.observed())
                    ));
                    if let Some(guess) = self.guess {
                        let name = STYLES[guess].name;
                        text.push(if name == session.bot_name() {
                            format!("You took it for a {name}: right.")
                        } else {
                            format!("You took it for a {name}: not this time.")
                        });
                    }
                }
                if self.llm.is_some() && self.mode != CoachMode::Off {
                    let facts = session_facts(session);
                    self.queue.push_back((Ask::Summary, Some(facts), None));
                }
                text.join("\n")
            }
            Game::Drill { spots, correct, .. } => format!(
                "Drill over: {correct} of {} spots played well.",
                spots.len()
            ),
        };
        self.say(Speaker::System, summary + "\nPress q to leave.", None);
    }

    // ---- coach plumbing ---------------------------------------------------

    fn say(&mut self, who: Speaker, text: String, grade: Option<Grade>) {
        self.coach.push(CoachLine {
            who,
            text,
            grade,
            extra: Extra::None,
        });
        self.coach_scroll = 0;
    }

    /// Opens the latest reviewed decision in full, or closes it again.
    fn toggle_details(&mut self) {
        let latest = self
            .coach
            .iter_mut()
            .rev()
            .find_map(|l| match &mut l.extra {
                Extra::Review(_, detail) => Some(detail),
                _ => None,
            });
        match latest {
            Some(detail) => {
                *detail = match *detail {
                    Detail::Brief(_) | Detail::Full => Detail::Opened,
                    Detail::Opened => Detail::Brief(0),
                };
                self.coach_scroll = 0;
            }
            None => self.flash("Nothing reviewed yet.".into()),
        }
    }

    /// The opponent's likely hands right now, for the grid.
    fn grid_now(&self) -> Option<Grid> {
        let state = self.state();
        let mut dead = state.hole(HERO).to_vec();
        dead.extend_from_slice(state.board());
        let range = match &self.game {
            Game::Play { session, .. } => session.range(),
            Game::Drill { spots, index, .. } => &spots[*index].range,
        };
        Some(range_grid(range, &dead))
    }

    fn flash(&mut self, text: String) {
        self.flash = Some((text, Instant::now()));
    }

    fn pump_coach(&mut self) {
        if let Some(inflight) = &self.inflight {
            let mut finished = None;
            while let Ok(event) = inflight.rx.try_recv() {
                match event {
                    CoachEvent::Text(text) => {
                        let line = inflight.line;
                        self.coach[line].text.push_str(&text);
                    }
                    CoachEvent::Done(result) => finished = Some(result),
                }
            }
            if let Some(result) = finished {
                let inflight = self.inflight.take().unwrap();
                match result {
                    Ok(text) => self.chat.reply(&text),
                    Err(error) => {
                        self.chat.abandon();
                        let line = &mut self.coach[inflight.line];
                        line.who = Speaker::System;
                        line.text = format!("(The coach could not answer: {error})");
                        if let Some(fallback) = inflight.fallback {
                            self.say(Speaker::Coach, fallback, None);
                        }
                    }
                }
            }
        }
        if self.inflight.is_none()
            && self.queue.is_empty()
            && let Some((text, grades, lost)) = self.pending_divider.take()
        {
            self.say(Speaker::Divider, text, None);
            if !grades.is_empty() {
                self.coach.push(CoachLine {
                    who: Speaker::System,
                    text: String::new(),
                    grade: None,
                    extra: Extra::Hand(grades, lost),
                });
            }
        }
        if self.inflight.is_none()
            && let Some(llm) = self.llm.clone()
            && let Some((ask, facts, fallback)) = self.queue.pop_front()
        {
            let history = self.history();
            let messages: Vec<Message> = self.chat.request(&ask, facts.as_ref(), &history);
            let (tx, rx) = channel();
            std::thread::spawn(move || {
                let result = llm.stream(SYSTEM, &messages, |text| {
                    let _ = tx.send(CoachEvent::Text(text.to_string()));
                });
                let _ = tx.send(CoachEvent::Done(result));
            });
            self.say(Speaker::Coach, String::new(), None);
            self.inflight = Some(Inflight {
                rx,
                line: self.coach.len() - 1,
                fallback,
            });
        }
    }

    fn submit_input(&mut self) {
        let Some(input) = self.input.take() else {
            return;
        };
        let text = input.text.trim().trim_start_matches('/').trim().to_string();
        if text.is_empty() {
            return;
        }
        self.say(Speaker::You, text.clone(), None);
        if self.llm.is_none() {
            self.say(
                Speaker::System,
                "Questions need the language-model coach: set ANTHROPIC_API_KEY and restart."
                    .into(),
                None,
            );
            return;
        }
        // In teach mode the spot's numbers stay hidden until the player acts.
        let current = self.spot.as_ref().filter(|_| self.mode != CoachMode::Teach);
        let facts = match (current, self.last_review.as_ref()) {
            (Some(spot), _) => Some(self.facts(spot, None)),
            (None, Some(review)) => Some(self.facts(&review.spot, Some(review.action))),
            (None, None) => None,
        };
        self.queue.push_back((Ask::Free(text), facts, None));
    }

    // ---- keys -------------------------------------------------------------

    fn key(&mut self, key: KeyEvent) -> io::Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.over = true;
            self.confirm_quit = true;
            return Ok(());
        }
        if self.show_help {
            self.show_help = false;
            return Ok(());
        }
        if self.confirm_quit {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('q') | KeyCode::Enter => {
                    if !self.over {
                        self.end_session();
                    }
                    self.confirm_quit = !self.over;
                    self.over = true;
                }
                _ => self.confirm_quit = false,
            }
            return Ok(());
        }
        if let Some(input) = &mut self.input {
            match key.code {
                KeyCode::Enter => self.submit_input(),
                KeyCode::Esc => self.input = None,
                KeyCode::Backspace => {
                    input.text.pop();
                }
                KeyCode::Char(c) => input.text.push(c),
                _ => {}
            }
            return Ok(());
        }
        if self.bet.is_some() {
            return self.bet_key(key);
        }
        if self.over {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => self.confirm_quit = true,
                KeyCode::PageUp => self.coach_scroll = self.coach_scroll.saturating_add(5),
                KeyCode::PageDown => self.coach_scroll = self.coach_scroll.saturating_sub(5),
                KeyCode::Char('d') => self.toggle_details(),
                _ => {}
            }
            return Ok(());
        }
        if let Some(quiz) = &self.quiz {
            match key.code {
                KeyCode::Char(d @ '1'..='9') if (d as usize - '1' as usize) < quiz.choices => {
                    self.answer_question(d as usize - '1' as usize);
                    return Ok(());
                }
                KeyCode::Esc => {
                    self.skip_question();
                    return Ok(());
                }
                _ => {}
            }
        }
        let legal = self.state().legal().filter(|_| self.hero_to_act());
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.confirm_quit = true,
            KeyCode::Char('?') | KeyCode::Char('h') => self.show_help = true,
            KeyCode::Char('/') | KeyCode::Char('t') | KeyCode::Tab => {
                if self.mode != CoachMode::Off {
                    self.input = Some(Input {
                        text: String::new(),
                    });
                }
            }
            KeyCode::Char('n') => self.show_numbers = !self.show_numbers,
            KeyCode::Char('d') => self.toggle_details(),
            KeyCode::PageUp => self.coach_scroll = self.coach_scroll.saturating_add(5),
            KeyCode::PageDown => self.coach_scroll = self.coach_scroll.saturating_sub(5),
            KeyCode::Char(' ') | KeyCode::Enter if legal.is_none() => self.next(),
            KeyCode::Char('f') if legal.is_some() => self.hero_act(Action::Fold)?,
            KeyCode::Char('c') | KeyCode::Char('x') | KeyCode::Char('k') if legal.is_some() => {
                let action = if legal.unwrap().to_call > 0 {
                    Action::Call
                } else {
                    Action::Check
                };
                self.hero_act(action)?
            }
            KeyCode::Char('a') if legal.is_some() => {
                let action = match legal.unwrap().raise {
                    Some((_, max)) => Action::RaiseTo(max),
                    None => Action::Call,
                };
                self.hero_act(action)?
            }
            KeyCode::Char('b') | KeyCode::Char('r') if legal.is_some() => self.open_bet(None),
            KeyCode::Char(d @ '1'..='5') if legal.is_some() => {
                self.open_bet(Some(d as usize - '1' as usize))
            }
            _ => {}
        }
        Ok(())
    }

    /// Standard sizes: a third, half, three quarters and all of the pot, then all-in.
    fn presets(&self) -> Vec<u32> {
        let state = self.state();
        let Some((legal, range)) = state.legal().and_then(|l| Some((l, l.raise?))) else {
            return Vec::new();
        };
        poker::coach::standard_sizes(
            state.pot(),
            legal.to_call,
            state.street_commit(1 - HERO),
            state.config().big_blind,
            range,
        )
    }

    fn open_bet(&mut self, preset: Option<usize>) {
        let presets = self.presets();
        if presets.is_empty() {
            self.flash("You cannot raise here.".into());
            return;
        }
        let amount = presets[preset.unwrap_or(1).min(presets.len() - 1)];
        self.bet = Some(BetSlider {
            amount,
            typed: String::new(),
        });
    }

    fn bet_key(&mut self, key: KeyEvent) -> io::Result<()> {
        let Some((min, max)) = self.state().legal().and_then(|l| l.raise) else {
            self.bet = None;
            return Ok(());
        };
        let big_blind = self.state().config().big_blind;
        let pot = self.state().pot();
        let step =
            ((pot / 20).max(big_blind / 2) / (big_blind / 2).max(1)) * (big_blind / 2).max(1);
        let presets = self.presets();
        let slider = self.bet.as_mut().unwrap();
        match key.code {
            KeyCode::Esc => self.bet = None,
            KeyCode::Enter => {
                let amount = if slider.typed.is_empty() {
                    Some(slider.amount)
                } else {
                    slider
                        .typed
                        .parse::<f64>()
                        .ok()
                        .map(|x| (x * big_blind as f64).round() as u32)
                };
                match amount {
                    Some(a) if (min..=max).contains(&a) => self.hero_act(Action::RaiseTo(a))?,
                    _ => {
                        slider.typed.clear();
                        self.flash(format!(
                            "Choose between {} and {}.",
                            bb(min, big_blind),
                            bb(max, big_blind)
                        ));
                    }
                }
            }
            KeyCode::Left | KeyCode::Char('-') => {
                slider.typed.clear();
                slider.amount = slider.amount.saturating_sub(step).max(min);
            }
            KeyCode::Right | KeyCode::Char('+') | KeyCode::Char('=') => {
                slider.typed.clear();
                slider.amount = (slider.amount + step).min(max);
            }
            KeyCode::Down => {
                slider.typed.clear();
                slider.amount = slider.amount.saturating_sub(step * 4).max(min);
            }
            KeyCode::Up => {
                slider.typed.clear();
                slider.amount = (slider.amount + step * 4).min(max);
            }
            KeyCode::Char(d @ '1'..='5') if slider.typed.is_empty() => {
                slider.amount = presets[d as usize - '1' as usize];
            }
            KeyCode::Char('a') => {
                slider.typed.clear();
                slider.amount = max;
            }
            KeyCode::Char(c) if c.is_ascii_digit() || c == '.' => slider.typed.push(c),
            KeyCode::Backspace => {
                slider.typed.pop();
            }
            _ => {}
        }
        Ok(())
    }

    // ---- drawing ----------------------------------------------------------

    fn draw(&self, frame: &mut Frame) {
        let area = frame.area();
        frame.render_widget(Block::new().bg(Color::Rgb(14, 17, 22)), area);
        let [title, body, controls] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .areas(area);
        self.draw_title(frame, title);

        let (width, height) = (body.width, body.height);
        if width >= 170 {
            // Three columns: the table, what is known, and the coach.
            let [table, middle, coach] = Layout::horizontal([
                Constraint::Length((width * 9 / 25).clamp(60, 90)),
                Constraint::Min(46),
                Constraint::Length((width / 3).clamp(50, 96)),
            ])
            .areas(body);
            // Under a felt as tall as its cards need: this hand, and the
            // hands before it if there is room beside.
            let lower = if height >= 36 {
                (height - 31).max(12)
            } else {
                7
            };
            let [felt, lower] =
                Layout::vertical([Constraint::Min(17), Constraint::Length(lower)]).areas(table);
            self.draw_felt(frame, felt);
            let mut past_shown = false;
            if lower.width >= 84 && lower.height >= 9 {
                let [log, past] =
                    Layout::horizontal([Constraint::Min(40), Constraint::Length(42)]).areas(lower);
                self.draw_log(frame, log);
                self.draw_past(frame, past);
                past_shown = true;
            } else {
                self.draw_log(frame, lower);
            }

            let grid_shown = height >= 34;
            let mut rest = middle;
            let opponent = |width: u16, candidates: bool| {
                self.opponent_lines(width.saturating_sub(2) as usize, !grid_shown, candidates)
            };
            if grid_shown && middle.width >= 43 + 32 {
                // The grid and the opponent side by side.
                let [top, below] =
                    Layout::vertical([Constraint::Length(GRID_HEIGHT), Constraint::Min(0)])
                        .areas(rest);
                let [grid, beside] =
                    Layout::horizontal([Constraint::Length(43), Constraint::Min(32)]).areas(top);
                let lines = opponent(beside.width, true);
                if lines.is_empty() {
                    // Drills have no opponent to describe: the grid takes the row.
                    self.draw_range(frame, top);
                } else {
                    self.draw_range(frame, grid);
                    self.draw_opponent(frame, beside, lines);
                }
                rest = below;
            } else {
                if grid_shown {
                    let [top, below] =
                        Layout::vertical([Constraint::Length(GRID_HEIGHT), Constraint::Min(0)])
                            .areas(rest);
                    self.draw_range(frame, top);
                    rest = below;
                }
                let lines = opponent(rest.width, false);
                let wanted = lines.len() as u16 + 2;
                if !lines.is_empty() && rest.height >= wanted {
                    let [top, below] =
                        Layout::vertical([Constraint::Length(wanted), Constraint::Min(0)])
                            .areas(rest);
                    self.draw_opponent(frame, top, lines);
                    rest = below;
                }
            }
            if !past_shown && rest.height >= 30 {
                let [top, below] =
                    Layout::vertical([Constraint::Min(20), Constraint::Length(10)]).areas(rest);
                self.draw_past(frame, below);
                rest = top;
            }
            if rest.height >= 6 {
                self.draw_session(frame, rest);
            }
            self.draw_coach(frame, coach);
        } else if width >= 110 {
            let [table, side] =
                Layout::horizontal([Constraint::Min(60), Constraint::Length(width * 2 / 5)])
                    .areas(body);
            let grid_shown = self.draw_table(frame, table);
            let opponent =
                self.opponent_lines(side.width.saturating_sub(2) as usize, !grid_shown, false);
            if !opponent.is_empty() && side.height >= 24 {
                let [top, rest] = Layout::vertical([
                    Constraint::Length(opponent.len() as u16 + 2),
                    Constraint::Min(10),
                ])
                .areas(side);
                self.draw_opponent(frame, top, opponent);
                self.draw_coach(frame, rest);
            } else {
                self.draw_coach(frame, side);
            }
        } else {
            // Stacked: a short table (the latest action is shown on it, so
            // the hand log is dropped) and the coach beneath.
            let felt = if height >= 34 { 20 } else { 16 };
            let [table, coach] =
                Layout::vertical([Constraint::Length(felt), Constraint::Min(4)]).areas(body);
            self.draw_felt(frame, table);
            self.draw_coach(frame, coach);
        }
        self.draw_controls(frame, controls);

        if self.show_help {
            draw_help(frame, area);
        } else if self.confirm_quit && !self.over {
            draw_popup(
                frame,
                area,
                "Leave the table?",
                "y / enter: yes    any other key: no",
            );
        }
    }

    fn draw_title(&self, frame: &mut Frame, area: Rect) {
        let (mut left, right) = match &self.game {
            Game::Play { session, .. } => {
                let big_blind = session.config().big_blind;
                let total = session.total();
                let color = if total > 0 {
                    Color::LightGreen
                } else if total < 0 {
                    Color::LightRed
                } else {
                    Color::Gray
                };
                let mut left = vec![
                    " ♠ poker ".bold().fg(Color::Black).bg(GOLD),
                    format!(
                        "  Hand {}  ·  vs {}  ·  ",
                        session.hand_no(),
                        session.opponent_label()
                    )
                    .into(),
                    format!("result {}", bb_signed(total, big_blind))
                        .bold()
                        .fg(color),
                ];
                if area.width < 170 && self.mode != CoachMode::Off && session.reviewed() > 0 {
                    left.push("  ·  mistakes cost ".fg(Color::Gray));
                    left.push(
                        fmt_bb(session.given_up(), big_blind)
                            .bold()
                            .fg(Color::Yellow),
                    );
                    if let Some(rate) =
                        per_hundred(session.given_up(), session.finished(), big_blind)
                    {
                        left.push(format!(" ({rate} per 100 hands)").fg(Color::Gray));
                    }
                }
                (left, format!("seed {}  ", session.seed()))
            }
            Game::Drill {
                spots,
                index,
                correct,
            } => (
                vec![
                    " ♠ poker drill ".bold().fg(Color::Black).bg(GOLD),
                    format!(
                        "  Spot {} of {}  ·  {} played well",
                        index + 1,
                        spots.len(),
                        correct
                    )
                    .into(),
                ],
                String::new(),
            ),
        };
        // On a wide screen the session panel shows these; on a crowded
        // title they make way.
        if area.width < 170 && self.skills.iter().any(|s| s.0 > 0) {
            let questions = format!("  ·  questions {}", skills_text(&self.skills));
            let used = Line::from(left.clone()).width() + right.len();
            if used + questions.chars().count() <= area.width as usize {
                left.push(questions.fg(Color::Gray));
            }
        }
        let [l, r] =
            Layout::horizontal([Constraint::Min(10), Constraint::Length(right.len() as u16)])
                .areas(area);
        frame.render_widget(Paragraph::new(Line::from(left)), l);
        frame.render_widget(Paragraph::new(right.dim()), r);
    }

    /// The opponent's name on the felt.
    fn opponent_name(&self) -> String {
        match &self.game {
            Game::Play { session, .. } => session.opponent_label().to_uppercase(),
            Game::Drill { spots, index, .. } => spots[*index].opponent.to_uppercase(),
        }
    }

    /// The left of a wide screen: the felt, with the hand log beneath and,
    /// if the screen is tall enough, the opponent's likely hands beside the
    /// log. Returns whether those were drawn.
    fn draw_table(&self, frame: &mut Frame, area: Rect) -> bool {
        if area.height >= 44 && area.width >= 66 {
            // The felt is as tall as its cards need; beyond that the room
            // is better spent on the panels.
            let felt = (area.height - GRID_HEIGHT).min(34);
            let [felt, lower] =
                Layout::vertical([Constraint::Length(felt), Constraint::Min(GRID_HEIGHT)])
                    .areas(area);
            let [range, log] =
                Layout::horizontal([Constraint::Length(43), Constraint::Min(20)]).areas(lower);
            self.draw_felt(frame, felt);
            self.draw_range(frame, range);
            self.draw_log(frame, log);
            true
        } else {
            let [felt, log] =
                Layout::vertical([Constraint::Min(17), Constraint::Length(7)]).areas(area);
            self.draw_felt(frame, felt);
            self.draw_log(frame, log);
            false
        }
    }

    /// The opponent's likely hands as a grid of the 169 starting hands. In
    /// teach mode it lags a decision behind, so it never answers a question
    /// for the player: `n` shows it live.
    fn draw_range(&self, frame: &mut Frame, area: Rect) {
        let block = Block::new()
            .title(" Their likely hands ".bold().fg(Color::LightRed))
            .title(Line::from(" brighter = likelier ".fg(Color::Gray)).right_aligned())
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(Color::DarkGray));
        let state = self.state();
        let live = self.show_numbers && self.spot.is_some();
        let over = state.outcome().is_some();
        let (grid, read, caption) = if self.mode == CoachMode::Off {
            (None, None, "The coach is off.")
        } else if live {
            (
                self.grid_now(),
                self.spot.as_ref().and_then(|s| s.read),
                "Right now",
            )
        } else if over {
            (
                self.grid_now(),
                self.last_review.as_ref().and_then(|r| r.spot.read),
                "At the end of the hand",
            )
        } else if self.last_grid.is_some() {
            (
                self.last_grid,
                self.last_review.as_ref().and_then(|r| r.spot.read),
                "When you last acted",
            )
        } else {
            (None, None, "Shown once you act · n peeks")
        };
        // A range nothing has narrowed yet is not worth lighting up.
        let any_two = grid.is_some_and(|g| g.iter().flatten().all(|&x| x > 0.999));
        let (grid, caption) = if any_two && (live || over) {
            (None, "Any two cards so far")
        } else if any_two {
            (None, "Any two cards when you last acted")
        } else {
            (grid, caption)
        };
        let mut lines = grid_lines(grid.as_ref(), state.hole(HERO));
        let gray = Style::new().fg(Color::Gray);
        lines.push(Line::default());
        lines.push(Line::from(vec![
            Span::styled(format!("{caption} · "), gray),
            Span::styled(" you ", HERO_CELL),
        ]));
        if let Some(read) = read.filter(|_| grid.is_some()) {
            lines.extend(range_bar(&read, 39));
        }
        frame.render_widget(
            Paragraph::new(lines)
                .block(block)
                .alignment(Alignment::Center),
            area,
        );
    }

    /// How the session is going: the result hand by hand, what mistakes
    /// cost and where, what to work on, and the questions answered.
    fn draw_session(&self, frame: &mut Frame, area: Rect) {
        let Game::Play { session, .. } = &self.game else {
            return;
        };
        let block = Block::new()
            .title(" Session ".bold().fg(GOLD))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(Color::DarkGray));
        let width = area.width.saturating_sub(2) as usize;
        let big_blind = session.config().big_blind;
        let gray = Style::new().fg(Color::Gray);
        let head = |text: &str| Line::from(Span::styled(text.to_string(), gray.bold()));
        let total = session.total();
        let color = if total > 0 {
            Color::LightGreen
        } else if total < 0 {
            Color::LightRed
        } else {
            Color::Gray
        };
        let hands = session.finished();
        let mut lines = vec![Line::from(vec![
            Span::styled("RESULT  ", gray.bold()),
            bb_signed(total, big_blind).bold().fg(color),
            Span::styled(
                format!("  after {hands} hand{}", if hands == 1 { "" } else { "s" }),
                gray,
            ),
        ])];
        if self.totals.len() >= 2 {
            lines.extend(chart(&self.totals, width, 3));
        }
        if self.mode != CoachMode::Off && session.reviewed() > 0 {
            lines.push(Line::default());
            let mut cost = vec![
                Span::styled("MISTAKES COST  ", gray.bold()),
                fmt_bb(session.given_up(), big_blind)
                    .bold()
                    .fg(Color::Yellow),
            ];
            if let Some(rate) = per_hundred(session.given_up(), hands, big_blind) {
                cost.push(Span::styled(format!("  {rate} per 100 hands"), gray));
            }
            lines.push(Line::from(cost));
            let count = |g: Grade| session.grades().get(&g).copied().unwrap_or(0);
            let parts = [
                (Grade::Good, "good"),
                (Grade::Close, "close"),
                (Grade::Inaccuracy, "inaccurate"),
                (Grade::Mistake, "mistakes"),
                (Grade::Blunder, "blunders"),
            ];
            let all: u32 = parts.iter().map(|&(g, _)| count(g)).sum();
            if all > 0 {
                let mut bar = Vec::new();
                let mut used = 0;
                for (i, &(grade, _)) in parts.iter().enumerate() {
                    let n = if i == parts.len() - 1 {
                        width - used
                    } else {
                        ((count(grade) as f64 / all as f64 * width as f64).round() as usize)
                            .min(width - used)
                    };
                    used += n;
                    bar.push(Span::styled("█".repeat(n), grade_style(grade)));
                }
                lines.push(Line::from(bar));
                let mut legend = Vec::new();
                for (grade, name) in parts {
                    if count(grade) > 0 {
                        legend.push(Span::styled(
                            format!(
                                "{} {}  ",
                                count(grade),
                                if count(grade) == 1 {
                                    name.trim_end_matches('s')
                                } else {
                                    name
                                }
                            ),
                            grade_style(grade).remove_modifier(ratatui::style::Modifier::BOLD),
                        ));
                    }
                }
                lines.push(Line::from(legend));
            }
            if self.run >= 3 {
                lines.push(Line::from(vec![
                    Span::styled("Run  ", gray),
                    format!("{} sound decisions in a row", self.run).fg(Color::LightGreen),
                ]));
            }
            let leaks = session.tally().lines();
            if !leaks.is_empty() {
                lines.push(Line::default());
                lines.push(head("WHERE IT GOES"));
                let by_street: Vec<(Street, f64)> =
                    [Street::Preflop, Street::Flop, Street::Turn, Street::River]
                        .iter()
                        .map(|&street| {
                            let lost = leaks.iter().filter(|l| l.street == street).map(|l| l.loss);
                            (street, lost.sum())
                        })
                        .collect();
                let worst = by_street.iter().map(|x| x.1).fold(0.0, f64::max);
                for (street, lost) in by_street {
                    lines.push(gauge(
                        street.name(),
                        9,
                        if worst > 0.0 { lost / worst } else { 0.0 },
                        width.saturating_sub(9 + 9).min(24),
                        Color::Yellow,
                        fmt_bb(lost, big_blind),
                    ));
                }
                lines.push(Line::default());
                lines.push(head("WORK ON"));
                for (i, leak) in leaks.iter().take(2).enumerate() {
                    lines.extend(wrap_styled(
                        Some((format!("{}. ", i + 1), Style::new().fg(GOLD).bold())),
                        &format!(
                            "{} on the {} ({}×, {})",
                            capitalise(leak.leak.label()),
                            leak.street.name().to_lowercase(),
                            leak.count,
                            fmt_bb(leak.loss, big_blind)
                        ),
                        Style::new(),
                        width,
                    ));
                }
            }
        }
        if self.skills.iter().any(|s| s.0 > 0) {
            lines.push(Line::default());
            lines.push(head("QUESTIONS"));
            for skill in Skill::ALL {
                let (asked, right) = self.skills[skill as usize];
                if asked > 0 {
                    lines.push(gauge(
                        skill.name(),
                        9,
                        right as f64 / asked as f64,
                        width.saturating_sub(9 + 9).min(24),
                        Color::LightCyan,
                        format!("{right}/{asked}"),
                    ));
                }
            }
        }
        frame.render_widget(Paragraph::new(lines).block(block), area);
    }

    fn draw_felt(&self, frame: &mut Frame, area: Rect) {
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(FELT_EDGE))
            .bg(FELT);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let state = self.state();
        let big_blind = state.config().big_blind;
        let showdown = state.outcome().is_some_and(|o| o.showdown);
        // On a short table the opponent's face-down cards make way.
        let compact = inner.height < 18 && !showdown;
        // And on a tall one the cards are drawn large.
        let big = inner.height >= 26 && inner.width >= 48;
        let seat = if big { 8 } else { 6 };
        let [bot, middle, hero, strip] = Layout::vertical([
            Constraint::Length(if compact { 2 } else { seat }),
            Constraint::Min(4),
            Constraint::Length(seat),
            Constraint::Length(2),
        ])
        .areas(inner);

        // The bot's seat.
        let bot_seat = 1 - HERO;
        let bot_cards = if showdown {
            card_row(&state.hole(bot_seat), 0, big)
        } else if compact {
            Vec::new()
        } else {
            card_backs(2, big)
        };
        let mut lines = vec![seat_label(
            &self.opponent_name(),
            position(state, bot_seat),
            state.stack(bot_seat),
            big_blind,
            state.to_act() == Some(bot_seat) && !self.over,
        )];
        lines.extend(bot_cards);
        lines.push(bet_chip(state.street_commit(bot_seat), big_blind));
        if showdown {
            lines.push(Line::from(
                state.showdown_rank(bot_seat).describe().italic(),
            ));
        }
        frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), bot);

        // The board and pot.
        let mut lines = card_row(state.board(), 5 - state.board().len(), big);
        lines.push(Line::from(vec![
            "Pot ".fg(Color::Gray),
            bb(state.pot(), big_blind).bold().fg(GOLD),
            format!("   ·   {}", state.street().name()).fg(Color::Gray),
        ]));
        // The latest action, so the table reads without the hand log.
        if let Some(last) = self.log.last().filter(|_| self.log.len() > 1) {
            let mut last = last.clone();
            last.style = Style::new().italic();
            lines.push(last);
        }
        let pad = middle.height.saturating_sub(lines.len() as u16) / 2;
        let [_, mid] =
            Layout::vertical([Constraint::Length(pad), Constraint::Min(1)]).areas(middle);
        frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), mid);

        // The player's seat.
        let hole = state.hole(HERO);
        let mut lines = vec![bet_chip(state.street_commit(HERO), big_blind)];
        lines.extend(card_row(&hole, 0, big));
        let hand = poker::coach::describe_hand(hole, state.board());
        lines.push(seat_label(
            "YOU",
            position(state, HERO),
            state.stack(HERO),
            big_blind,
            self.hero_to_act(),
        ));
        lines.push(Line::from(hand.italic().fg(Color::White)));
        frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), hero);

        self.draw_strip(frame, strip);
    }

    /// The decision in numbers, under the player's seat. What anyone at the
    /// table can see is always shown; what the coach worked out (the price in
    /// teach mode, your equity and the best play) is shown when the numbers
    /// are on.
    fn draw_strip(&self, frame: &mut Frame, area: Rect) {
        let state = self.state();
        let big_blind = state.config().big_blind;
        let Some(legal) = state.legal().filter(|_| self.hero_to_act()) else {
            return;
        };
        let gray = |text: &str| Span::styled(text.to_string(), Style::new().fg(Color::Gray));
        let sep = || gray("   ·   ");
        let mut first = Vec::new();
        if legal.to_call > 0 {
            first.extend([
                gray("To call "),
                bb(legal.to_call, big_blind).bold(),
                gray(" into "),
                bb(state.pot(), big_blind).bold(),
                sep(),
                gray("Price "),
            ]);
            if self.mode == CoachMode::Teach && !self.show_numbers {
                first.push("?".bold().fg(GOLD));
            } else {
                let need = poker::coach::pot_odds(state.pot(), legal.to_call);
                first.push(format!("win {:.0}%+", need * 100.0).bold());
            }
            first.push(sep());
        }
        let behind = state.stack(HERO).min(state.stack(1 - HERO));
        first.extend([
            gray("Stacks "),
            format!("{:.1}× pot", behind as f64 / state.pot().max(1) as f64).bold(),
        ]);
        let mut lines = vec![Line::from(first)];
        if self.mode != CoachMode::Off {
            match self.spot.as_ref().filter(|_| self.show_numbers) {
                Some(spot) => {
                    let mut second = vec![
                        gray("Equity "),
                        format!("{:.0}%", spot.equity * 100.0).bold(),
                    ];
                    if let Some(&(best, ev)) = spot.ranked(None).first() {
                        second.extend([
                            sep(),
                            gray("Best "),
                            format!("{} ({})", spot.phrase(best), fmt_bb_signed(ev, big_blind))
                                .bold()
                                .fg(GOLD),
                        ]);
                    }
                    lines.push(Line::from(second));
                }
                None => lines.push(Line::from(
                    "n  peek at your equity and the best play".fg(Color::Rgb(120, 170, 140)),
                )),
            }
        }
        frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
    }

    /// Who the opponent is and what it has been seen to do, with what it is
    /// likely to hold when `with_range` (the grid shows that otherwise) and,
    /// with `candidates`, the kinds of player a mystery opponent could be.
    /// Empty when there is nothing to show (drills).
    fn opponent_lines(
        &self,
        width: usize,
        with_range: bool,
        candidates: bool,
    ) -> Vec<Line<'static>> {
        let Game::Play { session, .. } = &self.game else {
            return Vec::new();
        };
        let mut lines = Vec::new();
        let seen = session.observed();
        let gray = Style::new().fg(Color::Gray);
        if session.opponent_hidden() && !self.over {
            let read = match seen.read() {
                Some(read) => format!("Looks {read}."),
                None => format!(
                    "Unknown. Watch what it does: a read needs about 12 hands ({} so far).",
                    seen.hands
                ),
            };
            lines.extend(wrap_styled(None, &read, Style::new().bold(), width));
            if let Some(guess) = self.guess {
                lines.push(Line::from(vec![
                    Span::styled("Your guess: ", gray),
                    Span::styled(STYLES[guess].name, Style::new().fg(GOLD).bold()),
                ]));
            }
        } else if let Some(style) = session.bot().style() {
            lines.extend(wrap_styled(None, style.blurb, Style::new().bold(), width));
            lines.extend(wrap_styled(
                Some(("Beat it: ".into(), Style::new().fg(GOLD).bold())),
                style.counter,
                Style::new(),
                width,
            ));
        }
        let rows = [
            ("Plays hands", seen.plays()),
            ("Raises preflop", seen.raises()),
            ("Folds to bets", seen.folds_to_bets()),
            ("Bets, not calls", seen.aggression()),
        ];
        if rows.iter().all(|r| r.1.is_none()) {
            lines.push(Line::from(Span::styled("Seen: nothing yet.", gray)));
        }
        for (label, share) in rows {
            if let Some(share) = share {
                lines.push(gauge(
                    label,
                    16,
                    share,
                    width.saturating_sub(16 + 5).min(24),
                    Color::LightRed,
                    format!("{:.0}%", share * 100.0),
                ));
            }
        }
        if seen.hands > 0 {
            lines.push(Line::from(Span::styled(
                format!(
                    "over {} hand{}",
                    seen.hands,
                    if seen.hands == 1 { "" } else { "s" }
                ),
                gray,
            )));
        }
        // With room to spare, the kinds of player a mystery could be.
        if candidates && session.opponent_hidden() && !self.over {
            lines.push(Line::default());
            lines.push(Line::from(Span::styled("IT IS ONE OF", gray.bold())));
            for style in STYLES {
                // "A nit: plays few hands, folds to pressure…" in a line.
                let trait_ = style.blurb.split(": ").nth(1).unwrap_or(style.blurb);
                let trait_ = trait_.split(',').next().unwrap_or(trait_);
                lines.push(Line::from(vec![
                    Span::styled(format!("{:<8}", style.name), Style::new().fg(GOLD).bold()),
                    Span::styled(trait_.trim_end_matches('.').to_string(), gray),
                ]));
            }
        }
        if !with_range {
            return lines;
        }
        // Its likely hands: now if the numbers are on, else at the last review.
        let read = if self.show_numbers {
            self.spot.as_ref().map(|s| s.read)
        } else {
            self.last_review.as_ref().map(|r| r.spot.read)
        };
        match read {
            Some(Some(read)) => {
                let mut bar = range_bar(&read, width.saturating_sub(8));
                bar[0].spans.insert(
                    0,
                    Span::styled("Holding ", Style::new().fg(Color::Gray).bold()),
                );
                lines.extend(bar);
            }
            Some(None) => lines.push(Line::from(Span::styled(
                "Holding: any two cards so far.",
                gray,
            ))),
            None if self.mode != CoachMode::Off => lines.push(Line::from(Span::styled(
                "Holding: shown once you act (n to peek).",
                gray,
            ))),
            None => {}
        }
        lines
    }

    fn draw_opponent(&self, frame: &mut Frame, area: Rect, lines: Vec<Line<'static>>) {
        let block = Block::new()
            .title(
                format!(" Opponent · {} ", self.opponent_name().to_lowercase())
                    .bold()
                    .fg(Color::LightRed),
            )
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(Color::DarkGray));
        frame.render_widget(Paragraph::new(lines).block(block), area);
    }

    /// The hands before this one, newest first: cards, seat, result and a
    /// dot for each decision.
    fn draw_past(&self, frame: &mut Frame, area: Rect) {
        let block = Block::new()
            .title(" Earlier hands ".bold())
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(Color::DarkGray));
        let big_blind = self.state().config().big_blind;
        let gray = Style::new().fg(Color::Gray);
        let card = |c: Card| {
            Span::styled(
                c.pretty(false),
                Style::new()
                    .fg(suit_color(c))
                    .bg(Color::Rgb(245, 243, 236))
                    .bold(),
            )
        };
        let mut lines: Vec<Line> = Vec::new();
        for hand in self.past.iter().rev().take(area.height as usize) {
            let color = match hand.net {
                n if n > 0 => Color::LightGreen,
                n if n < 0 => Color::LightRed,
                _ => Color::Gray,
            };
            let mut spans = vec![
                Span::styled(format!("{:>3} ", hand.number), gray),
                card(hand.hole[0]),
                card(hand.hole[1]),
                Span::styled(format!(" {:<3}", hand.position), gray),
                Span::styled(
                    format!("{:>8} ", bb_signed(hand.net, big_blind)),
                    Style::new().fg(color).bold(),
                ),
            ];
            for &grade in hand.grades.iter().take(6) {
                spans.push(Span::styled("●", grade_style(grade)));
            }
            if let Some(shown) = hand.shown {
                let pad = 7usize.saturating_sub(hand.grades.len().min(6));
                spans.push(Span::styled(format!("{}vs ", " ".repeat(pad)), gray));
                spans.push(card(shown[0]));
                spans.push(card(shown[1]));
            }
            lines.push(Line::from(spans));
        }
        if lines.is_empty() {
            lines.push(Line::from(Span::styled("None yet.", gray)));
        }
        frame.render_widget(Paragraph::new(lines).block(block), area);
    }

    fn draw_log(&self, frame: &mut Frame, area: Rect) {
        let block = Block::new()
            .title(" Hand ".bold())
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(Color::DarkGray));
        let height = area.height.saturating_sub(2) as usize;
        let skip = self.log.len().saturating_sub(height);
        let lines: Vec<Line> = self.log[skip..].to_vec();
        frame.render_widget(
            Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
            area,
        );
    }

    fn draw_coach(&self, frame: &mut Frame, area: Rect) {
        let thinking = self
            .inflight
            .as_ref()
            .is_some_and(|i| self.coach[i.line].text.is_empty());
        let title = if thinking {
            " Coach · thinking… ".to_string()
        } else {
            " Coach ".to_string()
        };
        let block = Block::new()
            .title(title.bold().fg(GOLD))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(Color::DarkGray));
        let width = area.width.saturating_sub(2).max(1) as usize;
        // Wrapped here rather than by the widget, so the line count is exact
        // and the newest message is always fully in view.
        let mut lines: Vec<Line> = Vec::new();
        for line in &self.coach {
            match &line.extra {
                Extra::Review(review, detail) => {
                    lines.extend(match detail {
                        Detail::Brief(run) => review_brief(review, *run, width),
                        Detail::Full => review_card(review, width, self.llm.is_none()),
                        Detail::Opened => review_card(review, width, true),
                    });
                    lines.push(Line::default());
                    continue;
                }
                Extra::Hand(grades, lost) => {
                    lines.push(hand_line(grades, *lost, self.state().config().big_blind));
                    lines.push(Line::default());
                    continue;
                }
                _ => {}
            }
            if line.text.is_empty() {
                continue;
            }
            let (label, style) = match (line.who, line.grade) {
                (Speaker::You, _) => ("You", Style::new().fg(Color::LightCyan).bold()),
                (Speaker::Coach, _) => ("Coach", Style::new().fg(GOLD).bold()),
                (Speaker::System, Some(grade)) => ("", grade_style(grade)),
                (Speaker::System, None) => ("", Style::new().fg(Color::Gray)),
                (Speaker::Divider, _) => {
                    lines.push(divider(&line.text, width));
                    continue;
                }
            };
            let (head, body) = if label.is_empty() {
                (None, style)
            } else {
                (Some((format!("{label} "), style)), Style::new())
            };
            lines.extend(wrap_styled(head, &line.text, body, width));
            if let Extra::Choices(choices) = &line.extra {
                lines.extend(choice_lines(choices, width));
            }
            lines.push(Line::default());
        }
        // Scroll to the bottom unless the reader scrolled up.
        let height = area.height.saturating_sub(2) as usize;
        let bottom = lines.len().saturating_sub(height) as u16;
        let offset = bottom.saturating_sub(self.coach_scroll);
        frame.render_widget(
            Paragraph::new(Text::from(lines))
                .block(block)
                .scroll((offset, 0)),
            area,
        );
    }

    fn draw_controls(&self, frame: &mut Frame, area: Rect) {
        let block = Block::new()
            .borders(Borders::TOP)
            .border_style(Style::new().fg(Color::DarkGray));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let key = |k: &str| {
            Span::styled(
                format!(" {k} "),
                Style::new().fg(Color::Black).bg(Color::Gray).bold(),
            )
        };
        let big_blind = self.state().config().big_blind;

        let mut line: Vec<Span> = Vec::new();
        if let Some((text, at)) = &self.flash
            && at.elapsed() < Duration::from_secs(3)
        {
            line.push(text.clone().fg(Color::LightRed).bold());
            line.push("   ".into());
        }
        if let Some(input) = &self.input {
            line.push("Ask the coach: ".bold().fg(GOLD));
            line.push(Span::raw(input.text.clone()));
            line.push("▏".fg(GOLD));
            line.push("   enter: send   esc: cancel".dim());
        } else if let Some(slider) = &self.bet {
            let state = self.state();
            let opp = state.street_commit(1 - HERO);
            let base = state.pot() + state.legal().map_or(0, |l| l.to_call);
            let amount = if slider.typed.is_empty() {
                bb(slider.amount, big_blind)
            } else {
                format!("{}bb", slider.typed)
            };
            let share = slider.amount.saturating_sub(opp) as f64 / base.max(1) as f64;
            let verb = if opp == 0 { "Bet" } else { "Raise to" };
            line.push(format!("{verb} {amount}").bold().fg(GOLD));
            if slider.typed.is_empty() {
                line.push(format!(" ({:.0}% pot)", share * 100.0).fg(Color::Gray));
            }
            line.extend([
                "   ".into(),
                key("←→"),
                " adjust  ".into(),
                key("1-5"),
                " ⅓ ½ ¾ pot all-in  ".into(),
                key("0-9"),
                " type bb  ".into(),
                key("enter"),
                " confirm  ".into(),
                key("esc"),
                " cancel".into(),
            ]);
        } else if self.over {
            line.extend([
                key("q"),
                " leave  ".into(),
                key("PgUp/PgDn"),
                " scroll coach".into(),
            ]);
        } else if let Some(legal) = self.state().legal().filter(|_| self.hero_to_act()) {
            if let Some(quiz) = &self.quiz {
                line.extend([
                    key(&format!("1-{}", quiz.choices)),
                    " answer the coach  ".bold().fg(GOLD),
                    key("esc"),
                    " skip   ".into(),
                    "or act:  ".fg(Color::Gray),
                ]);
                line.extend(action_keys(legal, big_blind, &key, false));
            } else {
                line.extend(action_keys(legal, big_blind, &key, true));
                if self.mode != CoachMode::Off {
                    line.extend([
                        key("/"),
                        " ask coach  ".into(),
                        key("n"),
                        " numbers  ".into(),
                    ]);
                    if area.width >= 130 {
                        line.extend([key("d"), " last review  ".into()]);
                    }
                }
                line.extend([key("?"), " help".into()]);
            }
        } else if self.hero_to_act()
            || self.hand_done
            || self.last_review.is_some() && matches!(self.game, Game::Drill { .. })
        {
            line.extend([
                key("space"),
                " next hand  ".bold().fg(GOLD),
                key("/"),
                " ask coach  ".into(),
                key("d"),
                " last review  ".into(),
                key("q"),
                " quit".into(),
            ]);
        } else {
            line.push("The bot is thinking…".italic().fg(Color::Gray));
        }
        frame.render_widget(Paragraph::new(Line::from(line)), inner);
    }
}

fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// Chips given up per hundred hands, once a hand has finished.
fn per_hundred(given_up: f64, hands: u64, big_blind: u32) -> Option<String> {
    (hands > 0).then(|| {
        let rate = given_up / hands as f64 * 100.0 / big_blind as f64;
        format!("{rate:.0}bb")
    })
}

/// "Reads 2/3 · Equity 4/6", for the skills asked about so far.
fn skills_text(skills: &[(u32, u32)]) -> String {
    Skill::ALL
        .iter()
        .filter(|&&skill| skills[skill as usize].0 > 0)
        .map(|&skill| {
            let (asked, right) = skills[skill as usize];
            format!("{} {right}/{asked}", skill.name())
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

/// What the opponent has been seen to do, in words.
fn observed_text(seen: &Observed) -> String {
    let pct = |x: f64| format!("{:.0}%", x * 100.0);
    let mut parts = Vec::new();
    if let Some(x) = seen.plays() {
        parts.push(format!("plays {} of hands", pct(x)));
    }
    if let Some(x) = seen.raises() {
        parts.push(format!("raises {} before the flop", pct(x)));
    }
    if let Some(x) = seen.folds_to_bets() {
        parts.push(format!("folds to {} of bets ({})", pct(x), seen.faced));
    }
    if let Some(x) = seen.aggression() {
        parts.push(format!(
            "after the flop bets {} / calls {}",
            pct(x),
            pct(1.0 - x)
        ));
    }
    if parts.is_empty() {
        return "nothing yet.".to_string();
    }
    if seen.hands == 0 {
        return parts.join(" · ");
    }
    format!(
        "{} · {} hand{}",
        parts.join(" · "),
        seen.hands,
        if seen.hands == 1 { "" } else { "s" }
    )
}

fn grade_icon(grade: Grade) -> &'static str {
    match grade {
        Grade::Good => "✓",
        Grade::Close => "≈",
        Grade::Inaccuracy => "?",
        Grade::Mistake => "✗",
        Grade::Blunder => "✗✗",
        Grade::Note => "·",
    }
}

/// A sound decision in a line: the verdict, what the play was worth and
/// what came next to it.
fn review_brief(review: &Review, run: u32, width: usize) -> Vec<Line<'static>> {
    let (spot, fb) = (&review.spot, &review.feedback);
    let mut verdict = format!("{} {}", grade_icon(fb.grade), fb.grade.label());
    if run >= 3 {
        verdict.push_str(&format!(" ×{run}"));
    }
    verdict.push(' ');
    let priced = |action: Action, ev: f64| {
        format!(
            "{} {}",
            spot.phrase(action),
            fmt_bb_signed(ev, spot.big_blind)
        )
    };
    let mut rest = match spot.ev(review.action) {
        Some(ev) => format!("· {}", priced(review.action, ev)),
        None => format!("· {}", spot.phrase(review.action)),
    };
    let ranked = spot.ranked(Some(review.action));
    let other = ranked.iter().find(|&&(a, _)| a != review.action);
    if let (Some(&(action, ev)), Some(&(best, _))) = (other, ranked.first()) {
        let label = if best == review.action {
            "next"
        } else {
            "best"
        };
        rest.push_str(&format!(" · {label}: {}", priced(action, ev)));
    }
    wrap_styled(
        Some((verdict, grade_style(fb.grade))),
        &rest,
        Style::new().fg(Color::Gray),
        width,
    )
}

/// "Label      ████░░░░  tail": a share between 0 and 1 as a bar.
fn gauge(
    label: &str,
    label_width: usize,
    share: f64,
    cells: usize,
    color: Color,
    tail: String,
) -> Line<'static> {
    let filled = ((share.clamp(0.0, 1.0) * cells as f64).round() as usize).min(cells);
    Line::from(vec![
        Span::styled(
            format!("{label:<label_width$}"),
            Style::new().fg(Color::Gray),
        ),
        Span::styled("█".repeat(filled), Style::new().fg(color)),
        Span::styled(
            "░".repeat(cells - filled),
            Style::new().fg(Color::Rgb(60, 64, 72)),
        ),
        Span::styled(format!(" {tail}"), Style::new().bold()),
    ])
}

/// A running total as a small column chart, `height` rows tall: one column
/// per hand, the latest hands if there are more than fit. Green above zero.
fn chart(totals: &[i64], width: usize, height: usize) -> Vec<Line<'static>> {
    const BLOCKS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let mut points = vec![0];
    points.extend_from_slice(totals);
    let points = &points[points.len().saturating_sub(width.max(1))..];
    // Few hands: wider columns, so the chart is not a sliver.
    let column = (width / points.len().max(1)).clamp(1, 4);
    let low = points.iter().copied().min().unwrap_or(0);
    let high = points.iter().copied().max().unwrap_or(0);
    let span = (high - low).max(1) as f64;
    (0..height)
        .rev()
        .map(|row| {
            let spans: Vec<Span<'static>> = points
                .iter()
                .map(|&value| {
                    let level = ((value - low) as f64 / span * (height * 8 - 1) as f64).round()
                        as usize
                        + 1;
                    let cell = level.saturating_sub(row * 8).min(8);
                    let color = if value >= 0 {
                        Color::Rgb(90, 190, 120)
                    } else {
                        Color::Rgb(210, 90, 90)
                    };
                    Span::styled(
                        BLOCKS[cell].to_string().repeat(column),
                        Style::new().fg(color),
                    )
                })
                .collect();
            Line::from(spans)
        })
        .collect()
}

/// Rows of the range panel: the grid, its caption and the make-up bar.
const GRID_HEIGHT: u16 = 13 + 2 + 2 + 2;

const HERO_CELL: Style = Style::new()
    .fg(Color::Black)
    .bg(Color::Rgb(110, 210, 230))
    .add_modifier(ratatui::style::Modifier::BOLD);

/// Where a starting hand sits in the grid: aces first, suited hands above
/// the diagonal.
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
/// leaving out combinations that use a `dead` card.
fn range_grid(range: &Range, dead: &[Card]) -> Grid {
    let mut sum = [[0.0f64; 13]; 13];
    let mut count = [[0u32; 13]; 13];
    for (i, &weight) in range.weights().iter().enumerate() {
        let hole = combo(i);
        if dead.iter().any(|d| hole.contains(d)) {
            continue;
        }
        let (row, col) = grid_cell(hole);
        sum[row][col] += weight;
        count[row][col] += 1;
    }
    let mut grid = [[0.0f32; 13]; 13];
    let mut top = 0.0f64;
    for row in 0..13 {
        for col in 0..13 {
            if count[row][col] > 0 {
                sum[row][col] /= count[row][col] as f64;
                top = top.max(sum[row][col]);
            }
        }
    }
    for row in 0..13 {
        for col in 0..13 {
            if top > 0.0 {
                grid[row][col] = (sum[row][col] / top) as f32;
            }
        }
    }
    grid
}

/// The 169 starting hands, each lit by how likely it is; all dim without a
/// `grid`. The player's own hand is marked.
fn grid_lines(grid: Option<&Grid>, hero: [Card; 2]) -> Vec<Line<'static>> {
    const RANKS: [char; 13] = [
        'A', 'K', 'Q', 'J', 'T', '9', '8', '7', '6', '5', '4', '3', '2',
    ];
    let mine = grid_cell(hero);
    (0..13)
        .map(|row| {
            let spans: Vec<Span<'static>> = (0..13)
                .map(|col| {
                    let (high, low) = (RANKS[row.min(col)], RANKS[row.max(col)]);
                    let kind = match row.cmp(&col) {
                        std::cmp::Ordering::Less => 's',
                        std::cmp::Ordering::Greater => 'o',
                        std::cmp::Ordering::Equal => ' ',
                    };
                    let heat = grid.map_or(0.0, |g| g[row][col]) as f64;
                    let mix =
                        |from: u8, to: u8| (from as f64 + (to as f64 - from as f64) * heat) as u8;
                    let style = if (row, col) == mine {
                        HERO_CELL
                    } else {
                        Style::new()
                            .bg(Color::Rgb(mix(26, 226), mix(30, 140), mix(38, 50)))
                            .fg(if heat > 0.5 {
                                Color::Black
                            } else if heat > 0.04 {
                                Color::Rgb(190, 190, 190)
                            } else {
                                Color::Rgb(88, 92, 102)
                            })
                    };
                    Span::styled(format!("{high}{low}{kind}"), style)
                })
                .collect();
            Line::from(spans)
        })
        .collect()
}

/// A reviewed decision: the verdict, the spot, the options compared and,
/// with `points`, the facts that decided it.
fn review_card(review: &Review, width: usize, points: bool) -> Vec<Line<'static>> {
    let (spot, fb) = (&review.spot, &review.feedback);
    let style = grade_style(fb.grade);
    let mut verdict = format!("{} {}", grade_icon(fb.grade), fb.grade.label());
    if fb.loss > 0.0 {
        verdict.push_str(&format!(" · {} given up", fmt_bb(fb.loss, spot.big_blind)));
    }
    let mut lines = wrap_styled(None, &verdict, style, width);
    let against = if spot.read.is_some() {
        "vs its likely hands"
    } else {
        "vs a random hand"
    };
    lines.extend(wrap_styled(
        None,
        &format!("{} · {:.0}% {against}", headline(spot), spot.equity * 100.0),
        Style::new().fg(Color::Gray),
        width,
    ));
    if spot.ev(review.action).is_none() {
        // A size that was not priced: say what it was for.
        lines.extend(wrap_styled(None, &fb.text, Style::new(), width));
        return lines;
    }
    let ranked = spot.ranked(Some(review.action));
    let best = ranked[0].0;
    let mut rows: Vec<(Action, f64)> = ranked.iter().copied().take(3).collect();
    if let Some(&chosen) = ranked.iter().find(|&&(a, _)| a == review.action)
        && !rows.contains(&chosen)
    {
        rows.push(chosen);
    }
    let name_width = width.saturating_sub(2 + 10 + 7).max(8);
    for (action, ev) in rows {
        let chosen = action == review.action;
        let mut name = capitalise(&spot.phrase(action));
        if name.chars().count() > name_width {
            name = name.chars().take(name_width - 1).collect::<String>() + "…";
        }
        let row_style = if chosen {
            Style::new().bold()
        } else {
            Style::new().fg(Color::Gray)
        };
        let tag = match (action == best, chosen) {
            (true, true) => Span::styled("  ★ you", Style::new().fg(Color::LightGreen).bold()),
            (true, false) => Span::styled("  ★ best", Style::new().fg(GOLD).bold()),
            (false, true) => Span::styled("  you", style),
            (false, false) => Span::raw(""),
        };
        lines.push(Line::from(vec![
            Span::styled(if chosen { "▶ " } else { "  " }, style),
            Span::styled(format!("{name:<name_width$}"), row_style),
            Span::styled(
                format!("{:>9}", fmt_bb_signed(ev, spot.big_blind)),
                row_style,
            ),
            tag,
        ]));
    }
    if points {
        for point in why(spot, review.action) {
            lines.extend(wrap_styled(
                Some(("· ".into(), Style::new().fg(GOLD))),
                &point,
                Style::new(),
                width,
            ));
        }
    }
    lines
}

/// "Your play  ● ● ●   0.4bb given up": one dot per decision in the hand.
fn hand_line(grades: &[Grade], lost: f64, big_blind: u32) -> Line<'static> {
    let mut spans = vec![Span::styled("Your play  ", Style::new().fg(Color::Gray))];
    for &grade in grades {
        spans.push(Span::styled("● ", grade_style(grade)));
    }
    spans.push(Span::styled(
        if lost > 0.0 {
            format!("  {} given up", fmt_bb(lost, big_blind))
        } else {
            "  nothing given up".to_string()
        },
        Style::new().fg(Color::Gray),
    ));
    Line::from(spans)
}

/// The answers to a question, each behind its number key, wrapped to `width`.
fn choice_lines(choices: &[String], width: usize) -> Vec<Line<'static>> {
    let key = Style::new().fg(Color::Black).bg(GOLD).bold();
    let mut lines = Vec::new();
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut used = 0;
    for (i, choice) in choices.iter().enumerate() {
        let w = Span::raw(choice.as_str()).width() + 6;
        if used > 0 && used + w > width {
            lines.push(Line::from(std::mem::take(&mut spans)));
            used = 0;
        }
        spans.push(Span::styled(format!(" {} ", i + 1), key));
        spans.push(Span::raw(format!(" {choice}  ")));
        used += w;
    }
    if !spans.is_empty() {
        lines.push(Line::from(spans));
    }
    lines
}

fn action_keys<'a>(
    legal: Legal,
    big_blind: u32,
    key: &dyn Fn(&str) -> Span<'a>,
    sizes: bool,
) -> Vec<Span<'a>> {
    let mut spans = Vec::new();
    if legal.to_call > 0 {
        spans.extend([
            key("f"),
            " fold  ".into(),
            key("c"),
            format!(" call {}  ", bb(legal.to_call, big_blind)).into(),
        ]);
    } else {
        spans.extend([key("x"), " check  ".into()]);
    }
    if let Some((_, max)) = legal.raise {
        let verb = if legal.to_call > 0 {
            " raise  "
        } else {
            " bet  "
        };
        spans.extend([key("b"), verb.into()]);
        if sizes {
            spans.extend([key("1-5"), " ⅓ ½ ¾ pot all-in  ".into()]);
        }
        spans.extend([key("a"), format!(" all-in {}  ", bb(max, big_blind)).into()]);
    }
    spans
}

/// Word-wraps `text` to `width` columns, after an optional styled label on
/// the first line. Words longer than a line are split.
fn wrap_styled(
    head: Option<(String, Style)>,
    text: &str,
    style: Style,
    width: usize,
) -> Vec<Line<'static>> {
    let width = width.max(1);
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut used = 0;
    // Whether the next word needs a space before it (not after a label).
    let mut gap_due = false;
    if let Some((label, label_style)) = head {
        used = Span::raw(label.as_str()).width();
        spans.push(Span::styled(label, label_style));
    }
    for (p, paragraph) in text.split('\n').enumerate() {
        if p > 0 {
            lines.push(Line::from(std::mem::take(&mut spans)));
            used = 0;
            gap_due = false;
        }
        for word in paragraph.split_whitespace() {
            let mut word = word.to_string();
            loop {
                let w = Span::raw(word.as_str()).width();
                let gap = usize::from(gap_due);
                if used + gap + w <= width {
                    if gap == 1 {
                        spans.push(Span::styled(" ", style));
                    }
                    spans.push(Span::styled(word, style));
                    used += gap + w;
                    gap_due = true;
                    break;
                }
                if used > 0 {
                    lines.push(Line::from(std::mem::take(&mut spans)));
                    used = 0;
                    gap_due = false;
                    continue;
                }
                // A word wider than the whole line: split it.
                let cut: String = word.chars().take(width).collect();
                let rest: String = word.chars().skip(width).collect();
                lines.push(Line::from(Span::styled(cut, style)));
                word = rest;
                if word.is_empty() {
                    break;
                }
            }
        }
    }
    if !spans.is_empty() {
        lines.push(Line::from(spans));
    }
    lines
}

/// "── End of hand 3 · you won 2bb ─────────"
fn divider(text: &str, width: usize) -> Line<'static> {
    let head = format!("── {text} ");
    let fill = width.saturating_sub(Span::raw(head.as_str()).width());
    Line::from(Span::styled(
        format!("{head}{}", "─".repeat(fill)),
        Style::new()
            .fg(GOLD)
            .add_modifier(ratatui::style::Modifier::DIM),
    ))
}

fn grade_style(grade: Grade) -> Style {
    let color = match grade {
        Grade::Good => Color::LightGreen,
        Grade::Close | Grade::Note => Color::LightCyan,
        Grade::Inaccuracy => Color::Yellow,
        Grade::Mistake => Color::LightRed,
        Grade::Blunder => Color::Red,
    };
    Style::new().fg(color).bold()
}

fn seat_label(
    name: &str,
    position: &str,
    stack: u32,
    big_blind: u32,
    acting: bool,
) -> Line<'static> {
    let marker = if acting { "▶ " } else { "  " };
    let name_style = if acting {
        Style::new().fg(Color::Black).bg(GOLD).bold()
    } else {
        Style::new().fg(Color::White).bold()
    };
    Line::from(vec![
        marker.fg(GOLD),
        Span::styled(format!(" {name} "), name_style),
        format!("  {position}  ·  ").fg(Color::Gray),
        bb(stack, big_blind).bold(),
    ])
}

fn bet_chip(chips: u32, big_blind: u32) -> Line<'static> {
    if chips == 0 {
        Line::default()
    } else {
        Line::from(vec!["● ".fg(GOLD), bb(chips, big_blind).bold().fg(GOLD)])
    }
}

fn suit_color(card: Card) -> Color {
    match card.suit() {
        0 => Color::Rgb(30, 140, 60),
        1 => Color::Rgb(40, 90, 210),
        2 => Color::Rgb(210, 40, 40),
        _ => Color::Rgb(20, 20, 20),
    }
}

/// Cards as boxes, side by side, with `empty` blank slots after them:
/// four columns by three rows, or seven by five when `big`.
fn card_row(cards: &[Card], empty: usize, big: bool) -> Vec<Line<'static>> {
    let face = Style::new().bg(Color::Rgb(245, 243, 236));
    let (wide, tall) = if big { (7, 5) } else { (4, 3) };
    let mut rows: Vec<Vec<Span<'static>>> = vec![Vec::new(); tall];
    for &card in cards {
        let text = card.pretty(false);
        let (rank, suit) = text.split_at(1);
        let ink = face.fg(suit_color(card)).bold();
        for (i, row) in rows.iter_mut().enumerate() {
            let cell = if i == 0 {
                format!("{rank:<wide$}")
            } else if i == tall - 1 {
                format!("{rank:>wide$}")
            } else if i == tall / 2 {
                format!("{suit:^wide$}")
            } else {
                " ".repeat(wide)
            };
            row.push(Span::styled(cell, ink));
            row.push(Span::raw(" "));
        }
    }
    let slot = Style::new().fg(Color::Rgb(60, 130, 90));
    for _ in 0..empty {
        for (i, row) in rows.iter_mut().enumerate() {
            let cell = if i == 0 {
                format!("┌{}┐", "─".repeat(wide - 2))
            } else if i == tall - 1 {
                format!("└{}┘", "─".repeat(wide - 2))
            } else {
                format!("│{}│", " ".repeat(wide - 2))
            };
            row.push(Span::styled(cell, slot));
            row.push(Span::raw(" "));
        }
    }
    rows.into_iter().map(Line::from).collect()
}

fn card_backs(n: usize, big: bool) -> Vec<Line<'static>> {
    let back = Style::new()
        .fg(Color::Rgb(150, 40, 50))
        .bg(Color::Rgb(90, 20, 30));
    let (wide, tall) = if big { (7, 5) } else { (4, 3) };
    (0..tall)
        .map(|_| {
            let mut spans = Vec::new();
            for _ in 0..n {
                spans.push(Span::styled("╳".repeat(wide), back));
                spans.push(Span::raw(" "));
            }
            Line::from(spans)
        })
        .collect()
}

fn cards_text(cards: &[Card]) -> String {
    cards
        .iter()
        .map(|c| c.pretty(false))
        .collect::<Vec<_>>()
        .join(" ")
}

/// What a range is made of, as a stacked bar `width` cells long with its
/// legend beneath.
fn range_bar(read: &Profile, width: usize) -> Vec<Line<'static>> {
    let parts = [
        ("strong", read.strong, Color::Rgb(210, 70, 70)),
        ("medium", read.medium, Color::Rgb(220, 160, 60)),
        ("draws", read.draws, Color::Rgb(90, 150, 220)),
        ("weak", read.weak, Color::Rgb(120, 120, 120)),
    ];
    let cells = width.clamp(10, 48);
    let mut bar = Vec::new();
    let mut used = 0;
    for (i, (_, share, color)) in parts.iter().enumerate() {
        // The last part takes what is left, so rounding never changes the length.
        let n = if i == parts.len() - 1 {
            cells - used
        } else {
            ((share * cells as f64).round() as usize).min(cells - used)
        };
        used += n;
        bar.push(Span::styled("█".repeat(n), Style::new().fg(*color)));
    }
    let mut legend = Vec::new();
    for (name, share, color) in parts {
        legend.push(Span::styled(
            format!("{name} {:.0}%  ", share * 100.0),
            Style::new().fg(color),
        ));
    }
    vec![Line::from(bar), Line::from(legend)]
}

fn popup_area(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

fn draw_popup(frame: &mut Frame, area: Rect, title: &str, body: &str) {
    let rect = popup_area(area, 48, 5);
    frame.render_widget(Clear, rect);
    let block = Block::new()
        .title(format!(" {title} ").bold().fg(GOLD))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    frame.render_widget(
        Paragraph::new(body.to_string())
            .block(block)
            .alignment(Alignment::Center),
        rect,
    );
}

fn draw_help(frame: &mut Frame, area: Rect) {
    let rows = [
        ("f / c / x", "fold, call, check"),
        ("b or r", "bet or raise: choose a size, then enter"),
        (
            "1 2 3 4 5",
            "bet a third, half, three quarters, the pot, all-in",
        ),
        ("← →  ↑ ↓", "adjust the size; or type an amount in bb"),
        ("a", "all-in"),
        ("/  or  tab", "ask the coach anything"),
        (
            "1-6",
            "answer the coach's question; esc skips it, or just act",
        ),
        ("n", "show or hide your equity, the price and the best play"),
        ("d", "open or close the last review in full"),
        ("PgUp PgDn", "scroll the coach"),
        ("space", "next hand"),
        ("q", "leave the table"),
    ];
    let mut lines = vec![Line::default()];
    for (keys, what) in rows {
        lines.push(Line::from(vec![
            format!("  {keys:<12}").bold().fg(GOLD),
            Span::raw(what),
        ]));
    }
    lines.push(Line::default());
    lines.push(Line::from(
        "  The coach asks one question before you act and prices every option after.".dim(),
    ));
    lines.push(Line::from("  Press any key to close.".dim()));
    let rect = popup_area(area, 64, lines.len() as u16 + 2);
    frame.render_widget(Clear, rect);
    let block = Block::new()
        .title(" Keys ".bold().fg(GOLD))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    frame.render_widget(Paragraph::new(lines).block(block), rect);
}

/// Runs the table full-screen until the player leaves.
pub fn run(game: Game, mode: CoachMode, llm: Option<Llm>) -> io::Result<()> {
    let mut terminal = ratatui::init();
    let result = App::new(game, mode, llm).run(&mut terminal);
    ratatui::restore();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use poker::bot::SolidBot;
    use poker::game::{Config, Street};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn press(app: &mut App, code: KeyCode) {
        app.key(KeyEvent::new(code, KeyModifiers::NONE)).unwrap();
    }

    fn screen(app: &App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| app.draw(f)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        let mut out = String::new();
        for y in 0..height {
            for x in 0..width {
                out.push_str(buffer[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    fn play_app(mode: CoachMode) -> App {
        let path = std::env::temp_dir().join(format!(
            "poker-tui-test-{}-{:?}.jsonl",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_file(&path);
        let session = Session::new(Config::default(), Box::new(SolidBot::new(5)), 5);
        App::new(
            Game::Play {
                session: Box::new(session),
                log_file: path,
                hands: Some(3),
            },
            mode,
            None,
        )
    }

    /// Drives the app until the player must act or the session ends.
    fn settle(app: &mut App) {
        for _ in 0..50 {
            app.bot_due = Some(Instant::now());
            app.tick().unwrap();
            if app.hero_to_act() || app.over || app.hand_done {
                break;
            }
        }
    }

    #[test]
    fn plays_a_session_with_keys_and_draws_every_screen() {
        let mut app = play_app(CoachMode::Teach);
        for _ in 0..200 {
            settle(&mut app);
            for size in [(140, 44), (90, 40)] {
                let text = screen(&app, size.0, size.1);
                assert!(text.contains("poker"));
            }
            if app.over {
                break;
            }
            if app.hand_done {
                press(&mut app, KeyCode::Char(' '));
                continue;
            }
            // Answer the coach's question with one key, then act.
            if app.quiz.is_some() {
                assert!(screen(&app, 140, 44).contains("answer the coach"));
                press(&mut app, KeyCode::Char('1'));
                assert!(app.quiz.is_none());
            }
            let legal = app.state().legal().unwrap();
            if legal.raise.is_some() && app.state().street() == Street::Flop {
                press(&mut app, KeyCode::Char('2'));
                assert!(screen(&app, 140, 44).contains("pot"));
                press(&mut app, KeyCode::Right);
                press(&mut app, KeyCode::Enter);
            } else {
                press(&mut app, KeyCode::Char('c'));
            }
        }
        assert!(app.over, "three hands should finish");
        assert!(
            app.coach.iter().any(|l| l.grade.is_some()),
            "decisions were reviewed"
        );
        let text = screen(&app, 140, 44);
        assert!(text.contains("SESSION REPORT"), "{text}");
        assert!(app.skills.iter().any(|s| s.0 > 0), "questions were asked");
    }

    #[test]
    fn a_review_shows_the_options_compared() {
        let mut app = play_app(CoachMode::After);
        settle(&mut app);
        press(&mut app, KeyCode::Char('a'));
        let text = screen(&app, 140, 44);
        assert!(text.contains("you"), "{text}");
        assert!(text.contains("★"), "{text}");
        assert!(
            app.coach
                .iter()
                .any(|l| matches!(l.extra, Extra::Review(..))),
            "no review card"
        );
    }

    #[test]
    fn a_mystery_opponent_is_named_only_at_the_end() {
        let mut app = play_app(CoachMode::After);
        if let Game::Play { session, .. } = &mut app.game {
            session.hide_opponent();
        }
        settle(&mut app);
        let text = screen(&app, 140, 44);
        assert!(
            text.contains("MYSTERY") && !text.contains("solid"),
            "{text}"
        );
        press(&mut app, KeyCode::Char('q'));
        press(&mut app, KeyCode::Char('y'));
        let text = screen(&app, 140, 60);
        assert!(text.contains("YOUR OPPONENT WAS: SOLID"), "{text}");
    }

    #[test]
    fn the_newest_coach_message_is_always_in_view() {
        let mut app = play_app(CoachMode::After);
        let long = "word ".repeat(40);
        for i in 0..12 {
            app.say(Speaker::Coach, format!("{long}marker{i}"), None);
        }
        // Long words wrap early: a character count would undercount the lines.
        app.say(
            Speaker::Coach,
            "supercalifragilistic expialidocious ".repeat(8) + "LASTWORD",
            None,
        );
        for (w, h) in [(140, 44), (90, 30), (112, 24)] {
            let text = screen(&app, w, h);
            assert!(text.contains("LASTWORD"), "{w}x{h}:\n{text}");
        }
    }

    #[test]
    fn wrapping_is_exact() {
        let lines = wrap_styled(
            Some(("Coach ".into(), Style::new())),
            "one two three\nfour",
            Style::new(),
            9,
        );
        let text: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        assert_eq!(text, ["Coach one", "two three", "four"]);
        assert!(lines.iter().all(|l| l.width() <= 9));
        let split = wrap_styled(None, "abcdefghijk", Style::new(), 4);
        assert_eq!(split.len(), 3);
    }

    #[test]
    fn the_end_of_each_hand_is_marked_in_the_coach_panel() {
        let mut app = play_app(CoachMode::After);
        for _ in 0..200 {
            settle(&mut app);
            if app.hand_done || app.over {
                break;
            }
            press(&mut app, KeyCode::Char('f'));
            press(&mut app, KeyCode::Char('x'));
        }
        app.tick().unwrap();
        assert!(
            app.coach
                .iter()
                .any(|l| l.who == Speaker::Divider && l.text.starts_with("End of hand 1")),
            "no end-of-hand divider"
        );
        assert!(screen(&app, 140, 44).contains("── End of hand 1"));
    }

    #[test]
    fn a_wide_screen_shows_the_range_the_session_and_earlier_hands() {
        let mut app = play_app(CoachMode::After);
        settle(&mut app);
        press(&mut app, KeyCode::Char('a'));
        for (w, h) in [(250, 55), (180, 40), (170, 30), (140, 48)] {
            let text = screen(&app, w, h);
            assert!(text.contains("Coach") && text.contains("Hand"), "{w}x{h}");
            if h >= 34 {
                assert!(text.contains("Their likely hands"), "{w}x{h}:\n{text}");
                assert!(text.contains("AKs") && text.contains("72o"), "{w}x{h}");
            }
        }
        let text = screen(&app, 250, 55);
        assert!(text.contains("Session") && text.contains("Earlier hands"));
    }

    #[test]
    fn the_last_review_opens_and_closes() {
        let mut app = play_app(CoachMode::After);
        settle(&mut app);
        press(&mut app, KeyCode::Char('a'));
        let detail = |app: &App| {
            app.coach.iter().rev().find_map(|l| match l.extra {
                Extra::Review(_, detail) => Some(detail),
                _ => None,
            })
        };
        assert!(detail(&app) == Some(Detail::Full));
        press(&mut app, KeyCode::Char('d'));
        assert!(detail(&app) == Some(Detail::Opened));
        press(&mut app, KeyCode::Char('d'));
        assert!(matches!(detail(&app), Some(Detail::Brief(_))));
        assert!(screen(&app, 140, 44).contains("next:") || screen(&app, 140, 44).contains("best:"));
    }

    #[test]
    fn questions_thin_out_as_they_are_answered_right() {
        let asked = |streak: u32| {
            let mut app = play_app(CoachMode::Teach);
            settle(&mut app);
            app.skills = [(8, 8); 5];
            app.streaks = [streak; 5];
            let spot = app.analyse_quietly();
            let mut count = 0;
            for _ in 0..200 {
                app.quiz = None;
                app.asked.clear();
                app.ask_question(&spot);
                count += u32::from(app.quiz.is_some());
            }
            count
        };
        let (fresh, practised) = (asked(0), asked(6));
        assert_eq!(fresh, 200);
        assert!(practised < 90, "{practised}");
    }

    #[test]
    fn a_mystery_opponent_is_guessed_at() {
        let mut app = play_app(CoachMode::After);
        if let Game::Play { session, hands, .. } = &mut app.game {
            session.hide_opponent();
            *hands = None;
        }
        for _ in 0..2000 {
            settle(&mut app);
            if app.quiz.as_ref().is_some_and(|q| q.question.is_none()) {
                break;
            }
            press(&mut app, KeyCode::Char('f'));
            press(&mut app, KeyCode::Char('x'));
            press(&mut app, KeyCode::Char(' '));
        }
        assert!(
            app.coach
                .iter()
                .any(|l| l.text.contains("What kind of player")),
            "never asked"
        );
        press(&mut app, KeyCode::Char('3'));
        assert_eq!(app.guess, Some(2));
        assert!(app.quiz.is_none());
        assert!(screen(&app, 250, 55).contains("Your guess: nit"));
    }

    #[test]
    fn help_and_quit_prompts() {
        let mut app = play_app(CoachMode::Full);
        settle(&mut app);
        press(&mut app, KeyCode::Char('?'));
        assert!(screen(&app, 120, 40).contains("Keys"));
        press(&mut app, KeyCode::Char('x'));
        assert!(!app.show_help);
        press(&mut app, KeyCode::Char('q'));
        assert!(screen(&app, 120, 40).contains("Leave the table?"));
        press(&mut app, KeyCode::Char('y'));
        assert!(app.over);
    }
}
