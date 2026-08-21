//! Ephemeral, encrypted browser image uploads stored beside the daemon.

use std::collections::HashMap;
use std::io::{ErrorKind, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{bail, ensure, Context, Result};
use sshx_core::proto::ImageUploadChunk;
use sshx_core::Sid;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

use crate::encrypt::Encrypt;

const CACHE_DIRECTORY: &str = "cache";
const UPLOAD_DIRECTORY: &str = "uploads";
const MAX_ACTIVE_UPLOADS: usize = 8;
const MAX_IMAGE_BYTES: u64 = 20 << 20;
const MAX_CHUNK_BYTES: usize = 64 << 10;
const MAX_CACHE_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const MAX_ACTIVE_AGE: Duration = Duration::from_secs(2 * 60);

struct ActiveUpload {
    file: File,
    temporary_path: PathBuf,
    final_path: PathBuf,
    media_type: String,
    total_size: u64,
    stream_num: u64,
    received: u64,
    updated_at: Instant,
}

/// Tracks incomplete image uploads and commits completed files atomically.
pub(crate) struct UploadManager {
    root: PathBuf,
    active: HashMap<(Sid, String), ActiveUpload>,
}

impl UploadManager {
    /// Prepare the daemon-local cache and discard expired upload files.
    pub(crate) async fn new(root: PathBuf) -> Result<Self> {
        let cache = root
            .parent()
            .context("image upload root has no cache parent")?;
        ensure!(
            cache
                .file_name()
                .is_some_and(|name| name == CACHE_DIRECTORY)
                && root
                    .file_name()
                    .is_some_and(|name| name == UPLOAD_DIRECTORY),
            "image uploads must use a cache/uploads directory"
        );
        create_private_directory(cache).await?;
        create_private_directory(&root).await?;
        cleanup_expired(&root).await?;
        Ok(Self {
            root,
            active: HashMap::new(),
        })
    }

    /// Accept one ordered encrypted chunk and return the completed absolute
    /// path.
    pub(crate) async fn accept(
        &mut self,
        encrypt: &Encrypt,
        chunk: &ImageUploadChunk,
    ) -> Result<Option<PathBuf>> {
        validate_chunk(chunk)?;
        self.discard_stale(Instant::now()).await;
        let id = Sid(chunk.id);
        let key = (id, chunk.upload_id.clone());

        if chunk.offset == 0 {
            ensure!(
                self.active.len() < MAX_ACTIVE_UPLOADS,
                "too many image uploads are active"
            );
            ensure!(
                !self.active.contains_key(&key),
                "image upload already exists"
            );

            let directory = self.root.join(id.to_string());
            create_private_directory(&directory).await?;
            let extension = extension_for_media_type(&chunk.media_type)
                .context("unsupported image media type")?;
            let final_path = directory.join(format!("{}.{}", chunk.upload_id, extension));
            ensure!(
                !tokio::fs::try_exists(&final_path).await?,
                "image upload destination already exists"
            );
            let temporary_path = directory.join(format!(".{}.part", chunk.upload_id));
            let mut options = tokio::fs::OpenOptions::new();
            options.read(true).write(true).create_new(true);
            #[cfg(unix)]
            {
                options.mode(0o600);
            }
            let file = options
                .open(&temporary_path)
                .await
                .with_context(|| format!("failed to create {}", temporary_path.display()))?;
            self.active.insert(
                key.clone(),
                ActiveUpload {
                    file,
                    temporary_path,
                    final_path,
                    media_type: chunk.media_type.clone(),
                    total_size: chunk.total_size,
                    stream_num: chunk.stream_num,
                    received: 0,
                    updated_at: Instant::now(),
                },
            );
        }

        let upload = self
            .active
            .get_mut(&key)
            .context("image upload did not start at offset zero")?;
        ensure!(
            upload.media_type == chunk.media_type,
            "image media type changed"
        );
        ensure!(upload.total_size == chunk.total_size, "image size changed");
        ensure!(
            upload.stream_num == chunk.stream_num,
            "image encryption stream changed"
        );
        ensure!(
            upload.received == chunk.offset,
            "image chunks arrived out of order"
        );

        let plaintext = encrypt.segment(chunk.stream_num, chunk.offset, &chunk.data);
        upload.file.write_all(&plaintext).await?;
        upload.received += plaintext.len() as u64;
        upload.updated_at = Instant::now();

        if !chunk.complete {
            return Ok(None);
        }
        ensure!(
            upload.received == upload.total_size,
            "image upload is incomplete"
        );

        let mut upload = self.active.remove(&key).expect("active upload disappeared");
        upload.file.flush().await?;
        upload.file.sync_all().await?;
        upload.file.seek(SeekFrom::Start(0)).await?;
        let mut header = [0u8; 12];
        let header_len = upload.file.read(&mut header).await?;
        if !valid_image_signature(&upload.media_type, &header[..header_len]) {
            drop(upload.file);
            tokio::fs::remove_file(&upload.temporary_path).await.ok();
            bail!("image content does not match its media type");
        }
        drop(upload.file);
        tokio::fs::rename(&upload.temporary_path, &upload.final_path)
            .await
            .with_context(|| {
                format!(
                    "failed to commit image upload {}",
                    upload.final_path.display()
                )
            })?;
        Ok(Some(upload.final_path))
    }

    /// Reclaim upload slots left behind by disconnected browser clients.
    async fn discard_stale(&mut self, now: Instant) {
        let stale = self
            .active
            .iter()
            .filter(|(_, upload)| {
                now.saturating_duration_since(upload.updated_at) >= MAX_ACTIVE_AGE
            })
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        for key in stale {
            if let Some(upload) = self.active.remove(&key) {
                drop(upload.file);
                tokio::fs::remove_file(upload.temporary_path).await.ok();
            }
        }
    }

    /// Remove an incomplete upload after validation or I/O fails.
    pub(crate) async fn abort(&mut self, id: Sid, upload_id: &str) {
        if let Some(upload) = self.active.remove(&(id, upload_id.to_owned())) {
            drop(upload.file);
            tokio::fs::remove_file(upload.temporary_path).await.ok();
        } else if upload_id.len() == 32
            && upload_id
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            let path = self
                .root
                .join(id.to_string())
                .join(format!(".{upload_id}.part"));
            tokio::fs::remove_file(path).await.ok();
        }
    }
}

/// Return the image cache inside the daemon's current working directory.
pub(crate) fn path_in_current_dir() -> Result<PathBuf> {
    Ok(std::env::current_dir()
        .context("failed to determine daemon working directory")?
        .join(CACHE_DIRECTORY)
        .join(UPLOAD_DIRECTORY))
}

fn validate_chunk(chunk: &ImageUploadChunk) -> Result<()> {
    ensure!(
        chunk.upload_id.len() == 32
            && chunk
                .upload_id
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "invalid image upload identifier"
    );
    ensure!(
        extension_for_media_type(&chunk.media_type).is_some(),
        "unsupported image media type"
    );
    ensure!(
        (1..=MAX_IMAGE_BYTES).contains(&chunk.total_size),
        "image size is out of range"
    );
    ensure!(
        chunk.stream_num & (1 << 63) != 0,
        "invalid image encryption stream"
    );
    ensure!(
        !chunk.data.is_empty() && chunk.data.len() <= MAX_CHUNK_BYTES,
        "image chunk size is out of range"
    );
    let end = chunk
        .offset
        .checked_add(chunk.data.len() as u64)
        .context("image chunk offset overflowed")?;
    ensure!(end <= chunk.total_size, "image chunk exceeds declared size");
    ensure!(
        !chunk.complete || end == chunk.total_size,
        "final image chunk does not end at declared size"
    );
    Ok(())
}

fn extension_for_media_type(media_type: &str) -> Option<&'static str> {
    match media_type {
        "image/png" => Some("png"),
        "image/jpeg" => Some("jpg"),
        "image/webp" => Some("webp"),
        "image/gif" => Some("gif"),
        _ => None,
    }
}

fn valid_image_signature(media_type: &str, data: &[u8]) -> bool {
    match media_type {
        "image/png" => data.starts_with(b"\x89PNG\r\n\x1a\n"),
        "image/jpeg" => data.starts_with(b"\xff\xd8\xff"),
        "image/gif" => data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a"),
        "image/webp" => data.len() >= 12 && data.starts_with(b"RIFF") && &data[8..12] == b"WEBP",
        _ => false,
    }
}

async fn create_private_directory(path: &Path) -> Result<()> {
    tokio::fs::create_dir_all(path)
        .await
        .with_context(|| format!("failed to create {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).await?;
    }
    Ok(())
}

async fn cleanup_expired(root: &Path) -> Result<()> {
    let mut directories = tokio::fs::read_dir(root).await?;
    while let Some(directory) = directories.next_entry().await? {
        if !directory.file_type().await?.is_dir() {
            continue;
        }
        let directory_path = directory.path();
        let mut files = tokio::fs::read_dir(&directory_path).await?;
        while let Some(file) = files.next_entry().await? {
            if !file.file_type().await?.is_file() {
                continue;
            }
            let expired = file
                .metadata()
                .await?
                .modified()
                .ok()
                .and_then(|modified| modified.elapsed().ok())
                .is_some_and(|age| age >= MAX_CACHE_AGE);
            if expired {
                match tokio::fs::remove_file(file.path()).await {
                    Ok(()) => {}
                    Err(err) if err.kind() == ErrorKind::NotFound => {}
                    Err(err) => return Err(err.into()),
                }
            }
        }
        drop(files);
        match tokio::fs::remove_dir(&directory_path).await {
            Ok(()) => {}
            Err(err)
                if matches!(
                    err.kind(),
                    ErrorKind::DirectoryNotEmpty | ErrorKind::NotFound
                ) => {}
            Err(err) => return Err(err.into()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stores_encrypted_chunks_and_returns_cache_path() -> Result<()> {
        let base = std::env::temp_dir().join(format!(
            "sshxx-upload-test-{}",
            sshx_core::rand_alphanumeric(12)
        ));
        let root = base.join(CACHE_DIRECTORY).join(UPLOAD_DIRECTORY);
        let mut manager = UploadManager::new(root.clone()).await?;
        let encrypt = Encrypt::new("test upload encryption key");
        let content = b"\x89PNG\r\n\x1a\nnot-a-real-png-but-signature-valid";
        let upload_id = "0123456789abcdef0123456789abcdef";
        let stream_num = 0x8000_0000_0000_0042;

        let first = &content[..10];
        assert!(manager
            .accept(
                &encrypt,
                &ImageUploadChunk {
                    id: 7,
                    upload_id: upload_id.into(),
                    media_type: "image/png".into(),
                    total_size: content.len() as u64,
                    stream_num,
                    offset: 0,
                    data: encrypt.segment(stream_num, 0, first).into(),
                    complete: false,
                },
            )
            .await?
            .is_none());

        let second = &content[10..];
        let path = manager
            .accept(
                &encrypt,
                &ImageUploadChunk {
                    id: 7,
                    upload_id: upload_id.into(),
                    media_type: "image/png".into(),
                    total_size: content.len() as u64,
                    stream_num,
                    offset: 10,
                    data: encrypt.segment(stream_num, 10, second).into(),
                    complete: true,
                },
            )
            .await?
            .context("upload did not complete")?;

        assert_eq!(path, root.join("7").join(format!("{upload_id}.png")));
        assert_eq!(tokio::fs::read(&path).await?, content);
        tokio::fs::remove_dir_all(base).await?;
        Ok(())
    }

    #[tokio::test]
    async fn discards_stalled_uploads() -> Result<()> {
        let base = std::env::temp_dir().join(format!(
            "sshxx-stalled-upload-test-{}",
            sshx_core::rand_alphanumeric(12)
        ));
        let root = base.join(CACHE_DIRECTORY).join(UPLOAD_DIRECTORY);
        let mut manager = UploadManager::new(root.clone()).await?;
        let encrypt = Encrypt::new("test upload encryption key");
        let upload_id = "abcdef0123456789abcdef0123456789";
        let stream_num = 0x8000_0000_0000_0043;
        let content = b"\x89PNG\r\n\x1a\nunfinished";

        manager
            .accept(
                &encrypt,
                &ImageUploadChunk {
                    id: 8,
                    upload_id: upload_id.into(),
                    media_type: "image/png".into(),
                    total_size: content.len() as u64,
                    stream_num,
                    offset: 0,
                    data: encrypt.segment(stream_num, 0, &content[..8]).into(),
                    complete: false,
                },
            )
            .await?;
        let temporary_path = root.join("8").join(format!(".{upload_id}.part"));
        assert!(tokio::fs::try_exists(&temporary_path).await?);

        manager.discard_stale(Instant::now() + MAX_ACTIVE_AGE).await;
        assert!(manager.active.is_empty());
        assert!(!tokio::fs::try_exists(&temporary_path).await?);

        tokio::fs::remove_dir_all(base).await?;
        Ok(())
    }
}
