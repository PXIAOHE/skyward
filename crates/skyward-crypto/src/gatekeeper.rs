//! Having the phone's secure world check a credential. The secure world
//! keeps what it needs to recognise each credential, counts the wrong
//! answers, and holds off further tries once there have been too many; none
//! of that can be worked around from outside. What it hands back for a
//! right answer is the note the key service wants before it will use a key
//! the credential guards.

use crate::token::Token;
use anyhow::{bail, Context, Result};
use skyward_binder::driver::{Binder, Handle};
use skyward_binder::hidl::{self, Answer, Message};

/// The phone's credential service, which is a hardware service of the older
/// kind and so is reached over the other driver.
pub const SERVICE: &str = "android.hardware.gatekeeper@1.0::IGatekeeper";
pub const INSTANCE: &str = "default";

const VERIFY: u32 = 2;

/// Which of the secure world's slots a user's screen lock is enrolled in.
/// The part of the phone that enrols it counts a hundred thousand above the
/// user's own number, keeping those enrolments apart from the ones made
/// before the phone kept credentials this way.
const SLOT: u32 = 100_000;

/// How the service answers.
const RIGHT: i32 = 0;
const RIGHT_BUT_RENEW: i32 = 1;
const HOLDING_OFF: i32 = -2;

/// How long the answer itself is: what it says, how long the caller must
/// wait, and the frame for the note.
const ANSWER_BYTES: usize = 24;

/// Where the note's frame says how long the note is.
const NOTE_LENGTH: std::ops::Range<usize> = 16..20;

/// The phone's credential service, as this program talks to it.
pub struct Gatekeeper<'a> {
    device: Handle<'a>,
}

impl<'a> Gatekeeper<'a> {
    /// Reach the running credential service, or say that none is.
    pub fn open(binder: &'a Binder) -> Result<Gatekeeper<'a>> {
        let device = hidl::find(binder, SERVICE, INSTANCE)?
            .with_context(|| format!("{SERVICE} is not running; the phone's credential service has to be started first"))?;
        Ok(Gatekeeper { device })
    }

    /// What the service says it serves. This changes nothing and takes no
    /// credential, so it is what to call to see that the service answers.
    pub fn serves(&self) -> Result<String> {
        hidl::describes(&self.device)
    }

    /// Have the secure world check `password` against what it recognises
    /// for `user`, and give back the note it writes for a right answer.
    /// `enrolled` is what the phone recorded when the credential was set.
    ///
    /// A wrong answer is counted against the user by the secure world, so
    /// this is not something to call twice over on a guess.
    pub fn verify(&self, user: u32, enrolled: &[u8], password: &[u8]) -> Result<Token> {
        let mut call = Message::call(SERVICE);
        call.u32(SLOT + user);
        // Nothing asked for this note, so it answers no challenge.
        call.u64(0);
        call.bytes(enrolled);
        call.bytes(password);
        let reply = self.device.transact(VERIFY, &call).context("have the secure world check the credential")?;
        let mut read = Answer::new(reply.bytes());
        read.status().context("have the secure world check the credential")?;
        let answer = read.pointed().context("read what the secure world said about the credential")?;
        if answer.len() < ANSWER_BYTES {
            bail!("the secure world answered {} bytes about the credential", answer.len());
        }
        let said = i32::from_le_bytes(answer[0..4].try_into().expect("four bytes"));
        let wait = i32::from_le_bytes(answer[4..8].try_into().expect("four bytes"));
        let length = u32::from_le_bytes(answer[NOTE_LENGTH].try_into().expect("four bytes"));
        match said {
            RIGHT | RIGHT_BUT_RENEW => {}
            HOLDING_OFF => bail!("the phone is holding off further tries for another {} seconds", (wait + 999) / 1000),
            _ => bail!("the secure world does not recognise that credential"),
        }
        if length == 0 {
            bail!("the secure world took the credential but wrote no note for the key service");
        }
        let note = read.pointed().context("read the note the secure world wrote")?;
        Token::of(note)
    }
}
