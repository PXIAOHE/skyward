pub mod keystore;

#[cfg(target_os = "android")]
pub mod keymint;

#[cfg(target_os = "android")]
impl keystore::Decryptor for keymint::KeyMint<'_> {
    fn gcm_decrypt(&self, key_blob: &[u8], app_id: &[u8], nonce: &[u8], body: &[u8]) -> anyhow::Result<Vec<u8>> {
        keymint::KeyMint::gcm_decrypt(self, key_blob, app_id, nonce, body)
    }
}
