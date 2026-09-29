//! Omatalk: the CLI and the Daemon in one crate. `main.rs` only calls `cli::main`.
//!
//! Hotkey to first audio reads in three files: `cli.rs` (argv to a socket
//! request), `daemon/` (request to a decision on the actor), `stream.rs`
//! (decision to synthesized PCM in pw-cat).

pub mod cli;
pub mod config;
pub mod daemon;
pub mod exec;
pub mod protocol;
pub mod sink;
pub mod speech;
pub mod stream;
pub mod voices;

#[cfg(test)]
mod testutil;
