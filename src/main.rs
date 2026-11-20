use clap::{Parser, Subcommand, ValueEnum};
use poker::bot::{Bot, CallingStation, RandomBot};
use poker::cards::{Card, Rng, shuffled_deck};
use poker::coach::{self, Feedback, Grade, Spot};
use poker::game::{Action, Config, HandState, Legal, Street};
use poker::history::{self, HandRecord};
use std::collections::HashMap;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Parser)]
#[command(about = "Heads-up No-Limit Hold'em trainer")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    #[command(flatten)]
    play: PlayArgs,
}

#[derive(Subcommand)]
enum Command {
    /// Print the hands stored in a history file, verifying each one
    Replay {
        #[arg(default_value = "hands.jsonl")]
        file: PathBuf,
    },
}

#[derive(clap::Args)]
struct PlayArgs {
    /// Opponent to play against
    #[arg(long, value_enum, default_value_t)]
    bot: BotKind,
    /// How much the coach says: nothing, a review after each decision,
    /// or also your equity before you decide
    #[arg(long, value_enum, default_value_t)]
    coach: CoachMode,
    /// Seed for the cards and the bot; the same seed deals the same session
    #[arg(long)]
    seed: Option<u64>,
    /// Starting stack in big blinds, reset every hand
    #[arg(long, default_value_t = 100)]
    stack: u32,
    /// Stop after this many hands
    #[arg(long)]
    hands: Option<u64>,
    /// File that finished hands are appended to
    #[arg(long, default_value = "hands.jsonl")]
    log: PathBuf,
}

#[derive(Clone, Copy, Default, ValueEnum)]
enum BotKind {
    #[default]
    Random,
    Station,
}

#[derive(Clone, Copy, Default, PartialEq, ValueEnum)]
enum CoachMode {
    Off,
    #[default]
    After,
    Full,
}

const HERO: usize = 0;
const HELP: &str = "\
Commands (amounts are in big blinds):
  f            fold
  x            check
  c            call
  b 2.5        bet or raise to 2.5bb in total on this street (also: r 2.5)
  b 75%        bet 75% of the pot (when raising: of the pot after calling)
  a            all-in
  q            quit";

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        None => play(cli.play),
        Some(Command::Replay { file }) => replay(&file),
    };
    if let Err(error) = result {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

/// Formats and prints with knowledge of the table's stakes.
struct Table {
    big_blind: u32,
    color: bool,
}

impl Table {
    fn bb(&self, chips: u32) -> String {
        coach::fmt_bb(chips as f64, self.big_blind)
    }

    fn bb_signed(&self, chips: i64) -> String {
        let sign = if chips > 0 { "+" } else { "" };
        format!("{sign}{}", coach::fmt_bb(chips as f64, self.big_blind))
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

    /// Cards as inline text, e.g. "A♣ 4♥".
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

    fn describe(
        &self,
        who: &str,
        action: Action,
        legal: Legal,
        state_before: &HandState,
    ) -> String {
        let s = if who == "You" { "" } else { "s" };
        match action {
            Action::Fold => format!("{who} fold{s}."),
            Action::Check => format!("{who} check{s}."),
            Action::Call => format!("{who} call{s} {}.", self.bb(legal.to_call)),
            Action::RaiseTo(amount) => {
                let all_in = legal.raise.is_some_and(|(_, max)| amount == max);
                let suffix = if all_in { " (all-in)" } else { "" };
                let opp = 1 - state_before.to_act().unwrap();
                let verb = if state_before.street_commit(opp) == 0 {
                    "bet"
                } else {
                    "raise"
                };
                let to = if verb == "raise" { " to" } else { "" };
                format!("{who} {verb}{s}{to} {}{suffix}.", self.bb(amount))
            }
        }
    }

    /// Street header, the board and the player's hand with what it makes.
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

    fn show_feedback(&self, feedback: &Feedback) {
        let code = match feedback.grade {
            Grade::Good => "32",
            Grade::Close | Grade::Note => "36",
            Grade::Inaccuracy => "33",
            Grade::Mistake | Grade::Blunder => "1;31",
        };
        let label = self.paint(code, &format!("Coach · {}:", feedback.grade.label()));
        println!("  {label} {}", feedback.text);
    }
}

fn play(args: PlayArgs) -> io::Result<()> {
    let seed = args.seed.unwrap_or_else(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
    });
    if args.stack == 0 {
        return Err(io::Error::other("--stack must be at least 1"));
    }
    let cfg = Config {
        stack: args.stack * 100,
        ..Config::default()
    };
    let table = Table {
        big_blind: cfg.big_blind,
        color: io::stdout().is_terminal(),
    };
    let mut bot: Box<dyn Bot> = match args.bot {
        BotKind::Random => Box::new(RandomBot::new(seed ^ 0xB07)),
        BotKind::Station => Box::new(CallingStation),
    };
    let coaching = args.coach != CoachMode::Off;
    let stdin = io::stdin();
    let mut input = stdin.lock();

    println!(
        "Heads-up No-Limit Hold'em vs the {} bot. Session seed {seed}.",
        bot.name()
    );
    println!(
        "Both stacks start every hand at {}, so each hand is the same situation; \
         your running result is tracked separately. Type h for help.",
        table.bb(cfg.stack)
    );
    if coaching {
        println!(
            "{}",
            table.dim(
                "Coach: equity is your chance to win against a random hand, which is what this bot \
                 holds.\nPrices assume no further betting, so they are estimates before the river."
            )
        );
    }

    let mut total: i64 = 0;
    let mut finished = 0u64;
    let mut hand_no = 0u64;
    let mut grades: HashMap<Grade, u32> = HashMap::new();
    let mut total_loss = 0.0;
    'session: while args.hands.is_none_or(|limit| hand_no < limit) {
        hand_no += 1;
        let hand_seed = Rng::new(seed.wrapping_add(hand_no)).next_u64();
        let button = ((hand_no - 1) % 2) as usize;
        let mut state = HandState::deal(cfg, button, &shuffled_deck(hand_seed));

        let position = if button == HERO {
            "you are the button: small blind, first to act now, last to act after the flop"
        } else {
            "you are the big blind: last to act now, first to act after the flop"
        };
        let title = format!("━━ Hand {hand_no} ━━ session {}", table.bb_signed(total));
        println!("\n{}\n{position}", table.paint("1", &title));
        table.show_street(&state);

        // Equity only changes when the board does, so compute it once per street.
        let mut equity: Option<(Street, f64)> = None;
        while let Some(seat) = state.to_act() {
            let legal = state.legal().unwrap();
            let before = state.clone();
            if seat == HERO {
                let hole = state.hole(HERO);
                let spot = coaching.then(|| {
                    let known = equity.filter(|(street, _)| *street == state.street());
                    let (_, value) = known.unwrap_or_else(|| {
                        (state.street(), coach::equity_vs_random(hole, state.board()))
                    });
                    equity = Some((state.street(), value));
                    Spot {
                        big_blind: cfg.big_blind,
                        pot: state.pot(),
                        to_call: legal.to_call,
                        street_commit: state.street_commit(HERO),
                        equity: value,
                    }
                });
                if let Some(spot) = &spot {
                    if legal.to_call > 0 {
                        println!(
                            "  Pot odds: call {} to win {} → you need to win more than {:.0}% of the time.",
                            table.bb(legal.to_call),
                            table.bb(state.pot()),
                            coach::pot_odds(state.pot(), legal.to_call) * 100.0
                        );
                    }
                    if args.coach == CoachMode::Full {
                        println!(
                            "  Your equity against a random hand: {:.0}%.",
                            spot.equity * 100.0
                        );
                    }
                }
                let Some(action) = prompt(&mut input, &table, &state, legal)? else {
                    break 'session;
                };
                state.apply(action).map_err(io::Error::other)?;
                println!("{}", table.describe("You", action, legal, &before));
                if let Some(spot) = &spot {
                    let feedback = coach::review(spot, action);
                    table.show_feedback(&feedback);
                    *grades.entry(feedback.grade).or_default() += 1;
                    total_loss += feedback.loss;
                }
            } else {
                let action = bot.act(&state, seat);
                state.apply(action).map_err(io::Error::other)?;
                println!("{}", table.describe("Bot", action, legal, &before));
            }

            if state.outcome().is_none() && state.street() != before.street() {
                table.show_street(&state);
            }
        }

        let outcome = state.outcome().unwrap();
        if outcome.showdown {
            if state.street() != Street::River {
                println!("\nAll-in: the rest of the board is dealt.");
            }
            let (mine, theirs) = (state.hole(HERO), state.hole(1 - HERO));
            println!(
                "\n{}\n{}",
                table.paint("1", "── Showdown ──"),
                table.card_art(&[("Board", state.board()), ("You", &mine), ("Bot", &theirs)])
            );
            println!("  You: {}", state.showdown_rank(HERO).describe());
            println!("  Bot: {}", state.showdown_rank(1 - HERO).describe());
        }
        let result = match outcome.winner {
            None => table.paint("1", "Split pot."),
            Some(HERO) => table.paint(
                "1;32",
                &format!("You win {}.", table.bb(outcome.net[HERO] as u32)),
            ),
            Some(_) => table.paint(
                "1;31",
                &format!(
                    "Bot wins; you lose {}.",
                    table.bb(outcome.net[1 - HERO] as u32)
                ),
            ),
        };
        println!("{result}");
        total += outcome.net[HERO];
        finished += 1;

        let record = HandRecord::from_state(&state, hand_no, hand_seed, HERO, bot.name());
        history::append(&args.log, &record)?;
    }

    println!(
        "\nSession over: {} across {finished} finished hand(s).",
        table.bb_signed(total)
    );
    if coaching {
        let count = |grade: Grade| grades.get(&grade).copied().unwrap_or(0);
        let reviewed: u32 = grades.values().sum();
        println!(
            "Coach: {reviewed} decision(s) reviewed · inaccuracies {} · mistakes {} · blunders {} · about {} given up.",
            count(Grade::Inaccuracy),
            count(Grade::Mistake),
            count(Grade::Blunder),
            coach::fmt_bb(total_loss, cfg.big_blind)
        );
    }
    Ok(())
}

/// Asks the player for an action until a legal one is entered.
/// Returns `None` if the player quits or input ends.
fn prompt(
    input: &mut impl BufRead,
    table: &Table,
    state: &HandState,
    legal: Legal,
) -> io::Result<Option<Action>> {
    let mut options = Vec::new();
    if legal.to_call > 0 {
        options.push("[f]old".to_string());
        options.push(format!("[c]all {}", table.bb(legal.to_call)));
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
            options.push(format!("[a]ll-in {}", table.bb(max)));
        } else {
            options.push(format!("{verb} {}–{}", table.bb(min), table.bb(max)));
        }
    }
    loop {
        println!(
            "  {}",
            table.dim(&format!(
                "Pot {} · you {} · bot {}",
                table.bb(state.pot()),
                table.bb(state.stack(HERO)),
                table.bb(state.stack(1 - HERO)),
            ))
        );
        print!("  {} > ", options.join("  "));
        io::stdout().flush()?;
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            println!();
            return Ok(None);
        }
        // Keep transcripts readable when input is piped rather than typed.
        if !io::stdin().is_terminal() {
            println!("{}", line.trim());
        }
        match parse_command(&line, state, legal) {
            Ok(Input::Act(action)) => return Ok(Some(action)),
            Ok(Input::Quit) => return Ok(None),
            Ok(Input::Help) => println!("{HELP}"),
            Err(message) => println!("  {message}"),
        }
    }
}

#[derive(Debug, PartialEq)]
enum Input {
    Act(Action),
    Help,
    Quit,
}

fn parse_command(line: &str, state: &HandState, legal: Legal) -> Result<Input, String> {
    let line = line.trim().to_lowercase();
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
                let range = Table {
                    big_blind: big_blind as u32,
                    color: false,
                };
                return Err(format!(
                    "That would be {}; the legal range is {} to {}.",
                    range.bb(target),
                    range.bb(min),
                    range.bb(max)
                ));
            }
            Action::RaiseTo(target)
        }
        _ => return Err(format!("Unknown command '{command}'. Type h for help.")),
    };
    Ok(Input::Act(action))
}

fn replay(file: &std::path::Path) -> io::Result<()> {
    let records = history::load(file)?;
    let mut total = 0i64;
    for record in &records {
        let table = Table {
            big_blind: record.config.big_blind,
            color: io::stdout().is_terminal(),
        };
        let state = record.replay().map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("hand {}: {e}", record.hand_no),
            )
        })?;
        let hero = record.hero;
        let position = if record.button == hero {
            "button"
        } else {
            "big blind"
        };
        println!(
            "\n━━ Hand {} vs {} ━━ you: {} ({position}) · bot: {}",
            record.hand_no,
            record.opponent,
            table.cards(&record.hole[hero]),
            table.cards(&record.hole[1 - hero]),
        );

        // Walk the hand again to describe each action in context.
        let mut walk = HandState::new(record.config, record.button, record.hole, record.board);
        let mut street = Street::Preflop;
        for &(seat, action) in &record.actions {
            let legal = walk.legal().unwrap();
            let who = if seat == hero { "You" } else { "Bot" };
            println!("  {}", table.describe(who, action, legal, &walk));
            walk.apply(action).unwrap();
            if walk.street() != street {
                street = walk.street();
                println!(" {}: {}", street.name(), table.cards(walk.board()));
            }
        }
        if state.outcome().unwrap().showdown {
            println!(" Board: {}", table.cards(state.board()));
        }
        println!(" Result: {}", table.bb_signed(record.net[hero]));
        total += record.net[hero];
    }
    if let Some(first) = records.first() {
        let table = Table {
            big_blind: first.config.big_blind,
            color: false,
        };
        println!(
            "\n{} hand(s), all verified. Total {}.",
            records.len(),
            table.bb_signed(total)
        );
    } else {
        println!("No hands in {}.", file.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use poker::cards::parse_cards;

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
        assert_eq!(
            parse("c", &s),
            Ok(Input::Act(Action::Check)),
            "c means check when free"
        );
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
}
