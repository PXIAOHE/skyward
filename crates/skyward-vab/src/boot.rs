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
const INSTANCE: &str = "default";

/// The methods, numbered as the service numbers them: the first version's
/// in their order, then those each later version added.
const SET_ACTIVE: u32 = 4;
const BOOTABLE: u32 = 6;
const SUCCESSFUL: u32 = 7;
const ACTIVE: u32 = 11;

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
