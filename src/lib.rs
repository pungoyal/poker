//! Heads-up No-Limit Hold'em engine, opponents and coach. Pure: no input,
//! output, clocks or global state. The browser client in `web/` sits on top.

pub mod analysis;
pub mod bot;
pub mod cards;
pub mod chat;
pub mod coach;
pub mod eval;
pub mod game;
pub mod history;
pub mod range;
pub mod session;
pub mod text;
