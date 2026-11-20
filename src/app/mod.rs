//! The command-line front ends: a full-screen table and a plain line mode.

pub mod coach_ai;
pub mod llm;
pub mod plain;
pub mod text;
pub mod tui;

use clap::ValueEnum;
use poker::coach::{Spot, brief, fmt_bb};
use poker::game::Action;
use poker::session::Session;
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum CoachMode {
    /// No coaching.
    Off,
    /// A review after each decision.
    After,
    /// Reviews, plus your equity, the pot odds and the coach's pick before you act.
    Full,
    /// The coach asks you questions before each decision, then reviews it.
    #[default]
    Teach,
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
