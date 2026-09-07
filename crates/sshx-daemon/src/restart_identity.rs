//! Short-lived, authenticated-encrypted identity handoff for process restart.
//! This is not normal session persistence: consume after successful reconnect.

use std::io::{Read, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};

use crate::{encrypt::Encrypt, ssh_profiles};

pub(crate) const FILE_NAME: &str = ".sshx-restart";
const MAX_BYTES: u64 = 16 * 1024;
const LIFETIME_SECONDS: u64 = 10 * 60;

// Deliberately no Debug implementation: these fields are credentials.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Identity {
    pub origin: String,
    pub name: String,
    pub token: String,
    pub encryption_key: String,
    pub write_password: Option<String>,
    created_at: u64,
    format_version: u32,
}

impl Identity {
    pub fn new(
        origin: String,
        name: String,
        token: String,
        encryption_key: String,
        write_password: Option<String>,
    ) -> Self {
        Self {
            origin,
            name,
            token,
            encryption_key,
            write_password,
            created_at: now(),
            format_version: 1,
        }
    }

    pub fn save(&self, directory: &Path, encrypt: &Encrypt) -> Result<()> {
        ensure!(
            !self.name.is_empty()
                && self.name.len() <= 128
                && !self.token.is_empty()
                && self.token.len() <= 128,
            "restart identity exceeds server limits"
        );
        let nonce = sshx_core::rand_alphanumeric(12);
        let plaintext = serde_json::to_vec(self)?;
        ensure!(
            plaintext.len() < MAX_BYTES as usize - 28,
            "restart identity is too large"
        );
        let ciphertext = encrypt.seal(nonce.as_bytes().try_into()?, &plaintext)?;
        let mut file = tempfile::NamedTempFile::new_in(directory)?;
        file.write_all(nonce.as_bytes())?;
        file.write_all(&ciphertext)?;
        file.as_file().sync_all()?;
        file.persist(directory.join(FILE_NAME))
            .context("could not save encrypted restart handoff")?;
        Ok(())
    }

    pub async fn load(directory: &Path, origin: &str) -> Result<Option<Self>> {
        let file = match std::fs::File::open(directory.join(FILE_NAME)) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error).context("could not read restart handoff"),
        };
        let mut bytes = Vec::new();
        file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
        ensure!(
            (28..=MAX_BYTES as usize).contains(&bytes.len()),
            "invalid restart handoff size"
        );
        let encrypt =
            ssh_profiles::load_or_create_encryptor(&directory.join(ssh_profiles::FILE_NAME))
                .await?;
        Self::decode(&bytes, &encrypt, origin).map(Some)
    }

    fn decode(bytes: &[u8], encrypt: &Encrypt, origin: &str) -> Result<Self> {
        let plaintext = encrypt.open(bytes[..12].try_into()?, &bytes[12..])?;
        // Do not include parser errors or credential contents in diagnostics.
        let identity: Self = serde_json::from_slice(&plaintext)
            .map_err(|_| anyhow::anyhow!("invalid restart identity"))?;
        ensure!(
            identity.format_version == 1 && identity.origin == origin,
            "restart handoff version or server mismatch"
        );
        let current_time = now();
        ensure!(
            identity.created_at <= current_time
                && current_time - identity.created_at <= LIFETIME_SECONDS,
            "restart handoff expired; remove .sshx-restart to start a new session"
        );
        Ok(identity)
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handoff_is_encrypted_authenticated_and_scoped() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let encrypt = Encrypt::new("restart-test");
        let identity = Identity::new(
            "http://server.test".into(),
            "session".into(),
            "test-token".into(),
            "private-test-key".into(),
            Some("write-test-key".into()),
        );
        identity.save(directory.path(), &encrypt)?;
        let bytes = std::fs::read(directory.path().join(FILE_NAME))?;
        assert!(!bytes.windows(16).any(|bytes| bytes == b"private-test-key"));
        assert_eq!(
            Identity::decode(&bytes, &encrypt, &identity.origin)?.name,
            "session"
        );
        assert!(Identity::decode(&bytes, &encrypt, "http://other.test").is_err());
        let mut corrupt = bytes;
        corrupt[15] ^= 1;
        assert!(Identity::decode(&corrupt, &encrypt, &identity.origin).is_err());
        let expired = Identity {
            created_at: 0,
            ..identity
        };
        expired.save(directory.path(), &encrypt)?;
        assert!(Identity::decode(
            &std::fs::read(directory.path().join(FILE_NAME))?,
            &encrypt,
            &expired.origin
        )
        .is_err());
        Ok(())
    }
}
