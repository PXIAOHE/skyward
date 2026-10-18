//! Opening the data partition. A phone encrypts the whole of it with a key
//! its storage hardware holds, and encrypts each user's files again with a
//! key only that user's credential leads to. This is the first of the two:
//! what it opens holds every file, with the names and contents of the ones
//! that are a user's own still encrypted.

use crate::gatekeeper::Gatekeeper;
use crate::keymint::KeyMint;
use crate::keystore::{StoredKey, Unwrap};
use crate::token::Token;
use anyhow::{Context, Result};
use skyward_binder::driver::Binder;
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

/// Which users the phone keeps keys for, in order.
pub fn users() -> Result<Vec<u32>> {
    let dir = Path::new(USER_KEYS).join("de");
    let mut users = Vec::new();
    for entry in std::fs::read_dir(&dir).with_context(|| format!("read {}", dir.display()))? {
        let path = entry.with_context(|| format!("read {}", dir.display()))?.path();
        let Some(user) = path.file_name().and_then(|n| n.to_str()).and_then(|n| n.parse::<u32>().ok()) else { continue };
        users.push(user);
    }
    users.sort_unstable();
    Ok(users)
}

/// Let in the keys for the files of each user that are readable before
/// that user signs in, and give the users they belong to.
pub fn open_users(service: &KeyMint) -> Result<Vec<u32>> {
    let mode = std::fs::read_to_string(FILE_MODE).with_context(|| format!("read {FILE_MODE}"))?;
    let wrapped = crate::fscrypt::hardware_wrapped(mode.trim());
    let users = users()?;
    for user in &users {
        let dir = Path::new(USER_KEYS).join("de").join(user.to_string());
        let raw = hardware_key(service, &dir, wrapped)?;
        crate::fscrypt::add_key(Path::new(MOUNT), &raw, wrapped)?;
    }
    Ok(users)
}

/// Whether any user has set a screen lock, which is what makes a
/// credential needed before their own files open.
pub fn screen_lock_set() -> Result<bool> {
    for user in users()? {
        if protector(user)?.lock.is_some() {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Where a phone keeps what it needs to recover a user's own secret.
const PROTECTORS: &str = "/data/system_de";

/// Read what a user's secret is kept behind. A user has one of these at a
/// time; a phone part way through changing a credential may have left
/// another, and which is current is its own business, so this refuses to
/// guess.
pub fn protector(user: u32) -> Result<crate::user::Protector> {
    let dir = blobs(user);
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
    // A user who has set a screen lock has what the phone recorded about
    // it beside the blob; a user who has set none has nothing there.
    let recorded = dir.join(format!("{id}.pwd"));
    let lock = match recorded.exists() {
        true => Some(crate::user::Lock::of(
            &std::fs::read(&recorded).with_context(|| format!("read {}", recorded.display()))?,
        )?),
        false => None,
    };
    Ok(crate::user::Protector {
        blob: std::fs::read(dir.join(format!("{id}.spblob"))).with_context(|| format!("read the blob in {}", dir.display()))?,
        secdiscardable: std::fs::read(dir.join(format!("{id}.secdis")))
            .with_context(|| format!("read what the blob in {} is bound to", dir.display()))?,
        id: id.clone(),
        lock,
    })
}

/// Which of the secure world's slots a user's enrolments sit in. What the
/// user types is enrolled a hundred thousand above their own number,
/// keeping it apart from the enrolments made before phones kept
/// credentials this way; the secret behind it is enrolled under the
/// number itself.
const TYPED_SLOT: u32 = 100_000;

/// What the phone calls the enrolment of a user's own secret, among the
/// blobs it keeps for that user.
const BLOBS: &str = "spblob";
const SECRET_HANDLE: &str = "0000000000000000.handle";

/// Where a user's blobs are.
fn blobs(user: u32) -> PathBuf {
    Path::new(PROTECTORS).join(user.to_string()).join(BLOBS)
}

/// Where a phone keeps the credential it made up itself for a user whose
/// screen lock is the owner's. The file holds a nonce and then the
/// credential, encrypted under a key the owner's own secret guards.
const MADE_UP: &str = "/data/system/users";
const MADE_UP_FILE: &str = "gatekeeper.profile.key";

/// The name the key service keeps that key under.
const MADE_UP_KEY: &str = "profile_key_name_decrypt_";

/// The credential a phone made up for `user`, where it made one up. A
/// phone does this for the users it keeps beside the owner's own, such as
/// the one that holds a second copy of an app: nobody ever types their
/// credential, so the phone writes one down for itself and keeps it behind
/// the owner's. `owner` is the note the owner's credential gave.
fn made_up_credential(service: &KeyMint, user: u32, owner: Option<&Token>) -> Result<Option<Vec<u8>>> {
    let file = Path::new(MADE_UP).join(user.to_string()).join(MADE_UP_FILE);
    if !file.is_file() {
        return Ok(None);
    }
    let stored = std::fs::read(&file).with_context(|| format!("read {}", file.display()))?;
    if stored.len() < NONCE_BYTES + TAG_BYTES {
        anyhow::bail!("{} is {} bytes, too few to hold a credential", file.display(), stored.len());
    }
    let (nonce, body) = stored.split_at(NONCE_BYTES);
    let alias = format!("{MADE_UP_KEY}{user}");
    let held = crate::keystore2::key_blob(Path::new(crate::keystore2::DATABASE), &alias)?;
    let credential = service.gcm_decrypt(&held, &[], nonce, body, owner).with_context(|| {
        format!("open the credential the phone made up for user {user}, which is kept behind the owner's own")
    })?;
    Ok(Some(credential))
}

/// How long the nonce and the check are on everything the key service
/// encrypts.
const NONCE_BYTES: usize = 12;
const TAG_BYTES: usize = 16;

/// Let in the key for a user's own files, and give the note the secure
/// world wrote for the secret behind their credential, which is what the
/// keys the phone keeps for them are guarded by. `credential` is what the
/// user would type at their screen lock; a user who has set none needs
/// none, and a user the phone made one up for needs `owner`, the note the
/// owner's own secret gave.
pub fn open_user_files(service: &KeyMint, user: u32, credential: Option<&[u8]>, owner: Option<&Token>) -> Result<Option<Token>> {
    let protector = protector(user)?;
    let (stretched, note) = match &protector.lock {
        Some(lock) => {
            let typed = match made_up_credential(service, user, owner)? {
                Some(made_up) => made_up,
                None => credential
                    .with_context(|| format!("user {user} has set a screen lock, and their files do not open without it"))?
                    .to_vec(),
            };
            let stretched = lock.stretched(&typed).context("stretch the credential")?;
            // The key holding this user's secret is guarded by the screen
            // lock, so the secure world has to be shown the credential
            // itself before it will use that key.
            let hardware = Binder::hardware()?;
            let note = Gatekeeper::open(&hardware)?.verify(
                TYPED_SLOT + user,
                &lock.enrolled,
                &crate::user::gatekeeper_password(&stretched),
            )?;
            (stretched, Some(note))
        }
        None => (crate::user::stretched_without_credential(), None),
    };
    let stretched = &stretched[..];
    let held = crate::keystore2::key_blob(Path::new(crate::keystore2::DATABASE), &protector.key_alias())?;
    let content = crate::user::content(&protector.blob)?;
    if content.len() < NONCE_BYTES + TAG_BYTES {
        anyhow::bail!("the user's blob is too short to hold anything");
    }
    let (nonce, body) = content.split_at(NONCE_BYTES);
    let inner = service
        .gcm_decrypt(&held, &[], nonce, body, note.as_ref())
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
    secret_note(user, &synthetic_password)
}

/// Have the secure world check the user's own secret, so the keys the
/// phone keeps for that user can be used. A user whose secret the secure
/// world knows nothing about has no note to give.
fn secret_note(user: u32, synthetic_password: &[u8]) -> Result<Option<Token>> {
    let handle = blobs(user).join(SECRET_HANDLE);
    if !handle.is_file() {
        return Ok(None);
    }
    let enrolled = std::fs::read(&handle).with_context(|| format!("read {}", handle.display()))?;
    let hardware = Binder::hardware()?;
    let password = crate::user::secret_gatekeeper_password(synthetic_password);
    let note = Gatekeeper::open(&hardware)?
        .verify(user, &enrolled, &password)
        .with_context(|| format!("have the secure world check the secret behind user {user}'s credential"))?;
    Ok(Some(note))
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
