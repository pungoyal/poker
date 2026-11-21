//! The coach: facts about a spot (hand strength, draws, equity, pot odds)
//! and a review of the decision taken.
//!
//! Equity is measured against the hands the opponent is likely to hold: its
//! range, narrowed by Bayes' rule from its known strategy each time it acts.
//! Bets are priced by how often that range folds and how strong the part
//! that continues is. Decision values are showdown values plus an estimate of
//! the betting still to come (see [`Spot::ev`]); before the river they are
//! estimates, not exact.

use crate::bot::Bot;
use crate::cards::{Card, Rng};
use crate::eval::{Draws, HandRank, evaluate, rank_name, rank_plural};
use crate::game::{Action, HandState, Street};
use crate::range::{COMBOS, Profile, Range, Strengths, combo, equity_vs_ranges};

/// Formats a chip amount in big blinds, e.g. "2.5bb".
pub fn fmt_bb(chips: f64, big_blind: u32) -> String {
    let text = format!("{:.2}", chips / big_blind as f64);
    let text = text.trim_end_matches('0').trim_end_matches('.');
    let text = if text == "-0" { "0" } else { text };
    format!("{text}bb")
}

/// Like [`fmt_bb`], with a leading "+" for gains.
pub fn fmt_bb_signed(chips: f64, big_blind: u32) -> String {
    let text = fmt_bb(chips, big_blind);
    if text.starts_with('-') || text == "0bb" {
        text
    } else {
        format!("+{text}")
    }
}

/// Share of the final pot a call must win to break even.
pub fn pot_odds(pot: u32, to_call: u32) -> f64 {
    to_call as f64 / (pot + to_call) as f64
}

/// What the player holds right now: the starting hand before the flop,
/// the made hand after it. Draws are not included.
pub fn made_hand(hole: [Card; 2], board: &[Card]) -> String {
    if board.is_empty() {
        let (high, low) = (
            hole[0].rank().max(hole[1].rank()),
            hole[0].rank().min(hole[1].rank()),
        );
        if high == low {
            return format!("pocket {}", rank_plural(high));
        }
        let suited = if hole[0].suit() == hole[1].suit() {
            "suited"
        } else {
            "offsuit"
        };
        return format!("{}-{} {suited}", rank_name(high), rank_name(low));
    }
    let mut cards = board.to_vec();
    cards.extend(hole);
    evaluate(&cards).describe()
}

/// Plain-English description of the player's hand so far, including draws.
pub fn describe_hand(hole: [Card; 2], board: &[Card]) -> String {
    let mut parts = vec![made_hand(hole, board)];
    parts.extend(draws(hole, board));
    parts.join(" · ")
}

/// Draws to a straight or flush with cards still to come, e.g.
/// "flush draw (9 outs)". Only draws that use a hole card count.
pub fn draws(hole: [Card; 2], board: &[Card]) -> Vec<String> {
    Draws::find(hole, board)
        .list()
        .map(|d| format!("{} ({} outs)", d.name(), d.outs()))
        .collect()
}

/// The standard bets and raises: a third, half, three quarters and all of the
/// pot, then all-in, as `RaiseTo` amounts within the legal `range`. Sizes are
/// rounded to half a big blind, as a player would bet them.
pub fn standard_sizes(
    pot: u32,
    to_call: u32,
    opp_commit: u32,
    big_blind: u32,
    range: (u32, u32),
) -> Vec<u32> {
    let (min, max) = range;
    let base = (pot + to_call) as f64;
    let step = (big_blind / 2).max(1) as f64;
    let mut sizes: Vec<u32> = [1.0 / 3.0, 0.5, 0.75, 1.0]
        .iter()
        .map(|share| {
            let target = opp_commit as f64 + base * share;
            (((target / step).round() * step) as u32).clamp(min, max)
        })
        .collect();
    sizes.push(max);
    sizes
}

/// Chance of winning at showdown against a random hand, counting ties as half.
/// Exact from the flop onwards; a fixed-seed simulation before the flop.
pub fn equity_vs_random(hole: [Card; 2], board: &[Card]) -> f64 {
    let rest: Vec<Card> = (0..52)
        .map(Card::from_index)
        .filter(|c| !hole.contains(c) && !board.contains(c))
        .collect();
    match board.len() {
        0 => simulate_preflop(hole, &rest),
        3..=5 => enumerate(hole, board, &rest),
        n => panic!("a board cannot have {n} cards"),
    }
}

fn score(hero: HandRank, villain: HandRank) -> f64 {
    match hero.cmp(&villain) {
        std::cmp::Ordering::Greater => 1.0,
        std::cmp::Ordering::Equal => 0.5,
        std::cmp::Ordering::Less => 0.0,
    }
}

/// Every board completion against every opponent hand from `rest`.
fn enumerate(hole: [Card; 2], board: &[Card], rest: &[Card]) -> f64 {
    let n = rest.len();
    let mut cards = [hole[0]; 7];
    cards[..board.len()].copy_from_slice(board);
    let (mut total, mut count) = (0.0, 0u64);
    let mut add = |(t, c): (f64, u64)| {
        total += t;
        count += c;
    };
    match board.len() {
        5 => add(against_all(&mut cards, hole, rest, 0)),
        4 => {
            for i in 0..n {
                cards[4] = rest[i];
                add(against_all(&mut cards, hole, rest, 1 << i));
            }
        }
        _ => {
            for i in 0..n {
                for j in i + 1..n {
                    cards[3] = rest[i];
                    cards[4] = rest[j];
                    add(against_all(&mut cards, hole, rest, 1 << i | 1 << j));
                }
            }
        }
    }
    total / count as f64
}

/// Scores the complete board in `cards[..5]` against every opponent hand
/// from `rest` that avoids the cards marked in `used`. Returns (total, count).
fn against_all(cards: &mut [Card; 7], hole: [Card; 2], rest: &[Card], used: u64) -> (f64, u64) {
    cards[5] = hole[0];
    cards[6] = hole[1];
    let hero = evaluate(cards);
    let (mut total, mut count) = (0.0, 0u64);
    for i in (0..rest.len()).filter(|i| used & 1 << i == 0) {
        for j in (i + 1..rest.len()).filter(|j| used & 1 << j == 0) {
            cards[5] = rest[i];
            cards[6] = rest[j];
            total += score(hero, evaluate(cards));
            count += 1;
        }
    }
    (total, count)
}

fn simulate_preflop(hole: [Card; 2], rest: &[Card]) -> f64 {
    const SAMPLES: u32 = 40_000;
    // Seeded by the cards, in either order, so the same hand always reports the same number.
    let (a, b) = (hole[0].index(), hole[1].index());
    let mut rng = Rng::new(a.min(b) as u64 * 52 + a.max(b) as u64);
    let mut deck = rest.to_vec();
    let mut total = 0.0;
    for _ in 0..SAMPLES {
        // Partial shuffle: draw five board cards and two opponent cards.
        for i in 0..7 {
            let j = i + rng.below((deck.len() - i) as u64) as usize;
            deck.swap(i, j);
        }
        let mut cards = [hole[0]; 7];
        cards[..5].copy_from_slice(&deck[..5]);
        cards[6] = hole[1];
        let hero = evaluate(&cards);
        cards[5] = deck[5];
        cards[6] = deck[6];
        total += score(hero, evaluate(&cards));
    }
    total / SAMPLES as f64
}

/// How the opponent answers a bet or raise of one size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Response {
    pub amount: u32,
    /// Chance that the opponent folds.
    pub fold: f64,
    /// The player's equity against the hands that continue.
    pub equity_called: f64,
}

/// What the opponent does after the player checks, when it still has to act
/// on this street: bet, or check behind.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CheckResponse {
    /// Chance that the opponent bets.
    pub bet: f64,
    /// What it bets to.
    pub bet_to: u32,
    /// The player's equity against the hands that bet.
    pub equity_vs_bets: f64,
    /// The player's equity against the hands that check behind.
    pub equity_vs_checks: f64,
}

/// The situation the player faced when choosing an action.
#[derive(Clone, Debug)]
pub struct Spot {
    pub big_blind: u32,
    /// The player's cards.
    pub hole: [Card; 2],
    /// The board cards face up.
    pub board: Vec<Card>,
    pub street: Street,
    /// Chips in the middle before acting, including the bet being faced.
    pub pot: u32,
    pub to_call: u32,
    /// Chips the player has already put in on this street.
    pub street_commit: u32,
    /// Chips the opponent has put in on this street.
    pub opp_commit: u32,
    /// Smallest and largest legal `RaiseTo`, if raising is possible.
    pub raise: Option<(u32, u32)>,
    /// Chance of winning at showdown against the opponent's likely hands.
    pub equity: f64,
    /// The player's hand, from [`made_hand`].
    pub made: String,
    /// The player's draws, from [`draws`].
    pub draws: Vec<String>,
    /// Cards that complete the player's draws.
    pub outs: u32,
    /// The opponent's name, e.g. "calling station".
    pub opponent: &'static str,
    /// What the opponent's likely hands are made of, once its actions have
    /// narrowed them from any two cards.
    pub read: Option<Profile>,
    /// How the opponent answers each priced bet or raise size.
    pub responses: Vec<Response>,
    /// How the opponent answers a check, when it acts next on this street.
    pub check_response: Option<CheckResponse>,
    /// The share of its showdown equity the hand is counted as winning when
    /// more betting follows. Below one before the flop for weak hands, which
    /// are bet off their equity later; one after the flop and when all-in.
    pub realise: f64,
    /// The opponent's style, when it has a describable one.
    pub style: Option<crate::bot::Style>,
}

impl Spot {
    /// Analyses the spot for the player to act in `state` against `bot`,
    /// whose likely hands are `range`. `strengths` must be for the current
    /// board. Prices fold, check, call and the standard bet sizes, plus the
    /// raise to `extra` if given.
    pub fn analyse(
        state: &HandState,
        bot: &dyn Bot,
        range: &Range,
        strengths: &Strengths,
        extra: Option<u32>,
    ) -> Spot {
        let me = state.to_act().expect("no spot once the hand is over");
        let legal = state.legal().expect("no spot once the hand is over");
        let (hole, board) = (state.hole(me), state.board());
        let mut spot = Spot {
            big_blind: state.config().big_blind,
            hole,
            board: board.to_vec(),
            street: state.street(),
            pot: state.pot(),
            to_call: legal.to_call,
            street_commit: state.street_commit(me),
            opp_commit: state.street_commit(1 - me),
            raise: legal.raise,
            equity: 0.0,
            made: made_hand(hole, board),
            draws: draws(hole, board),
            outs: Draws::find(hole, board).outs(),
            opponent: bot.name(),
            read: None,
            responses: Vec::new(),
            check_response: None,
            realise: if state.street() == Street::Preflop {
                let position = if state.button() == me { 0.0 } else { 0.06 };
                (0.72 + 0.36 * strengths.of(hole) - position).min(1.0)
            } else {
                1.0
            },
            style: bot.style(),
        };

        let mut amounts: Vec<u32> = spot
            .options()
            .into_iter()
            .filter_map(|a| match a {
                Action::RaiseTo(amount) => Some(amount),
                _ => None,
            })
            .collect();
        if let Some(amount) = extra
            && legal.allows(Action::RaiseTo(amount))
            && !amounts.contains(&amount)
        {
            amounts.push(amount);
        }

        // For each size: how often the range folds, and what continues.
        let visible = |c: [Card; 2]| c.iter().all(|x| !hole.contains(x) && !board.contains(x));
        let mut folds = Vec::new();
        let mut continuing: Vec<Vec<f64>> = Vec::new();
        for &amount in &amounts {
            let mut after = state.clone();
            after
                .apply(Action::RaiseTo(amount))
                .expect("priced sizes are legal");
            let mut weights = vec![0.0; COMBOS];
            let (mut folded, mut total) = (0.0, 0.0);
            for (i, &weight) in range.weights().iter().enumerate() {
                let cards = combo(i);
                if weight <= 0.0 || !visible(cards) {
                    continue;
                }
                let mix = bot.tendencies(&after, cards, strengths);
                folded += weight * mix.fold;
                total += weight;
                weights[i] = weight * (1.0 - mix.fold);
            }
            folds.push(if total > 0.0 { folded / total } else { 0.0 });
            continuing.push(weights);
        }

        // Out of position, a check hands the bot the chance to bet.
        let mut check_split: Option<(f64, u32, Vec<f64>, Vec<f64>)> = None;
        if legal.to_call == 0 {
            let mut after = state.clone();
            after.apply(Action::Check).expect("checking is legal");
            if after.to_act() == Some(1 - me)
                && after.street() == state.street()
                && after.legal().is_some_and(|l| l.raise.is_some())
            {
                let bet_to = bot.raise_to(&after);
                let mut bets = vec![0.0; COMBOS];
                let mut checks = vec![0.0; COMBOS];
                let (mut betting, mut total) = (0.0, 0.0);
                for (i, &weight) in range.weights().iter().enumerate() {
                    let cards = combo(i);
                    if weight <= 0.0 || !visible(cards) {
                        continue;
                    }
                    let mix = bot.tendencies(&after, cards, strengths);
                    betting += weight * mix.aggressive;
                    total += weight;
                    bets[i] = weight * mix.aggressive;
                    checks[i] = weight * (1.0 - mix.aggressive);
                }
                let chance = if total > 0.0 { betting / total } else { 0.0 };
                check_split = Some((chance, bet_to, bets, checks));
            }
        }

        let mut ranges: Vec<&[f64]> = vec![range.weights()];
        ranges.extend(continuing.iter().map(Vec::as_slice));
        if let Some((_, _, bets, checks)) = &check_split {
            ranges.push(bets);
            ranges.push(checks);
        }
        let equities = equity_vs_ranges(hole, board, &ranges);
        if let Some((bet, bet_to, _, _)) = check_split {
            let n = equities.len();
            spot.check_response = Some(CheckResponse {
                bet,
                bet_to,
                equity_vs_bets: equities[n - 2],
                equity_vs_checks: equities[n - 1],
            });
        }
        spot.equity = equities[0];
        spot.responses = amounts
            .iter()
            .zip(folds)
            .zip(&equities[1..=amounts.len()])
            .map(|((&amount, fold), &equity_called)| Response {
                amount,
                fold,
                equity_called,
            })
            .collect();
        spot.read = (!range.is_uniform()).then(|| range.profile(strengths, &hole));
        spot
    }

    /// How the opponent answers a raise to `amount`, if that size was priced.
    pub fn response(&self, amount: u32) -> Option<Response> {
        self.responses.iter().copied().find(|r| r.amount == amount)
    }

    /// Average chips won from here on by taking `action`. The showdown value
    /// of each line, plus an estimate of the betting still to come: before
    /// the flop a weak hand is counted at less than its equity, and after it
    /// a hand ahead of what continues gains from later streets while a hand
    /// behind pays on them. After a check out of position, the opponent's
    /// bet and the player's best reply (call or fold) are included. `None`
    /// for a bet size that was not priced.
    pub fn ev(&self, action: Action) -> Option<f64> {
        let pot = self.pot as f64;
        // With chips behind, the hand wins `realise` of its showdown equity.
        let all_in = |amount: u32| self.raise.is_none_or(|(_, max)| amount >= max);
        let equity = self.equity * self.realise;
        match action {
            Action::Fold => Some(0.0),
            Action::Check => {
                let behind = self.behind(self.street_commit);
                Some(match self.check_response {
                    Some(c) => {
                        c.bet * self.call_after_check(&c).max(0.0)
                            + (1.0 - c.bet)
                                * (c.equity_vs_checks * self.realise * pot
                                    + self.later(pot, c.equity_vs_checks, behind))
                    }
                    None => equity * pot + self.later(pot, self.equity, behind),
                })
            }
            Action::Call => {
                let call = self.to_call as f64;
                if self.raise.is_none() {
                    // All-in: the showdown is all there is.
                    return Some(self.equity * (pot + call) - call);
                }
                let behind = self.behind(self.opp_commit);
                Some(equity * (pot + call) - call + self.later(pot + call, self.equity, behind))
            }
            Action::RaiseTo(amount) => {
                let r = self.response(amount)?;
                let risk = (amount - self.street_commit) as f64;
                let called = (amount - self.opp_commit) as f64;
                let total = pot + risk + called;
                let when_called = if all_in(amount) {
                    r.equity_called * total - risk
                } else {
                    r.equity_called * self.realise * total - risk
                        + self.later(total, r.equity_called, self.behind(amount))
                };
                Some(r.fold * pot + (1.0 - r.fold) * when_called)
            }
        }
    }

    /// Chips the player still has behind once the street's bets reach `level`.
    fn behind(&self, level: u32) -> f64 {
        self.raise
            .map_or(0.0, |(_, max)| max.saturating_sub(level) as f64)
    }

    /// An estimate of what the streets still to come add to a line that
    /// leaves a pot of `pot` with `behind` left to bet, for a hand with
    /// `equity` against what the opponent continues with. Ahead, more chips
    /// go in good; behind, more go in bad. A draw is spared the loss: it
    /// folds when it misses and gets paid when it hits.
    fn later(&self, pot: f64, equity: f64, behind: f64) -> f64 {
        // Ahead, betting two thirds of the pot and being called half the
        // time wins a third of the pot times twice the edge, on a pot that
        // grows with each call: 0.66, 1.43 and 2.3 edges over one, two and
        // three streets. Behind, the hand can fold, so it pays far less.
        let (ahead, behind_rate) = match self.street {
            Street::Preflop => (2.3, 0.45),
            Street::Flop => (1.43, 0.3),
            Street::Turn => (0.66, 0.15),
            Street::River => return 0.0,
        };
        let edge = equity - 0.5;
        if edge < 0.0 && self.outs >= 8 {
            return 0.0;
        }
        let rate = if edge > 0.0 { ahead } else { behind_rate };
        rate * edge * pot.min(behind)
    }

    /// Value of calling the opponent's bet after checking to it.
    fn call_after_check(&self, c: &CheckResponse) -> f64 {
        let pot = self.pot as f64;
        let added = c.bet_to.saturating_sub(self.opp_commit) as f64;
        let call = c.bet_to.saturating_sub(self.street_commit) as f64;
        let total = pot + added + call;
        c.equity_vs_bets * self.realise * total - call
            + self.later(total, c.equity_vs_bets, self.behind(c.bet_to))
    }

    /// Share of the final pot a call must win: zero when nothing is owed.
    pub fn pot_odds(&self) -> f64 {
        pot_odds(self.pot, self.to_call)
    }

    /// The legal actions worth comparing: fold or check, call, and bets of a
    /// third, half, three quarters and all of the pot, plus all-in when that
    /// is no more than twice the pot.
    pub fn options(&self) -> Vec<Action> {
        let mut options = if self.to_call > 0 {
            vec![Action::Fold, Action::Call]
        } else {
            vec![Action::Check]
        };
        if let Some(range) = self.raise {
            let sizes = standard_sizes(
                self.pot,
                self.to_call,
                self.opp_commit,
                self.big_blind,
                range,
            );
            // An all-in of many pots is not a size worth weighing against
            // the others; it is still priced if the player chooses it.
            let base = (self.pot + self.to_call) as f64;
            let sane = |amount: u32| (amount - self.opp_commit) as f64 <= 2.0 * base;
            options.extend(
                sizes
                    .into_iter()
                    .filter(|&amount| amount < range.1 || sane(amount))
                    .map(Action::RaiseTo),
            );
            options.dedup();
        }
        options
    }

    /// The priced options, best first. `extra` is included even if it is not
    /// one of the standard [`options`](Spot::options).
    pub fn ranked(&self, extra: Option<Action>) -> Vec<(Action, f64)> {
        let mut ranked: Vec<(Action, f64)> = Vec::new();
        for action in self.options().into_iter().chain(extra) {
            if let Some(ev) = self.ev(action)
                && !ranked.iter().any(|&(a, _)| a == action)
            {
                ranked.push((action, ev));
            }
        }
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
        ranked
    }

    /// The action in words, e.g. "betting 3bb (75% pot)".
    pub fn phrase(&self, action: Action) -> String {
        let bb = |chips: u32| fmt_bb(chips as f64, self.big_blind);
        match action {
            Action::Fold => "folding".to_string(),
            Action::Check => "checking".to_string(),
            Action::Call => format!("calling {}", bb(self.to_call)),
            Action::RaiseTo(amount) => {
                let verb = if self.opp_commit == 0 {
                    format!("betting {}", bb(amount))
                } else {
                    format!("raising to {}", bb(amount))
                };
                if self.raise.is_some_and(|(_, max)| amount == max) {
                    format!("{verb} all-in")
                } else {
                    let share =
                        (amount - self.opp_commit) as f64 / (self.pot + self.to_call) as f64;
                    format!("{verb} ({} pot)", percent(share))
                }
            }
        }
    }

    fn bb(&self, chips: f64) -> String {
        fmt_bb(chips, self.big_blind)
    }

    fn signed(&self, chips: f64) -> String {
        fmt_bb_signed(chips, self.big_blind)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Grade {
    Good,
    /// Wrong by less than a tenth of a big blind.
    Close,
    Inaccuracy,
    Mistake,
    Blunder,
    /// An observation with no price attached.
    Note,
}

impl Grade {
    fn from_loss(loss_bb: f64) -> Grade {
        match loss_bb {
            x if x < 0.1 => Grade::Close,
            x if x < 1.0 => Grade::Inaccuracy,
            x if x < 5.0 => Grade::Mistake,
            _ => Grade::Blunder,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Grade::Good => "Good",
            Grade::Close => "Close",
            Grade::Inaccuracy => "Inaccuracy",
            Grade::Mistake => "Mistake",
            Grade::Blunder => "Blunder",
            Grade::Note => "Note",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Feedback {
    pub grade: Grade,
    /// Estimated chips given up compared with the better option; zero if none.
    pub loss: f64,
    pub text: String,
}

fn percent(x: f64) -> String {
    format!("{:.0}%", x * 100.0)
}

fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

fn is_bet(action: Action) -> bool {
    matches!(action, Action::RaiseTo(_))
}

/// Reviews the action the player took in `spot`: what it was worth, what the
/// best option was, and the reasons that decided it.
pub fn review(spot: &Spot, action: Action) -> Feedback {
    let ranked = spot.ranked(Some(action));
    let Some(chosen) = spot.ev(action) else {
        return Feedback {
            grade: Grade::Note,
            loss: 0.0,
            text: unpriced_bet(spot, action),
        };
    };
    let (best, best_ev) = ranked[0];
    let loss = (best_ev - chosen).max(0.0);
    let grade = if loss < 1e-6 {
        Grade::Good
    } else {
        Grade::from_loss(loss / spot.big_blind as f64)
    };

    let mut text = vec![situation(spot)];
    if loss < 1e-6 {
        let runner_up = ranked.iter().find(|&&(a, _)| a != action);
        text.push(match runner_up {
            Some(&(alt, alt_ev)) => format!(
                "{} was best ({}); next, {} ({}).",
                capitalise(&spot.phrase(action)),
                spot.signed(chosen),
                spot.phrase(alt),
                spot.signed(alt_ev),
            ),
            None => format!(
                "{} was the only option ({}).",
                capitalise(&spot.phrase(action)),
                spot.signed(chosen)
            ),
        });
    } else {
        text.push(format!(
            "{} {}; {} {}: {} given up.",
            capitalise(&spot.phrase(action)),
            spot.signed(chosen),
            spot.phrase(best),
            spot.signed(best_ev),
            spot.bb(loss),
        ));
    }
    text.extend(reasons(spot, action, best));
    Feedback {
        grade,
        loss,
        text: text.join(" "),
    }
}

/// The facts that decided `action` in `spot`, one point each, most
/// important first. Empty for a bet size that was not priced.
pub fn why(spot: &Spot, action: Action) -> Vec<String> {
    if spot.ev(action).is_none() {
        return Vec::new();
    }
    let best = spot.ranked(Some(action))[0].0;
    reasons(spot, action, best)
}

/// The spot in a few words: "Pair of fives with a flush draw on the turn".
pub fn headline(spot: &Spot) -> String {
    let text = situation(spot);
    text.split(": ").next().unwrap_or(&text).to_string()
}

/// "A pair of fives and a flush draw on the turn: 41% against its likely hands."
fn situation(spot: &Spot) -> String {
    let mut hand = capitalise(&spot.made);
    if !spot.draws.is_empty() {
        let draws: Vec<&str> = spot
            .draws
            .iter()
            .map(|d| d.split(" (").next().unwrap_or(d))
            .collect();
        hand = format!("{hand} with a {}", draws.join(" and a "));
    }
    let when = match spot.street {
        Street::Preflop => "preflop".to_string(),
        street => format!("on the {}", street.name().to_lowercase()),
    };
    let against = if spot.read.is_some() {
        "against its likely hands"
    } else {
        "against a random hand"
    };
    format!("{hand} {when}: {} {against}.", percent(spot.equity))
}

/// The facts that decided this spot, chosen by what was done and what was best.
fn reasons(spot: &Spot, action: Action, best: Action) -> Vec<String> {
    let mut why = Vec::new();
    let equity = spot.equity;
    let pot = spot.pot as f64;
    let involves = |pred: fn(Action) -> bool| pred(action) || pred(best);

    // Fold or call: pot odds decide it.
    if spot.to_call > 0 && involves(|a| matches!(a, Action::Fold | Action::Call)) {
        if let Some(read) = spot.read {
            why.push(format!("Its range: {read}."));
        }
        let need = spot.pot_odds();
        let margin = equity - need;
        let verdict = if margin.abs() < 0.03 {
            "near coin-flip"
        } else if margin > 0.15 {
            "easy call"
        } else if margin > 0.0 {
            "enough"
        } else if margin > -0.15 {
            "just short"
        } else {
            "well short"
        };
        let draw = if !spot.draws.is_empty() && matches!(spot.street, Street::Flop | Street::Turn) {
            ", draw included"
        } else {
            ""
        };
        why.push(format!(
            "Calling {} into {} needs {}; you have {}{draw}: {verdict}.",
            spot.bb(spot.to_call as f64),
            spot.bb(pot),
            percent(need),
            percent(equity),
        ));
    }

    if spot.realise < 0.9 && spot.raise.is_some() {
        why.push(format!(
            "A weak hand wins less than its raw equity once betting continues: count it as {}.",
            percent(equity * spot.realise)
        ));
    }

    // Betting: how the opponent answers decides it.
    if involves(is_bet) {
        let bet = if is_bet(action) { action } else { best };
        if let Action::RaiseTo(amount) = bet
            && let Some(r) = spot.response(amount)
        {
            let risk = (amount - spot.street_commit) as f64;
            let needed = risk / (risk + pot);
            let called = percent(r.equity_called);
            if r.fold < 0.005 {
                why.push(if r.equity_called >= 0.5 {
                    format!("It does not fold here and you win {called} when called: every extra chip is profit.")
                } else {
                    format!("It does not fold here, so the bet cannot win the pot by itself; called, you win {called}.")
                });
            } else {
                let support = match (r.fold >= needed, r.equity_called >= 0.5) {
                    (true, true) => "folds alone pay, and you are ahead when called.",
                    (true, false) => "folds alone pay.",
                    (false, true) => "folds fall short; the value is in the calls.",
                    (false, false) => "folds fall short and the callers beat you.",
                };
                why.push(format!(
                    "{} gets folds {}; called, you win {called}. Folds alone need {}: {support}",
                    capitalise(&spot.phrase(bet)),
                    percent(r.fold),
                    percent(needed),
                ));
            }
        }
    }

    // Wrong bet size.
    if let (Action::RaiseTo(chosen), Action::RaiseTo(better)) = (action, best)
        && chosen != better
        && let (Some(c), Some(b)) = (spot.response(chosen), spot.response(better))
    {
        why.push(if better > chosen {
            if b.equity_called >= 0.5 {
                "Size up: you are ahead of the callers.".to_string()
            } else {
                format!(
                    "Size up: more folds ({} vs {}).",
                    percent(b.fold),
                    percent(c.fold)
                )
            }
        } else if c.equity_called < 0.5 {
            format!(
                "Size down: similar folds ({} vs {}) for less risk.",
                percent(b.fold),
                percent(c.fold)
            )
        } else {
            format!(
                "Size down: more worse hands call ({} vs {}).",
                percent(1.0 - b.fold),
                percent(1.0 - c.fold)
            )
        });
    }

    // Out of position, what a check leads to.
    if (action == Action::Check || best == Action::Check)
        && let Some(c) = spot.check_response
    {
        let reply = if spot.call_after_check(&c) > 0.0 {
            "call"
        } else {
            "fold"
        };
        why.push(format!(
            "If you check, the bot bets {}: you win {} against those bets ({reply}), {} when it checks back.",
            percent(c.bet),
            percent(c.equity_vs_bets),
            percent(c.equity_vs_checks),
        ));
    }

    // Checked when betting was better.
    if action == Action::Check
        && let Action::RaiseTo(amount) = best
        && let Some(r) = spot.response(amount)
    {
        why.push(if r.equity_called >= 0.5 {
            "Checking lets worse hands reach showdown free.".to_string()
        } else {
            format!(
                "Checking forgoes folds: it folds {} to that bet.",
                percent(r.fold)
            )
        });
    }

    // Bet when checking or calling was better.
    if let Action::RaiseTo(amount) = action
        && !is_bet(best)
        && best != Action::Fold
        && let Some(r) = spot.response(amount)
    {
        why.push(if r.equity_called < 0.5 {
            "The bet folds out worse hands and gets called by better.".to_string()
        } else {
            format!(
                "Its folds ({}) are mostly hands you already beat.",
                percent(r.fold)
            )
        });
    }
    why
}

/// A bet size that was not priced: describe it, don't grade it.
fn unpriced_bet(spot: &Spot, action: Action) -> String {
    let Action::RaiseTo(amount) = action else {
        unreachable!("only bets go unpriced")
    };
    let risk = (amount - spot.street_commit) as f64;
    let pot = spot.pot as f64;
    if spot.equity >= 0.5 {
        format!(
            "{} A value bet: you want calls from worse hands.",
            situation(spot)
        )
    } else {
        format!(
            "{} A bluff: risking {} to win {} needs folds {}.",
            situation(spot),
            spot.bb(risk),
            spot.bb(pot),
            percent(risk / (risk + pot)),
        )
    }
}

/// The facts behind a decision, as JSON for a language model to explain.
/// Amounts are in big blinds and chances in percent, rounded for reading.
/// With `action`, it includes the review of the action taken.
pub fn brief(spot: &Spot, action: Option<Action>) -> serde_json::Value {
    use serde_json::json;
    let bb = |chips: f64| (chips / spot.big_blind as f64 * 100.0).round() / 100.0;
    let pct = |x: f64| (x * 1000.0).round() / 10.0;
    let cards = |cs: &[Card]| {
        cs.iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join(" ")
    };
    let options: Vec<serde_json::Value> = spot
        .ranked(action)
        .into_iter()
        .map(|(a, ev)| {
            let mut option = json!({ "action": spot.phrase(a), "ev_bb": bb(ev) });
            if let Action::RaiseTo(amount) = a
                && let Some(r) = spot.response(amount)
            {
                option["opponent_folds_pct"] = json!(pct(r.fold));
                option["equity_when_called_pct"] = json!(pct(r.equity_called));
            }
            option
        })
        .collect();
    let mut facts = json!({
        "street": spot.street.name(),
        "opponent": spot.opponent,
        "hero_cards": cards(&spot.hole),
        "hero_starting_hand": crate::range::hand_class(spot.hole),
        "board": cards(&spot.board),
        "hero_hand": spot.made,
        "hero_draws": spot.draws,
        "pot_bb": bb(spot.pot as f64),
        "to_call_bb": bb(spot.to_call as f64),
        "equity_pct": pct(spot.equity),
        "equity_against": if spot.read.is_some() { "the opponent's likely hands given its actions" } else { "a random hand (the opponent's actions so far say nothing about its cards)" },
        "options_best_first": options,
    });
    if let Some(c) = spot.check_response {
        facts["if_hero_checks"] = json!({
            "opponent_bets_pct": pct(c.bet),
            "opponent_bets_to_bb": bb(c.bet_to as f64),
            "hero_equity_vs_those_bets_pct": pct(c.equity_vs_bets),
            "hero_equity_when_it_checks_behind_pct": pct(c.equity_vs_checks),
            "hero_best_reply_to_the_bet": if spot.call_after_check(&c) > 0.0 { "call" } else { "fold" },
        });
    }
    if spot.to_call > 0 {
        facts["pot_odds_needed_pct"] = json!(pct(spot.pot_odds()));
    }
    if let Some(read) = spot.read {
        facts["opponent_range_pct"] = json!({
            "strong": pct(read.strong),
            "medium": pct(read.medium),
            "draws": pct(read.draws),
            "weak": pct(read.weak),
        });
    }
    if let Some(style) = spot.style {
        facts["opponent_style"] = json!(style.blurb);
        facts["how_to_beat_it"] = json!(style.counter);
    }
    if spot.realise < 1.0 {
        facts["equity_counted_as_pct"] = json!(pct(spot.equity * spot.realise));
        facts["equity_note"] = json!(
            "before the flop, weak hands win less than their showdown equity once betting continues; option values use the counted figure unless all-in"
        );
    }
    if let Some(action) = action {
        let feedback = review(spot, action);
        facts["hero_action"] = json!(spot.phrase(action));
        facts["grade"] = json!(feedback.grade.label());
        facts["loss_bb"] = json!(bb(feedback.loss));
    }
    facts
}

/// A question the coach asks before a decision, to practise the steps of
/// making it: read the opponent's hands, estimate equity, work out the price.
#[derive(Clone, Debug, PartialEq)]
pub enum Question {
    /// What do the opponent's likely hands look like?
    Read(Profile),
    /// How often do you win at showdown?
    Equity,
    /// What share of the pot does a call need to win?
    PotOdds,
    /// How many cards complete the player's draw?
    Outs,
    /// How often does the opponent fold to a bet of this size?
    Folds(Response),
}

/// The questions worth asking in `spot`, in the order to ask them.
pub fn questions(spot: &Spot) -> Vec<Question> {
    let mut asked = Vec::new();
    if let Some(read) = spot.read {
        asked.push(Question::Read(read));
    }
    asked.push(Question::Equity);
    if spot.to_call > 0 {
        asked.push(Question::PotOdds);
    }
    asked
}

/// Every question that fits `spot`: the three steps of [`questions`], plus
/// counting outs with a draw and judging how often a bet gets a fold.
pub fn drills(spot: &Spot) -> Vec<Question> {
    let mut asked = questions(spot);
    if spot.outs >= 4 && matches!(spot.street, Street::Flop | Street::Turn) {
        asked.push(Question::Outs);
    }
    // A middling size: the all-in is the last response when it is priced.
    let sizes = spot.responses.len();
    if sizes > 0 {
        asked.push(Question::Folds(spot.responses[(sizes - 1) / 2]));
    }
    asked
}

/// "about 36% to hit" by the rule of four and two.
fn rule_of_outs(spot: &Spot) -> String {
    let (rule, cards) = if spot.street == Street::Flop {
        (4, "two cards")
    } else {
        (2, "one card")
    };
    format!(
        "Rule of {rule}: {} outs, {cards} to come, about {}% to hit.",
        spot.outs,
        spot.outs * rule
    )
}

/// What a bet to `response.amount` needs and gets in folds.
fn folds_text(spot: &Spot, response: &Response) -> String {
    let risk = response.amount.saturating_sub(spot.street_commit) as f64;
    let need = risk / (risk + spot.pot as f64);
    format!(
        "It folds {}. As a pure bluff that size needs {} folds to pay; your equity when called makes up the rest.",
        percent(response.fold),
        percent(need)
    )
}

/// "bet 6bb" or "raise to 12bb".
fn bet_words(spot: &Spot, amount: u32) -> String {
    let verb = if spot.opp_commit == 0 {
        "bet"
    } else {
        "raise to"
    };
    format!("{verb} {}", spot.bb(amount as f64))
}

/// Upper edges of the fold rates offered as answers.
const FOLD_EDGES: [f64; 4] = [0.2, 0.4, 0.6, 1.01];

/// How a strong range reads: mostly strong, mostly weak, or a mix.
fn read_label(read: &Profile) -> &'static str {
    if read.strong >= 0.4 {
        "strong"
    } else if read.weak + read.draws >= 0.6 {
        "weak"
    } else {
        "mixed"
    }
}

impl Question {
    /// The question as shown to the player, with how to answer.
    pub fn prompt(&self, spot: &Spot) -> String {
        match self {
            Question::Read(_) => {
                "After its actions, is the bot's range mostly [s]trong, mostly [w]eak, or [m]ixed?"
                    .to_string()
            }
            Question::Equity => {
                let against = if spot.read.is_some() {
                    "those hands"
                } else {
                    "a random hand"
                };
                format!("Your equity against {against}, in %?")
            }
            Question::PotOdds => format!(
                "Calling {} into {}: what equity do you need, in %?",
                spot.bb(spot.to_call as f64),
                spot.bb(spot.pot as f64)
            ),
            Question::Outs => "How many cards complete your draw?".to_string(),
            Question::Folds(response) => format!(
                "If you {}, how often does the bot fold, in %?",
                bet_words(spot, response.amount)
            ),
        }
    }

    /// Checks an answer. `Err` asks for another try with a hint on the format.
    pub fn check(&self, spot: &Spot, answer: &str) -> Result<Feedback, String> {
        let answer = answer.trim().to_lowercase();
        match self {
            Question::Read(read) => {
                let guess = match answer.chars().next() {
                    Some('s') => "strong",
                    Some('w') => "weak",
                    Some('m') => "mixed",
                    _ => return Err("Answer s, w or m.".into()),
                };
                let actual = read_label(read);
                let right = guess == actual;
                let why = if spot.to_call > 0 {
                    "Each bet strips out hands that would fold or check."
                } else {
                    "Checks and calls keep weak hands; strong hands mostly bet."
                };
                Ok(Feedback {
                    grade: if right {
                        Grade::Good
                    } else {
                        Grade::Inaccuracy
                    },
                    loss: 0.0,
                    text: format!(
                        "{} {read}: mostly {actual}. {why}",
                        if right { "Right." } else { "No." }
                    ),
                })
            }
            Question::Equity => {
                let guess = parse_percent(&answer)?;
                let actual = spot.equity;
                let off = guess - actual;
                let mut text = if off.abs() <= 0.07 {
                    format!("Close: {}.", percent(actual))
                } else {
                    format!(
                        "{}: you {} by {} points.",
                        percent(actual),
                        if off > 0.0 {
                            "overestimated"
                        } else {
                            "underestimated"
                        },
                        (off.abs() * 100.0).round()
                    )
                };
                if spot.read.is_some() && off > 0.07 {
                    text.push_str(" Measure against its range, not any two cards.");
                }
                if spot.outs > 0 && spot.street != Street::River {
                    let (rule, cards) = if spot.street == Street::Flop {
                        (4, "two cards")
                    } else {
                        (2, "one card")
                    };
                    text.push_str(&format!(
                        " Rule of {rule}: {} outs, {cards} to come, about {}% to hit.",
                        spot.outs,
                        spot.outs * rule
                    ));
                }
                let grade = match off.abs() {
                    x if x <= 0.07 => Grade::Good,
                    x if x <= 0.15 => Grade::Close,
                    _ => Grade::Inaccuracy,
                };
                Ok(Feedback {
                    grade,
                    loss: 0.0,
                    text,
                })
            }
            Question::PotOdds => {
                let guess = parse_percent(&answer)?;
                let need = spot.pot_odds();
                let total = spot.pot + spot.to_call;
                let working = format!(
                    "call ÷ (pot + call) = {} ÷ {} = {}",
                    spot.bb(spot.to_call as f64),
                    spot.bb(total as f64),
                    percent(need)
                );
                let right = (guess - need).abs() <= 0.03;
                Ok(Feedback {
                    grade: if right {
                        Grade::Good
                    } else {
                        Grade::Inaccuracy
                    },
                    loss: 0.0,
                    text: if right {
                        format!("Right: {working}.")
                    } else {
                        format!("No: {working}. Your call is part of the final pot.")
                    },
                })
            }
            Question::Outs => {
                let guess: u32 = answer
                    .parse()
                    .map_err(|_| "Answer with a number, e.g. 9.".to_string())?;
                let grade = match guess.abs_diff(spot.outs) {
                    0 => Grade::Good,
                    1 => Grade::Close,
                    _ => Grade::Inaccuracy,
                };
                Ok(Feedback {
                    grade,
                    loss: 0.0,
                    text: rule_of_outs(spot),
                })
            }
            Question::Folds(response) => {
                let guess = parse_percent(&answer)?;
                let grade = match (guess - response.fold).abs() {
                    x if x <= 0.1 => Grade::Good,
                    x if x <= 0.2 => Grade::Close,
                    _ => Grade::Inaccuracy,
                };
                Ok(Feedback {
                    grade,
                    loss: 0.0,
                    text: folds_text(spot, response),
                })
            }
        }
    }
}

/// The three things the questions practise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Skill {
    /// Reading the opponent's likely hands.
    Read,
    /// Estimating equity.
    Equity,
    /// Working out the price of a call.
    Price,
    /// Counting the cards that complete a draw.
    Outs,
    /// Judging how often a bet gets a fold.
    Folds,
}

impl Skill {
    pub const ALL: [Skill; 5] = [
        Skill::Read,
        Skill::Equity,
        Skill::Price,
        Skill::Outs,
        Skill::Folds,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Skill::Read => "Reads",
            Skill::Equity => "Equity",
            Skill::Price => "Price",
            Skill::Outs => "Outs",
            Skill::Folds => "Folds",
        }
    }
}

/// Upper edges of the equity ranges offered as answers.
const EQUITY_EDGES: [f64; 6] = [0.2, 0.35, 0.5, 0.65, 0.8, 1.01];

impl Question {
    pub fn skill(&self) -> Skill {
        match self {
            Question::Read(_) => Skill::Read,
            Question::Equity => Skill::Equity,
            Question::PotOdds => Skill::Price,
            Question::Outs => Skill::Outs,
            Question::Folds(_) => Skill::Folds,
        }
    }

    /// The question, short enough to answer with one key.
    pub fn ask(&self, spot: &Spot) -> String {
        match self {
            Question::Read(_) => "After its actions, the bot's likely hands are…".to_string(),
            Question::Equity => {
                if spot.read.is_some() {
                    "How often do you win against its likely hands?".to_string()
                } else {
                    "How often do you win against a random hand?".to_string()
                }
            }
            Question::PotOdds => format!(
                "Calling {} into {}: how often must you win?",
                spot.bb(spot.to_call as f64),
                spot.bb(spot.pot as f64)
            ),
            Question::Outs => "How many cards complete your draw?".to_string(),
            Question::Folds(response) => format!(
                "If you {}, how often does it fold?",
                bet_words(spot, response.amount)
            ),
        }
    }

    /// The answers to choose from and the index of the right one.
    pub fn choices(&self, spot: &Spot) -> (Vec<String>, usize) {
        match self {
            Question::Read(read) => {
                let labels = ["strong", "mixed", "weak"];
                let right = labels
                    .iter()
                    .position(|&l| l == read_label(read))
                    .unwrap_or(1);
                (
                    vec!["mostly strong".into(), "mixed".into(), "mostly weak".into()],
                    right,
                )
            }
            Question::Equity => {
                let names = [
                    "under 20%",
                    "20–35%",
                    "35–50%",
                    "50–65%",
                    "65–80%",
                    "over 80%",
                ];
                let right = EQUITY_EDGES
                    .iter()
                    .position(|&edge| spot.equity < edge)
                    .unwrap_or(5);
                (names.iter().map(|n| n.to_string()).collect(), right)
            }
            Question::PotOdds => {
                let (pot, call) = (spot.pot as f64, spot.to_call as f64);
                let need = (spot.pot_odds() * 100.0).round() as i64;
                // The usual slips: leaving your call out of the pot, or
                // dividing by the pot before the bet.
                let mut options = vec![need];
                for wrong in [
                    call / pot,
                    call / (pot - call).max(1.0),
                    call / (pot + 2.0 * call),
                    1.0 - spot.pot_odds(),
                ] {
                    let wrong = (wrong * 100.0).round() as i64;
                    if options.len() < 4
                        && (1..=99).contains(&wrong)
                        && options.iter().all(|&o| (o - wrong).abs() >= 3)
                    {
                        options.push(wrong);
                    }
                }
                let mut pad = 7;
                while options.len() < 4 {
                    let extra = (need + pad).rem_euclid(100).max(1);
                    if options.iter().all(|&o| (o - extra).abs() >= 3) {
                        options.push(extra);
                    }
                    pad += 9;
                }
                options.sort_unstable();
                let right = options.iter().position(|&o| o == need).unwrap();
                (options.iter().map(|o| format!("{o}%")).collect(), right)
            }
            Question::Outs => {
                let outs = spot.outs as i64;
                let mut wrong: Vec<i64> = [2, 4, 6, 8, 10, 12, 15, 18, 21]
                    .into_iter()
                    .filter(|x| (x - outs).abs() >= 2)
                    .collect();
                wrong.sort_by_key(|x| (x - outs).abs());
                let mut options: Vec<i64> = wrong.into_iter().take(3).collect();
                options.push(outs);
                options.sort_unstable();
                let right = options.iter().position(|&o| o == outs).unwrap();
                (options.iter().map(|o| o.to_string()).collect(), right)
            }
            Question::Folds(response) => {
                let names = ["under 20%", "20–40%", "40–60%", "over 60%"];
                let right = FOLD_EDGES
                    .iter()
                    .position(|&edge| response.fold < edge)
                    .unwrap_or(3);
                (names.iter().map(|n| n.to_string()).collect(), right)
            }
        }
    }

    /// Marks the answer `choice`, an index into [`choices`](Question::choices).
    pub fn answer(&self, spot: &Spot, choice: usize) -> Feedback {
        let (_, right) = self.choices(spot);
        let correct = choice == right;
        let (grade, text) = match self {
            Question::Read(read) => {
                let why = if spot.to_call > 0 {
                    "Each bet strips out hands that would fold or check."
                } else {
                    "Checks and calls keep weak hands; strong hands mostly bet."
                };
                (
                    if correct {
                        Grade::Good
                    } else {
                        Grade::Inaccuracy
                    },
                    // The reason is for those who got it wrong.
                    if correct {
                        format!("{read}.")
                    } else {
                        format!("{read}. {why}")
                    },
                )
            }
            Question::Equity => {
                let actual = spot.equity;
                // A neighbouring range counts as close when the truth sits near its edge.
                let near = choice.abs_diff(right) == 1 && {
                    let edge = EQUITY_EDGES[choice.min(right)];
                    (actual - edge).abs() <= 0.04
                };
                let mut text = format!("{}.", percent(actual));
                if !correct && !near {
                    text.push_str(if choice > right {
                        " You were too hopeful."
                    } else {
                        " You were too cautious."
                    });
                    if spot.read.is_some() && choice > right {
                        text.push_str(
                            " Measure against the hands it would play this way, not any two cards.",
                        );
                    }
                }
                if spot.outs > 0 && spot.street != Street::River {
                    let (rule, cards) = if spot.street == Street::Flop {
                        (4, "two cards")
                    } else {
                        (2, "one card")
                    };
                    text.push_str(&format!(
                        " Rule of {rule}: {} outs, {cards} to come, about {}% to hit.",
                        spot.outs,
                        spot.outs * rule
                    ));
                }
                let grade = if correct {
                    Grade::Good
                } else if near {
                    Grade::Close
                } else {
                    Grade::Inaccuracy
                };
                (grade, text)
            }
            Question::PotOdds => {
                let total = spot.pot + spot.to_call;
                let working = format!(
                    "call ÷ (pot + call) = {} ÷ {} = {}",
                    spot.bb(spot.to_call as f64),
                    spot.bb(total as f64),
                    percent(spot.pot_odds())
                );
                if correct {
                    (Grade::Good, format!("{working}."))
                } else {
                    (
                        Grade::Inaccuracy,
                        format!("{working}. Your call is part of the final pot."),
                    )
                }
            }
            Question::Outs => (
                if correct {
                    Grade::Good
                } else {
                    Grade::Inaccuracy
                },
                format!(
                    "{}. {}",
                    capitalise(&spot.draws.join(" and ")),
                    rule_of_outs(spot)
                ),
            ),
            Question::Folds(response) => {
                let near = choice.abs_diff(right) == 1
                    && (response.fold - FOLD_EDGES[choice.min(right)]).abs() <= 0.04;
                let grade = if correct {
                    Grade::Good
                } else if near {
                    Grade::Close
                } else {
                    Grade::Inaccuracy
                };
                (grade, folds_text(spot, response))
            }
        };
        Feedback {
            grade,
            loss: 0.0,
            text,
        }
    }
}

/// Parses "35", "35%" or "0.35" as a share between 0 and 1.
fn parse_percent(answer: &str) -> Result<f64, String> {
    let number: f64 = answer
        .trim_end_matches('%')
        .trim()
        .parse()
        .map_err(|_| "Answer with a number, e.g. 35.".to_string())?;
    let share = if number > 1.0 { number / 100.0 } else { number };
    if (0.0..=1.0).contains(&share) {
        Ok(share)
    } else {
        Err("Answer between 0 and 100.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::parse_cards;

    fn hole(s: &str) -> [Card; 2] {
        let c = parse_cards(s).unwrap();
        [c[0], c[1]]
    }

    fn equity(hand: &str, board: &str) -> f64 {
        equity_vs_random(hole(hand), &parse_cards(board).unwrap())
    }

    fn close(actual: f64, expected: f64, tolerance: f64) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "got {actual}, expected about {expected}"
        );
    }

    #[test]
    fn preflop_equities_match_known_values() {
        close(equity("As Ah", ""), 0.852, 0.01);
        close(equity("7s 2h", ""), 0.346, 0.01);
        close(equity("As Ks", ""), 0.670, 0.01);
        // Deterministic: the same hand gives the same number.
        assert_eq!(equity("Qs Jh", ""), equity("Qs Jh", ""));
    }

    #[test]
    fn postflop_equities_are_exact() {
        // The nuts on the river always wins.
        assert_eq!(equity("As Ks", "Qs Js Ts 2c 2d"), 1.0);
        // A royal flush on the board is always a split.
        assert_eq!(equity("2c 2d", "Ts Js Qs Ks As"), 0.5);
        // Top set on a dry flop is a huge favourite; seven high is not.
        assert!(equity("As Ah", "Ad 7c 2h") > 0.95);
        assert!(equity("7s 2h", "Ad Kc 9h") < 0.25);
        // Top two pair on a paired river board is strong, but trips and full houses beat it.
        let e = equity("Ah Kh", "Ac Kd 7s 7h 2c");
        assert!(e > 0.85 && e < 0.97, "{e}");
    }

    #[test]
    fn hand_descriptions() {
        let describe =
            |hand: &str, board: &str| describe_hand(hole(hand), &parse_cards(board).unwrap());
        assert_eq!(describe("5d 5h", ""), "pocket fives");
        assert_eq!(describe("Kd As", ""), "ace-king offsuit");
        assert_eq!(describe("7h 6h", ""), "seven-six suited");
        assert_eq!(describe("5d 5h", "Ac 4h Kd"), "a pair of fives");
        assert_eq!(
            describe("Ah 9h", "Kh 4h 2c"),
            "ace high · flush draw (9 outs)"
        );
        assert_eq!(
            describe("9s 8d", "7c 6h Kd"),
            "king high · open-ended straight draw (8 outs)"
        );
        assert_eq!(
            describe("9s 8d", "6c 5h Kd"),
            "king high · gutshot straight draw (4 outs)"
        );
        // No draws are reported once all the cards are out, or when already made.
        assert_eq!(describe("Ah 9h", "Kh 4h 2c 3s 8d"), "ace high");
        assert_eq!(describe("Ah 9h", "Kh 4h 2h"), "a flush, ace high");
        // Four of a suit on the board alone is not our flush draw.
        assert_eq!(describe("As Ad", "Kh 4h 2h 9h"), "a pair of aces");
    }

    #[test]
    fn straight_draws_need_a_hole_card() {
        let describe =
            |hand: &str, board: &str| describe_hand(hole(hand), &parse_cards(board).unwrap());
        // The board is four to a straight: any 4 or 9 makes it for everyone.
        assert_eq!(describe("Ah 2c", "5d 6s 7h 8c"), "ace high");
        // A 9 makes our hand a better straight than the board's, so it is a draw.
        assert_eq!(
            describe("Ts 2c", "5d 6s 7h 8c"),
            "ten high · gutshot straight draw (4 outs)"
        );
        // Two different inside cards complete it: a double gutshot, not open-ended.
        assert_eq!(
            describe("9s 7d", "Jc 8h 5d"),
            "jack high · double gutshot straight draw (8 outs)"
        );
        // Two-three-four-five is open at both ends: an ace or a six.
        assert_eq!(
            describe("2s 3d", "4c 5h Kd"),
            "king high · open-ended straight draw (8 outs)"
        );
        // Jack-queen-king-ace can only be completed by a ten.
        assert_eq!(
            describe("As Kd", "Qc Jh 3d"),
            "ace high · gutshot straight draw (4 outs)"
        );
    }

    #[test]
    fn preflop_equity_ignores_card_order() {
        assert_eq!(equity("As Ks", ""), equity("Ks As", ""));
    }

    fn spot(pot: u32, to_call: u32, equity: f64) -> Spot {
        let c = parse_cards("5d 5h Ac 4h Kd").unwrap();
        Spot {
            big_blind: 100,
            hole: [c[0], c[1]],
            board: c[2..].to_vec(),
            street: Street::Flop,
            pot,
            to_call,
            street_commit: 0,
            opp_commit: to_call,
            raise: None,
            equity,
            made: "a pair of fives".to_string(),
            draws: Vec::new(),
            outs: 0,
            opponent: "random",
            read: None,
            responses: Vec::new(),
            check_response: None,
            realise: 1.0,
            style: None,
        }
    }

    /// A spot where every bet size gets folds `fold` of the time, and the
    /// hands that continue are like any other (card-blind opponents).
    fn betting(pot: u32, equity: f64, fold: f64) -> Spot {
        let mut spot = Spot {
            raise: Some((100, 10_000)),
            ..spot(pot, 0, equity)
        };
        spot.responses = spot
            .options()
            .into_iter()
            .filter_map(|a| match a {
                Action::RaiseTo(amount) => Some(Response {
                    amount,
                    fold,
                    equity_called: equity,
                }),
                _ => None,
            })
            .collect();
        spot
    }

    #[test]
    fn calls_and_folds_are_priced_against_pot_odds() {
        // Call 100 into 300: needs 25%.
        close(pot_odds(300, 100), 0.25, 1e-9);

        let good_call = review(&spot(300, 100, 0.40), Action::Call);
        assert_eq!(good_call.grade, Grade::Good);
        assert!(good_call.text.contains("needs 25%"), "{}", good_call.text);

        // Folding 40% equity gives up 0.4 * 400 - 100 = 60 chips.
        let bad_fold = review(&spot(300, 100, 0.40), Action::Fold);
        assert_eq!(bad_fold.grade, Grade::Inaccuracy);
        close(bad_fold.loss, 60.0, 1e-9);
        assert!(
            bad_fold.text.contains("calling 1bb +0.6bb"),
            "{}",
            bad_fold.text
        );

        // Calling a pot-sized 2000 with 10% loses 2000 - 0.1 * 6000 = 1400.
        let bad_call = review(&spot(4000, 2000, 0.10), Action::Call);
        assert_eq!(bad_call.grade, Grade::Blunder);
        close(bad_call.loss, 1400.0, 1e-9);
        assert!(bad_call.text.contains("well short"), "{}", bad_call.text);

        let good_fold = review(&spot(4000, 2000, 0.10), Action::Fold);
        assert_eq!(good_fold.grade, Grade::Good);
        assert_eq!(good_fold.loss, 0.0);

        // Exactly at the odds, either choice is fine.
        let coin_flip = review(&spot(300, 100, 0.2501), Action::Fold);
        assert_eq!(coin_flip.grade, Grade::Close);
        assert!(coin_flip.text.contains("coin-flip"), "{}", coin_flip.text);
    }

    #[test]
    fn bets_are_priced_against_the_opponents_fold_rate() {
        // Against a calling station, a strong hand should bet the biggest
        // size on offer. An all-in of 25 pots is not one of them.
        let station = betting(400, 0.8, 0.0);
        let (best, _) = station.ranked(None)[0];
        assert_eq!(best, Action::RaiseTo(400));
        assert!(!station.options().contains(&Action::RaiseTo(10_000)));
        // With less behind, all-in is a size like any other.
        let short = Spot {
            raise: Some((100, 700)),
            ..spot(400, 0, 0.8)
        };
        assert!(short.options().contains(&Action::RaiseTo(700)));
        let small = review(&station, Action::RaiseTo(150));
        assert!(small.loss > 0.0);
        assert!(small.text.contains("Size up"), "{}", small.text);
        let slowplay = review(&station, Action::Check);
        assert!(slowplay.grade != Grade::Good);
        assert!(slowplay.text.contains("does not fold"), "{}", slowplay.text);

        // A weak hand should not bluff a player who never folds.
        let weak = betting(400, 0.2, 0.0);
        assert_eq!(review(&weak, Action::Check).grade, Grade::Good);
        let bluff = review(&weak, Action::RaiseTo(400));
        assert!(bluff.grade != Grade::Good);
        assert!(bluff.text.contains("cannot win the pot"), "{}", bluff.text);

        // A bot that folds 30% to any size on the river: a small bluff beats
        // a big one. Betting 200 into 400 needs folds 200 / 600 = 33% of the time.
        let mut folder = betting(400, 0.2, 0.3);
        folder.street = Street::River;
        let half = review(&folder, Action::RaiseTo(200));
        assert!(half.text.contains("33%"), "{}", half.text);
        assert!(half.text.contains("Size down"), "{}", half.text);
    }

    #[test]
    fn every_drill_has_one_right_answer() {
        use crate::bot::SolidBot;
        use crate::cards::shuffled_deck;
        use crate::game::Config;
        let mut bot = SolidBot::new(3);
        let mut kinds = std::collections::HashSet::new();
        for seed in 0..9 {
            let mut state =
                HandState::deal(Config::default(), (seed % 2) as usize, &shuffled_deck(seed));
            let mut range = Range::uniform();
            let mut strengths = Strengths::new(state.board());
            while let Some(seat) = state.to_act() {
                let action = if seat == 0 {
                    let spot = Spot::analyse(&state, &bot, &range, &strengths, None);
                    for question in drills(&spot) {
                        let (choices, right) = question.choices(&spot);
                        assert!(right < choices.len(), "{question:?}");
                        let mut unique = choices.clone();
                        unique.dedup();
                        assert_eq!(unique.len(), choices.len(), "{choices:?}");
                        assert_eq!(question.answer(&spot, right).grade, Grade::Good);
                        assert!(!question.ask(&spot).is_empty());
                        kinds.insert(question.skill() as usize);
                    }
                    if state.legal().unwrap().to_call > 0 {
                        Action::Call
                    } else {
                        Action::Check
                    }
                } else {
                    let action = bot.act(&state, seat);
                    range.update(|hole| bot.tendencies(&state, hole, &strengths).chance_of(action));
                    action
                };
                state.apply(action).unwrap();
                if strengths.board() != state.board() {
                    strengths = Strengths::new(state.board());
                }
            }
        }
        assert_eq!(kinds.len(), Skill::ALL.len(), "{kinds:?}");
    }

    #[test]
    fn one_key_answers_are_marked() {
        let facing = spot(300, 100, 0.40);
        // Pot odds: 100 into 300 needs 25%, offered among the usual slips.
        let (choices, right) = Question::PotOdds.choices(&facing);
        assert_eq!(choices.len(), 4);
        assert_eq!(choices[right], "25%");
        assert!(choices.contains(&"33%".to_string()), "{choices:?}");
        assert_eq!(Question::PotOdds.answer(&facing, right).grade, Grade::Good);
        let wrong = Question::PotOdds.answer(&facing, (right + 1) % 4);
        assert_eq!(wrong.grade, Grade::Inaccuracy);
        assert!(
            wrong.text.contains("part of the final pot"),
            "{}",
            wrong.text
        );

        // Equity: 40% is in the 35–50% range; 50–65% is wrong, not close.
        let (choices, right) = Question::Equity.choices(&facing);
        assert_eq!(choices[right], "35–50%");
        assert_eq!(Question::Equity.answer(&facing, right).grade, Grade::Good);
        assert_eq!(
            Question::Equity.answer(&facing, right + 1).grade,
            Grade::Inaccuracy
        );
        // 48% is close enough to the edge for the next range up to count as close.
        let edge = spot(300, 100, 0.48);
        let (_, right) = Question::Equity.choices(&edge);
        assert_eq!(
            Question::Equity.answer(&edge, right + 1).grade,
            Grade::Close
        );
    }

    #[test]
    fn weak_hands_are_discounted_before_the_flop_unless_all_in() {
        let mut weak = Spot {
            raise: Some((200, 10_000)),
            street: Street::Preflop,
            realise: 0.75,
            ..spot(300, 100, 0.40)
        };
        // Counted at 30%, a call needing 25% is still good, but worth less.
        close(
            weak.ev(Action::Call).unwrap(),
            0.30 * 400.0 - 100.0 + 0.15 * 3.0 * -0.1 * 400.0,
            1e-9,
        );
        // Facing an all-in there is no more betting: full equity.
        weak.raise = None;
        close(weak.ev(Action::Call).unwrap(), 0.40 * 400.0 - 100.0, 1e-9);
    }

    #[test]
    fn unpriced_sizes_get_notes() {
        let value = review(&spot(400, 0, 0.8), Action::RaiseTo(300));
        assert_eq!(value.grade, Grade::Note);
        assert!(value.text.contains("value bet"), "{}", value.text);
        let bluff = review(&spot(400, 0, 0.2), Action::RaiseTo(200));
        assert!(bluff.text.contains("33%"), "{}", bluff.text);
        assert_eq!(bluff.loss, 0.0);
    }

    #[test]
    fn questions_check_the_steps_of_a_decision() {
        let facing = spot(300, 100, 0.40);
        assert_eq!(
            questions(&facing),
            vec![Question::Equity, Question::PotOdds]
        );
        let odds = Question::PotOdds;
        assert_eq!(odds.check(&facing, "25").unwrap().grade, Grade::Good);
        assert_eq!(odds.check(&facing, "0.25").unwrap().grade, Grade::Good);
        let wrong = odds.check(&facing, "33%").unwrap();
        assert_eq!(wrong.grade, Grade::Inaccuracy);
        assert!(wrong.text.contains("1bb ÷ 4bb = 25%"), "{}", wrong.text);
        assert!(odds.check(&facing, "lots").is_err());
        assert!(odds.check(&facing, "250").is_err());

        let equity = Question::Equity.check(&facing, "70").unwrap();
        assert_eq!(equity.grade, Grade::Inaccuracy);
        assert!(
            equity.text.contains("overestimated by 30"),
            "{}",
            equity.text
        );

        let read = Profile {
            strong: 0.5,
            medium: 0.3,
            draws: 0.1,
            weak: 0.1,
        };
        let narrowed = Spot {
            read: Some(read),
            ..facing
        };
        assert_eq!(questions(&narrowed)[0], Question::Read(read));
        assert_eq!(
            Question::Read(read).check(&narrowed, "s").unwrap().grade,
            Grade::Good
        );
        assert_eq!(
            Question::Read(read).check(&narrowed, "weak").unwrap().grade,
            Grade::Inaccuracy
        );
    }

    #[test]
    fn checks_out_of_position_price_the_bots_bet() {
        use crate::bot::SolidBot;
        use crate::game::Config;
        let c = parse_cards("Ah Td Kc Qc 9s 7d 2h Jc 3s").unwrap();
        let hole = [[c[0], c[1]], [c[2], c[3]]];
        let board = [c[4], c[5], c[6], c[7], c[8]];
        let bot = SolidBot::new(0);
        // Seat 0 is the big blind: first to act on the flop.
        let mut state = HandState::new(Config::default(), 1, hole, board);
        state.apply(Action::Call).unwrap();
        state.apply(Action::Check).unwrap();
        let strengths = Strengths::new(state.board());
        let range = Range::uniform();
        let oop = Spot::analyse(&state, &bot, &range, &strengths, None);
        let c = oop.check_response.expect("the bot acts after a check");
        assert!(c.bet > 0.0 && c.bet < 1.0, "{c:?}");
        assert!(
            c.equity_vs_bets < c.equity_vs_checks,
            "bets come from stronger hands: {c:?}"
        );
        let check = oop.ev(Action::Check).unwrap();
        assert!(check >= (1.0 - c.bet) * c.equity_vs_checks * oop.pot as f64 - 1e-9);
        let text = review(&oop, Action::Check).text;
        assert!(text.contains("If you check, the bot bets"), "{text}");

        // In position, checking ends the street: no reply to price.
        state.apply(Action::Check).unwrap();
        let ip = Spot::analyse(&state, &bot, &range, &strengths, None);
        assert!(ip.check_response.is_none());
    }

    #[test]
    fn analysis_reads_the_solid_bot() {
        use crate::bot::SolidBot;
        use crate::game::Config;
        // Hero has one pair on the river and the solid bot bets big.
        let c = parse_cards("9s 9d 2c 3c Ks 7d 4h Jc 2s").unwrap();
        let mut state = HandState::new(
            Config::default(),
            0,
            [[c[0], c[1]], [c[2], c[3]]],
            [c[4], c[5], c[6], c[7], c[8]],
        );
        let bot = SolidBot::new(0);
        let mut range = Range::uniform();
        // Hero raises, the bot calls; then it checks the flop and turn, and bets the river.
        for (action, by_bot) in [
            (Action::RaiseTo(250), false),
            (Action::Call, true),
            (Action::Check, true),
            (Action::Check, false),
            (Action::Check, true),
            (Action::Check, false),
            (Action::RaiseTo(500), true),
        ] {
            if by_bot {
                let strengths = Strengths::new(state.board());
                range.update(|h| bot.tendencies(&state, h, &strengths).chance_of(action));
            }
            state.apply(action).unwrap();
        }
        let strengths = Strengths::new(state.board());
        let spot = Spot::analyse(&state, &bot, &range, &strengths, None);
        let random = equity_vs_random(state.hole(0), state.board());
        assert!(spot.read.is_some());
        assert!(
            spot.equity < random - 0.1,
            "a bet should make the bot's range stronger: {} vs {random}",
            spot.equity
        );
        assert!(spot.responses.iter().all(|r| (0.0..=1.0).contains(&r.fold)));
    }

    #[test]
    fn options_cover_standard_sizes_within_the_legal_range() {
        let spot = Spot {
            raise: Some((200, 1_000)),
            ..spot(1_200, 0, 0.5)
        };
        // A third, half and three-quarters of 1200 are 400, 600 and 900; pot is capped by all-in.
        assert_eq!(
            spot.options(),
            vec![
                Action::Check,
                Action::RaiseTo(400),
                Action::RaiseTo(600),
                Action::RaiseTo(900),
                Action::RaiseTo(1_000),
            ]
        );
        assert_eq!(spot.phrase(Action::RaiseTo(600)), "betting 6bb (50% pot)");
        assert_eq!(spot.phrase(Action::RaiseTo(1_000)), "betting 10bb all-in");
    }

    #[test]
    fn big_blind_formatting() {
        assert_eq!(fmt_bb(250.0, 100), "2.5bb");
        assert_eq!(fmt_bb(100.0, 100), "1bb");
        assert_eq!(fmt_bb(239.0, 100), "2.39bb");
        assert_eq!(fmt_bb(-0.1, 100), "0bb");
        assert_eq!(fmt_bb_signed(60.0, 100), "+0.6bb");
        assert_eq!(fmt_bb_signed(-60.0, 100), "-0.6bb");
        assert_eq!(fmt_bb_signed(0.0, 100), "0bb");
    }
}
