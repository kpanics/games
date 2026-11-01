//! The client's chat (Interface → Custom Chat): every message the game's chat gets comes here as its runs (words,
//! colour, bold…, whether it is clickable, what its hover says), is read the way the client reads a server (parse.rs:
//! whose line, which rank and clan, which event or request, a private message) and drawn anew in the client's type —
//! closed as one glass panel behind its newest lines, open as a window of its own (view.rs). What the client cannot
//! draw (a server's pack glyphs, faces, items) Java draws where it says (the HUD's icons 21–24); what is clicked goes
//! back to Java to be done (the outbox).

pub mod body;
pub mod parse;
pub mod store;
mod view;

pub use store::{add, clear, escape, faces, find, input, online, sent, server, show_hidden, talking_to};
