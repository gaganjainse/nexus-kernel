//! `nexusaos-terminal` — PTY manager and VT100/ANSI terminal parser.

pub mod parser;
pub mod pty;

pub use parser::{TerminalEmulator, TerminalScreen};
pub use pty::PtyManager;
