//! Local persistence for reusable daemon workspace metadata.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{ensure, Context, Result};
use prost::Message;
use sshx_core::proto::WorkspaceState;
use sshx_core::WORKSPACE_FORMAT_VERSION;
use tokio::io::AsyncWriteExt;
use tokio::sync::watch;
use tokio::time::{sleep, Duration};
use tracing::warn;

pub const FILE_NAME: &str = ".sshx-workspace";

const SAVE_DEBOUNCE: Duration = Duration::from_millis(250);

pub fn path_in_current_dir() -> Result<PathBuf> {
    Ok(std::env::current_dir()
        .context("failed to determine daemon working directory")?
        .join(FILE_NAME))
}

pub async fn load(path: &Path) -> Result<Option<WorkspaceState>> {
    let data = match tokio::fs::read(path).await {
        Ok(data) => data,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err).with_context(|| format!("failed to read {}", path.display())),
    };
    let workspace = WorkspaceState::decode(&*data)
        .with_context(|| format!("failed to decode {}", path.display()))?;
    ensure!(
        workspace.format_version == WORKSPACE_FORMAT_VERSION,
        "unsupported workspace format version {} in {}",
        workspace.format_version,
        path.display()
    );
    Ok(Some(workspace))
}

pub async fn save(path: &Path, workspace: &WorkspaceState) -> Result<()> {
    let temporary = path.with_extension("tmp");
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    options.mode(0o600);

    let mut file = options
        .open(&temporary)
        .await
        .with_context(|| format!("failed to create {}", temporary.display()))?;
    file.write_all(&workspace.encode_to_vec()).await?;
    file.sync_all().await?;
    drop(file);

    #[cfg(windows)]
    if tokio::fs::try_exists(path).await? {
        tokio::fs::remove_file(path).await?;
    }
    tokio::fs::rename(&temporary, path)
        .await
        .with_context(|| format!("failed to replace {}", path.display()))?;
    Ok(())
}

pub async fn writer(path: PathBuf, mut receiver: watch::Receiver<WorkspaceState>) {
    while receiver.changed().await.is_ok() {
        sleep(SAVE_DEBOUNCE).await;
        while receiver.has_changed().unwrap_or(false) {
            receiver.borrow_and_update();
        }
        let workspace = receiver.borrow().clone();
        if let Err(err) = save(&path, &workspace).await {
            warn!(?err, path = %path.display(), "failed to persist workspace");
        }
    }
}

#[cfg(test)]
mod tests {
    use sshx_core::proto::{WorkspaceNote, WorkspacePage, WorkspaceShell};

    use super::*;

    #[tokio::test]
    async fn roundtrips_workspace_file() -> Result<()> {
        let directory = std::env::temp_dir().join(format!(
            "sshx-workspace-test-{}",
            sshx_core::rand_alphanumeric(12)
        ));
        tokio::fs::create_dir(&directory).await?;
        let path = directory.join(FILE_NAME);
        let workspace = WorkspaceState {
            format_version: WORKSPACE_FORMAT_VERSION,
            shells: vec![WorkspaceShell {
                id: 7,
                x: 12,
                y: 24,
                rows: 30,
                cols: 100,
                width: 714,
                height: 518,
                title: "Logs".into(),
                background: "#112233".into(),
                opacity: 75,
                page_id: 2,
                theme: "Tokyo Night".into(),
            }],
            notes: vec![WorkspaceNote {
                id: 8,
                x: 36,
                y: 48,
                width: 400,
                height: 240,
                text: "Deploy".into(),
                background: "#445566".into(),
                opacity: 80,
                page_id: 2,
            }],
            pages: vec![
                WorkspacePage {
                    id: 1,
                    name: "Page 1".into(),
                },
                WorkspacePage {
                    id: 2,
                    name: "Work".into(),
                },
            ],
        };

        save(&path, &workspace).await?;
        assert_eq!(load(&path).await?, Some(workspace));
        tokio::fs::remove_dir_all(directory).await?;
        Ok(())
    }
}
