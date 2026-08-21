//! Stable local PTY ownership for sshxx.
//!
//! The terminal host owns processes and pseudo-terminals. A daemon is only a
//! reconnectable protocol client, so dropping or restarting it does not close
//! terminal processes.

use std::path::Path;

pub mod client;
pub mod protocol;
#[cfg(feature = "host")]
pub mod server;
#[cfg(feature = "host")]
mod session;

pub const PROTOCOL_VERSION: u32 = 1;
pub const HOST_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Resolve the platform-local endpoint for one terminal-host state directory.
#[cfg(unix)]
pub fn endpoint_for_state_directory(state_directory: &Path) -> String {
    state_directory
        .join("host.sock")
        .to_string_lossy()
        .into_owned()
}

/// Resolve a distinct named pipe for one Windows terminal-host state directory.
#[cfg(windows)]
pub fn endpoint_for_state_directory(state_directory: &Path) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in state_directory.to_string_lossy().bytes() {
        hash ^= u64::from(byte.to_ascii_lowercase());
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!(r"\\.\pipe\sshxx-terminal-host-{hash:016x}")
}
