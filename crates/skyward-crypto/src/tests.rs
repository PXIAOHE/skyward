use crate::flow::{Unlocker, UserKey};
use crate::hal::HalError;
use crate::keystorage::{self, Stretching};
use crate::mock::enroll;
use crate::password::derive_fbe_secret;

const CRED: &[u8] = &[0, 1, 2, 4, 8];
const SALT: &[u8] = &[2u8; 16];
const SP: &[u8] = &[9u8; 32];
const FBE: &[u8] = &[0xABu8; 64];

fn provision(stretching: Stretching) -> (crate::mock::Mocks, keystorage::StoredKey) {
    enroll(0, 0, CRED, SALT, vec![7u8; 256], SP, [3u8; 32], [1u8; 12], FBE, stretching)
}

#[test]
fn unlock_with_correct_pattern_installs_key() {
    let (mut m, ce) = provision(Stretching::None);
    let uk = UserKey { user: 0, weaver_slot: 0, weaver_salt: SALT, ce: &ce };
    let id = {
        let mut u = Unlocker {
            gatekeeper: &mut m.gatekeeper,
            weaver: &mut m.weaver,
            keymint: &m.keymint,
            fscrypt: &mut m.fscrypt,
        };
        u.unlock_user(&uk, CRED).unwrap()
    };
    assert!(id.starts_with("key:"));
    assert_eq!(m.fscrypt.installed, vec![FBE.to_vec()]);
}

#[test]
fn unlock_with_scrypt_stretching_round_trips() {
    let (mut m, ce) = provision(Stretching::Scrypt { log_n: 10, r: 8, p: 1, salt: vec![5u8; 16] });
    let uk = UserKey { user: 0, weaver_slot: 0, weaver_salt: SALT, ce: &ce };
    {
        let mut u = Unlocker {
            gatekeeper: &mut m.gatekeeper,
            weaver: &mut m.weaver,
            keymint: &m.keymint,
            fscrypt: &mut m.fscrypt,
        };
        u.unlock_user(&uk, CRED).unwrap();
    }
    assert_eq!(m.fscrypt.installed, vec![FBE.to_vec()]);
}

#[test]
fn wrong_pattern_is_rejected_before_any_key_is_installed() {
    let (mut m, ce) = provision(Stretching::None);
    let uk = UserKey { user: 0, weaver_slot: 0, weaver_salt: SALT, ce: &ce };
    let err = {
        let mut u = Unlocker {
            gatekeeper: &mut m.gatekeeper,
            weaver: &mut m.weaver,
            keymint: &m.keymint,
            fscrypt: &mut m.fscrypt,
        };
        u.unlock_user(&uk, &[9, 9, 9]).unwrap_err()
    };
    assert_eq!(err, HalError::WrongCredential);
    assert!(m.fscrypt.installed.is_empty());
}

#[test]
fn keymint_app_id_binding_rejects_a_wrong_secret() {
    let (m, ce) = provision(Stretching::None);
    // A wrong secret yields a wrong application id, so KeyMint refuses.
    let err = keystorage::decrypt(&m.keymint, &ce, b"not-the-secret").unwrap_err();
    assert_eq!(err, HalError::KeyMintFailed);
    // The real derived secret unwraps the key.
    let ok = keystorage::decrypt(&m.keymint, &ce, &derive_fbe_secret(SP)).unwrap();
    assert_eq!(ok, FBE);
}

#[test]
fn repeated_wrong_pattern_throttles() {
    let (mut m, _ce) = provision(Stretching::None);
    use crate::hal::Gatekeeper;
    let mut last = HalError::WrongCredential;
    for _ in 0..5 {
        last = m.gatekeeper.verify(0, &[7, 7, 7]).unwrap_err();
    }
    assert!(matches!(last, HalError::Throttled(_)));
}
