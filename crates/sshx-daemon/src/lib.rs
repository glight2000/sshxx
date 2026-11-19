//! Library code for the sshxx terminal daemon.
//!
//! This crate does not forbid use of unsafe code because it needs to interact
//! with operating-system APIs to access pseudoterminal (PTY) devices.

#![deny(unsafe_code)]
#![warn(missing_docs)]

/// Daemon component version advertised in the connection handshake.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod controller;
pub mod encrypt;
mod file_browser;
mod media;
mod restart_identity;
pub mod runner;
mod runtime_update;
mod safe_file;
mod ssh_profiles;
pub mod terminal;
mod terminal_checkpoint;
pub mod terminal_host;
mod uploads;
mod workspace;
