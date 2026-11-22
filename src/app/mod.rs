//! The command-line front ends: a full-screen table and a plain line mode.

pub mod llm;
pub mod plain;
pub mod tui;

use clap::ValueEnum;
pub use poker::chat as coach_ai;
pub use poker::chat::{facts, session_facts};
pub use poker::text;

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
