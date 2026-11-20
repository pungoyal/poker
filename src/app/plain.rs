//! The line-by-line front end: for piped input, transcripts and terminals
//! that cannot show the full-screen table. Also the `replay` and `leaks`
//! reports.

use super::coach_ai::{Ask, Chat, SYSTEM};
use super::llm::Llm;
use super::text::{bb, bb_signed, describe, narrate, position};
use super::{CoachMode, facts, session_facts};
use poker::analysis::{Decision, LeakTally, analyse_with, decisions};
use poker::bot::by_name;
use poker::cards::Card;
use poker::coach::{self, Feedback, Grade, Spot, questions};
use poker::game::{Action, HandState, Legal, Street};
use poker::history::{self, HandRecord};
use poker::range::Strengths;
use poker::session::{HERO, Session};
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::Path;

const HELP: &str = "\
Commands (amounts are in big blinds):
  f            fold
  x            check
  c            call
  b 2.5        bet or raise to 2.5bb in total on this street (also: r 2.5)
  b 75%        bet 75% of the pot (when raising: of the pot after calling)
  a            all-in
  ? question   ask the coach something
  q            quit";

/// Formats and prints with knowledge of the table's stakes.
pub struct Printer {
    pub big_blind: u32,
    pub color: bool,
}

impl Printer {
    pub fn new(big_blind: u32) -> Printer {
        Printer {
            big_blind,
            color: io::stdout().is_terminal(),
        }
    }

    fn bb(&self, chips: u32) -> String {
        bb(chips, self.big_blind)
    }

    fn paint(&self, code: &str, text: &str) -> String {
        if self.color {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    fn dim(&self, text: &str) -> String {
        self.paint("2", text)
    }

    fn cards(&self, cards: &[Card]) -> String {
        cards
            .iter()
            .map(|c| c.pretty(self.color))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Labelled groups of cards drawn side by side as small boxes.
    fn card_art(&self, groups: &[(&str, &[Card])]) -> String {
        let mut rows = [String::new(), String::new(), String::new(), String::new()];
        for (label, cards) in groups.iter().filter(|(_, cards)| !cards.is_empty()) {
            let width = cards.len() * 6;
            rows[0] += &format!(" {}  ", self.dim(&format!("{label:<w$}", w = width - 1)));
            for (row, text) in rows.iter_mut().enumerate().skip(1) {
                for card in *cards {
                    *text += &match row {
                        1 => "┌────┐".to_string(),
                        2 => format!("│ {} │", card.pretty(self.color)),
                        _ => "└────┘".to_string(),
                    };
                }
                *text += "  ";
            }
        }
        rows.map(|row| format!("  {}", row.trim_end())).join("\n")
    }

    fn show_street(&self, state: &HandState) {
        let title = format!(
            "── {} · pot {} ──",
            state.street().name(),
            self.bb(state.pot())
        );
        println!("\n{}", self.paint("1", &title));
        let hole = state.hole(HERO);
        println!(
            "{}",
            self.card_art(&[("Board", state.board()), ("Your hand", &hole)])
        );
        println!(
            "  You have {}.",
            self.paint("1", &coach::describe_hand(hole, state.board()))
        );
    }

    fn show_hand_banner(&self, hand_no: u64, session: i64, position: &str) {
        let rule = "═".repeat(64);
        println!("\n\n{}", self.paint("1;36", &rule));
        println!(
            "{}",
            self.paint(
                "1",
                &format!(
                    "  HAND {hand_no}   ·   session {}",
                    bb_signed(session, self.big_blind)
                )
            )
        );
        println!("  you are on the {position}");
        println!("{}", self.paint("1;36", &rule));
    }

    fn grade_label(&self, grade: Grade) -> String {
        let code = match grade {
            Grade::Good => "32",
            Grade::Close | Grade::Note => "36",
            Grade::Inaccuracy => "33",
            Grade::Mistake | Grade::Blunder => "1;31",
        };
        self.paint(code, &format!("Coach · {}:", grade.label()))
    }

    fn show_feedback(&self, feedback: &Feedback) {
        println!("  {}", self.grade_label(feedback.grade));
        self.coach_text(&feedback.text);
    }

    fn coach_text(&self, text: &str) {
        for line in wrap(text, 76) {
            println!("    {line}");
        }
    }
}

/// Splits text into lines of at most `width` characters, breaking at spaces.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(line) if line.chars().count() + 1 + word.chars().count() <= width => {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(word.to_string()),
        }
    }
    lines
}

/// The language-model coach, used blocking: replies are streamed to stdout.
struct Talk<'a> {
    llm: Option<&'a Llm>,
    chat: Chat,
}

impl Talk<'_> {
    /// Asks the coach and prints the reply as it streams. Returns false if
    /// there is no coach or the request failed.
    fn ask(&mut self, ask: Ask, facts: Option<&serde_json::Value>, history: &[String]) -> bool {
        let Some(llm) = self.llm else {
            return false;
        };
        let messages = self.chat.request(&ask, facts, history);
        print!("  Coach: ");
        let _ = io::stdout().flush();
        let mut column = 9;
        let result = llm.stream(SYSTEM, &messages, |text| {
            for word in text.split_inclusive(' ') {
                if column + word.len() > 80 && word.trim() != "" {
                    print!("\n    ");
                    column = 4;
                }
                print!("{word}");
                column += word.chars().count();
            }
            let _ = io::stdout().flush();
        });
        println!();
        match result {
            Ok(text) => {
                self.chat.reply(&text);
                true
            }
            Err(error) => {
                self.chat.abandon();
                println!("    (The coach could not answer: {error})");
                false
            }
        }
    }
}

pub struct PlayOptions<'a> {
    pub session: Session,
    pub coach: CoachMode,
    pub hands: Option<u64>,
    pub log: &'a Path,
    pub llm: Option<Llm>,
}

pub fn play(options: PlayOptions) -> io::Result<()> {
    let PlayOptions {
        mut session,
        coach: mode,
        hands,
        log,
        llm,
    } = options;
    let cfg = session.config();
    let printer = Printer::new(cfg.big_blind);
    let coaching = mode != CoachMode::Off;
    let mut talk = Talk {
        llm: llm.as_ref().filter(|_| coaching),
        chat: Chat::default(),
    };
    let stdin = io::stdin();
    let mut input = stdin.lock();

    println!(
        "Heads-up No-Limit Hold'em vs the {} bot. Session seed {}.",
        session.opponent_label(),
        session.seed()
    );
    println!(
        "Both stacks start every hand at {}; your running result is tracked separately. Type h for help.",
        printer.bb(cfg.stack)
    );
    if coaching {
        let who = match &llm {
            Some(llm) => format!("Coach: {} explains the engine's numbers.", llm.model()),
            None => "Coach: set ANTHROPIC_API_KEY for a coach that talks with you.".to_string(),
        };
        println!("{}", printer.dim(&who));
    }

    'session: loop {
        talk.chat.reset();
        let state = session.state();
        printer.show_hand_banner(session.hand_no(), session.total(), position(state, HERO));
        printer.show_street(state);

        while let Some(seat) = session.state().to_act() {
            let before = session.state().clone();
            let legal = before.legal().unwrap();
            if seat != HERO {
                let action = session.bot_act();
                println!("{}", describe("Bot", action, legal, &before));
            } else {
                let spot = coaching.then(|| session.analyse());
                if let Some(spot) = &spot {
                    if legal.to_call > 0 && mode != CoachMode::Teach {
                        println!(
                            "  Pot odds: call {} to win {} → need {:.0}% equity.",
                            printer.bb(legal.to_call),
                            printer.bb(before.pot()),
                            spot.pot_odds() * 100.0
                        );
                    }
                    if mode == CoachMode::Full {
                        show_numbers(&printer, spot);
                    }
                    if mode == CoachMode::Teach {
                        let history = narrate(&before, HERO);
                        let fact = facts(spot, None, Some(&session));
                        if !talk.ask(Ask::Question, Some(&fact), &history) {
                            if !ask_scripted(&mut input, &printer, spot)? {
                                break 'session;
                            }
                        } else if let Some(answer) = {
                            print!("  {} ", printer.dim("(your answer, or enter to skip) >"));
                            io::stdout().flush()?;
                            read_answer(&mut input)?
                        } {
                            if !answer.is_empty() {
                                talk.ask(Ask::Answer(answer), None, &[]);
                            }
                        } else {
                            break 'session;
                        }
                    }
                }
                let action = loop {
                    match prompt(&mut input, &printer, &before, legal)? {
                        Prompted::Quit => break 'session,
                        Prompted::Act(action) => break action,
                        Prompted::Ask(question) => {
                            let fact = spot.as_ref().map(|s| facts(s, None, Some(&session)));
                            if !talk.ask(
                                Ask::Free(question),
                                fact.as_ref(),
                                &narrate(&before, HERO),
                            ) {
                                println!(
                                    "  The coach needs ANTHROPIC_API_KEY to answer questions."
                                );
                            }
                        }
                    }
                };
                let review = session
                    .hero_act(action, coaching, spot)
                    .map_err(io::Error::other)?;
                println!("{}", describe("You", action, legal, &before));
                if let Some(review) = review {
                    let fact = facts(&review.spot, Some(action), Some(&session));
                    if talk.llm.is_some() {
                        println!("  {}", printer.grade_label(review.feedback.grade));
                        if !talk.ask(Ask::Review, Some(&fact), &narrate(&before, HERO)) {
                            printer.coach_text(&review.feedback.text);
                        }
                    } else {
                        printer.show_feedback(&review.feedback);
                    }
                }
            }
            let after = session.state();
            if after.outcome().is_none() && after.street() != before.street() {
                printer.show_street(after);
            }
        }

        show_result(&printer, session.state());
        let record = session.finish_hand().expect("hand is over");
        history::append(log, &record)?;
        println!(
            "{}",
            printer.dim(&format!("── end of hand {} ──", session.hand_no()))
        );
        if hands.is_some_and(|limit| session.hand_no() >= limit) {
            break;
        }
        session.next_hand();
    }

    println!(
        "\nSession over: {} across {} finished hand(s).",
        bb_signed(session.total(), cfg.big_blind),
        session.finished()
    );
    if coaching {
        let count = |grade: Grade| session.grades().get(&grade).copied().unwrap_or(0);
        let reviewed: u32 = session.grades().values().sum();
        println!(
            "Coach: {reviewed} decision(s) reviewed in finished hands · inaccuracies {} · mistakes {} · blunders {} · about {} given up.",
            count(Grade::Inaccuracy),
            count(Grade::Mistake),
            count(Grade::Blunder),
            coach::fmt_bb(session.given_up(), cfg.big_blind)
        );
        print_leaks(&printer, session.tally(), 3);
        if session.finished() > 0 {
            talk.chat.reset();
            talk.ask(Ask::Summary, Some(&session_facts(&session)), &[]);
        }
    }
    Ok(())
}

fn show_numbers(printer: &Printer, spot: &Spot) {
    let mut line = format!("  Your equity: {:.0}%", spot.equity * 100.0);
    if let Some(read) = spot.read {
        line.push_str(&format!(" against the bot's likely hands ({read})"));
    } else {
        line.push_str(" against a random hand");
    }
    println!("{line}.");
    if let Some(&(best, ev)) = spot.ranked(None).first() {
        println!(
            "  {}",
            printer.dim(&format!(
                "Coach's pick: {} ({}).",
                spot.phrase(best),
                coach::fmt_bb_signed(ev, spot.big_blind)
            ))
        );
    }
}

/// Asks the scripted questions. Returns false if the player quit.
fn ask_scripted(input: &mut impl BufRead, printer: &Printer, spot: &Spot) -> io::Result<bool> {
    for question in questions(spot) {
        loop {
            println!("  Coach: {}", question.prompt(spot));
            print!("  {} ", printer.dim("(answer, or enter to skip) >"));
            io::stdout().flush()?;
            let Some(answer) = read_answer(input)? else {
                return Ok(false);
            };
            if answer.is_empty() {
                return Ok(true);
            }
            match question.check(spot, &answer) {
                Ok(feedback) => {
                    printer.coach_text(&feedback.text);
                    break;
                }
                Err(hint) => println!("    {hint}"),
            }
        }
    }
    Ok(true)
}

/// Reads one line; `None` at the end of input or on "q".
fn read_answer(input: &mut impl BufRead) -> io::Result<Option<String>> {
    let mut line = String::new();
    if input.read_line(&mut line)? == 0 {
        println!();
        return Ok(None);
    }
    if !io::stdin().is_terminal() {
        println!("{}", line.trim());
    }
    let line = line.trim().to_string();
    Ok((line != "q").then_some(line))
}

fn show_result(printer: &Printer, state: &HandState) {
    let outcome = state.outcome().unwrap();
    if outcome.showdown {
        if state.street() != Street::River {
            println!("\nAll-in: the rest of the board is dealt.");
        }
        let (mine, theirs) = (state.hole(HERO), state.hole(1 - HERO));
        println!(
            "\n{}\n{}",
            printer.paint("1", "── Showdown ──"),
            printer.card_art(&[("Board", state.board()), ("You", &mine), ("Bot", &theirs)])
        );
        println!("  You: {}", state.showdown_rank(HERO).describe());
        println!("  Bot: {}", state.showdown_rank(1 - HERO).describe());
    }
    let result = match outcome.winner {
        None => printer.paint("1", "Split pot."),
        Some(HERO) => printer.paint(
            "1;32",
            &format!("You win {}.", printer.bb(outcome.net[HERO] as u32)),
        ),
        Some(_) => printer.paint(
            "1;31",
            &format!(
                "Bot wins; you lose {}.",
                printer.bb(outcome.net[1 - HERO] as u32)
            ),
        ),
    };
    println!("{result}");
}

enum Prompted {
    Act(Action),
    Ask(String),
    Quit,
}

/// Asks for an action until a legal one is entered.
fn prompt(
    input: &mut impl BufRead,
    printer: &Printer,
    state: &HandState,
    legal: Legal,
) -> io::Result<Prompted> {
    let mut options = Vec::new();
    if legal.to_call > 0 {
        options.push("[f]old".to_string());
        options.push(format!("[c]all {}", printer.bb(legal.to_call)));
    } else {
        options.push("[x] check".to_string());
    }
    if let Some((min, max)) = legal.raise {
        let verb = if state.street_commit(1 - HERO) == 0 {
            "[b]et"
        } else {
            "[r]aise to"
        };
        if min == max {
            options.push(format!("[a]ll-in {}", printer.bb(max)));
        } else {
            options.push(format!("{verb} {}–{}", printer.bb(min), printer.bb(max)));
        }
    }
    loop {
        println!(
            "  {}",
            printer.dim(&format!(
                "Pot {} · you {} · bot {}",
                printer.bb(state.pot()),
                printer.bb(state.stack(HERO)),
                printer.bb(state.stack(1 - HERO)),
            ))
        );
        print!("  {} > ", options.join("  "));
        io::stdout().flush()?;
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            println!();
            return Ok(Prompted::Quit);
        }
        if !io::stdin().is_terminal() {
            println!("{}", line.trim());
        }
        match parse_command(&line, state, legal) {
            Ok(Input::Act(action)) => return Ok(Prompted::Act(action)),
            Ok(Input::Quit) => return Ok(Prompted::Quit),
            Ok(Input::Ask(question)) => return Ok(Prompted::Ask(question)),
            Ok(Input::Help) => println!("{HELP}"),
            Err(message) => println!("  {message}"),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Input {
    Act(Action),
    Ask(String),
    Help,
    Quit,
}

pub fn parse_command(line: &str, state: &HandState, legal: Legal) -> Result<Input, String> {
    let trimmed = line.trim();
    if let Some(question) = trimmed.strip_prefix('?').map(str::trim)
        && !question.is_empty()
    {
        return Ok(Input::Ask(question.to_string()));
    }
    let line = trimmed.to_lowercase();
    let mut words = line.split_whitespace();
    let Some(command) = words.next() else {
        return Err("Type an action, or h for help.".into());
    };
    let argument = words.next();
    let big_blind = state.config().big_blind as f64;

    let action = match command {
        "q" | "quit" | "exit" => return Ok(Input::Quit),
        "h" | "help" | "?" => return Ok(Input::Help),
        "f" | "fold" => {
            if legal.to_call == 0 {
                return Err("Nothing to call, so there is no reason to fold: check with x.".into());
            }
            Action::Fold
        }
        "x" | "k" | "check" => {
            if legal.to_call > 0 {
                return Err("You cannot check facing a bet: fold, call or raise.".into());
            }
            Action::Check
        }
        // Be forgiving: "c" with nothing to call means check.
        "c" | "call" => {
            if legal.to_call > 0 {
                Action::Call
            } else {
                Action::Check
            }
        }
        "a" | "allin" | "all-in" | "shove" => match legal.raise {
            Some((_, max)) => Action::RaiseTo(max),
            // Facing an all-in, calling is the only way to get the chips in.
            None => Action::Call,
        },
        "b" | "bet" | "r" | "raise" => {
            let Some((min, max)) = legal.raise else {
                return Err("You cannot raise here: your opponent is all-in.".into());
            };
            let Some(text) = argument else {
                return Err(
                    "Give an amount, e.g. 'b 2.5' (big blinds) or 'b 75%' (of the pot).".into(),
                );
            };
            let opp_bet = state.street_commit(1 - state.to_act().unwrap());
            let target = if let Some(percent) = text.strip_suffix('%') {
                let percent: f64 = percent
                    .parse()
                    .map_err(|_| format!("'{text}' is not a percentage."))?;
                let pot_after_call = (state.pot() + legal.to_call) as f64;
                opp_bet as f64 + pot_after_call * percent / 100.0
            } else {
                let amount: f64 = text
                    .trim_end_matches("bb")
                    .parse()
                    .map_err(|_| format!("'{text}' is not an amount."))?;
                amount * big_blind
            };
            if !target.is_finite() || target < 0.0 {
                return Err(format!("'{text}' is not a valid amount."));
            }
            let target = target.round().min(u32::MAX as f64) as u32;
            if target < min || target > max {
                let to_bb = |chips: u32| bb(chips, big_blind as u32);
                return Err(format!(
                    "That would be {}; the legal range is {} to {}.",
                    to_bb(target),
                    to_bb(min),
                    to_bb(max)
                ));
            }
            Action::RaiseTo(target)
        }
        _ => return Err(format!("Unknown command '{command}'. Type h for help.")),
    };
    Ok(Input::Act(action))
}

/// `poker replay`: prints the stored hands, verifying each one.
pub fn replay(file: &Path) -> io::Result<()> {
    let records = history::load(file)?;
    let mut total = 0i64;
    for record in &records {
        let printer = Printer::new(record.config.big_blind);
        let state = record.replay().map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("hand {}: {e}", record.hand_no),
            )
        })?;
        let hero = record.hero;
        println!(
            "\n━━ Hand {} vs {} ━━ you: {} ({}) · bot: {}",
            record.hand_no,
            record.opponent,
            printer.cards(&record.hole[hero]),
            position(&state, hero),
            printer.cards(&record.hole[1 - hero]),
        );
        for line in narrate(&state, hero) {
            println!("  {line}");
        }
        if state.outcome().unwrap().showdown {
            println!("  Board: {}", printer.cards(state.board()));
        }
        println!(
            "  Result: {}",
            bb_signed(record.net[hero], record.config.big_blind)
        );
        total += record.net[hero];
    }
    match records.first() {
        Some(first) => println!(
            "\n{} hand(s), all verified. Total {}.",
            records.len(),
            bb_signed(total, first.config.big_blind)
        ),
        None => println!("No hands in {}.", file.display()),
    }
    Ok(())
}

/// Every decision in a history file, re-run through the coach.
pub fn load_decisions(file: &Path, last: usize) -> io::Result<(Vec<HandRecord>, Vec<Decision>)> {
    let records = history::load(file)?;
    let start = records.len().saturating_sub(last);
    let mut all = Vec::new();
    for record in &records[start..] {
        match decisions(record) {
            Ok(found) => all.extend(found),
            Err(error) => eprintln!("skipping {error}"),
        }
    }
    Ok((records, all))
}

/// `poker leaks`: the costliest recurring mistakes in a history file.
pub fn leaks(file: &Path, last: usize) -> io::Result<()> {
    let (records, all) = load_decisions(file, last)?;
    let Some(first) = records.first() else {
        println!("No hands in {}.", file.display());
        return Ok(());
    };
    let printer = Printer::new(first.config.big_blind);
    let mut tally = LeakTally::default();
    let mut given_up = 0.0;
    for d in &all {
        tally.add(&d.spot, d.action, &d.feedback);
        given_up += d.feedback.loss;
    }
    let hands = records.len().min(last);
    println!(
        "{} decisions in the last {hands} hand(s); about {} given up in total.",
        all.len(),
        coach::fmt_bb(given_up, printer.big_blind)
    );
    if tally.is_empty() {
        println!("No recurring mistakes found. Play more hands, or try a tougher opponent.");
    } else {
        print_leaks(&printer, &tally, 6);
        println!("\nPractise them with: poker drill {}", file.display());
    }
    Ok(())
}

fn print_leaks(printer: &Printer, tally: &LeakTally, limit: usize) {
    let lines = tally.lines();
    if lines.is_empty() {
        return;
    }
    println!("\n{}", printer.paint("1", "Leaks, costliest first:"));
    for (i, line) in lines.iter().take(limit).enumerate() {
        println!(
            "  {}. {} on the {} — {} time(s), about {}",
            i + 1,
            line.leak.label(),
            line.street.name().to_lowercase(),
            line.count,
            coach::fmt_bb(line.loss, printer.big_blind)
        );
        if i < 3 {
            for text in wrap(line.leak.tip(), 72) {
                println!("     {}", printer.dim(&text));
            }
        }
    }
}

/// `poker drill` without the full-screen table.
pub fn drill(spots: Vec<Decision>, llm: Option<Llm>) -> io::Result<()> {
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let mut talk = Talk {
        llm: llm.as_ref(),
        chat: Chat::default(),
    };
    let mut correct = 0;
    let total = spots.len();
    for (i, d) in spots.iter().enumerate() {
        let printer = Printer::new(d.state.config().big_blind);
        talk.chat.reset();
        println!(
            "\n{}",
            printer.paint(
                "1;36",
                &format!(
                    "━━ Drill {} of {total} · from hand {} · you are on the {} ━━",
                    i + 1,
                    d.hand_no,
                    position(&d.state, HERO)
                )
            )
        );
        for line in narrate(&d.state, HERO) {
            println!("  {line}");
        }
        printer.show_street(&d.state);
        let legal = d.state.legal().expect("drills are live decisions");
        let action = loop {
            match prompt(&mut input, &printer, &d.state, legal)? {
                Prompted::Quit => return Ok(()),
                Prompted::Act(action) => break action,
                Prompted::Ask(question) => {
                    let fact = facts(&d.spot, None, None);
                    talk.ask(Ask::Free(question), Some(&fact), &narrate(&d.state, HERO));
                }
            }
        };
        let bot = by_name(&d.opponent, 0).expect("drills come from known bots");
        let strengths = Strengths::new(d.state.board());
        let spot = analyse_with(&d.state, bot.as_ref(), &d.range, &strengths, action);
        let feedback = coach::review(&spot, action);
        if feedback.loss < 0.1 * printer.big_blind as f64 {
            correct += 1;
        }
        if talk.llm.is_some() {
            println!("  {}", printer.grade_label(feedback.grade));
            let fact = facts(&spot, Some(action), None);
            if !talk.ask(Ask::Review, Some(&fact), &narrate(&d.state, HERO)) {
                printer.coach_text(&feedback.text);
            }
        } else {
            printer.show_feedback(&feedback);
        }
        println!(
            "  {}",
            printer.dim(&format!(
                "In the game you chose {} ({}).",
                d.spot.phrase(d.action),
                d.feedback.grade.label().to_lowercase()
            ))
        );
    }
    println!("\nDrill over: {correct} of {total} spots played well.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use poker::cards::parse_cards;
    use poker::game::Config;

    fn state() -> HandState {
        let c = parse_cards("As Ah Ks Kh 2c 7d 9h Jc 3s").unwrap();
        HandState::new(
            Config::default(),
            HERO,
            [[c[0], c[1]], [c[2], c[3]]],
            [c[4], c[5], c[6], c[7], c[8]],
        )
    }

    fn parse(line: &str, state: &HandState) -> Result<Input, String> {
        parse_command(line, state, state.legal().unwrap())
    }

    #[test]
    fn basic_commands() {
        let s = state(); // on the button, facing the big blind
        assert_eq!(parse("f", &s), Ok(Input::Act(Action::Fold)));
        assert_eq!(parse(" Call ", &s), Ok(Input::Act(Action::Call)));
        assert_eq!(parse("r 2.5", &s), Ok(Input::Act(Action::RaiseTo(250))));
        assert_eq!(parse("b 3bb", &s), Ok(Input::Act(Action::RaiseTo(300))));
        assert_eq!(parse("a", &s), Ok(Input::Act(Action::RaiseTo(10_000))));
        assert_eq!(parse("q", &s), Ok(Input::Quit));
        assert_eq!(parse("?", &s), Ok(Input::Help));
        assert_eq!(
            parse("? why fold here", &s),
            Ok(Input::Ask("why fold here".into()))
        );
        assert!(parse("x", &s).is_err());
        assert!(parse("r", &s).is_err());
        assert!(parse("r 1.5", &s).is_err(), "below the minimum raise");
        assert!(parse("r 101", &s).is_err(), "more than the stack");
        assert!(parse("r nan", &s).is_err());
        assert!(parse("dance", &s).is_err());
        assert!(parse("", &s).is_err());
    }

    #[test]
    fn pot_percentage_sizes() {
        let mut s = state();
        // Preflop: pot 1.5bb, 0.5bb to call. A pot-sized raise is to 1 + 2 = 3bb.
        assert_eq!(parse("r 100%", &s), Ok(Input::Act(Action::RaiseTo(300))));
        s.apply(Action::RaiseTo(300)).unwrap();
        s.apply(Action::Call).unwrap();
        s.apply(Action::Check).unwrap();
        // Flop: pot 6bb, checked to us. Half pot is 3bb.
        assert_eq!(parse("b 50%", &s), Ok(Input::Act(Action::RaiseTo(300))));
        assert_eq!(parse("c", &s), Ok(Input::Act(Action::Check)));
        assert!(parse("f", &s).is_err());
    }

    #[test]
    fn all_in_facing_a_shove_means_call() {
        let mut s = state();
        s.apply(Action::Call).unwrap();
        s.apply(Action::RaiseTo(10_000)).unwrap();
        assert_eq!(parse("a", &s), Ok(Input::Act(Action::Call)));
        assert!(parse("r 50", &s).is_err());
    }

    #[test]
    fn wrapping_breaks_at_spaces() {
        assert_eq!(wrap("one two three four", 9), ["one two", "three", "four"]);
        assert_eq!(wrap("", 9), Vec::<String>::new());
    }
}
