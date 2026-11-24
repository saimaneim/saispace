use crate::error::{CoreError, CoreResult};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use std::io::Write;
use std::path::Path;

const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 12;

pub struct Vault {
    cipher: ChaCha20Poly1305,
}

impl std::fmt::Debug for Vault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Vault")
    }
}

fn vault_error(context: &str, err: impl std::fmt::Display) -> CoreError {
    CoreError::Vault(format!("{context}: {err}"))
}

fn random_bytes<const N: usize>() -> CoreResult<[u8; N]> {
    let mut buf = [0u8; N];
    getrandom::fill(&mut buf).map_err(|e| vault_error("random generator", e))?;
    Ok(buf)
}

fn create_key_file(path: &Path) -> CoreResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| vault_error("create key dir", e))?;
    }
    let key = random_bytes::<KEY_LEN>()?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(path) {
        Ok(mut file) => {
            file.write_all(&key)
                .and_then(|()| file.sync_all())
                .map_err(|e| vault_error("write key", e))?;
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(vault_error("create key", e)),
    }
}

impl Vault {
    pub fn open(key_path: &Path) -> CoreResult<Self> {
        if !key_path.exists() {
            create_key_file(key_path)?;
        }
        let bytes = std::fs::read(key_path).map_err(|e| vault_error("read key", e))?;
        if bytes.len() != KEY_LEN {
            return Err(CoreError::Vault(format!(
                "key file {} must be {KEY_LEN} bytes",
                key_path.display()
            )));
        }
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&bytes));
        Ok(Self { cipher })
    }

    pub fn seal(&self, name: &str, plaintext: &[u8]) -> CoreResult<(Vec<u8>, Vec<u8>)> {
        let nonce = random_bytes::<NONCE_LEN>()?;
        let ciphertext = self
            .cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: plaintext,
                    aad: name.as_bytes(),
                },
            )
            .map_err(|e| vault_error("encrypt", e))?;
        Ok((nonce.to_vec(), ciphertext))
    }

    pub fn unseal(&self, name: &str, nonce: &[u8], ciphertext: &[u8]) -> CoreResult<Vec<u8>> {
        if nonce.len() != NONCE_LEN {
            return Err(CoreError::Vault("corrupt nonce".to_string()));
        }
        self.cipher
            .decrypt(
                Nonce::from_slice(nonce),
                Payload {
                    msg: ciphertext,
                    aad: name.as_bytes(),
                },
            )
            .map_err(|e| vault_error("decrypt", e))
    }
}
