//! What a partition holds now: the filesystem on it, which need not be the
//! one the phone's own table names for it. blkid names the filesystems it
//! knows; erofs, which the ramdisk's blkid does not know, is read by the
//! erofs tools the ramdisk carries.

use crate::tools;
use anyhow::Result;
use std::path::Path;

/// blkid is one of the commands in the ramdisk's box of them, which takes
/// the name of the one to run.
const BOX: &str = "toybox";
const BLKID: &str = "blkid";

/// What reads an erofs superblock, ending well only on one.
const DUMP_EROFS: &str = "dump.erofs";
const EROFS: &str = "erofs";

/// The filesystem on `device`; none where it holds nothing known, which is
/// how a partition reads after it has been discarded.
pub fn filesystem(device: &Path) -> Result<Option<String>> {
    let path = tools::path(device)?;
    let ran = tools::find(BOX)?.run(&[BLKID.to_string(), path.clone()])?;
    if let Some(named) = named(&ran.said) {
        return Ok(Some(named));
    }
    let erofs = tools::find(DUMP_EROFS)?.run(&[path])?;
    Ok((erofs.status == 0).then(|| EROFS.to_string()))
}

/// The filesystem blkid names in a line such as
/// `/dev/block/dm-8: LABEL="data" UUID="…" TYPE="f2fs"`.
fn named(said: &str) -> Option<String> {
    said.split_whitespace().find_map(|field| {
        let value = field.strip_prefix("TYPE=")?.trim_matches('"');
        (!value.is_empty()).then(|| value.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_filesystem_is_read_off_the_line_blkid_prints() {
        assert_eq!(named("/dev/block/dm-8: TYPE=\"f2fs\"\n").as_deref(), Some("f2fs"));
        assert_eq!(named("/dev/block/dm-9: LABEL=\"metadata\" UUID=\"2d0f\" TYPE=\"ext4\"").as_deref(), Some("ext4"));
    }

    #[test]
    fn a_partition_blkid_makes_nothing_of_holds_no_filesystem_it_knows() {
        assert_eq!(named(""), None, "a partition it says nothing about");
        assert_eq!(named("/dev/block/dm-8: PTTYPE=\"dos\""), None, "a partition table is not a filesystem");
    }
}
