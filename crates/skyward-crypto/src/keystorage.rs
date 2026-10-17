use crate::hal::{HalError, HalResult, Keymint};
use crate::password::app_id;
use scrypt::{scrypt, Params};
use std::fs;
use std::path::Path;

/// Secret-stretching method recorded alongside a stored key.
#[derive(Clone, Debug)]
pub enum Stretching {
    None,
    Scrypt { log_n: u8, r: u32, p: u32, salt: Vec<u8> },
}

/// The on-disk material for one vold-style wrapped key: the KeyMint-wrapped key
/// bytes, the KeyMint key blob that unwraps them, the secdiscardable blob and
/// the stretching method. Mirrors the cryptographic structure of vold's
/// KeyStorage so the device and host paths are identical.
#[derive(Clone)]
pub struct StoredKey {
    /// nonce(12) || ciphertext || tag(16)
    pub encrypted_key: Vec<u8>,
    pub key_blob: Vec<u8>,
    pub secdiscardable: Vec<u8>,
    pub stretching: Stretching,
}

impl StoredKey {
    pub fn load(dir: impl AsRef<Path>) -> HalResult<Self> {
        let dir = dir.as_ref();
        let rd = |n: &str| {
            fs::read(dir.join(n)).map_err(|e| HalError::Io(format!("{}: {e}", dir.join(n).display())))
        };
        let encrypted_key = rd("encrypted_key")?;
        let key_blob = rd("keymaster_key_blob")?;
        let secdiscardable = rd("secdiscardable")?;
        let stretching = match fs::read_to_string(dir.join("stretching")) {
            Ok(s) if s.trim_start().starts_with("scrypt") => {
                let p: Vec<_> = s.trim().split_whitespace().collect();
                Stretching::Scrypt {
                    log_n: p.get(1).and_then(|v| v.parse().ok()).unwrap_or(15),
                    r: p.get(2).and_then(|v| v.parse().ok()).unwrap_or(8),
                    p: p.get(3).and_then(|v| v.parse().ok()).unwrap_or(1),
                    salt: rd("salt")?,
                }
            }
            _ => Stretching::None,
        };
        Ok(StoredKey { encrypted_key, key_blob, secdiscardable, stretching })
    }

    pub fn stretch(&self, secret: &[u8]) -> HalResult<Vec<u8>> {
        match &self.stretching {
            Stretching::None => Ok(secret.to_vec()),
            Stretching::Scrypt { log_n, r, p, salt } => {
                let params = Params::new(*log_n, *r, *p, 32).map_err(|_| HalError::KeyMintFailed)?;
                let mut out = vec![0u8; 32];
                scrypt(secret, salt, &params, &mut out).map_err(|_| HalError::KeyMintFailed)?;
                Ok(out)
            }
        }
    }
}

/// Unwrap a stored key with `secret`. The secret is stretched, combined with
/// the secdiscardable hash into the KeyMint application id, and the unwrap is
/// delegated to KeyMint — which only succeeds when the application id matches.
pub fn decrypt(km: &dyn Keymint, stored: &StoredKey, secret: &[u8]) -> HalResult<Vec<u8>> {
    if stored.encrypted_key.len() < 12 + 16 {
        return Err(HalError::KeyMintFailed);
    }
    let stretched = stored.stretch(secret)?;
    let app = app_id(&stretched, &stored.secdiscardable);
    let (nonce, body) = stored.encrypted_key.split_at(12);
    km.gcm_decrypt(&stored.key_blob, &app, nonce, body)
}
