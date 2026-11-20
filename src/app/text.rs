//! Words for actions and hands, shared by every front end.

use poker::cards::Card;
use poker::coach::fmt_bb;
use poker::game::{Action, HandState, Legal, Street};

pub fn bb(chips: u32, big_blind: u32) -> String {
    fmt_bb(chips as f64, big_blind)
}

pub fn bb_signed(chips: i64, big_blind: u32) -> String {
    poker::coach::fmt_bb_signed(chips as f64, big_blind)
}

pub fn cards(cards: &[Card]) -> String {
    cards
        .iter()
        .map(|c| c.pretty(false))
        .collect::<Vec<_>>()
        .join(" ")
}

/// "You raise to 2.5bb.", "Bot calls 1.5bb (all-in)."
pub fn describe(who: &str, action: Action, legal: Legal, before: &HandState) -> String {
    let big_blind = before.config().big_blind;
    let s = if who == "You" { "" } else { "s" };
    match action {
        Action::Fold => format!("{who} fold{s}."),
        Action::Check => format!("{who} check{s}."),
        Action::Call => {
            let me = before.to_act().unwrap_or(0);
            let all_in = legal.to_call >= before.stack(me);
            let suffix = if all_in { " (all-in)" } else { "" };
            format!("{who} call{s} {}{suffix}.", bb(legal.to_call, big_blind))
        }
        Action::RaiseTo(amount) => {
            let all_in = legal.raise.is_some_and(|(_, max)| amount == max);
            let suffix = if all_in { " (all-in)" } else { "" };
            let opp = 1 - before.to_act().unwrap_or(0);
            if before.street_commit(opp) == 0 {
                format!("{who} bet{s} {}{suffix}.", bb(amount, big_blind))
            } else {
                format!("{who} raise{s} to {}{suffix}.", bb(amount, big_blind))
            }
        }
    }
}

/// The hand so far, one line per street, e.g.
/// "Flop A♠ 7♣ 2♦: Bot checks. You bet 3bb."
pub fn narrate(state: &HandState, hero: usize) -> Vec<String> {
    let cfg = state.config();
    let mut walk = HandState::new(
        cfg,
        state.button(),
        [state.hole(0), state.hole(1)],
        state.full_board(),
    );
    let mut lines = Vec::new();
    let mut line = "Preflop:".to_string();
    let mut street = Street::Preflop;
    for &(seat, action) in state.actions() {
        let legal = walk.legal().expect("recorded actions are legal");
        let who = if seat == hero { "You" } else { "Bot" };
        line.push(' ');
        line.push_str(&describe(who, action, legal, &walk));
        walk.apply(action).expect("recorded actions are legal");
        if walk.street() != street && walk.outcome().is_none() {
            street = walk.street();
            lines.push(std::mem::take(&mut line));
            line = format!("{} {}:", street.name(), cards(walk.board()));
        }
    }
    lines.push(line);
    lines
}

/// "button (small blind)" or "big blind".
pub fn position(state: &HandState, seat: usize) -> &'static str {
    if state.button() == seat {
        "button"
    } else {
        "big blind"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use poker::cards::parse_cards;
    use poker::game::Config;

    #[test]
    fn narrates_by_street() {
        let c = parse_cards("As Ah Ks Kh 2c 7d 9h Jc 3s").unwrap();
        let mut state = HandState::new(
            Config::default(),
            0,
            [[c[0], c[1]], [c[2], c[3]]],
            [c[4], c[5], c[6], c[7], c[8]],
        );
        for action in [
            Action::RaiseTo(250),
            Action::Call,
            Action::Check,
            Action::RaiseTo(300),
        ] {
            state.apply(action).unwrap();
        }
        let lines = narrate(&state, 0);
        assert_eq!(
            lines,
            [
                "Preflop: You raise to 2.5bb. Bot calls 1.5bb.",
                "Flop 2♣ 7♦ 9♥: Bot checks. You bet 3bb.",
            ]
        );
    }
}
