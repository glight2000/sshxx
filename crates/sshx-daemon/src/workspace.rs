//! Local persistence for reusable daemon workspace metadata.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{ensure, Context, Result};
use prost::Message;
use sshx_core::proto::WorkspaceState;
use sshx_core::{rand_alphanumeric, WORKSPACE_FORMAT_VERSION};
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

/// Preserve an unreadable or future-format workspace for manual recovery.
pub async fn quarantine(path: &Path) -> Result<PathBuf> {
    let destination =
        path.with_file_name(format!("{}.invalid-{}", FILE_NAME, rand_alphanumeric(8)));
    tokio::fs::rename(path, &destination)
        .await
        .with_context(|| format!("failed to quarantine {}", path.display()))?;
    Ok(destination)
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
    use sshx_core::proto::{WorkspaceFileWindow, WorkspaceNote, WorkspacePage, WorkspaceShell};

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
                ssh_profile_id: "work-server".into(),
            }],
            notes: vec![WorkspaceNote {
                id: 8,
                x: 36,
                y: 48,
                width: 400,
                height: 240,
                text: "Deploy".into(),
                paragraphs: vec!["Deploy".into()],
                linked_shell_ids: vec![7],
                linked_note_ids: Vec::new(),
                linked_file_window_ids: vec![9],
                title: "Release plan".into(),
                background: "#445566".into(),
                opacity: 80,
                page_id: 2,
            }],
            file_windows: vec![WorkspaceFileWindow {
                id: 9,
                shell_id: 7,
                page_id: 2,
                path: "/tmp".into(),
                title: "Logs".into(),
                background: "#111827".into(),
                x: 48,
                y: 60,
                width: 1040,
                height: 680,
                current_path: "/tmp/project".into(),
                expanded_paths: vec!["/".into(), "/tmp".into(), "/tmp/project".into()],
                selected_path: "/tmp/project/config.toml".into(),
                selected_kind: "file".into(),
                tree_scroll_top: 96,
                editor_path: "/tmp/project/config.toml".into(),
                editor_stream: 1 << 63,
                editor_data: b"encrypted editor".as_slice().into(),
                editor_dirty: true,
                sidebar_width: 360,
                tree_revision: 4,
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

    #[tokio::test]
    async fn quarantine_preserves_invalid_workspace() -> Result<()> {
        let directory = std::env::temp_dir().join(format!(
            "sshxx-workspace-invalid-test-{}",
            rand_alphanumeric(12)
        ));
        tokio::fs::create_dir(&directory).await?;
        let path = directory.join(FILE_NAME);
        tokio::fs::write(&path, b"invalid workspace").await?;

        assert!(load(&path).await.is_err());
        let destination = quarantine(&path).await?;
        assert!(!tokio::fs::try_exists(&path).await?);
        assert_eq!(tokio::fs::read(&destination).await?, b"invalid workspace");

        tokio::fs::remove_dir_all(directory).await?;
        Ok(())
    }
}
