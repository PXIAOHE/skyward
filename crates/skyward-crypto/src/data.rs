//! Opening the data partition. A phone encrypts the whole of it with a key
//! its storage hardware holds, and encrypts each user's files again with a
//! key only that user's credential leads to. This is the first of the two:
//! what it opens holds every file, with the names and contents of the ones
//! that are a user's own still encrypted.

use crate::keymint::KeyMint;
use crate::keystore::{StoredKey, Unwrap};
use anyhow::{Context, Result};
use skyward_block::dm::Target;
use skyward_block::fstab::Entry;
use std::path::{Path, PathBuf};

/// Where the data partition is mounted, and the name its decrypted form is
/// given among the device-mapper's devices.
pub const MOUNT: &str = "/data";
pub const MAPPED: &str = "userdata";

/// The key is in a directory of this name under the one the phone names.
const KEY_DIR: &str = "key";

/// How the kernel knows the cipher a phone names.
const CIPHERS: [(&str, &str); 2] = [("aes-256-xts", "aes-xts-plain64"), ("adiantum", "xchacha12,aes-adiantum-plain64")];

/// The hardware reads and writes in 4096-byte units, so the partition is
/// mapped in whole multiples of eight 512-byte sectors.
const SECTORS_PER_UNIT: u64 = 8;

/// How a phone says its data partition is encrypted: the cipher, and
/// whether the key is one only the storage hardware can unwrap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Encryption {
    pub cipher: String,
    pub wrapped: bool,
}

impl Encryption {
    /// Read what an entry of the phone's own table says.
    pub fn of(entry: &Entry) -> Result<Encryption> {
        let said = entry.extra("metadata_encryption").context("the phone says its data partition is not encrypted whole")?;
        let (name, rest) = said.split_once(':').unwrap_or((said, ""));
        let cipher = CIPHERS
            .iter()
            .find_map(|(theirs, kernels)| (*theirs == name).then_some(*kernels))
            .with_context(|| format!("no cipher here is the {name} the phone names"))?;
        Ok(Encryption { cipher: cipher.to_string(), wrapped: rest == "wrappedkey_v0" })
    }
}

/// Where the phone keeps the key for the filesystem an entry describes.
pub fn key_dir(entry: &Entry) -> Result<PathBuf> {
    let dir = entry.extra("keydirectory").context("the phone says where the key for its data partition is not")?;
    Ok(Path::new(dir).join(KEY_DIR))
}

/// Unwrap the key the phone keeps for its data partition and give the
/// storage hardware the form of it that lasts until the phone is started
/// again.
pub fn storage_key(service: &KeyMint, entry: &Entry, encryption: &Encryption) -> Result<Vec<u8>> {
    hardware_key(service, &key_dir(entry)?, encryption.wrapped)
}

/// The key a directory holds, in the form the storage hardware takes for
/// this start of the phone. A key the hardware holds never appears as
/// itself: what comes back is wrapped again, for this start only.
fn hardware_key(service: &KeyMint, dir: &Path, wrapped: bool) -> Result<Vec<u8>> {
    let stored = StoredKey::load(dir).with_context(|| format!("read the key in {}", dir.display()))?;
    let key = stored.unwrap(Unwrap::Hardware(service)).with_context(|| format!("unwrap the key in {}", dir.display()))?;
    match wrapped {
        true => service.ephemeral_storage_key(&key).with_context(|| format!("make the key in {} usable now", dir.display())),
        false => Ok(key),
    }
}

/// Open the phone's data partition: unwrap the key it is encrypted with,
/// give that to the storage hardware, and mount what comes out read-only.
/// The names and contents of each user's own files stay encrypted until
/// that user's key is let in as well.
pub fn open(service: &KeyMint) -> Result<(PathBuf, &'static str)> {
    let known = skyward_block::fstab::read(MOUNT)?;
    let entry = known.first().with_context(|| format!("the phone's own table says nothing about {MOUNT}"))?;
    let encryption = Encryption::of(entry)?;
    let device = skyward_block::fstab::device(entry)?;
    let key = storage_key(service, entry, &encryption)?;
    let sectors = skyward_block::size_bytes(&device)? / 512;
    let mapped = skyward_block::dm::create(MAPPED, &[target(&device, sectors, &key, &encryption)], true)?;
    std::fs::create_dir_all(MOUNT).with_context(|| format!("make {MOUNT}"))?;
    // Files whose keys the storage hardware holds are decrypted as they
    // are read, by the hardware; a filesystem not told to let it do that
    // tries to decrypt them itself and cannot.
    let options = if encryption.wrapped { "inlinecrypt" } else { "" };
    let filesystem = skyward_block::mount::mount_ro_with(&mapped, Path::new(MOUNT), options)?;
    Ok((mapped, filesystem))
}

/// Where the phone keeps the key for the files that are readable before
/// anyone signs in, and what it says about how those files are encrypted.
const SHARED_KEY: &str = "/data/unencrypted/key";
const SHARED_NAME: &str = "/data/unencrypted/ref";
const FILE_MODE: &str = "/data/unencrypted/mode";

/// Where the phone keeps each user's own keys.
const USER_KEYS: &str = "/data/misc/vold/user_keys";

/// Let in the key for the files that belong to no one user. Until it is
/// in, most of what the data partition holds reads as scrambled names.
pub fn open_shared(service: &KeyMint) -> Result<[u8; crate::fscrypt::NAME_BYTES]> {
    let mode = std::fs::read_to_string(FILE_MODE).with_context(|| format!("read {FILE_MODE}"))?;
    let wrapped = crate::fscrypt::hardware_wrapped(mode.trim());
    let raw = hardware_key(service, Path::new(SHARED_KEY), wrapped)?;
    let name = crate::fscrypt::add_key(Path::new(MOUNT), &raw, wrapped)?;
    let recorded = crate::fscrypt::recorded_name(Path::new(SHARED_NAME))?;
    if name != recorded {
        anyhow::bail!("the key in {SHARED_KEY} is not the one {SHARED_NAME} names");
    }
    Ok(name)
}

/// Let in the keys for the files of each user that are readable before
/// that user signs in, and give the users they belong to.
pub fn open_users(service: &KeyMint) -> Result<Vec<u32>> {
    let mode = std::fs::read_to_string(FILE_MODE).with_context(|| format!("read {FILE_MODE}"))?;
    let wrapped = crate::fscrypt::hardware_wrapped(mode.trim());
    let dir = Path::new(USER_KEYS).join("de");
    let mut users = Vec::new();
    for entry in std::fs::read_dir(&dir).with_context(|| format!("read {}", dir.display()))? {
        let path = entry.with_context(|| format!("read {}", dir.display()))?.path();
        let Some(user) = path.file_name().and_then(|n| n.to_str()).and_then(|n| n.parse::<u32>().ok()) else { continue };
        let raw = hardware_key(service, &path, wrapped)?;
        crate::fscrypt::add_key(Path::new(MOUNT), &raw, wrapped)?;
        users.push(user);
    }
    users.sort_unstable();
    Ok(users)
}

/// Where a phone keeps what it needs to recover a user's own secret.
const PROTECTORS: &str = "/data/system_de";

/// Read what a user's secret is kept behind. A user has one of these at a
/// time; a phone part way through changing a credential may have left
/// another, and which is current is its own business, so this refuses to
/// guess.
pub fn protector(user: u32) -> Result<crate::user::Protector> {
    let dir = Path::new(PROTECTORS).join(user.to_string()).join("spblob");
    let mut ids = Vec::new();
    for entry in std::fs::read_dir(&dir).with_context(|| format!("read {}", dir.display()))? {
        let path = entry.with_context(|| format!("read {}", dir.display()))?.path();
        if path.extension().is_some_and(|e| e == "spblob") {
            if let Some(id) = path.file_stem().and_then(|s| s.to_str()) {
                ids.push(id.to_string());
            }
        }
    }
    let [id] = &ids[..] else {
        anyhow::bail!("{} holds {} of the user's blobs, and one is expected", dir.display(), ids.len());
    };
    Ok(crate::user::Protector {
        blob: std::fs::read(dir.join(format!("{id}.spblob"))).with_context(|| format!("read the blob in {}", dir.display()))?,
        secdiscardable: std::fs::read(dir.join(format!("{id}.secdis")))
            .with_context(|| format!("read what the blob in {} is bound to", dir.display()))?,
        id: id.clone(),
    })
}

/// Let in the key for a user's own files. `credential` is what the user
/// knows, already stretched; a user who has set none has a fixed stand-in
/// for it.
pub fn open_user_files(service: &KeyMint, user: u32, stretched: &[u8]) -> Result<()> {
    let protector = protector(user)?;
    let held = crate::keystore2::key_blob(Path::new(crate::keystore2::DATABASE), &protector.key_alias())?;
    let content = crate::user::content(&protector.blob)?;
    if content.len() < 12 {
        anyhow::bail!("the user's blob is too short to hold anything");
    }
    let (nonce, body) = content.split_at(12);
    let inner = service
        .gcm_decrypt(&held, &[], nonce, body)
        .context("open what the secure world holds the user's blob closed with")?;
    let secret = crate::user::protector_secret(stretched, &protector.secdiscardable);
    let synthetic_password = crate::user::open_inner(&secret, &inner)?;
    let file_secret = crate::user::file_secret(&synthetic_password);

    let mode = std::fs::read_to_string(FILE_MODE).with_context(|| format!("read {FILE_MODE}"))?;
    let wrapped = crate::fscrypt::hardware_wrapped(mode.trim());
    let dir = Path::new(USER_KEYS).join("ce").join(user.to_string()).join("current");
    let stored = StoredKey::load(&dir).with_context(|| format!("read the key in {}", dir.display()))?;
    let raw = stored.unwrap(Unwrap::<KeyMint>::Secret(&file_secret)).with_context(|| format!("unwrap the key in {}", dir.display()))?;
    let raw = match wrapped {
        true => service.ephemeral_storage_key(&raw).with_context(|| format!("make the key in {} usable now", dir.display()))?,
        false => raw,
    };
    crate::fscrypt::add_key(Path::new(MOUNT), &raw, wrapped)?;
    Ok(())
}

/// Build the device-mapper target that reads the partition through the
/// storage hardware's decryption.
pub fn target(device: &str, sectors: u64, key: &[u8], encryption: &Encryption) -> Target {
    Target::DefaultKey {
        sectors: sectors - sectors % SECTORS_PER_UNIT,
        device: device.to_string(),
        cipher: encryption.cipher.clone(),
        key: key.iter().map(|b| format!("{b:02x}")).collect(),
        wrapped: encryption.wrapped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(extras: &str) -> Entry {
        Entry {
            device: "/dev/block/bootdevice/by-name/userdata".into(),
            mount_point: MOUNT.into(),
            filesystem: "f2fs".into(),
            extras: extras.split(',').map(str::to_string).collect(),
        }
    }

    #[test]
    fn the_phones_own_words_for_the_cipher_become_the_kernels() {
        let e = entry("metadata_encryption=aes-256-xts:wrappedkey_v0,keydirectory=/metadata/vold/metadata_encryption");
        assert_eq!(Encryption::of(&e).unwrap(), Encryption { cipher: "aes-xts-plain64".into(), wrapped: true });
        let plain = entry("metadata_encryption=adiantum");
        assert_eq!(Encryption::of(&plain).unwrap(), Encryption { cipher: "xchacha12,aes-adiantum-plain64".into(), wrapped: false });
    }

    #[test]
    fn a_cipher_or_a_key_the_phone_does_not_name_is_an_error() {
        assert!(Encryption::of(&entry("quota")).is_err());
        assert!(Encryption::of(&entry("metadata_encryption=rot13")).is_err());
        assert!(key_dir(&entry("quota")).is_err());
    }

    #[test]
    fn the_key_is_in_a_directory_under_the_one_the_phone_names() {
        let e = entry("keydirectory=/metadata/vold/metadata_encryption");
        assert_eq!(key_dir(&e).unwrap(), Path::new("/metadata/vold/metadata_encryption/key"));
    }

    #[test]
    fn the_partition_is_mapped_in_whole_units_of_the_hardwares_reads() {
        let e = Encryption { cipher: "aes-xts-plain64".into(), wrapped: true };
        let t = target("/dev/block/by-name/userdata", 969_825_751, &[0xAB, 0x01], &e);
        let Target::DefaultKey { sectors, key, cipher, device, wrapped } = t else { panic!("a decrypting target") };
        assert_eq!(sectors, 969_825_744, "a whole number of 4096-byte units");
        assert_eq!(key, "ab01");
        assert_eq!(cipher, "aes-xts-plain64");
        assert_eq!(device, "/dev/block/by-name/userdata");
        assert!(wrapped);
    }
}
