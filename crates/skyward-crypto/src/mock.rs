//! Host implementations of the secure-world interfaces plus an `enroll` helper
//! that provisions matching fixtures. KeyMint's application-id binding is
//! modelled with AES-256-GCM: the application id is the AEAD associated data,
//! so a wrong credential produces a wrong id and the tag check fails — exactly
//! how the real TEE refuses to release the key.

use crate::hal::{AuthToken, FscryptSink, Gatekeeper, HalError, HalResult, Keymint, Weaver};
use crate::keystorage::{StoredKey, Stretching};
use crate::password::{app_id, derive_fbe_secret, weaver_key};
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use std::collections::HashMap;

#[derive(Default)]
pub struct MockGatekeeper {
    creds: HashMap<u32, Vec<u8>>,
    fails: HashMap<u32, u32>,
}

impl Gatekeeper for MockGatekeeper {
    fn verify(&mut self, user: u32, credential: &[u8]) -> HalResult<AuthToken> {
        match self.creds.get(&user) {
            None => Err(HalError::NotEnrolled),
            Some(c) if c == credential => {
                self.fails.remove(&user);
                Ok(AuthToken(b"skyward-hat".to_vec()))
            }
            Some(_) => {
                let n = self.fails.entry(user).or_insert(0);
                *n += 1;
                if *n >= 5 {
                    Err(HalError::Throttled(30_000))
                } else {
                    Err(HalError::WrongCredential)
                }
            }
        }
    }
}

#[derive(Default)]
pub struct MockWeaver {
    slots: HashMap<u32, (Vec<u8>, Vec<u8>)>,
}

impl Weaver for MockWeaver {
    fn read(&mut self, slot: u32, key: &[u8]) -> HalResult<Vec<u8>> {
        match self.slots.get(&slot) {
            None => Err(HalError::NotEnrolled),
            Some((k, v)) if k == key => Ok(v.clone()),
            Some(_) => Err(HalError::WrongCredential),
        }
    }
}

pub struct MockKeymint;

impl Keymint for MockKeymint {
    fn gcm_decrypt(&self, key_blob: &[u8], app_id: &[u8], nonce: &[u8], ct_and_tag: &[u8]) -> HalResult<Vec<u8>> {
        if key_blob.len() != 32 || nonce.len() != 12 {
            return Err(HalError::KeyMintFailed);
        }
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key_blob));
        cipher
            .decrypt(Nonce::from_slice(nonce), Payload { msg: ct_and_tag, aad: app_id })
            .map_err(|_| HalError::KeyMintFailed)
    }
}

#[derive(Default)]
pub struct MockFscrypt {
    pub installed: Vec<Vec<u8>>,
}

impl FscryptSink for MockFscrypt {
    fn install_key(&mut self, raw: &[u8]) -> HalResult<String> {
        self.installed.push(raw.to_vec());
        Ok(format!("key:{:02x}{:02x}..", raw.first().copied().unwrap_or(0), raw.get(1).copied().unwrap_or(0)))
    }
}

pub struct Mocks {
    pub gatekeeper: MockGatekeeper,
    pub weaver: MockWeaver,
    pub keymint: MockKeymint,
    pub fscrypt: MockFscrypt,
}

/// Provision all four mocks and build the CE [`StoredKey`] that wraps `fbe_key`
/// for `credential`. Deterministic inputs keep tests reproducible without an
/// RNG dependency.
#[allow(clippy::too_many_arguments)]
pub fn enroll(
    user: u32,
    slot: u32,
    credential: &[u8],
    weaver_salt: &[u8],
    secdiscardable: Vec<u8>,
    sp_secret: &[u8],
    key_blob: [u8; 32],
    nonce: [u8; 12],
    fbe_key: &[u8],
    stretching: Stretching,
) -> (Mocks, StoredKey) {
    let mut gatekeeper = MockGatekeeper::default();
    gatekeeper.creds.insert(user, credential.to_vec());

    let mut weaver = MockWeaver::default();
    weaver.slots.insert(slot, (weaver_key(credential, weaver_salt), sp_secret.to_vec()));

    let secret = derive_fbe_secret(sp_secret);
    let mut stored = StoredKey {
        encrypted_key: Vec::new(),
        key_blob: key_blob.to_vec(),
        secdiscardable,
        stretching,
    };
    let stretched = stored.stretch(&secret).expect("stretch");
    let app = app_id(&stretched, &stored.secdiscardable);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key_blob));
    let ct = cipher
        .encrypt(Nonce::from_slice(&nonce), Payload { msg: fbe_key, aad: &app })
        .expect("seal");
    let mut ek = nonce.to_vec();
    ek.extend_from_slice(&ct);
    stored.encrypted_key = ek;

    (Mocks { gatekeeper, weaver, keymint: MockKeymint, fscrypt: MockFscrypt::default() }, stored)
}
