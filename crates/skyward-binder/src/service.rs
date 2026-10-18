use crate::driver::{Binder, Handle};
use crate::parcel::Parcel;
use crate::status;
use anyhow::{Context, Result};

/// The directory every program registers with and looks things up in.
const DIRECTORY: &str = "android.os.IServiceManager";

/// Ask for a service without waiting for one that has not registered yet.
const CHECK_SERVICE: u32 = 2;

/// List the names registered at the dump priorities given.
const LIST_SERVICES: u32 = 4;

/// Every dump priority a service can register at.
const DUMP_PRIORITY_ALL: i32 = 0b1111;

/// Ask whether the phone says it has a service, whether or not it is running.
const IS_DECLARED: u32 = 7;

/// Ask any object which process serves it. libbinder answers this itself,
/// for every object, before any interface sees the call.
const DEBUG_PID: u32 = u32::from_be_bytes(*b"_PID");

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

/// Every name registered with the directory, the directory's own among
/// them.
pub fn names(binder: &Binder) -> Result<Vec<String>> {
    let directory = binder.directory()?;
    let mut call = Parcel::call(DIRECTORY);
    call.i32(DUMP_PRIORITY_ALL);
    let reply = directory.transact(LIST_SERVICES, &call).context("list what has registered")?;
    let mut read = reply.read();
    status::ok(&mut read).context("list what has registered")?;
    let count = read.i32().context("read how many have registered")?;
    anyhow::ensure!(count >= 0, "the directory says {count} have registered");
    (0..count).map(|_| read.string().context("read a registered name")).collect()
}

/// The process that serves an object.
pub fn pid(service: &Handle) -> Result<u32> {
    let reply = service.transact(DEBUG_PID, &Parcel::default()).context("ask which process serves an object")?;
    let pid = reply.read().i32().context("read which process serves an object")?;
    u32::try_from(pid).with_context(|| format!("an object says process {pid} serves it"))
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
