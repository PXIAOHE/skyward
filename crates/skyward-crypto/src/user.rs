//! Recovering the secret a user's own files are wrapped with. A phone
//! keeps, for each user, a blob that only two things together open: a key
//! its secure world holds, and a secret that follows from what the user
//! knows. What comes out is the synthetic password, and every key that
//! belongs to the user follows from that.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use anyhow::{bail, Result};
use hmac::{Hmac, Mac};
use sha2::Sha256;

/// Each of these names one use of the hashing, so the same bytes hashed
/// for two purposes never agree.
const SECDISCARDABLE: &[u8] = b"secdiscardable-transform";
const PROTECTOR_SECRET: &[u8] = b"application-id";
const FILE_KEY: &[u8] = b"fbe-key";
const CONTEXT: &[u8] = b"android-synthetic-password-personalization-context";

/// What stands in for a credential on a user who has set none.
const NO_CREDENTIAL: &[u8] = b"default-password";

/// How long the stretched credential is, whether it was stretched or
/// stood in for.
const STRETCHED_BYTES: usize = 32;

const NONCE_BYTES: usize = 12;
const KEY_BYTES: usize = 32;

/// The layout the blob is written in. Only the newest is read here: the
/// two before it derive their subkeys another way, and a phone that still
/// has one has not been through an update in years.
const LATEST: u8 = 3;

/// The blob belongs to what the user knows, rather than to a token some
/// other part of the phone holds.
const BY_CREDENTIAL: u8 = 0;

/// What a user's secret is kept behind.
pub struct Protector {
    /// What the phone calls it, which is also what the key held for it is
    /// named.
    pub id: String,
    /// The blob itself, as the phone wrote it.
    pub blob: Vec<u8>,
    /// The bytes whose erasure makes the blob unopenable.
    pub secdiscardable: Vec<u8>,
}

impl Protector {
    pub fn key_alias(&self) -> String {
        format!("synthetic_password_{}", self.id.trim_start_matches('0'))
    }
}

/// SHA-512 of the parts behind a fixed name for what they are for.
pub fn personalized_hash(personalization: &[u8], parts: &[&[u8]]) -> Vec<u8> {
    use sha2::{Digest, Sha512};
    let mut padded = personalization.to_vec();
    padded.resize(128, 0);
    let mut h = Sha512::new();
    h.update(&padded);
    for part in parts {
        h.update(part);
    }
    h.finalize().to_vec()
}

/// What a user who has set no credential has in place of one.
pub fn stretched_without_credential() -> Vec<u8> {
    let mut out = NO_CREDENTIAL.to_vec();
    out.resize(STRETCHED_BYTES, 0);
    out
}

/// The secret that, with the key the secure world holds, opens the blob.
pub fn protector_secret(stretched: &[u8], secdiscardable: &[u8]) -> Vec<u8> {
    let mut out = stretched.to_vec();
    out.extend_from_slice(&personalized_hash(SECDISCARDABLE, &[secdiscardable]));
    out
}

/// Undo the software layer of the blob, which is what the user's own
/// secret opens. The layer the secure world holds has to come off first.
pub fn open_inner(secret: &[u8], blob: &[u8]) -> Result<Vec<u8>> {
    if blob.len() < NONCE_BYTES + 16 {
        bail!("the user's blob is {} bytes, too few to hold anything", blob.len());
    }
    let mut key = personalized_hash(PROTECTOR_SECRET, &[secret]);
    key.truncate(KEY_BYTES);
    let (nonce, body) = blob.split_at(NONCE_BYTES);
    Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key))
        .decrypt(Nonce::from_slice(nonce), body)
        .map_err(|_| anyhow::anyhow!("the user's blob did not come apart; the secret is not the one it was closed with"))
}

/// What the phone put in the blob, past the two bytes that say how it is
/// written.
pub fn content(blob: &[u8]) -> Result<&[u8]> {
    let [version, kind, rest @ ..] = blob else { bail!("the user's blob says nothing about itself") };
    if *version != LATEST {
        bail!("the user's blob is written in layout {version}, and Skyward reads layout {LATEST}");
    }
    if *kind != BY_CREDENTIAL {
        bail!("the user's blob is kept behind something other than what the user knows");
    }
    Ok(rest)
}

/// The secret a user's own files are wrapped with, from the synthetic
/// password the blob held.
pub fn file_secret(synthetic_password: &[u8]) -> Vec<u8> {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(synthetic_password).expect("HMAC takes a key of any length");
    mac.update(&1u32.to_be_bytes());
    mac.update(FILE_KEY);
    mac.update(&[0]);
    mac.update(CONTEXT);
    mac.update(&((CONTEXT.len() * 8) as u32).to_be_bytes());
    mac.update(&256u32.to_be_bytes());
    mac.finalize().into_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_user_who_set_no_credential_has_a_stand_in_for_one() {
        let stretched = stretched_without_credential();
        assert_eq!(stretched.len(), STRETCHED_BYTES);
        assert_eq!(&stretched[..16], NO_CREDENTIAL);
        assert_eq!(&stretched[16..], [0u8; 16], "it is padded out, not repeated");
    }

    /// Checked against SHA-512 taken outside this code.
    #[test]
    fn a_name_for_what_a_hash_is_for_is_padded_to_a_whole_block() {
        let out = personalized_hash(b"secdiscardable-transform", &[b"test"]);
        assert_eq!(
            out.iter().map(|b| format!("{b:02x}")).collect::<String>(),
            "b6ea00e9c56b167287e68821a4a302ee34b3d8eb43e67efd403b4ebbdfe34386\
             5c0bd2783da07e8566f8311820e57551318757cce3a3a8379ac0e1a6c72233b2"
        );
    }

    #[test]
    fn the_secret_is_the_stretched_credential_and_the_blobs_hash() {
        let secret = protector_secret(b"stretched", b"random bytes");
        assert_eq!(&secret[..9], b"stretched");
        assert_eq!(secret.len(), 9 + 64);
    }

    #[test]
    fn the_blob_says_how_it_is_written_before_anything_else() {
        assert_eq!(content(&[3, 0, 9, 9]).unwrap(), &[9, 9]);
        assert!(content(&[2, 0, 9]).is_err(), "an older layout");
        assert!(content(&[3, 1, 9]).is_err(), "kept behind something else");
        assert!(content(&[3]).is_err());
    }

    #[test]
    fn the_software_layer_opens_only_with_the_secret_it_was_closed_with() {
        let mut key = personalized_hash(PROTECTOR_SECRET, &[b"the secret"]);
        key.truncate(KEY_BYTES);
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
        let nonce = [4u8; NONCE_BYTES];
        let mut blob = nonce.to_vec();
        blob.extend_from_slice(&cipher.encrypt(Nonce::from_slice(&nonce), &b"the password"[..]).unwrap());
        assert_eq!(open_inner(b"the secret", &blob).unwrap(), b"the password");
        assert!(open_inner(b"another secret", &blob).is_err());
        assert!(open_inner(b"the secret", &blob[..20]).is_err());
    }

    #[test]
    fn the_key_the_secure_world_holds_is_named_after_the_protector() {
        let p = Protector { id: "08a692c0f39d97c6".into(), blob: Vec::new(), secdiscardable: Vec::new() };
        assert_eq!(p.key_alias(), "synthetic_password_8a692c0f39d97c6");
    }
}
