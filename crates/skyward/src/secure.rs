//! What the phone's own secure services need before they will run inside a
//! recovery: the libraries their partitions were built against, and a
//! service directory that knows the phone describes them.

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Where a phone keeps the bundles of shared libraries its own partitions
/// were built against.
const BUNDLE_DIRS: [&str; 4] = ["/mnt/system_ext/apex", "/mnt/system/system/apex", "/mnt/product/apex", "/vendor/apex"];

/// The file inside a bundle that holds its filesystem. It is stored whole,
/// so the bundle can be opened where it lies.
const BUNDLE_IMAGE: &str = "apex_payload.img";

/// Where Android opens its bundles, and the directory of 64-bit libraries
/// inside one.
const BUNDLE_MOUNT: &str = "/apex";
const BUNDLE_LIBS: &str = "lib64";

/// Which build of the vendor interface the phone's own partitions were
/// made for.
const VENDOR_INTERFACE: &str = "ro.vndk.version";

/// Where the phone describes the services its own partitions carry, and
/// where a recovery looks for such descriptions.
const DESCRIPTIONS: [&str; 2] = ["/vendor/etc/vintf/manifest", "/odm/etc/vintf/manifest"];
const RECOVERY_DESCRIPTIONS: &str = "/system/etc/vintf/manifest";

/// The name init knows the service directory by, and the program it runs.
const DIRECTORY_SERVICE: &str = "servicemanager";
const DIRECTORY_PROGRAM: &str = "/system/bin/servicemanager";

/// Open the bundle of libraries the phone's own partitions were built
/// against and give the directory they are in. A phone whose partitions
/// need no bundle has none to open.
pub fn libraries() -> Result<Option<PathBuf>> {
    let Some(version) = skyward_sys::prop(VENDOR_INTERFACE) else { return Ok(None) };
    let name = format!("com.android.vndk.v{version}");
    let at = Path::new(BUNDLE_MOUNT).join(&name);
    let libs = at.join(BUNDLE_LIBS);
    if libs.is_dir() {
        return Ok(Some(libs));
    }
    let Some(file) = BUNDLE_DIRS.iter().map(|d| Path::new(d).join(format!("{name}.apex"))).find(|p| p.is_file()) else {
        return Ok(None);
    };
    let (offset, length) = image(&file)?;
    let device = skyward_block::loopdev::attach(&file, offset, length)?;
    fs::create_dir_all(&at).with_context(|| format!("make {}", at.display()))?;
    skyward_block::mount::mount_ro(&device, &at)?;
    Ok(Some(libs))
}

/// Where the filesystem lies inside a bundle.
fn image(file: &Path) -> Result<(u64, u64)> {
    let f = fs::File::open(file).with_context(|| format!("open {}", file.display()))?;
    let mut bundle = zip::ZipArchive::new(f).with_context(|| format!("read what is in {}", file.display()))?;
    let entry = bundle.by_name(BUNDLE_IMAGE).with_context(|| format!("{} holds no {BUNDLE_IMAGE}", file.display()))?;
    if entry.compression() != zip::CompressionMethod::Stored {
        anyhow::bail!("{} keeps its {BUNDLE_IMAGE} packed, which cannot be opened where it lies", file.display());
    }
    let start = entry.data_start().with_context(|| format!("{BUNDLE_IMAGE} in {} says where it is not", file.display()))?;
    Ok((start, entry.size()))
}

/// Give the phone's service directory the descriptions of the services in
/// `wanted`. The directory turns away a hardware service the running system
/// does not describe, and a recovery starts up describing only the few
/// services of its own. Only the descriptions that are needed are taken:
/// the directory reads the whole of its description directory at once, and
/// one file it cannot make sense of leaves it describing nothing.
pub fn publish_descriptions(wanted: &[String]) -> Result<usize> {
    let into = Path::new(RECOVERY_DESCRIPTIONS);
    fs::create_dir_all(into).with_context(|| format!("make {}", into.display()))?;
    let mut copied = 0;
    for dir in DESCRIPTIONS {
        let listing = match fs::read_dir(dir) {
            Ok(listing) => listing,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e).with_context(|| format!("read {dir}")),
        };
        for entry in listing {
            let from = entry.with_context(|| format!("read {dir}"))?.path();
            if from.extension().is_none_or(|e| e != "xml") {
                continue;
            }
            let text = fs::read_to_string(&from).with_context(|| format!("read {}", from.display()))?;
            if !wanted.iter().any(|w| describes(&text, w)) {
                continue;
            }
            let name = from.file_name().expect("a listed file has a name");
            fs::write(into.join(name), &text).with_context(|| format!("copy {}", from.display()))?;
            copied += 1;
        }
    }
    if copied > 0 {
        // The directory reads the descriptions once and keeps to them, so
        // it has to start again to see these.
        let was = crate::services::running(DIRECTORY_PROGRAM);
        skyward_sys::set_prop("ctl.restart", DIRECTORY_SERVICE).context("have the service directory start again")?;
        answering_again(was)?;
    }
    Ok(copied)
}

/// Whether a description is of the service registered under `name`, which
/// reads as the interface and then the instance: the description names the
/// two separately.
fn describes(text: &str, name: &str) -> bool {
    let Some((interface, instance)) = name.rsplit_once('/') else { return false };
    let Some((package, interface)) = interface.rsplit_once('.') else { return false };
    text.contains(package) && text.contains(&format!("{interface}/{instance}"))
}

/// Wait until the service directory is a new process and takes calls
/// again. init is only asked to restart it, so for a moment the one that
/// has not read the new descriptions is still the one answering.
fn answering_again(was: Option<u32>) -> Result<()> {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let now = crate::services::running(DIRECTORY_PROGRAM);
        // Looking the directory up in itself answers nothing, and that it
        // answers at all is the whole question.
        if now.is_some() && now != was && crate::services::reachable() {
            return Ok(());
        }
        if std::time::Instant::now() >= until {
            anyhow::bail!("the service directory did not come back");
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
