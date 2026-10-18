use crate::driver::{Binder, Handle};
use crate::parcel::Parcel;
use crate::status;
use anyhow::{Context, Result};

/// The directory every program registers with and looks things up in.
const DIRECTORY: &str = "android.os.IServiceManager";

/// Ask for a service without waiting for one that has not registered yet.
const CHECK_SERVICE: u32 = 2;

/// Ask whether the phone says it has a service, whether or not it is running.
const IS_DECLARED: u32 = 7;

/// The service registered under `name`, or `None` while nothing has
/// registered it.
pub fn find<'a>(binder: &'a Binder, name: &str) -> Result<Option<Handle<'a>>> {
    let directory = binder.directory()?;
    let mut call = Parcel::call(DIRECTORY);
    call.string(name);
    let reply = directory.transact(CHECK_SERVICE, &call).with_context(|| format!("look up {name}"))?;
    let mut read = reply.read();
    status::ok(&mut read).with_context(|| format!("look up {name}"))?;
    match read.object().with_context(|| format!("read what the directory answered for {name}"))? {
        Some(handle) => Ok(Some(Handle::hold(binder, handle)?)),
        None => Ok(None),
    }
}

/// Whether the phone's own description says it has this service. The
/// directory turns away a hardware service the description does not name,
/// so a service that will not register is often one the directory cannot
/// see described.
pub fn declared(binder: &Binder, name: &str) -> Result<bool> {
    let directory = binder.directory()?;
    let mut call = Parcel::call(DIRECTORY);
    call.string(name);
    let reply = directory.transact(IS_DECLARED, &call).with_context(|| format!("ask whether {name} is described"))?;
    let mut read = reply.read();
    status::ok(&mut read).with_context(|| format!("ask whether {name} is described"))?;
    read.bool().with_context(|| format!("read whether {name} is described"))
}
