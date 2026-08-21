//! Filesystem operations requested by the encrypted browser session.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use bytes::BytesMut;
use openssh_sftp_client::{Sftp, SftpOptions};
use serde::{Deserialize, Serialize};
use sshx_core::proto::{SshAuthMethod, SshProfile};
use tokio::process::Command;
use tokio_stream::StreamExt;

use crate::runner::ssh_command;

const MAX_FILE_BYTES: usize = 8 << 20;
const MAX_DIRECTORY_ENTRIES: usize = 2_000;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileOperationRequest {
    pub operation: FileOperation,
    pub path: String,
    #[serde(default)]
    pub destination: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub encoding: Option<String>,
    #[serde(default)]
    pub recursive: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub(crate) enum FileOperation {
    List,
    Read,
    Write,
    CreateFile,
    CreateDirectory,
    Rename,
    Move,
    Delete,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileOperationResponse {
    pub ok: bool,
    pub operation: FileOperation,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entries: Option<Vec<FileEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileEntry {
    name: String,
    path: String,
    kind: &'static str,
    size: u64,
}

impl FileOperationResponse {
    pub(crate) fn failure(request: &FileOperationRequest, error: impl std::fmt::Display) -> Self {
        Self {
            ok: false,
            operation: request.operation,
            path: request.path.clone(),
            error: Some(error.to_string()),
            entries: None,
            content: None,
            encoding: None,
            size: None,
        }
    }
}

pub(crate) async fn execute(
    request: &FileOperationRequest,
    working_directory: Option<PathBuf>,
    ssh_profile: Option<&SshProfile>,
) -> Result<FileOperationResponse> {
    if request.path.is_empty() || request.path.contains('\0') || request.path.len() > 16_384 {
        bail!("file path is invalid");
    }
    if matches!(
        request.operation,
        FileOperation::CreateFile | FileOperation::CreateDirectory
    ) {
        validate_new_entry_path(Path::new(&request.path))?;
    }
    if matches!(
        request.operation,
        FileOperation::Rename | FileOperation::Move
    ) {
        let destination = request
            .destination
            .as_deref()
            .context("filesystem operation destination is missing")?;
        if destination.is_empty() || destination.contains('\0') || destination.len() > 16_384 {
            bail!("filesystem operation destination is invalid");
        }
        validate_new_entry_path(Path::new(destination))?;
    }
    if let Some(profile) = ssh_profile {
        execute_remote(request, profile).await
    } else {
        execute_local(request, working_directory).await
    }
}

async fn execute_local(
    request: &FileOperationRequest,
    working_directory: Option<PathBuf>,
) -> Result<FileOperationResponse> {
    let base = working_directory.unwrap_or(std::env::current_dir()?);
    let requested = Path::new(&request.path);
    let path = if requested == Path::new(".") {
        base.clone()
    } else if requested.is_absolute() {
        requested.to_owned()
    } else {
        base.join(requested)
    };

    match request.operation {
        FileOperation::List => {
            let canonical = tokio::fs::canonicalize(&path)
                .await
                .with_context(|| format!("cannot open {}", path.display()))?;
            let mut directory = tokio::fs::read_dir(&canonical).await?;
            let mut entries = Vec::new();
            while let Some(entry) = directory.next_entry().await? {
                if entries.len() >= MAX_DIRECTORY_ENTRIES {
                    bail!("directory contains more than {MAX_DIRECTORY_ENTRIES} entries");
                }
                let metadata = tokio::fs::symlink_metadata(entry.path()).await?;
                let file_type = metadata.file_type();
                entries.push(FileEntry {
                    name: entry.file_name().to_string_lossy().into_owned(),
                    path: entry.path().to_string_lossy().into_owned(),
                    kind: if file_type.is_dir() {
                        "directory"
                    } else if file_type.is_file() {
                        "file"
                    } else if file_type.is_symlink() {
                        "symlink"
                    } else {
                        "other"
                    },
                    size: metadata.len(),
                });
            }
            sort_entries(&mut entries);
            Ok(success(request, canonical, Some(entries), None, None, None))
        }
        FileOperation::Read => {
            let canonical = tokio::fs::canonicalize(&path).await?;
            let metadata = tokio::fs::metadata(&canonical).await?;
            let size = usize::try_from(metadata.len()).unwrap_or(usize::MAX);
            if size > MAX_FILE_BYTES {
                bail!("files larger than 8 MiB cannot be previewed");
            }
            let bytes = tokio::fs::read(&canonical).await?;
            Ok(read_success(request, canonical, bytes))
        }
        FileOperation::Write => {
            let content = decoded_content(request)?;
            if content.len() > MAX_FILE_BYTES {
                bail!("files larger than 8 MiB cannot be saved");
            }
            tokio::fs::write(&path, &content).await?;
            Ok(success(
                request,
                path,
                None,
                None,
                None,
                Some(content.len() as u64),
            ))
        }
        FileOperation::CreateFile => {
            let mut options = tokio::fs::OpenOptions::new();
            options.write(true).create_new(true);
            options.open(&path).await?;
            Ok(success(request, path, None, None, None, Some(0)))
        }
        FileOperation::CreateDirectory => {
            if request.recursive {
                tokio::fs::create_dir_all(&path).await?;
            } else {
                tokio::fs::create_dir(&path).await?;
            }
            Ok(success(request, path, None, None, None, None))
        }
        FileOperation::Rename | FileOperation::Move => {
            let source = tokio::fs::canonicalize(&path)
                .await
                .with_context(|| format!("cannot open {}", path.display()))?;
            if is_root_path(&source) {
                bail!("filesystem roots cannot be moved or renamed");
            }
            let requested_destination = Path::new(
                request
                    .destination
                    .as_deref()
                    .context("filesystem operation destination is missing")?,
            );
            let destination = if requested_destination.is_absolute() {
                requested_destination.to_owned()
            } else {
                base.join(requested_destination)
            };
            if tokio::fs::try_exists(&destination).await? {
                bail!("the destination already exists");
            }
            let destination_parent = destination
                .parent()
                .context("destination folder is invalid")?;
            let canonical_parent = tokio::fs::canonicalize(destination_parent)
                .await
                .with_context(|| format!("cannot open {}", destination_parent.display()))?;
            let source_parent = source.parent().context("source folder is invalid")?;
            if matches!(request.operation, FileOperation::Rename)
                && source_parent != canonical_parent
            {
                bail!("rename must keep the item in its current directory");
            }
            if canonical_parent.starts_with(&source) {
                bail!("a folder cannot be moved inside itself");
            }
            tokio::fs::rename(&path, &destination).await?;
            Ok(success(request, destination, None, None, None, None))
        }
        FileOperation::Delete => {
            let metadata = tokio::fs::symlink_metadata(&path).await?;
            let delete_path = if metadata.file_type().is_dir() {
                tokio::fs::canonicalize(&path)
                    .await
                    .with_context(|| format!("cannot open {}", path.display()))?
            } else {
                path.clone()
            };
            if is_root_path(&delete_path) {
                bail!("filesystem roots cannot be deleted");
            }
            if metadata.file_type().is_dir() {
                tokio::fs::remove_dir_all(&delete_path).await?;
            } else {
                tokio::fs::remove_file(&delete_path).await?;
            }
            Ok(success(request, delete_path, None, None, None, None))
        }
    }
}

async fn execute_remote(
    request: &FileOperationRequest,
    profile: &SshProfile,
) -> Result<FileOperationResponse> {
    if SshAuthMethod::try_from(profile.auth_method).ok() == Some(SshAuthMethod::SshAuthPassword) {
        bail!("remote file browsing cannot reuse an interactive SSH password; use an SSH key or agent profile");
    }
    let (program, mut args) = ssh_command(profile)?;
    let host = args.pop().context("SSH destination is missing")?;
    args.extend([
        "-o".into(),
        "BatchMode=yes".into(),
        "-o".into(),
        "ConnectTimeout=10".into(),
        "-s".into(),
        host,
        "sftp".into(),
    ]);
    let mut child = Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .context("cannot start OpenSSH SFTP subsystem")?;
    let stdin = child.stdin.take().context("cannot open SSH stdin")?;
    let stdout = child.stdout.take().context("cannot open SSH stdout")?;
    let sftp = Sftp::new(stdin, stdout, SftpOptions::new())
        .await
        .context("cannot establish SFTP connection")?;
    let result = execute_sftp(request, &sftp).await;
    sftp.close().await.ok();
    child.wait().await.ok();
    result
}

async fn execute_sftp(
    request: &FileOperationRequest,
    sftp: &Sftp,
) -> Result<FileOperationResponse> {
    let mut fs = sftp.fs();
    let requested_path = PathBuf::from(&request.path);
    let canonical = if matches!(
        request.operation,
        FileOperation::Write
            | FileOperation::CreateFile
            | FileOperation::CreateDirectory
            | FileOperation::Rename
            | FileOperation::Move
            | FileOperation::Delete
    ) {
        requested_path.clone()
    } else {
        fs.canonicalize(&request.path).await?
    };
    match request.operation {
        FileOperation::List => {
            let directory = fs.open_dir(&canonical).await?.read_dir();
            tokio::pin!(directory);
            let mut entries = Vec::new();
            while let Some(entry) = directory.next().await.transpose()? {
                let name = entry.filename().to_string_lossy().into_owned();
                if name == "." || name == ".." {
                    continue;
                }
                if entries.len() >= MAX_DIRECTORY_ENTRIES {
                    bail!("directory contains more than {MAX_DIRECTORY_ENTRIES} entries");
                }
                let metadata = entry.metadata();
                let file_type = metadata.file_type();
                entries.push(FileEntry {
                    name: name.clone(),
                    path: canonical.join(name).to_string_lossy().into_owned(),
                    kind: if file_type.is_some_and(|kind| kind.is_dir()) {
                        "directory"
                    } else if file_type.is_some_and(|kind| kind.is_file()) {
                        "file"
                    } else if file_type.is_some_and(|kind| kind.is_symlink()) {
                        "symlink"
                    } else {
                        "other"
                    },
                    size: metadata.len().unwrap_or(0),
                });
            }
            sort_entries(&mut entries);
            Ok(success(request, canonical, Some(entries), None, None, None))
        }
        FileOperation::Read => {
            let mut file = sftp.open(&canonical).await?;
            let size = file.metadata().await?.len().unwrap_or(0);
            let size = usize::try_from(size).unwrap_or(usize::MAX);
            if size > MAX_FILE_BYTES {
                bail!("files larger than 8 MiB cannot be previewed");
            }
            let bytes = file.read_all(size, BytesMut::new()).await?.to_vec();
            file.close().await.ok();
            Ok(read_success(request, canonical, bytes))
        }
        FileOperation::Write => {
            let content = decoded_content(request)?;
            if content.len() > MAX_FILE_BYTES {
                bail!("files larger than 8 MiB cannot be saved");
            }
            let mut file = sftp.create(&canonical).await?;
            file.write_all(&content).await?;
            file.close().await?;
            Ok(success(
                request,
                canonical,
                None,
                None,
                None,
                Some(content.len() as u64),
            ))
        }
        FileOperation::CreateFile => {
            if fs.metadata(&canonical).await.is_ok() {
                bail!("the selected file already exists");
            }
            let file = sftp.create(&canonical).await?;
            file.close().await?;
            Ok(success(request, canonical, None, None, None, Some(0)))
        }
        FileOperation::CreateDirectory => {
            if fs.metadata(&canonical).await.is_ok() {
                if request.recursive {
                    return Ok(success(request, canonical, None, None, None, None));
                }
                bail!("the selected directory already exists");
            }
            fs.create_dir(&canonical).await?;
            Ok(success(request, canonical, None, None, None, None))
        }
        FileOperation::Rename | FileOperation::Move => {
            let source = fs
                .canonicalize(&requested_path)
                .await
                .with_context(|| format!("cannot open {}", requested_path.display()))?;
            if is_root_path(&source) {
                bail!("filesystem roots cannot be moved or renamed");
            }
            let requested_destination = PathBuf::from(
                request
                    .destination
                    .as_deref()
                    .context("filesystem operation destination is missing")?,
            );
            if fs.metadata(&requested_destination).await.is_ok() {
                bail!("the destination already exists");
            }
            let destination_parent = requested_destination
                .parent()
                .context("destination folder is invalid")?;
            let canonical_parent = fs
                .canonicalize(destination_parent)
                .await
                .with_context(|| format!("cannot open {}", destination_parent.display()))?;
            let destination = canonical_parent.join(
                requested_destination
                    .file_name()
                    .context("destination name is invalid")?,
            );
            if matches!(request.operation, FileOperation::Rename)
                && source.parent() != destination.parent()
            {
                bail!("rename must keep the item in its current directory");
            }
            if destination.starts_with(&source) {
                bail!("a folder cannot be moved inside itself");
            }
            fs.rename(&requested_path, &destination).await?;
            Ok(success(request, destination, None, None, None, None))
        }
        FileOperation::Delete => {
            let metadata = fs.symlink_metadata(&requested_path).await?;
            let delete_path = if metadata.file_type().is_some_and(|kind| kind.is_dir()) {
                fs.canonicalize(&requested_path)
                    .await
                    .with_context(|| format!("cannot open {}", requested_path.display()))?
            } else {
                requested_path
            };
            if is_root_path(&delete_path) {
                bail!("filesystem roots cannot be deleted");
            }
            let mut pending = vec![(delete_path.clone(), false)];
            let mut visited = 0usize;
            while let Some((path, expanded)) = pending.pop() {
                visited += 1;
                if visited > 100_000 {
                    bail!("selected directory contains too many entries to delete safely");
                }
                let metadata = fs.symlink_metadata(&path).await?;
                let is_directory = metadata.file_type().is_some_and(|kind| kind.is_dir());
                if !is_directory {
                    fs.remove_file(&path).await?;
                    continue;
                }
                if expanded {
                    fs.remove_dir(&path).await?;
                    continue;
                }
                let children = {
                    let directory = fs.open_dir(&path).await?.read_dir();
                    tokio::pin!(directory);
                    let mut children = Vec::new();
                    while let Some(entry) = directory.next().await.transpose()? {
                        let name = entry.filename();
                        if name != "." && name != ".." {
                            children.push(path.join(name));
                        }
                    }
                    children
                };
                pending.push((path, true));
                pending.extend(children.into_iter().map(|child| (child, false)));
            }
            Ok(success(request, delete_path, None, None, None, None))
        }
    }
}

fn is_root_path(path: &Path) -> bool {
    path.parent().is_none()
}

fn validate_new_entry_path(path: &Path) -> Result<()> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("new file or folder name is invalid")?;
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.len() > 255
        || name.chars().any(char::is_control)
    {
        bail!("new file or folder name is invalid");
    }
    Ok(())
}

fn sort_entries(entries: &mut [FileEntry]) {
    entries.sort_by(|left, right| {
        (left.kind != "directory", left.name.to_lowercase())
            .cmp(&(right.kind != "directory", right.name.to_lowercase()))
    });
}

fn decoded_content(request: &FileOperationRequest) -> Result<Vec<u8>> {
    let content = request
        .content
        .as_deref()
        .context("file content is missing")?;
    match request.encoding.as_deref().unwrap_or("utf8") {
        "utf8" => Ok(content.as_bytes().to_vec()),
        "base64" => STANDARD
            .decode(content)
            .context("uploaded file content is not valid base64"),
        _ => bail!("file content encoding is unsupported"),
    }
}

fn read_success(
    request: &FileOperationRequest,
    path: impl AsRef<Path>,
    bytes: Vec<u8>,
) -> FileOperationResponse {
    let size = bytes.len() as u64;
    match String::from_utf8(bytes) {
        Ok(content) => success(request, path, None, Some(content), Some("utf8"), Some(size)),
        Err(error) => success(
            request,
            path,
            None,
            Some(STANDARD.encode(error.into_bytes())),
            Some("base64"),
            Some(size),
        ),
    }
}

fn success(
    request: &FileOperationRequest,
    path: impl AsRef<Path>,
    entries: Option<Vec<FileEntry>>,
    content: Option<String>,
    encoding: Option<&'static str>,
    size: Option<u64>,
) -> FileOperationResponse {
    FileOperationResponse {
        ok: true,
        operation: request.operation,
        path: path.as_ref().to_string_lossy().into_owned(),
        error: None,
        entries,
        content,
        encoding,
        size,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(
        operation: FileOperation,
        path: &Path,
        content: Option<&str>,
    ) -> FileOperationRequest {
        FileOperationRequest {
            operation,
            path: path.to_string_lossy().into_owned(),
            destination: None,
            content: content.map(str::to_owned),
            encoding: None,
            recursive: false,
        }
    }

    #[tokio::test]
    async fn local_files_can_be_listed_read_and_written() -> Result<()> {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "sshxx-file-browser-test-{}-{nonce}",
            std::process::id()
        ));
        tokio::fs::create_dir(&directory).await?;
        let path = directory.join("example.txt");
        tokio::fs::write(&path, "before").await?;

        let listed = execute_local(&request(FileOperation::List, &directory, None), None).await?;
        assert_eq!(listed.entries.as_ref().map(Vec::len), Some(1));

        let read = execute_local(&request(FileOperation::Read, &path, None), None).await?;
        assert_eq!(read.content.as_deref(), Some("before"));
        assert_eq!(read.encoding, Some("utf8"));

        execute_local(&request(FileOperation::Write, &path, Some("after")), None).await?;
        assert_eq!(tokio::fs::read_to_string(&path).await?, "after");

        let binary_path = directory.join("binary.dat");
        let mut upload = request(FileOperation::Write, &binary_path, Some("AP8Q"));
        upload.encoding = Some("base64".into());
        execute_local(&upload, None).await?;
        assert_eq!(tokio::fs::read(&binary_path).await?, [0, 255, 16]);

        let created_directory = directory.join("created");
        execute_local(
            &request(FileOperation::CreateDirectory, &created_directory, None),
            None,
        )
        .await?;
        let created_file = created_directory.join("empty.txt");
        execute_local(
            &request(FileOperation::CreateFile, &created_file, None),
            None,
        )
        .await?;
        let renamed_file = created_directory.join("renamed.txt");
        let mut rename = request(FileOperation::Rename, &created_file, None);
        rename.destination = Some(renamed_file.to_string_lossy().into_owned());
        execute_local(&rename, None).await?;
        assert!(tokio::fs::try_exists(&renamed_file).await?);

        let moved_file = directory.join("moved.txt");
        let mut move_request = request(FileOperation::Move, &renamed_file, None);
        move_request.destination = Some(moved_file.to_string_lossy().into_owned());
        execute_local(&move_request, None).await?;
        assert!(tokio::fs::try_exists(&moved_file).await?);

        let collision = directory.join("collision.txt");
        tokio::fs::write(&collision, "keep").await?;
        let mut collision_move = request(FileOperation::Move, &moved_file, None);
        collision_move.destination = Some(collision.to_string_lossy().into_owned());
        assert!(execute_local(&collision_move, None).await.is_err());
        assert_eq!(tokio::fs::read_to_string(&collision).await?, "keep");
        assert!(tokio::fs::try_exists(&moved_file).await?);

        let nested_directory = created_directory.join("nested");
        tokio::fs::create_dir(&nested_directory).await?;
        let mut recursive_move = request(FileOperation::Move, &created_directory, None);
        recursive_move.destination = Some(
            nested_directory
                .join("created")
                .to_string_lossy()
                .into_owned(),
        );
        assert!(execute_local(&recursive_move, None).await.is_err());

        execute_local(
            &request(FileOperation::Delete, &created_directory, None),
            None,
        )
        .await?;
        assert!(!tokio::fs::try_exists(&created_directory).await?);
        execute_local(&request(FileOperation::Delete, &moved_file, None), None).await?;
        execute_local(&request(FileOperation::Delete, &collision, None), None).await?;

        tokio::fs::remove_dir_all(&directory).await?;
        Ok(())
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn local_root_alias_cannot_be_deleted() -> Result<()> {
        let root_alias = Path::new("/tmp/..");
        let result = execute_local(&request(FileOperation::Delete, root_alias, None), None).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("filesystem roots cannot be deleted"));
        Ok(())
    }
}
