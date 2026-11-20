//! The full-screen table: cards, stacks and pot on a felt, an action log, a
//! coach that talks with you, and one-key actions.

use super::coach_ai::{Ask, Chat, SYSTEM};
use super::llm::{Llm, Message};
use super::text::{bb, bb_signed, describe, narrate, position};
use super::{CoachMode, facts, session_facts};
use poker::analysis::{Decision, analyse_with};
use poker::bot::by_name;
use poker::cards::Card;
use poker::coach::{Grade, Question, Spot, questions, review};
use poker::game::{Action, HandState, Legal};
use poker::history;
use poker::range::{Profile, Strengths};
use poker::session::{HERO, Review, Session};
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
}

#[derive(PartialEq)]
enum InputPurpose {
    /// Answering the coach's question.
    Answer,
    /// Asking the coach something.
    Ask,
}

struct Input {
    purpose: InputPurpose,
    text: String,
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
    /// Questions still to ask, without a language model.
    script: VecDeque<Question>,
    answers: u32,
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
    /// A divider to add once the coach has finished speaking.
    pending_divider: Option<String>,
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
            script: VecDeque::new(),
            answers: 0,
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
            (Some(llm), _) => format!("Coach: {}. / to ask, ? for keys.", llm.model()),
            (None, _) => {
                "Coach: engine only (set ANTHROPIC_API_KEY for Claude). ? for keys.".to_string()
            }
        };
        app.say(Speaker::System, intro, None);
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
        self.answers = 0;
        self.script.clear();
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
        if self.mode == CoachMode::Off {
            self.spot = Some(self.analyse_quietly());
            return;
        }
        let spot = match &self.game {
            Game::Play { session, .. } => session.analyse(),
            Game::Drill { spots, index, .. } => spots[*index].spot.clone(),
        };
        self.answers = 0;
        if self.mode == CoachMode::Teach {
            if self.llm.is_some() {
                let facts = self.facts(&spot, None);
                self.queue.push_back((Ask::Question, Some(facts), None));
                self.input = Some(Input {
                    purpose: InputPurpose::Answer,
                    text: String::new(),
                });
            } else {
                self.script = questions(&spot).into();
                self.next_scripted_question(&spot);
            }
        }
        self.spot = Some(spot);
    }

    /// Without a coach the spot is still needed for sizing hints.
    fn analyse_quietly(&self) -> Spot {
        match &self.game {
            Game::Play { session, .. } => session.analyse(),
            Game::Drill { spots, index, .. } => spots[*index].spot.clone(),
        }
    }

    fn next_scripted_question(&mut self, spot: &Spot) {
        if let Some(q) = self.script.front() {
            let prompt = q.prompt(spot);
            self.say(Speaker::Coach, prompt, None);
            self.input = Some(Input {
                purpose: InputPurpose::Answer,
                text: String::new(),
            });
        } else {
            self.say(Speaker::Coach, "Decide.".into(), None);
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
        self.script.clear();
        let before = self.state().clone();
        let coached = self.mode != CoachMode::Off;
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

    fn show_review(&mut self, review: &Review, before: &HandState) {
        let big_blind = before.config().big_blind;
        let fb = &review.feedback;
        let headline = if fb.loss > 0.0 {
            format!(
                "{} · about {} given up",
                fb.grade.label(),
                poker::coach::fmt_bb(fb.loss, big_blind)
            )
        } else {
            fb.grade.label().to_string()
        };
        self.say(Speaker::System, headline, Some(fb.grade));
        if self.llm.is_some() {
            let facts = self.facts(&review.spot, Some(review.action));
            self.queue
                .push_back((Ask::Review, Some(facts), Some(fb.text.clone())));
        } else {
            self.say(Speaker::Coach, fb.text.clone(), None);
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
        self.pending_divider = Some(format!("End of hand {} · {summary}", session.hand_no()));
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
                self.pending_divider = Some(format!("End of spot {}", *index + 1));
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
        let summary = match &self.game {
            Game::Play { session, .. } => {
                let big_blind = session.config().big_blind;
                let mut text = format!(
                    "Session over: {} over {} hands.",
                    bb_signed(session.total(), big_blind),
                    session.finished()
                );
                if let Some(top) = session.tally().lines().first() {
                    text.push_str(&format!(
                        " Your costliest leak: {} on the {} ({} times, about {}). {}",
                        top.leak.label(),
                        top.street.name().to_lowercase(),
                        top.count,
                        poker::coach::fmt_bb(top.loss, big_blind),
                        top.leak.tip()
                    ));
                }
                if self.llm.is_some() && self.mode != CoachMode::Off {
                    let facts = session_facts(session);
                    self.queue.push_back((Ask::Summary, Some(facts), None));
                }
                text
            }
            Game::Drill { spots, correct, .. } => format!(
                "Drill over: {correct} of {} spots played well.",
                spots.len()
            ),
        };
        self.say(Speaker::System, summary + " Press q to leave.", None);
    }

    // ---- coach plumbing ---------------------------------------------------

    fn say(&mut self, who: Speaker, text: String, grade: Option<Grade>) {
        self.coach.push(CoachLine { who, text, grade });
        self.coach_scroll = 0;
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
            && let Some(text) = self.pending_divider.take()
        {
            self.say(Speaker::Divider, text, None);
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
        let mut input = input;
        // A leading "/" asks the coach something rather than answering it.
        if let Some(question) = input.text.trim().strip_prefix('/') {
            input = Input {
                purpose: InputPurpose::Ask,
                text: question.to_string(),
            };
        }
        let text = input.text.trim().to_string();
        if text.is_empty() {
            if input.purpose == InputPurpose::Answer {
                self.skip_questions();
            }
            return;
        }
        self.say(Speaker::You, text.clone(), None);
        match input.purpose {
            InputPurpose::Ask => {
                if self.llm.is_some() {
                    let facts = self
                        .spot
                        .as_ref()
                        .or(self.last_review.as_ref().map(|r| &r.spot))
                        .map(|spot| {
                            let action = self
                                .spot
                                .is_none()
                                .then(|| self.last_review.as_ref().map(|r| r.action))
                                .flatten();
                            self.facts(spot, action)
                        });
                    self.queue.push_back((Ask::Free(text), facts, None));
                } else {
                    self.say(
                        Speaker::System,
                        "Questions need the language-model coach: set ANTHROPIC_API_KEY and restart.".into(),
                        None,
                    );
                }
            }
            InputPurpose::Answer => {
                self.answers += 1;
                if self.llm.is_some() {
                    self.queue.push_back((Ask::Answer(text), None, None));
                    if self.answers < 2 {
                        self.input = Some(Input {
                            purpose: InputPurpose::Answer,
                            text: String::new(),
                        });
                    }
                } else if let (Some(q), Some(spot)) =
                    (self.script.front().cloned(), self.spot.clone())
                {
                    match q.check(&spot, &text) {
                        Ok(feedback) => {
                            self.script.pop_front();
                            self.say(Speaker::Coach, feedback.text, Some(feedback.grade));
                            self.next_scripted_question(&spot);
                        }
                        Err(hint) => {
                            self.say(Speaker::System, hint, None);
                            self.input = Some(Input {
                                purpose: InputPurpose::Answer,
                                text: String::new(),
                            });
                        }
                    }
                }
            }
        }
    }

    fn skip_questions(&mut self) {
        self.input = None;
        if !self.script.is_empty() {
            self.script.clear();
            self.say(Speaker::System, "Skipped. Decide.".into(), None);
        }
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
                KeyCode::Esc => {
                    let answering = input.purpose == InputPurpose::Answer;
                    self.input = None;
                    if answering {
                        self.skip_questions();
                    }
                }
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
                _ => {}
            }
            return Ok(());
        }
        let legal = self.state().legal().filter(|_| self.hero_to_act());
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.confirm_quit = true,
            KeyCode::Char('?') | KeyCode::Char('h') => self.show_help = true,
            KeyCode::Char('/') | KeyCode::Char('t') | KeyCode::Tab => {
                if self.mode != CoachMode::Off {
                    self.input = Some(Input {
                        purpose: InputPurpose::Ask,
                        text: String::new(),
                    });
                }
            }
            KeyCode::Char('n') => self.show_numbers = !self.show_numbers,
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
        let Some(legal) = state.legal() else {
            return Vec::new();
        };
        let Some((min, max)) = legal.raise else {
            return Vec::new();
        };
        let opp = state.street_commit(1 - HERO) as f64;
        let base = (state.pot() + legal.to_call) as f64;
        let mut sizes: Vec<u32> = [1.0 / 3.0, 0.5, 0.75, 1.0]
            .iter()
            .map(|share| ((opp + base * share).round() as u32).clamp(min, max))
            .collect();
        sizes.push(max);
        sizes
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

        let wide = body.width >= 110;
        let (table_area, side) = if wide {
            let [a, b] =
                Layout::horizontal([Constraint::Min(60), Constraint::Length(body.width * 2 / 5)])
                    .areas(body);
            (a, b)
        } else {
            let [a, b] =
                Layout::vertical([Constraint::Min(18), Constraint::Length(body.height / 3)])
                    .areas(body);
            (a, b)
        };
        let [felt, log] =
            Layout::vertical([Constraint::Min(15), Constraint::Length(7)]).areas(table_area);
        self.draw_felt(frame, felt);
        self.draw_log(frame, log);
        self.draw_coach(frame, side);
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
        let (left, right) = match &self.game {
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
                (
                    vec![
                        " ♠ poker ".bold().fg(Color::Black).bg(GOLD),
                        format!(
                            "  Hand {}  ·  vs {}  ·  ",
                            session.hand_no(),
                            session.bot_name()
                        )
                        .into(),
                        format!("session {}", bb_signed(total, big_blind))
                            .bold()
                            .fg(color),
                    ],
                    format!("seed {}  ", session.seed()),
                )
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
        let [l, r] =
            Layout::horizontal([Constraint::Min(10), Constraint::Length(right.len() as u16)])
                .areas(area);
        frame.render_widget(Paragraph::new(Line::from(left)), l);
        frame.render_widget(Paragraph::new(right.dim()), r);
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
        let numbers_height = if self.show_numbers { 4 } else { 0 };
        let [bot, middle, hero, numbers] = Layout::vertical([
            Constraint::Length(6),
            Constraint::Min(5),
            Constraint::Length(6),
            Constraint::Length(numbers_height),
        ])
        .areas(inner);

        // The bot's seat.
        let bot_seat = 1 - HERO;
        let bot_cards = if showdown {
            card_row(&state.hole(bot_seat), 0)
        } else {
            card_backs(2)
        };
        let mut lines = vec![seat_label(
            "BOT",
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
        let mut lines = card_row(state.board(), 5 - state.board().len());
        lines.push(Line::from(vec![
            "Pot ".fg(Color::Gray),
            bb(state.pot(), big_blind).bold().fg(GOLD),
            format!("   ·   {}", state.street().name()).fg(Color::Gray),
        ]));
        let pad = middle.height.saturating_sub(lines.len() as u16) / 2;
        let [_, mid] =
            Layout::vertical([Constraint::Length(pad), Constraint::Min(1)]).areas(middle);
        frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), mid);

        // The player's seat.
        let hole = state.hole(HERO);
        let mut lines = vec![bet_chip(state.street_commit(HERO), big_blind)];
        lines.extend(card_row(&hole, 0));
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

        if self.show_numbers {
            self.draw_numbers(frame, numbers);
        }
    }

    /// Pot odds, equity, the coach's pick and the bot's likely hands. In teach
    /// mode these appear only after you decide, so they don't give answers away.
    fn draw_numbers(&self, frame: &mut Frame, area: Rect) {
        let spot = if self.mode == CoachMode::Teach || self.mode == CoachMode::After {
            self.last_review.as_ref().map(|r| &r.spot)
        } else {
            self.spot
                .as_ref()
                .or(self.last_review.as_ref().map(|r| &r.spot))
        };
        let Some(spot) = spot else {
            let hint = if self.mode == CoachMode::Teach {
                "The numbers appear once you decide."
            } else {
                ""
            };
            frame.render_widget(
                Paragraph::new(hint.dim()).alignment(Alignment::Center),
                area,
            );
            return;
        };
        let big_blind = spot.big_blind;
        let mut first = vec![
            "Equity ".fg(Color::Gray),
            format!("{:.0}%", spot.equity * 100.0).bold(),
        ];
        if spot.to_call > 0 {
            first.push("   Pot odds ".fg(Color::Gray));
            first.push(format!("{:.0}%", spot.pot_odds() * 100.0).bold());
        }
        if let Some(&(best, ev)) = spot.ranked(None).first() {
            first.push("   Best ".fg(Color::Gray));
            first.push(
                format!(
                    "{} ({})",
                    spot.phrase(best),
                    poker::coach::fmt_bb_signed(ev, big_blind)
                )
                .bold()
                .fg(GOLD),
            );
        }
        let mut lines = vec![Line::from(first)];
        match spot.read {
            Some(read) => lines.push(range_bar(&read, area.width.saturating_sub(4) as usize)),
            None => lines.push(Line::from(
                "The bot's actions say nothing about its cards yet.".fg(Color::Gray),
            )),
        }
        frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
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
                    lines.push(Line::default());
                    continue;
                }
            };
            let (head, body) = if label.is_empty() {
                (None, style)
            } else {
                (Some((format!("{label} "), style)), Style::new())
            };
            lines.extend(wrap_styled(head, &line.text, body, width));
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
            let label = match input.purpose {
                InputPurpose::Answer => "Your answer",
                InputPurpose::Ask => "Ask the coach",
            };
            line.push(format!("{label}: ").bold().fg(GOLD));
            line.push(Span::raw(input.text.clone()));
            line.push("▏".fg(GOLD));
            line.push("   enter: send   esc: ".dim());
            line.push(if input.purpose == InputPurpose::Answer {
                "skip to acting".dim()
            } else {
                "cancel".dim()
            });
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
            line.extend(action_keys(legal, big_blind, &key));
            if self.mode != CoachMode::Off {
                line.extend([key("/"), " ask coach  ".into()]);
            }
            line.extend([key("n"), " numbers  ".into(), key("?"), " help".into()]);
        } else if self.hero_to_act() || self.hand_done || self.last_review.is_some() {
            line.extend([
                key("space"),
                " next  ".into(),
                key("/"),
                " ask coach  ".into(),
                key("q"),
                " quit".into(),
            ]);
        } else {
            line.push("The bot is thinking…".italic().fg(Color::Gray));
        }
        frame.render_widget(Paragraph::new(Line::from(line)), inner);
    }
}

fn action_keys<'a>(legal: Legal, big_blind: u32, key: &dyn Fn(&str) -> Span<'a>) -> Vec<Span<'a>> {
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
        spans.extend([
            key("b"),
            " bet/raise  ".into(),
            key("1-5"),
            " sizes  ".into(),
            key("a"),
            format!(" all-in {}  ", bb(max, big_blind)).into(),
        ]);
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

/// Cards as small boxes, side by side, with `empty` blank slots after them.
fn card_row(cards: &[Card], empty: usize) -> Vec<Line<'static>> {
    let face = Style::new().bg(Color::Rgb(245, 243, 236));
    let mut rows = vec![Vec::new(), Vec::new(), Vec::new()];
    for &card in cards {
        let text = card.pretty(false);
        let (rank, suit) = text.split_at(1);
        let ink = face.fg(suit_color(card)).bold();
        rows[0].push(Span::styled(format!("{rank}   "), ink));
        rows[1].push(Span::styled(format!(" {suit}  "), ink));
        rows[2].push(Span::styled(format!("   {rank}"), ink));
        for row in &mut rows {
            row.push(Span::raw(" "));
        }
    }
    let slot = Style::new().fg(Color::Rgb(60, 130, 90));
    for _ in 0..empty {
        rows[0].push(Span::styled("┌──┐", slot));
        rows[1].push(Span::styled("│  │", slot));
        rows[2].push(Span::styled("└──┘", slot));
        for row in &mut rows {
            row.push(Span::raw(" "));
        }
    }
    rows.into_iter().map(Line::from).collect()
}

fn card_backs(n: usize) -> Vec<Line<'static>> {
    let back = Style::new()
        .fg(Color::Rgb(150, 40, 50))
        .bg(Color::Rgb(90, 20, 30));
    (0..3)
        .map(|_| {
            let mut spans = Vec::new();
            for _ in 0..n {
                spans.push(Span::styled("╳╳╳╳", back));
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

/// The bot's likely hands as a stacked bar.
fn range_bar(read: &Profile, width: usize) -> Line<'static> {
    let width = width.clamp(20, 60);
    let parts = [
        ("strong", read.strong, Color::Rgb(210, 70, 70)),
        ("medium", read.medium, Color::Rgb(220, 160, 60)),
        ("draws", read.draws, Color::Rgb(90, 150, 220)),
        ("weak", read.weak, Color::Rgb(120, 120, 120)),
    ];
    let mut spans = vec!["Bot's likely hands ".fg(Color::Gray)];
    for (_, share, color) in parts {
        let cells = (share * width as f64).round() as usize;
        spans.push(Span::styled("█".repeat(cells), Style::new().fg(color)));
    }
    spans.push(" ".into());
    for (name, share, color) in parts {
        spans.push(Span::styled(
            format!(" {name} {:.0}%", share * 100.0),
            Style::new().fg(color),
        ));
    }
    Line::from(spans)
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
        ("esc", "skip the coach's questions and act"),
        ("n", "show or hide the numbers"),
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
        "  In teach mode the coach asks before you act and reviews after.".dim(),
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
            // Answer the first scripted question, skip the rest, then act.
            if app.input.is_some() {
                for c in "50".chars() {
                    press(&mut app, KeyCode::Char(c));
                }
                press(&mut app, KeyCode::Enter);
                press(&mut app, KeyCode::Esc);
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
        assert!(text.contains("Session over"), "{text}");
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
