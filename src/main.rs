mod app;

use app::CoachMode;
use app::llm::Llm;
use clap::{Args, Parser, Subcommand, ValueEnum};
use poker::analysis::drill_spots;
use poker::bot::{Bot, CallingStation, RandomBot, SolidBot};
use poker::game::Config;
use poker::session::Session;
use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Parser)]
#[command(
    about = "Heads-up No-Limit Hold'em trainer: play a solid bot with a coach that teaches you to decide"
)]
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
    /// Find your costliest recurring mistakes in a history file
    Leaks {
        #[arg(default_value = "hands.jsonl")]
        file: PathBuf,
        /// Only look at this many of the most recent hands
        #[arg(long, default_value_t = 300)]
        last: usize,
    },
    /// Replay your costliest past mistakes as practice spots
    Drill {
        #[arg(default_value = "hands.jsonl")]
        file: PathBuf,
        /// How many spots to practise
        #[arg(long, default_value_t = 10)]
        count: usize,
        /// Only look at this many of the most recent hands
        #[arg(long, default_value_t = 300)]
        last: usize,
        /// Use the line-by-line interface instead of the full-screen table
        #[arg(long)]
        plain: bool,
    },
}

#[derive(Args)]
struct PlayArgs {
    /// Opponent to play against
    #[arg(long, value_enum, default_value_t)]
    bot: BotKind,
    /// How the coach helps: questions before each decision (teach), the
    /// numbers before and a review after (full), a review after (after), or off
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
    /// Use the line-by-line interface instead of the full-screen table
    #[arg(long)]
    plain: bool,
    /// Never use the language-model coach, even if a key is set
    #[arg(long)]
    no_llm: bool,
}

#[derive(Clone, Copy, Default, ValueEnum)]
enum BotKind {
    /// A sound regular: plays ranges, bets for value, bluffs in balance
    #[default]
    Solid,
    /// Ignores its cards
    Random,
    /// Never folds, never bets
    Station,
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        None => play(cli.play),
        Some(Command::Replay { file }) => app::plain::replay(&file),
        Some(Command::Leaks { file, last }) => app::plain::leaks(&file, last),
        Some(Command::Drill {
            file,
            count,
            last,
            plain,
        }) => drill(file, count, last, plain, cli.play.no_llm),
    };
    if let Err(error) = result {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn interactive() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

fn llm(disabled: bool) -> Option<Llm> {
    if disabled { None } else { Llm::from_env() }
}

fn play(args: PlayArgs) -> io::Result<()> {
    let seed = args.seed.unwrap_or_else(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
    });
    let default = Config::default();
    let max_stack = Config::MAX_STACK / default.big_blind;
    if !(1..=max_stack).contains(&args.stack) {
        return Err(io::Error::other(format!(
            "--stack must be between 1 and {max_stack} big blinds"
        )));
    }
    let cfg = Config {
        stack: args.stack * default.big_blind,
        ..default
    };
    let bot: Box<dyn Bot> = match args.bot {
        BotKind::Solid => Box::new(SolidBot::new(seed ^ 0xB07)),
        BotKind::Random => Box::new(RandomBot::new(seed ^ 0xB07)),
        BotKind::Station => Box::new(CallingStation),
    };
    let session = Session::new(cfg, bot, seed);
    let llm = llm(args.no_llm || args.coach == CoachMode::Off);
    if args.plain || !interactive() {
        app::plain::play(app::plain::PlayOptions {
            session,
            coach: args.coach,
            hands: args.hands,
            log: &args.log,
            llm,
        })
    } else {
        let game = app::tui::Game::Play {
            session: Box::new(session),
            log_file: args.log,
            hands: args.hands,
        };
        app::tui::run(game, args.coach, llm)
    }
}

fn drill(file: PathBuf, count: usize, last: usize, plain: bool, no_llm: bool) -> io::Result<()> {
    let (records, all) = app::plain::load_decisions(&file, last)?;
    if records.is_empty() {
        println!("No hands in {} yet: play some first.", file.display());
        return Ok(());
    }
    let spots = drill_spots(all, count);
    if spots.is_empty() {
        println!("No mistakes worth drilling in {}. Nice.", file.display());
        return Ok(());
    }
    let llm = llm(no_llm);
    if plain || !interactive() {
        app::plain::drill(spots, llm)
    } else {
        let game = app::tui::Game::Drill {
            spots,
            index: 0,
            correct: 0,
        };
        app::tui::run(game, CoachMode::After, llm)
    }
}
