use crate::hal::{FscryptSink, Gatekeeper, HalResult, Keymint, Weaver};
use crate::keystorage::{self, StoredKey};
use crate::password::{derive_fbe_secret, weaver_key};

/// The CE key material for one Android user.
pub struct UserKey<'a> {
    pub user: u32,
    pub weaver_slot: u32,
    pub weaver_salt: &'a [u8],
    pub ce: &'a StoredKey,
}

/// Drives the file-based-encryption unlock sequence across the four secure-world
/// interfaces. The ordering matches the platform: verify the credential
/// (throttled), release the synthetic password from Weaver, derive the disk
/// secret, unwrap the CE key via KeyMint, then install it into the keyring.
pub struct Unlocker<'a> {
    pub gatekeeper: &'a mut dyn Gatekeeper,
    pub weaver: &'a mut dyn Weaver,
    pub keymint: &'a dyn Keymint,
    pub fscrypt: &'a mut dyn FscryptSink,
}

impl Unlocker<'_> {
    pub fn unlock_user(&mut self, uk: &UserKey, credential: &[u8]) -> HalResult<String> {
        let _hat = self.gatekeeper.verify(uk.user, credential)?;
        let wkey = weaver_key(credential, uk.weaver_salt);
        let synthetic_password = self.weaver.read(uk.weaver_slot, &wkey)?;
        let secret = derive_fbe_secret(&synthetic_password);
        let raw = keystorage::decrypt(self.keymint, uk.ce, &secret)?;
        self.fscrypt.install_key(&raw)
    }
}
