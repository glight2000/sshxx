//! Authenticated local persistence for reusable SSH connection profiles.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{ensure, Context, Result};
use prost::Message;
use sshx_core::proto::SshProfileCollection;
use sshx_core::{rand_alphanumeric, SSH_PROFILE_FORMAT_VERSION};
use tokio::io::AsyncWriteExt;

use crate::encrypt::Encrypt;

pub const FILE_NAME: &str = ".sshx-connections";
pub const KEY_FILE_NAME: &str = ".sshx-connections.key";

const MAGIC: &[u8; 8] = b"SSHXXCFG";
const ENVELOPE_VERSION: u8 = 1;
const NONCE_LEN: usize = 12;

pub fn path_in_current_dir() -> Result<PathBuf> {
    Ok(std::env::current_dir()
        .context("failed to determine daemon working directory")?
        .join(FILE_NAME))
}

pub fn empty() -> SshProfileCollection {
    SshProfileCollection {
        format_version: SSH_PROFILE_FORMAT_VERSION,
        profiles: Vec::new(),
    }
}

/// Load or create the local key protecting the connection profile file.
pub async fn load_or_create_encryptor(profile_path: &Path) -> Result<Encrypt> {
    let key_path = profile_path.with_file_name(KEY_FILE_NAME);
    match tokio::fs::read_to_string(&key_path).await {
        Ok(key) => {
            let key = key.trim();
            ensure!(key.len() >= 24, "SSH profile key file is invalid");
            Ok(Encrypt::new(key))
        }
        Err(err) if err.kind() == ErrorKind::NotFound => {
            let key = rand_alphanumeric(32);
            let mut options = tokio::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            options.mode(0o600);
            match options.open(&key_path).await {
                Ok(mut file) => {
                    file.write_all(key.as_bytes()).await?;
                    file.write_all(b"\n").await?;
                    file.sync_all().await?;
                    Ok(Encrypt::new(&key))
                }
                Err(err) if err.kind() == ErrorKind::AlreadyExists => {
                    let key = tokio::fs::read_to_string(&key_path).await?;
                    let key = key.trim();
                    ensure!(key.len() >= 24, "SSH profile key file is invalid");
                    Ok(Encrypt::new(key))
                }
                Err(err) => {
                    Err(err).with_context(|| format!("failed to create {}", key_path.display()))
                }
            }
        }
        Err(err) => Err(err).with_context(|| format!("failed to read {}", key_path.display())),
    }
}

/// Preserve an invalid key and its now-unreadable data, then create a fresh
/// key.
pub async fn replace_invalid_encryptor(profile_path: &Path) -> Result<Encrypt> {
    let key_path = profile_path.with_file_name(KEY_FILE_NAME);
    if tokio::fs::try_exists(&key_path).await? {
        let invalid_key = key_path.with_file_name(format!(
            "{}.invalid-{}",
            KEY_FILE_NAME,
            rand_alphanumeric(8)
        ));
        tokio::fs::rename(&key_path, invalid_key).await?;
    }
    if tokio::fs::try_exists(profile_path).await? {
        quarantine(profile_path).await?;
    }
    load_or_create_encryptor(profile_path).await
}

pub async fn load(path: &Path, encrypt: &Encrypt) -> Result<Option<SshProfileCollection>> {
    let data = match tokio::fs::read(path).await {
        Ok(data) => data,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err).with_context(|| format!("failed to read {}", path.display())),
    };
    ensure!(
        data.len() >= MAGIC.len() + 1 + NONCE_LEN + 16,
        "encrypted SSH profile file is truncated"
    );
    ensure!(
        &data[..MAGIC.len()] == MAGIC,
        "invalid SSH profile file header"
    );
    ensure!(
        data[MAGIC.len()] == ENVELOPE_VERSION,
        "unsupported encrypted SSH profile envelope version {}",
        data[MAGIC.len()]
    );
    let nonce_start = MAGIC.len() + 1;
    let nonce: &[u8; NONCE_LEN] = data[nonce_start..nonce_start + NONCE_LEN]
        .try_into()
        .expect("nonce slice has a fixed length");
    let plaintext = encrypt
        .open(nonce, &data[nonce_start + NONCE_LEN..])
        .with_context(|| format!("failed to decrypt {}", path.display()))?;
    let profiles = SshProfileCollection::decode(&*plaintext)
        .with_context(|| format!("failed to decode {}", path.display()))?;
    ensure!(
        profiles.format_version == SSH_PROFILE_FORMAT_VERSION,
        "unsupported SSH profile format version {}",
        profiles.format_version
    );
    Ok(Some(profiles))
}

pub async fn save(path: &Path, encrypt: &Encrypt, profiles: &SshProfileCollection) -> Result<()> {
    ensure!(
        profiles.format_version == SSH_PROFILE_FORMAT_VERSION,
        "cannot save unsupported SSH profile format version {}",
        profiles.format_version
    );
    let nonce_string = rand_alphanumeric(NONCE_LEN);
    let nonce: &[u8; NONCE_LEN] = nonce_string
        .as_bytes()
        .try_into()
        .expect("generated nonce has a fixed length");
    let ciphertext = encrypt.seal(nonce, &profiles.encode_to_vec())?;
    let temporary = path.with_extension("tmp");
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    options.mode(0o600);

    let mut file = options
        .open(&temporary)
        .await
        .with_context(|| format!("failed to create {}", temporary.display()))?;
    file.write_all(MAGIC).await?;
    file.write_all(&[ENVELOPE_VERSION]).await?;
    file.write_all(nonce).await?;
    file.write_all(&ciphertext).await?;
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

/// Preserve an unreadable file for manual recovery and return its new path.
pub async fn quarantine(path: &Path) -> Result<PathBuf> {
    let file_name = format!("{}.invalid-{}", FILE_NAME, rand_alphanumeric(8));
    let destination = path.with_file_name(file_name);
    tokio::fs::rename(path, &destination)
        .await
        .with_context(|| format!("failed to quarantine {}", path.display()))?;
    Ok(destination)
}

#[cfg(test)]
mod tests {
    use sshx_core::proto::{SshAuthMethod, SshProfile};

    use super::*;

    fn sample() -> SshProfileCollection {
        SshProfileCollection {
            format_version: SSH_PROFILE_FORMAT_VERSION,
            profiles: vec![SshProfile {
                id: "home".into(),
                name: "Home server".into(),
                host: "server.example.test".into(),
                port: 22,
                username: "dev".into(),
                auth_method: SshAuthMethod::SshAuthKeyFile.into(),
                key_path: "/private/id_ed25519".into(),
                accept_new_host_key: true,
                theme: String::new(),
                background_enabled: false,
                background: String::new(),
            }],
        }
    }

    #[tokio::test]
    async fn encrypted_roundtrip_hides_plaintext() -> Result<()> {
        let directory =
            std::env::temp_dir().join(format!("sshxx-profiles-test-{}", rand_alphanumeric(12)));
        tokio::fs::create_dir(&directory).await?;
        let path = directory.join(FILE_NAME);
        let encrypt = Encrypt::new("test-encryption-key");
        let profiles = sample();

        save(&path, &encrypt, &profiles).await?;
        let stored = tokio::fs::read(&path).await?;
        assert!(!stored.windows(11).any(|part| part == b"Home server"));
        assert_eq!(load(&path, &encrypt).await?, Some(profiles));
        assert!(load(&path, &Encrypt::new("wrong-key")).await.is_err());

        tokio::fs::remove_dir_all(directory).await?;
        Ok(())
    }

    #[tokio::test]
    async fn quarantine_preserves_invalid_file() -> Result<()> {
        let directory = std::env::temp_dir().join(format!(
            "sshxx-profiles-invalid-test-{}",
            rand_alphanumeric(12)
        ));
        tokio::fs::create_dir(&directory).await?;
        let path = directory.join(FILE_NAME);
        tokio::fs::write(&path, b"invalid").await?;
        let destination = quarantine(&path).await?;
        assert!(!tokio::fs::try_exists(&path).await?);
        assert!(tokio::fs::try_exists(&destination).await?);
        tokio::fs::remove_dir_all(directory).await?;
        Ok(())
    }

    #[tokio::test]
    async fn local_key_survives_restart_and_recovers_from_invalid_data() -> Result<()> {
        let directory =
            std::env::temp_dir().join(format!("sshxx-profile-key-test-{}", rand_alphanumeric(12)));
        tokio::fs::create_dir(&directory).await?;
        let path = directory.join(FILE_NAME);
        let first = load_or_create_encryptor(&path).await?;
        save(&path, &first, &sample()).await?;
        let second = load_or_create_encryptor(&path).await?;
        assert_eq!(load(&path, &second).await?, Some(sample()));

        tokio::fs::write(directory.join(KEY_FILE_NAME), b"broken").await?;
        let replacement = replace_invalid_encryptor(&path).await?;
        assert_eq!(load(&path, &replacement).await?, None);
        let mut entries = tokio::fs::read_dir(&directory).await?;
        let mut preserved_invalid_file = false;
        while let Some(entry) = entries.next_entry().await? {
            preserved_invalid_file |= entry.file_name().to_string_lossy().contains("invalid");
        }
        assert!(preserved_invalid_file);

        tokio::fs::remove_dir_all(directory).await?;
        Ok(())
    }
}
