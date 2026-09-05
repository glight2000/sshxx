//! Stage complete file contents before replacing a destination. Never truncate
//! the destination as a fallback when atomic replacement is unavailable.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{ensure, Context, Result};
use openssh_sftp_client::{error::SftpErrorKind, metadata::MetaDataBuilder, Error, Sftp};
use tempfile::NamedTempFile;

mod metadata;

struct StagedLocal {
    target: PathBuf,
    temporary: NamedTempFile,
    original: Option<fs::Metadata>,
}

impl StagedLocal {
    fn prepare(path: &Path, content: &[u8]) -> Result<Self> {
        let target = match fs::symlink_metadata(path) {
            Ok(_) => fs::canonicalize(path)?, // Follow links, never replace the link itself.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => path.to_owned(),
            Err(error) => return Err(error.into()),
        };
        let source = match fs::metadata(&target) {
            Ok(info) => {
                ensure!(info.is_file(), "safe saving requires a regular file");
                let mut options = OpenOptions::new();
                options.read(true).write(true); // Respect target permissions, not just directory permissions.
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK);
                }
                Some(options.open(&target)?)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        let mut temporary = tempfile::Builder::new()
            .prefix(".sshxx-save-")
            .tempfile_in(target.parent().context("file has no parent directory")?)?;
        let original = source.as_ref().map(File::metadata).transpose()?;
        if let Some(info) = &original {
            ensure!(info.is_file(), "safe saving requires a regular file");
            metadata::validate(info)?;
        }
        temporary.write_all(content)?;
        if let Some(source) = source.as_ref() {
            metadata::copy(source, temporary.as_file())?;
        }
        temporary.as_file().sync_all()?;
        Ok(Self {
            target,
            temporary,
            original,
        })
    }

    fn commit(self) -> Result<()> {
        if let Some(original) = &self.original {
            let current = fs::metadata(&self.target)?;
            ensure!(
                metadata::unchanged(original, &current),
                "file changed while saving; reload it before retrying"
            );
            metadata::replace(self.temporary, &self.target)?;
        } else {
            self.temporary.persist_noclobber(&self.target)?;
        }
        Ok(())
    }
}

pub(crate) async fn write_local(path: &Path, content: Vec<u8>) -> Result<()> {
    let path = path.to_owned();
    // Cancellation during preparation drops the returned staging file. The
    // blocking worker never commits a save after its requesting future is gone.
    let staged =
        tokio::task::spawn_blocking(move || StagedLocal::prepare(&path, &content)).await??;
    staged.commit()
}

pub(crate) async fn write_sftp(sftp: &Sftp, path: &Path, content: &[u8]) -> Result<()> {
    let mut fs = sftp.fs();
    let original = match fs.metadata(path).await {
        Ok(info) => Some(info),
        Err(Error::SftpError(SftpErrorKind::NoSuchFile, _)) => None,
        Err(error) => return Err(error.into()),
    };
    let target = if let Some(info) = original {
        ensure!(
            info.file_type().is_some_and(|kind| kind.is_file()),
            "safe saving requires a regular remote file"
        );
        ensure!(
            sftp.support_posix_rename(),
            "this SFTP server cannot atomically replace files; original file was not changed"
        );
        // Opening without truncate checks the original file's write permission.
        sftp.options().write(true).open(path).await?.close().await?;
        fs.canonicalize(path).await?
    } else {
        // Reject dangling links rather than replacing the link itself.
        match fs.symlink_metadata(path).await {
            Err(Error::SftpError(SftpErrorKind::NoSuchFile, _)) => {}
            Err(error) => return Err(error.into()),
            Ok(_) => anyhow::bail!("cannot safely save through a dangling remote link"),
        }
        fs.canonicalize(path.parent().context("file has no parent directory")?)
            .await?
            .join(path.file_name().context("file has no name")?)
    };
    let temporary =
        target.with_file_name(format!(".sshxx-save-{}", sshx_core::rand_alphanumeric(24)));
    let mut file = sftp
        .options()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&temporary)
        .await?;
    let result: Result<()> = async {
        file.set_permissions(0o600.into()).await?;
        file.write_all(content).await?;
        if let Some(info) = original {
            let current = file.metadata().await?;
            let uid = info
                .uid()
                .context("SFTP server omitted original file owner")?;
            let gid = info
                .gid()
                .context("SFTP server omitted original file group")?;
            if current.uid() != Some(uid) || current.gid() != Some(gid) {
                file.set_metadata(MetaDataBuilder::new().id((uid, gid)).create())
                    .await?;
            }
            file.set_permissions(
                info.permissions()
                    .context("SFTP server omitted file permissions")?,
            )
            .await?;
        }
        if sftp.support_fsync() {
            file.sync_all().await?;
        }
        file.close().await?;
        if let Some(info) = original {
            ensure!(
                fs.metadata(&target).await? == info,
                "remote file changed while saving; reload it before retrying"
            );
            fs.rename(&temporary, &target).await?;
        } else {
            // SFTP v3's basic rename rejects existing targets, unlike the
            // OpenSSH POSIX extension. Use a hard link for exclusive publication.
            ensure!(
                sftp.support_hardlink(),
                "SFTP server lacks exclusive atomic file publication"
            );
            fs.hard_link(&temporary, &target).await?;
            fs.remove_file(&temporary).await?;
        }
        Ok(())
    }
    .await;
    if result.is_err() {
        fs.remove_file(&temporary).await.ok();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staging_failure_or_cancellation_keeps_original_and_cleans_up() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test.txt");
        fs::write(&path, "original")?;
        let staged = StagedLocal::prepare(&path, b"replacement")?;
        assert_eq!(fs::read(&path)?, b"original");
        drop(staged);
        assert_eq!(fs::read(&path)?, b"original");
        assert_eq!(fs::read_dir(dir.path())?.count(), 1);
        let staged = StagedLocal::prepare(&path, b"replacement")?;
        fs::write(&path, "concurrent edit")?;
        assert!(staged.commit().is_err());
        assert_eq!(fs::read(&path)?, b"concurrent edit");
        let staged = StagedLocal::prepare(&path, b"replacement")?;
        staged.commit()?;
        assert_eq!(fs::read(&path)?, b"replacement");
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn follows_symlinks_preserves_mode_and_rejects_hard_links() -> Result<()> {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test.txt");
        let link = dir.path().join("link.txt");
        fs::write(&path, "original")?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640))?;
        symlink(&path, &link)?;
        StagedLocal::prepare(&link, b"replacement")?.commit()?;
        assert!(fs::symlink_metadata(&link)?.is_symlink());
        assert_eq!(fs::metadata(&path)?.permissions().mode() & 0o777, 0o640);
        assert_eq!(fs::read(&path)?, b"replacement");
        fs::hard_link(&path, dir.path().join("hard.txt"))?;
        assert!(StagedLocal::prepare(&path, b"must not overwrite").is_err());
        Ok(())
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn safe_replace_preserves_posix_acl() -> Result<()> {
        use std::process::Command;
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("acl.txt");
        fs::write(&path, "before")?;
        let status = match Command::new("setfacl")
            .args(["-m", "u:12345:r--"])
            .arg(&path)
            .status()
        {
            Ok(status) => status,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                eprintln!("SKIP: setfacl is needed for the native ACL integration test");
                return Ok(());
            }
            Err(error) => return Err(error.into()),
        };
        ensure!(status.success(), "setfacl failed");
        let before = Command::new("getfacl").arg("-cpn").arg(&path).output()?;
        StagedLocal::prepare(&path, b"after")?.commit()?;
        let after = Command::new("getfacl").arg("-cpn").arg(&path).output()?;
        ensure!(
            before.status.success() && after.status.success(),
            "getfacl failed"
        );
        assert_eq!(before.stdout, after.stdout);
        assert_eq!(fs::read(&path)?, b"after");
        Ok(())
    }
}
