//! Local configuration shared by the daemon and its independent terminal host.

use std::fs::OpenOptions;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use sshx_core::rand_alphanumeric;
use sshxx_terminal_host::endpoint_for_state_directory;

/// Default terminal-host state directory, relative to the daemon working directory.
pub const DEFAULT_STATE_DIRECTORY: &str = "cache/terminal-host";

const INSTANCE_FILE: &str = "instance.id";
const TOKEN_FILE: &str = "host.token";

/// Connection and launch policy for a terminal host serving one daemon workspace.
#[derive(Clone, Debug)]
pub struct TerminalHostConfig {
    /// Local Unix-socket path or Windows named-pipe name.
    pub endpoint: String,
    /// Authentication token shared only by local daemon and host processes.
    pub authentication_token: Vec<u8>,
    /// Stable namespace used to reconnect workspace terminal IDs after a daemon restart.
    pub instance_id: String,
    /// Directory containing independent shell history files.
    pub history_directory: PathBuf,
}

impl TerminalHostConfig {
    /// Load host credentials and prepare stable daemon-owned state.
    pub fn load(state_directory: &Path) -> Result<Self> {
        let state_directory = std::fs::canonicalize(state_directory)
            .context("failed to resolve terminal-host state directory")?;
        let authentication_token = std::fs::read(state_directory.join(TOKEN_FILE))
            .context("terminal host is not initialized; run `sshxx-daemon terminal-host start`")?;
        if authentication_token.len() < 32 {
            bail!("terminal-host authentication token is invalid");
        }

        set_private_directory_permissions(&state_directory)?;
        let instance_id = load_or_create_instance_id(&state_directory)?;
        let history_directory = state_directory.join("history");
        std::fs::create_dir_all(&history_directory)
            .context("failed to create terminal history directory")?;
        set_private_directory_permissions(&history_directory)?;

        Ok(Self {
            endpoint: endpoint_for_state_directory(&state_directory),
            authentication_token,
            instance_id,
            history_directory,
        })
    }

    /// Return the stable host ID for a workspace terminal.
    pub fn terminal_id(&self, shell_id: u32) -> String {
        format!("{}-{shell_id}", self.instance_id)
    }

    /// Return the daemon-owned history file for a workspace terminal.
    pub fn history_path(&self, shell_id: u32) -> PathBuf {
        self.history_directory
            .join(format!("{}.history", self.terminal_id(shell_id)))
    }

    /// Copy the persisted command-history snapshot used by a duplicated terminal.
    pub async fn clone_history(&self, source_id: u32, target_id: u32) -> Result<bool> {
        let source = self.history_path(source_id);
        let target = self.history_path(target_id);
        match tokio::fs::copy(&source, &target).await {
            Ok(_) => {
                set_private_file_permissions(&target).await?;
                Ok(true)
            }
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error).with_context(|| {
                format!(
                    "failed to copy terminal history from {} to {}",
                    source.display(),
                    target.display()
                )
            }),
        }
    }

    /// Delete command history after a terminal is explicitly or naturally closed.
    pub async fn remove_history(&self, shell_id: u32) -> Result<bool> {
        let path = self.history_path(shell_id);
        for attempt in 0..20 {
            match tokio::fs::remove_file(&path).await {
                Ok(()) => return Ok(true),
                Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
                Err(error) if error.kind() == ErrorKind::PermissionDenied && attempt < 19 => {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("failed to remove terminal history at {}", path.display())
                    });
                }
            }
        }
        Ok(false)
    }
}

fn load_or_create_instance_id(state_directory: &Path) -> Result<String> {
    let path = state_directory.join(INSTANCE_FILE);
    match std::fs::read_to_string(&path) {
        Ok(instance_id) => validate_instance_id(instance_id.trim()),
        Err(error) if error.kind() == ErrorKind::NotFound => {
            let instance_id = format!("sshxx-{}", rand_alphanumeric(20));
            match private_new_file(&path) {
                Ok(mut file) => {
                    file.write_all(instance_id.as_bytes())
                        .context("failed to write terminal-host instance ID")?;
                    file.sync_all()
                        .context("failed to persist terminal-host instance ID")?;
                    Ok(instance_id)
                }
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                    let value = std::fs::read_to_string(&path)
                        .context("failed to read terminal-host instance ID")?;
                    validate_instance_id(value.trim())
                }
                Err(error) => Err(error).context("failed to create terminal-host instance ID"),
            }
        }
        Err(error) => Err(error).context("failed to read terminal-host instance ID"),
    }
}

fn validate_instance_id(value: &str) -> Result<String> {
    if value.len() < 16
        || value.len() > 64
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        bail!("terminal-host instance ID is invalid");
    }
    Ok(value.to_owned())
}

fn private_new_file(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

#[cfg(unix)]
fn set_private_directory_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(windows)]
fn set_private_directory_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
async fn set_private_file_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).await?;
    Ok(())
}

#[cfg(windows)]
async fn set_private_file_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_ids_are_stable_per_instance() -> Result<()> {
        let directory = tempfile::tempdir()?;
        std::fs::write(directory.path().join(TOKEN_FILE), [7; 32])?;
        let first = TerminalHostConfig::load(directory.path())?;
        let second = TerminalHostConfig::load(directory.path())?;
        assert_eq!(first.terminal_id(42), second.terminal_id(42));
        assert_ne!(first.terminal_id(42), first.terminal_id(43));
        assert!(first.history_path(42).is_absolute());
        Ok(())
    }

    #[tokio::test]
    async fn duplicated_terminals_receive_a_private_history_snapshot() -> Result<()> {
        let directory = tempfile::tempdir()?;
        std::fs::write(directory.path().join(TOKEN_FILE), [7; 32])?;
        let config = TerminalHostConfig::load(directory.path())?;
        tokio::fs::write(config.history_path(4), "cargo test\n").await?;

        assert!(config.clone_history(4, 9).await?);
        assert_eq!(
            tokio::fs::read_to_string(config.history_path(9)).await?,
            "cargo test\n"
        );
        assert!(config.remove_history(9).await?);
        assert!(!config.history_path(9).exists());
        assert!(!config.remove_history(9).await?);
        assert!(!config.clone_history(100, 101).await?);
        Ok(())
    }
}
