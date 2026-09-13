//! Encryption of byte streams based on a random key.

use aes::cipher::{KeyIvInit, StreamCipher, StreamCipherSeek};
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes128Gcm, Nonce};
use anyhow::{anyhow, Result};

type Aes128Ctr64BE = ctr::Ctr64BE<aes::Aes128>;

// Note: The KDF salt is public, as it needs to be used from the web client. It
// only exists to make rainbow table attacks less likely.
const SALT: &str =
    "This is a non-random salt for sshx.io, since we want to stretch the security of 83-bit keys!";

/// Encrypts byte streams using the Argon2 hash of a random key.
#[derive(Clone)]
pub struct Encrypt {
    aes_key: [u8; 16], // 128-bit
    checkpoint_key: [u8; 16],
}

impl Encrypt {
    /// Construct a new encryptor.
    pub fn new(key: &str) -> Self {
        use argon2::{Algorithm, Argon2, Params, Version};
        // These parameters must match the browser implementation.
        let hasher = Argon2::new(
            Algorithm::Argon2id,
            Version::V0x13,
            Params::new(19 * 1024, 2, 1, Some(16)).unwrap(),
        );
        let mut aes_key = [0; 16];
        hasher
            .hash_password_into(key.as_bytes(), SALT.as_bytes(), &mut aes_key)
            .expect("failed to hash key with argon2");
        Self {
            checkpoint_key: checkpoint_key(&aes_key),
            aes_key,
        }
    }

    /// Get the encrypted zero block.
    pub fn zeros(&self) -> Vec<u8> {
        let mut zeros = [0; 16];
        let mut cipher = Aes128Ctr64BE::new(&self.aes_key.into(), &zeros.into());
        cipher.apply_keystream(&mut zeros);
        zeros.to_vec()
    }

    /// Encrypt a segment of data from a stream.
    ///
    /// Note that in CTR mode, the encryption operation is the same as the
    /// decryption operation.
    pub fn segment(&self, stream_num: u64, offset: u64, data: &[u8]) -> Vec<u8> {
        Self::segment_with_key(&self.aes_key, stream_num, offset, data)
    }

    /// A fresh epoch prevents key-stream reuse when a terminal or daemon restarts.
    pub fn output_segment(
        &self,
        epoch: &[u8],
        stream_num: u64,
        offset: u64,
        data: &[u8],
    ) -> Vec<u8> {
        if epoch.is_empty() {
            return self.segment(stream_num, offset, data);
        }
        assert_eq!(epoch.len(), 16, "invalid terminal output epoch");
        let key = output_key(&self.aes_key, epoch);
        Self::segment_with_key(&key, stream_num, offset, data)
    }

    fn segment_with_key(key: &[u8; 16], stream_num: u64, offset: u64, data: &[u8]) -> Vec<u8> {
        assert_ne!(stream_num, 0, "stream number must be nonzero"); // security check

        let mut iv = [0; 16];
        iv[0..8].copy_from_slice(&stream_num.to_be_bytes());

        let mut cipher = Aes128Ctr64BE::new(&(*key).into(), &iv.into());
        let mut buf = data.to_vec();
        cipher.seek(offset);
        cipher.apply_keystream(&mut buf);
        buf
    }

    /// Encrypt and authenticate a standalone value using a unique nonce.
    pub fn seal(&self, nonce: &[u8; 12], plaintext: &[u8]) -> Result<Vec<u8>> {
        let cipher = Aes128Gcm::new_from_slice(&self.aes_key)
            .map_err(|_| anyhow!("invalid AES key length"))?;
        cipher
            .encrypt(Nonce::from_slice(nonce), plaintext)
            .map_err(|_| anyhow!("failed to encrypt authenticated value"))
    }

    /// Decrypt and authenticate a standalone value using its stored nonce.
    pub fn open(&self, nonce: &[u8; 12], ciphertext: &[u8]) -> Result<Vec<u8>> {
        let cipher = Aes128Gcm::new_from_slice(&self.aes_key)
            .map_err(|_| anyhow!("invalid AES key length"))?;
        cipher
            .decrypt(Nonce::from_slice(nonce), ciphertext)
            .map_err(|_| anyhow!("encrypted value failed authentication"))
    }

    /// Domain-separated from CTR, whose encrypted zero block is public.
    pub fn seal_checkpoint(&self, nonce: &[u8; 12], plaintext: &[u8]) -> Result<Vec<u8>> {
        Aes128Gcm::new(&self.checkpoint_key.into())
            .encrypt(Nonce::from_slice(nonce), plaintext)
            .map_err(|_| anyhow!("failed to encrypt checkpoint"))
    }

    /// Authenticate a checkpoint with its dedicated, domain-separated key.
    pub fn open_checkpoint(&self, nonce: &[u8; 12], ciphertext: &[u8]) -> Result<Vec<u8>> {
        Aes128Gcm::new(&self.checkpoint_key.into())
            .decrypt(Nonce::from_slice(nonce), ciphertext)
            .map_err(|_| anyhow!("checkpoint failed authentication"))
    }
}

/// RFC 5869 HKDF-SHA256, one expand block (16-byte AES key). Reuses the
/// repository's HMAC/SHA dependencies; WebCrypto supplies the browser side.
fn output_key(input: &[u8; 16], epoch: &[u8]) -> [u8; 16] {
    derive_key(input, b"sshxx/terminal-output/v1", epoch)
}

fn checkpoint_key(input: &[u8; 16]) -> [u8; 16] {
    derive_key(input, b"sshxx/terminal-checkpoint/v1", b"aes-128-gcm")
}

fn derive_key(input: &[u8; 16], salt: &[u8], info: &[u8]) -> [u8; 16] {
    use hmac::{Hmac, KeyInit as _, Mac};
    use sha2::Sha256;
    let mut extract = Hmac::<Sha256>::new_from_slice(salt).unwrap();
    extract.update(input);
    let mut expand = Hmac::<Sha256>::new_from_slice(&extract.finalize().into_bytes()).unwrap();
    expand.update(info);
    expand.update(&[1]);
    expand.finalize().into_bytes()[..16].try_into().unwrap()
}

#[cfg(test)]
mod tests {
    use super::Encrypt;

    #[test]
    fn output_incarnations_do_not_reuse_ctr_keystream() {
        let vector = Encrypt {
            aes_key: [0; 16],
            checkpoint_key: [0; 16],
        }
        .output_segment(&[1; 16], 0x100000007, 0, b"synthetic output");
        assert_eq!(
            vector
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            "ac80c57ee9df8e5fa71f7a6059a86228"
        );
        let encrypt = Encrypt::new("synthetic-output-epochs");
        let known = b"known synthetic output";
        let next = b"other synthetic bytes";
        let old = encrypt.output_segment(&[1; 16], 0x100000007, 0, known);
        let fresh = encrypt.output_segment(&[2; 16], 0x100000007, 0, next);
        let recovered: Vec<u8> = fresh
            .iter()
            .zip(&old)
            .zip(known)
            .map(|((new, old), plain)| new ^ old ^ plain)
            .collect();
        assert_ne!(recovered, next);
        assert_eq!(
            encrypt.output_segment(&[2; 16], 0x100000007, 0, &fresh),
            next
        );
        assert_eq!(
            encrypt.output_segment(&[2; 16], 0x100000007, 3, &fresh[3..]),
            &next[3..]
        );
        assert_eq!(
            encrypt.output_segment(&[], 0x100000007, 0, known),
            encrypt.segment(0x100000007, 0, known)
        );
    }

    #[test]
    fn checkpoint_key_matches_webcrypto_and_is_not_the_ctr_key() -> anyhow::Result<()> {
        let key = super::checkpoint_key(&[0; 16]);
        assert_eq!(
            key,
            [
                0xb4, 0x4e, 0x77, 0x17, 0x76, 0x47, 0x7f, 0x26, 0x03, 0x90, 0x72, 0x89, 0x23, 0x6c,
                0x95, 0xb9
            ]
        );
        let encrypt = Encrypt {
            aes_key: [0; 16],
            checkpoint_key: key,
        };
        let data = encrypt.seal_checkpoint(&[0; 12], b"synthetic checkpoint")?;
        assert_eq!(
            data.iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            "1f2029956effdcdb39c74aef6a504af819c591f3580d3b1cf297be8bb5a6346526599c92"
        );
        assert!(encrypt.open(&[0; 12], &data).is_err());
        assert_eq!(
            encrypt.open_checkpoint(&[0; 12], &data)?,
            b"synthetic checkpoint"
        );
        Ok(())
    }

    #[test]
    fn make_encrypt() {
        let encrypt = Encrypt::new("test");
        assert_eq!(
            encrypt.zeros(),
            [198, 3, 249, 238, 65, 10, 224, 98, 253, 73, 148, 1, 138, 3, 108, 143],
        );
    }

    #[test]
    fn roundtrip_ctr() {
        let encrypt = Encrypt::new("this is a test key");
        let data = b"hello world";
        let encrypted = encrypt.segment(1, 0, data);
        assert_eq!(encrypted.len(), data.len());
        let decrypted = encrypt.segment(1, 0, &encrypted);
        assert_eq!(decrypted, data);
    }

    #[test]
    fn matches_offset() {
        let encrypt = Encrypt::new("this is a test key");
        let data = b"1st block.(16B)|2nd block......|3rd block";
        let encrypted = encrypt.segment(1, 0, data);
        assert_eq!(encrypted.len(), data.len());
        for i in 1..data.len() {
            let encrypted_suffix = encrypt.segment(1, i as u64, &data[i..]);
            assert_eq!(encrypted_suffix, &encrypted[i..]);
        }
    }

    #[test]
    fn roundtrip_authenticated_value() -> anyhow::Result<()> {
        let encrypt = Encrypt::new("this is a test key");
        let nonce = *b"unique-nonce";
        let encrypted = encrypt.seal(&nonce, b"connection profile")?;
        assert_ne!(encrypted, b"connection profile");
        assert_eq!(encrypt.open(&nonce, &encrypted)?, b"connection profile");
        assert!(encrypt
            .open(&nonce, &encrypted[..encrypted.len() - 1])
            .is_err());
        Ok(())
    }

    #[test]
    #[should_panic]
    fn zero_stream_num() {
        let encrypt = Encrypt::new("this is a test key");
        encrypt.segment(0, 0, b"hello world");
    }
}
