//! Private, bounded attachment storage shared by room chat and notes.
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

const CHUNK: usize = 64 << 10;
const MAX_FILE: u64 = 20 << 20;
const QUOTA: u64 = 512 << 20;
const GRACE: Duration = Duration::from_secs(3600);

#[derive(Deserialize)]
pub(crate) struct Request {
    operation: String,
    path: String,
    #[serde(default)]
    offset: u64,
    #[serde(default)]
    total: u64,
    #[serde(default)]
    content: String,
}

pub(crate) struct Store {
    directory: PathBuf,
    workspace: PathBuf,
    pending: HashMap<String, (u64, u64, SystemTime)>,
    reserved: u64,
}

impl Store {
    pub(crate) async fn new(workspace: &Path) -> Result<Self> {
        let directory = workspace
            .parent()
            .context("workspace directory missing")?
            .join("cache/attachments");
        tokio::fs::create_dir_all(&directory).await?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).await?;
        }
        Ok(Self {
            directory,
            workspace: workspace.to_owned(),
            pending: HashMap::new(),
            reserved: 0,
        })
    }

    // Collect only unreferenced, aged files, and only when a new upload starts.
    // A grace period protects uploaded files before their metadata is committed.
    async fn collect(&mut self) -> Result<()> {
        let workspace = crate::workspace::load(&self.workspace)
            .await?
            .unwrap_or_default();
        let referenced = workspace
            .notes
            .iter()
            .flat_map(|n| &n.attachments)
            .chain(workspace.chat_history.iter().flat_map(|m| &m.attachments))
            .map(|a| a.id.as_str())
            .collect::<HashSet<_>>();
        self.pending
            .retain(|_, (_, _, at)| at.elapsed().unwrap_or_default() < GRACE);
        let mut entries = tokio::fs::read_dir(&self.directory).await?;
        self.reserved = 0;
        let mut count = 0;
        while let Some(entry) = entries.next_entry().await? {
            count += 1;
            ensure!(count <= 8192, "attachment store contains too many files");
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let Some((id, extension)) = name.rsplit_once('.') else {
                continue;
            };
            if !valid_id(id) || !matches!(extension, "bin" | "part") {
                continue;
            }
            let metadata = tokio::fs::symlink_metadata(entry.path()).await?;
            ensure!(
                metadata.is_file(),
                "attachment store contains an invalid entry"
            );
            if !referenced.contains(id)
                && !self.pending.contains_key(id)
                && metadata.modified()?.elapsed().unwrap_or_default() >= GRACE
            {
                tokio::fs::remove_file(entry.path()).await?;
            } else {
                self.reserved += self.pending.get(id).map_or(metadata.len(), |p| p.0);
            }
        }
        Ok(())
    }

    pub(crate) async fn execute(&mut self, request: Request, read_only: bool) -> Result<Value> {
        ensure!(valid_id(&request.path), "invalid attachment ID");
        let path = self.directory.join(format!("{}.bin", request.path));
        match request.operation.as_str() {
            "read" => {
                let metadata = tokio::fs::symlink_metadata(&path)
                    .await
                    .context("attachment is unavailable")?;
                ensure!(
                    metadata.is_file()
                        && metadata.len() <= MAX_FILE
                        && request.offset <= metadata.len(),
                    "invalid attachment read"
                );
                let mut file = tokio::fs::File::open(&path).await?;
                file.seek(std::io::SeekFrom::Start(request.offset)).await?;
                let mut bytes = vec![0; CHUNK.min((metadata.len() - request.offset) as usize)];
                file.read_exact(&mut bytes).await?;
                Ok(
                    json!({"ok":true,"operation":"read","path":request.path,"content":STANDARD.encode(bytes),"size":metadata.len()}),
                )
            }
            "write" => {
                ensure!(!read_only, "no write permission");
                ensure!(
                    request.content.len() <= CHUNK.div_ceil(3) * 4,
                    "attachment chunk is too large"
                );
                let bytes = STANDARD.decode(&request.content)?;
                ensure!(
                    !bytes.is_empty()
                        && bytes.len() <= CHUNK
                        && request.total > 0
                        && request.total <= MAX_FILE
                        && request.offset <= request.total
                        && bytes.len() as u64 <= request.total - request.offset,
                    "invalid attachment chunk"
                );
                let temporary = self.directory.join(format!("{}.part", request.path));
                let mut options = tokio::fs::OpenOptions::new();
                options.write(true);
                if request.offset == 0 {
                    self.collect().await?;
                    ensure!(
                        self.pending.len() < 8 && self.reserved + request.total <= QUOTA,
                        "attachment storage quota exceeded (512 MiB or 8 pending uploads)"
                    );
                    ensure!(
                        !tokio::fs::try_exists(&path).await?,
                        "attachment already exists"
                    );
                    options.create_new(true);
                    #[cfg(unix)]
                    options.mode(0o600);
                } else {
                    let pending = self
                        .pending
                        .get(&request.path)
                        .context("upload expired; retry attaching the file")?;
                    ensure!(
                        pending.0 == request.total && pending.1 == request.offset,
                        "out-of-order attachment chunk"
                    );
                    ensure!(
                        tokio::fs::symlink_metadata(&temporary).await?.is_file(),
                        "invalid upload file"
                    );
                    options.append(true);
                }
                let mut file = options.open(&temporary).await?;
                if request.offset == 0 {
                    self.reserved += request.total;
                }
                file.write_all(&bytes).await?;
                let next = request.offset + bytes.len() as u64;
                if next == request.total {
                    file.sync_all().await?;
                    drop(file);
                    tokio::fs::rename(&temporary, path).await?;
                    self.pending.remove(&request.path);
                } else {
                    self.pending.insert(
                        request.path.clone(),
                        (request.total, next, SystemTime::now()),
                    );
                }
                Ok(json!({"ok":true,"operation":"write","path":request.path,"size":next}))
            }
            _ => anyhow::bail!("unsupported attachment operation"),
        }
    }
}

fn valid_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn attachments_are_bounded_private_and_ordered() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let mut store = Store::new(&directory.path().join(".sshx-workspace")).await?;
        let request = |operation: &str, path: &str, offset, total, data: &[u8]| Request {
            operation: operation.into(),
            path: path.into(),
            offset,
            total,
            content: STANDARD.encode(data),
        };
        let id = "a".repeat(32);
        assert!(store
            .execute(request("write", "../escape", 0, 3, b"abc"), false)
            .await
            .is_err());
        assert!(store
            .execute(request("write", &id, 0, 3, b"abc"), true)
            .await
            .is_err());
        assert!(store
            .execute(request("write", &id, 0, MAX_FILE + 1, b"abc"), false)
            .await
            .is_err());
        store
            .execute(request("write", &id, 0, 6, b"abc"), false)
            .await?;
        assert!(store
            .execute(request("read", &id, 0, 0, b""), true)
            .await
            .is_err());
        assert!(store
            .execute(request("write", &id, 4, 6, b"ef"), false)
            .await
            .is_err());
        store
            .execute(request("write", &id, 3, 6, b"def"), false)
            .await?;
        let mut restarted = Store::new(&directory.path().join(".sshx-workspace")).await?;
        assert_eq!(
            restarted
                .execute(request("read", &id, 0, 0, b""), true)
                .await?["content"],
            STANDARD.encode(b"abcdef")
        );
        assert!(restarted
            .execute(request("write", &id, 0, 3, b"abc"), false)
            .await
            .is_err());
        Ok(())
    }
}
