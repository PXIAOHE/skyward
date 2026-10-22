//! The phone's boot control: which slot the bootloader starts next, and
//! what it holds of each slot. The phone's own service does the work, as
//! the bootloader reads it from where only the phone's own code knows to
//! write it.

use crate::{BootSlots, Slot};
use anyhow::{bail, Context, Result};
use skyward_binder::driver::{Binder, Handle};
use skyward_binder::hidl::{self, Answer, Message};

/// The service, a hardware service of the older kind, by the version that
/// also tells which slot starts next. Each method is called by the name of
/// the version it came with, which the service checks.
pub const SERVICE: &str = "android.hardware.boot@1.2::IBootControl";
const FIRST: &str = "android.hardware.boot@1.0::IBootControl";
/// The version that tells the bootloader where an update stands.
const MERGE: &str = "android.hardware.boot@1.1::IBootControl";
const INSTANCE: &str = "default";

/// The methods, numbered as the service numbers them: the first version's
/// in their order, then those each later version added.
const SET_ACTIVE: u32 = 4;
const BOOTABLE: u32 = 6;
const SUCCESSFUL: u32 = 7;
const SET_MERGE_STATUS: u32 = 9;
const MERGE_STATUS: u32 = 10;
const ACTIVE: u32 = 11;

/// Where an update stands, as the boot control keeps it for the bootloader,
/// which will not erase or write the data partition, /metadata or super
/// while an update's snapshots wait to be merged or are being merged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeStatus {
    None = 0,
    Unknown = 1,
    /// The new slot's partitions are snapshots, waiting to be merged.
    Snapshotted = 2,
    Merging = 3,
    Cancelled = 4,
}

/// How the service answers whether something holds of a slot.
const NO: i32 = 0;
const YES: i32 = 1;

/// The phone's boot control, as this program talks to it.
pub struct BootControl<'a> {
    service: Handle<'a>,
}

/// The slot the bootloader starts next, as the running boot control tells
/// it.
pub fn next_slot() -> Result<Slot> {
    let binder = Binder::hardware()?;
    let control = BootControl::open(&binder)?;
    control.active()
}

impl<'a> BootControl<'a> {
    /// Reach the running boot control, or say that none is.
    pub fn open(binder: &'a Binder) -> Result<BootControl<'a>> {
        let service = hidl::find(binder, SERVICE, INSTANCE)?
            .with_context(|| format!("{SERVICE} is not running; the phone's boot control has to be started first"))?;
        Ok(BootControl { service })
    }

    /// The slot the bootloader starts next.
    pub fn active(&self) -> Result<Slot> {
        let call = Message::call(SERVICE);
        let reply = self.service.transact(ACTIVE, &call).context("ask which slot starts next")?;
        let mut read = Answer::new(reply.bytes());
        read.status().context("ask which slot starts next")?;
        match read.i32().context("read which slot starts next")? {
            0 => Ok(Slot::A),
            1 => Ok(Slot::B),
            other => bail!("the boot control says slot {other} starts next"),
        }
    }

    /// What it holds of the slots now.
    pub fn slots(&self) -> Result<BootSlots> {
        let mut slots = BootSlots { active: self.active()?, bootable: [false; 2], successful: [false; 2] };
        for slot in [Slot::A, Slot::B] {
            slots.bootable[slot.number() as usize] = self.holds(BOOTABLE, slot).with_context(|| format!("ask whether slot {} starts", slot.letter()))?;
            slots.successful[slot.number() as usize] =
                self.holds(SUCCESSFUL, slot).with_context(|| format!("ask whether slot {} has started fully", slot.letter()))?;
        }
        Ok(slots)
    }

    /// Where an update stands, as the bootloader is told.
    pub fn merge_status(&self) -> Result<MergeStatus> {
        let call = Message::call(MERGE);
        let reply = self.service.transact(MERGE_STATUS, &call).context("ask where an update stands")?;
        let mut read = Answer::new(reply.bytes());
        read.status().context("ask where an update stands")?;
        Ok(match read.i32().context("read where an update stands")? {
            0 => MergeStatus::None,
            1 => MergeStatus::Unknown,
            2 => MergeStatus::Snapshotted,
            3 => MergeStatus::Merging,
            4 => MergeStatus::Cancelled,
            other => bail!("the boot control says an update stands at {other}"),
        })
    }

    /// Tell the bootloader where an update stands.
    pub fn set_merge_status(&self, status: MergeStatus) -> Result<()> {
        let mut call = Message::call(MERGE);
        call.u32(status as u32);
        let what = || format!("tell the bootloader an update stands at {status:?}");
        let reply = self.service.transact(SET_MERGE_STATUS, &call).with_context(what)?;
        let mut read = Answer::new(reply.bytes());
        read.status().with_context(what)?;
        if !read.bool().with_context(what)? {
            bail!("the boot control would not {}", what());
        }
        Ok(())
    }

    /// Whether what `method` asks holds of `slot`.
    fn holds(&self, method: u32, slot: Slot) -> Result<bool> {
        let mut call = Message::call(FIRST);
        call.u32(slot.number());
        let reply = self.service.transact(method, &call)?;
        let mut read = Answer::new(reply.bytes());
        read.status()?;
        match read.i32()? {
            YES => Ok(true),
            NO => Ok(false),
            other => bail!("the boot control answered {other}"),
        }
    }

    /// Have the bootloader start `slot` next. The boot control gives it the
    /// tries a slot gets and takes away any mark that it does not start.
    pub fn set_active(&self, slot: Slot) -> Result<()> {
        let mut call = Message::call(FIRST);
        call.u32(slot.number());
        let what = || format!("have slot {} start next", slot.letter());
        let reply = self.service.transact(SET_ACTIVE, &call).with_context(what)?;
        let mut read = Answer::new(reply.bytes());
        read.status().with_context(what)?;
        // The answer is whether it worked, and why not in text it points at.
        let done = read.pointed().with_context(what)?;
        if done.first().is_some_and(|&b| b != 0) {
            return Ok(());
        }
        let why = read.pointed().with_context(what)?;
        let why = String::from_utf8_lossy(why.split(|&b| b == 0).next().unwrap_or_default());
        bail!("the boot control would not {}: {why}", what())
    }
}
