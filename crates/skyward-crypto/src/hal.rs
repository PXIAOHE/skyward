//! Secure-world interfaces the decrypt flow depends on. On the device these are
//! backed by the phone's own binder HALs (started from its vendor partition);
//! the mock in [`crate::mock`] implements the same traits so the whole flow is
//! exercised on the host without a TEE.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HalError {
    WrongCredential,
    /// Throttled by the secure world; the payload is the wait in milliseconds.
    Throttled(u32),
    NotEnrolled,
    KeyMintFailed,
    Io(String),
}

pub type HalResult<T> = Result<T, HalError>;

/// A hardware auth token proving a credential was verified.
#[derive(Debug, Clone)]
pub struct AuthToken(pub Vec<u8>);

pub trait Gatekeeper {
    fn verify(&mut self, user: u32, credential: &[u8]) -> HalResult<AuthToken>;
}

/// Weaver releases a stored secret only when the per-slot key matches, and
/// throttles on mismatch. It backs the synthetic-password secret.
pub trait Weaver {
    fn read(&mut self, slot: u32, key: &[u8]) -> HalResult<Vec<u8>>;
}

/// KeyMint unwraps the storage key. The unwrap is bound to `app_id`: the TEE
/// only returns plaintext when the presented application id matches the one the
/// key was created with, so a wrong credential fails here rather than yielding
/// a usable key.
pub trait Keymint {
    fn gcm_decrypt(&self, key_blob: &[u8], app_id: &[u8], nonce: &[u8], ct_and_tag: &[u8]) -> HalResult<Vec<u8>>;
}

/// Installs a raw file-based-encryption key into the kernel keyring. On device
/// this is the fscrypt `add_key` ioctl; the mock records it.
pub trait FscryptSink {
    fn install_key(&mut self, raw: &[u8]) -> HalResult<String>;
}
