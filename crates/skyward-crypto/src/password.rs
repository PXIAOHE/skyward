use hkdf::Hkdf;
use sha2::{Digest, Sha256, Sha512};

/// Encode a lock pattern (dot indices 0..9) into credential bytes the same way
/// the platform treats the gesture before hashing.
pub fn pattern_credential(dots: &[u8]) -> Vec<u8> {
    dots.to_vec()
}

/// SHA-512 of the secdiscardable blob. Wiping the blob makes the key
/// unrecoverable; its hash is mixed into the KeyMint application id.
pub fn secdiscardable_hash(blob: &[u8]) -> Vec<u8> {
    Sha512::digest(blob).to_vec()
}

/// Per-slot Weaver key derived from the credential and a slot salt.
pub fn weaver_key(credential: &[u8], salt: &[u8]) -> Vec<u8> {
    hkdf32(credential, salt, b"skyward-weaver-key").to_vec()
}

/// Disk-encryption auth secret derived from the released synthetic password.
pub fn derive_fbe_secret(synthetic_password: &[u8]) -> Vec<u8> {
    hkdf32(synthetic_password, &[], b"skyward-fbe-secret").to_vec()
}

/// KeyMint application id = stretched-secret || secdiscardable-hash.
pub fn app_id(stretched_secret: &[u8], secdiscardable: &[u8]) -> Vec<u8> {
    let mut id = Vec::with_capacity(stretched_secret.len() + 64);
    id.extend_from_slice(stretched_secret);
    id.extend_from_slice(&secdiscardable_hash(secdiscardable));
    id
}

fn hkdf32(ikm: &[u8], salt: &[u8], info: &[u8]) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(if salt.is_empty() { None } else { Some(salt) }, ikm);
    let mut okm = [0u8; 32];
    hk.expand(info, &mut okm).expect("32 is a valid HKDF length");
    okm
}

#[allow(dead_code)]
fn sha256(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}
