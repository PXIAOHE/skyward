//! The phone's side of the adb protocol, as much as `adb sideload` needs:
//! the connection handshake, the sideload-host stream, and the block
//! requests that pull a package from the computer a piece at a time.

use std::fmt;
use std::io;
use zerocopy::little_endian::U32;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

pub const SYNC: u32 = 0x434e_5953;
pub const CNXN: u32 = 0x4e58_4e43;
pub const AUTH: u32 = 0x4854_5541;
pub const OPEN: u32 = 0x4e45_504f;
pub const OKAY: u32 = 0x5941_4b4f;
pub const CLSE: u32 = 0x4553_4c43;
pub const WRTE: u32 = 0x4554_5257;
pub const STLS: u32 = 0x534c_5453;

/// Protocol versions: the older one checksums every payload.
pub const VERSION_MIN: u32 = 0x0100_0000;
pub const VERSION: u32 = 0x0100_0001;

/// The largest payload this side takes.
pub const MAX_PAYLOAD: u32 = 1024 * 1024;

pub const HEADER_LEN: usize = 24;

/// Sent in place of a block number once the package has been read; the
/// computer then reports how much it sent and exits.
pub const DONE: &[u8; 8] = b"DONEDONE";

/// The most taken from the USB endpoint in one read.
const READ_CHUNK: usize = 16 * 1024;

/// The block sizes the package can be served in, and the most blocks: a
/// read never spans more than two blocks, and every block's hash is kept.
pub const MIN_BLOCK: u32 = 4096;
pub const MAX_BLOCK: u32 = 4 << 20;
pub const MAX_BLOCKS: u64 = 1 << 18;

/// The stream this side opens; the only one there is.
const LOCAL_ID: u32 = 1;

#[derive(Debug)]
pub enum Error {
    /// The USB endpoint failed: the cable came out or the function went
    /// away.
    Io(io::Error),
    /// The read was interrupted on purpose, to stop waiting.
    Interrupted,
    /// The computer closed the stream: `adb sideload` was stopped.
    Closed,
    /// The computer's adb is too old to send a package a block at a time.
    OldHost,
    /// The computer broke the protocol.
    Protocol(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "USB: {e}"),
            Error::Interrupted => f.write_str("interrupted"),
            Error::Closed => f.write_str("the computer closed the stream"),
            Error::OldHost => f.write_str("the computer's adb predates sideload-host"),
            Error::Protocol(m) => write!(f, "protocol: {m}"),
        }
    }
}

impl std::error::Error for Error {}

/// The header every packet starts with, as adb lays it out.
#[derive(FromBytes, IntoBytes, KnownLayout, Immutable)]
#[repr(C)]
struct Message {
    command: U32,
    arg0: U32,
    arg1: U32,
    data_length: U32,
    data_check: U32,
    magic: U32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet {
    pub command: u32,
    pub arg0: u32,
    pub arg1: u32,
    pub payload: Vec<u8>,
}

impl Packet {
    pub fn new(command: u32, arg0: u32, arg1: u32, payload: impl Into<Vec<u8>>) -> Packet {
        Packet { command, arg0, arg1, payload: payload.into() }
    }

    /// The packet's header; `version` decides whether it carries the
    /// payload's checksum.
    pub fn header(&self, version: u32) -> [u8; HEADER_LEN] {
        let check = if version < VERSION { checksum(&self.payload) } else { 0 };
        let len = u32::try_from(self.payload.len()).expect("payloads stay under MAX_PAYLOAD");
        let message = Message {
            command: U32::new(self.command),
            arg0: U32::new(self.arg0),
            arg1: U32::new(self.arg1),
            data_length: U32::new(len),
            data_check: U32::new(check),
            magic: U32::new(!self.command),
        };
        message.as_bytes().try_into().expect("a message is HEADER_LEN bytes")
    }
}

/// A header off the wire, with the payload still to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub command: u32,
    pub arg0: u32,
    pub arg1: u32,
    pub len: u32,
    pub check: u32,
}

impl Header {
    pub fn parse(bytes: &[u8; HEADER_LEN]) -> Result<Header, Error> {
        let m = Message::read_from_bytes(bytes).expect("a message is HEADER_LEN bytes");
        let (command, magic) = (m.command.get(), m.magic.get());
        if magic != !command {
            return Err(Error::Protocol(format!("magic {magic:#010x} does not match command {command:#010x}")));
        }
        if ![SYNC, CNXN, AUTH, OPEN, OKAY, CLSE, WRTE, STLS].contains(&command) {
            return Err(Error::Protocol(format!("unknown command {command:#010x}")));
        }
        let len = m.data_length.get();
        if len > MAX_PAYLOAD {
            return Err(Error::Protocol(format!("a payload of {len} bytes")));
        }
        Ok(Header { command, arg0: m.arg0.get(), arg1: m.arg1.get(), len, check: m.data_check.get() })
    }
}

/// The payload checksum of the first protocol version: the sum of its
/// bytes.
pub fn checksum(data: &[u8]) -> u32 {
    data.iter().fold(0u32, |sum, &b| sum.wrapping_add(u32::from(b)))
}

/// What the phone answers the computer's CNXN with: it is in sideload
/// mode, and which product it is.
pub fn banner(name: &str, model: &str, device: &str) -> Vec<u8> {
    format!("sideload::ro.product.name={name};ro.product.model={model};ro.product.device={device};features=").into_bytes()
}

/// What the computer offers to send: the package's size and the block size
/// it serves it in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Offer {
    pub size: u64,
    pub block_size: u32,
}

impl Offer {
    pub fn blocks(&self) -> u64 {
        self.size.div_ceil(u64::from(self.block_size))
    }

    /// The length of block `index`: the last one may be short.
    pub fn block_len(&self, index: u64) -> usize {
        let start = index * u64::from(self.block_size);
        (self.size.saturating_sub(start)).min(u64::from(self.block_size)) as usize
    }

    /// Whether the package can be served as offered.
    pub fn check(&self) -> Result<(), Error> {
        if self.size == 0 {
            return Err(Error::Protocol("an empty package".into()));
        }
        if !(MIN_BLOCK..=MAX_BLOCK).contains(&self.block_size) {
            return Err(Error::Protocol(format!("a block size of {} bytes", self.block_size)));
        }
        if self.blocks() > MAX_BLOCKS {
            return Err(Error::Protocol(format!("{} blocks", self.blocks())));
        }
        Ok(())
    }
}

/// A service the computer asked to open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Service {
    /// `sideload-host:<size>:<block size>`: send a package block by block.
    SideloadHost(Offer),
    /// `sideload:<size>`: send the whole package in one go, as adb did
    /// before Android 4.4.
    Sideload,
    Other(String),
}

pub fn service(payload: &[u8]) -> Service {
    let name = String::from_utf8_lossy(payload.strip_suffix(&[0]).unwrap_or(payload)).into_owned();
    if let Some(args) = name.strip_prefix("sideload-host:") {
        let offer = args.split_once(':').and_then(|(size, block)| {
            Some(Offer { size: size.parse().ok()?, block_size: block.parse().ok()? })
        });
        return match offer {
            Some(offer) => Service::SideloadHost(offer),
            None => Service::Other(name),
        };
    }
    if name.starts_with("sideload:") {
        return Service::Sideload;
    }
    Service::Other(name)
}

/// The USB endpoints the protocol runs over. A read fails with
/// [`io::ErrorKind::Interrupted`] when waiting is to stop, and with
/// [`io::ErrorKind::ConnectionAborted`] when the cable came out.
pub trait Link {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;
    fn write(&mut self, data: &[u8]) -> io::Result<()>;
}

#[derive(Debug, Clone, Copy)]
struct Stream {
    remote: u32,
    /// Whether the computer has taken the last write, so another may go.
    writable: bool,
}

enum Incoming {
    Data(Vec<u8>),
    Ready,
}

/// A connection to the computer's adb.
pub struct Connection<L> {
    link: L,
    inbox: Vec<u8>,
    chunk: Vec<u8>,
    version: u32,
    banner: Vec<u8>,
    stream: Option<Stream>,
}

impl<L: Link> Connection<L> {
    pub fn new(link: L, banner: Vec<u8>) -> Connection<L> {
        Connection { link, inbox: Vec::new(), chunk: vec![0; READ_CHUNK], version: VERSION, banner, stream: None }
    }

    /// The endpoints back, once the connection is done with.
    pub fn into_link(self) -> L {
        self.link
    }

    /// Fill the inbox to `n` bytes. USB transfers end where the computer
    /// ended its write, so a read can bring part of a packet, several, or
    /// nothing at all.
    fn fill(&mut self, n: usize) -> Result<(), Error> {
        while self.inbox.len() < n {
            let got = match self.link.read(&mut self.chunk) {
                Ok(got) => got,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => return Err(Error::Interrupted),
                Err(e) => return Err(Error::Io(e)),
            };
            self.inbox.extend_from_slice(&self.chunk[..got]);
        }
        Ok(())
    }

    pub fn receive(&mut self) -> Result<Packet, Error> {
        self.fill(HEADER_LEN)?;
        let head: [u8; HEADER_LEN] = self.inbox[..HEADER_LEN].try_into().expect("filled above");
        let header = Header::parse(&head)?;
        let len = header.len as usize;
        self.fill(HEADER_LEN + len)?;
        let payload = self.inbox[HEADER_LEN..HEADER_LEN + len].to_vec();
        self.inbox.drain(..HEADER_LEN + len);
        if self.version < VERSION && checksum(&payload) != header.check {
            return Err(Error::Protocol("payload checksum mismatch".into()));
        }
        Ok(Packet::new(header.command, header.arg0, header.arg1, payload))
    }

    pub fn send(&mut self, packet: &Packet) -> Result<(), Error> {
        self.link.write(&packet.header(self.version)).map_err(Error::Io)?;
        if !packet.payload.is_empty() {
            self.link.write(&packet.payload).map_err(Error::Io)?;
        }
        Ok(())
    }

    /// Answer the computer's CNXN: this side's version and largest payload,
    /// and the banner. A new CNXN means the computer's adb started over, so
    /// any stream is gone.
    fn connect(&mut self, cnxn: &Packet) -> Result<(), Error> {
        if cnxn.arg0 < VERSION_MIN {
            return Err(Error::Protocol(format!("version {:#010x}", cnxn.arg0)));
        }
        self.version = cnxn.arg0.min(VERSION);
        self.stream = None;
        let banner = self.banner.clone();
        self.send(&Packet::new(CNXN, VERSION, MAX_PAYLOAD, banner))
    }

    /// Answer the computer until it opens sideload-host, and return what it
    /// offers to send. A cable pulled out meanwhile only means waiting for
    /// the computer to come back.
    pub fn accept(&mut self) -> Result<Offer, Error> {
        loop {
            let p = match self.receive() {
                Ok(p) => p,
                Err(Error::Io(e)) if e.kind() == io::ErrorKind::ConnectionAborted => {
                    self.inbox.clear();
                    self.stream = None;
                    continue;
                }
                Err(e) => return Err(e),
            };
            match p.command {
                CNXN => self.connect(&p)?,
                OPEN => match service(&p.payload) {
                    Service::SideloadHost(offer) => {
                        self.stream = Some(Stream { remote: p.arg0, writable: true });
                        self.send(&Packet::new(OKAY, LOCAL_ID, p.arg0, []))?;
                        return Ok(offer);
                    }
                    Service::Sideload => {
                        self.send(&Packet::new(CLSE, 0, p.arg0, []))?;
                        return Err(Error::OldHost);
                    }
                    Service::Other(_) => self.send(&Packet::new(CLSE, 0, p.arg0, []))?,
                },
                // Streams this side never opened, and keys it never asked
                // for.
                _ => {}
            }
        }
    }

    /// The next thing the computer says on the stream: data, which is
    /// acknowledged at once, or that it took this side's last write.
    fn incoming(&mut self) -> Result<Incoming, Error> {
        loop {
            let p = self.receive()?;
            let Some(stream) = self.stream else { return Err(Error::Closed) };
            match p.command {
                WRTE if p.arg1 == LOCAL_ID => {
                    self.send(&Packet::new(OKAY, LOCAL_ID, stream.remote, []))?;
                    return Ok(Incoming::Data(p.payload));
                }
                OKAY if p.arg1 == LOCAL_ID => return Ok(Incoming::Ready),
                CLSE if p.arg1 == LOCAL_ID => {
                    self.stream = None;
                    return Err(Error::Closed);
                }
                CNXN => {
                    self.connect(&p)?;
                    return Err(Error::Closed);
                }
                _ => {}
            }
        }
    }

    fn mark_writable(&mut self) {
        if let Some(s) = self.stream.as_mut() {
            s.writable = true;
        }
    }

    /// Write on the stream once the computer has taken the last write.
    fn write_stream(&mut self, data: &[u8]) -> Result<(), Error> {
        loop {
            match self.stream {
                None => return Err(Error::Closed),
                Some(s) if s.writable => break,
                Some(_) => match self.incoming()? {
                    Incoming::Ready => self.mark_writable(),
                    Incoming::Data(_) => return Err(Error::Protocol("data nobody asked for".into())),
                },
            }
        }
        let stream = self.stream.as_mut().expect("checked above");
        stream.writable = false;
        let remote = stream.remote;
        self.send(&Packet::new(WRTE, LOCAL_ID, remote, data))
    }

    /// Ask the computer for block `index` and read it into `out`, which is
    /// as long as the block.
    pub fn fetch(&mut self, index: u64, out: &mut [u8]) -> Result<(), Error> {
        self.write_stream(format!("{index:08}").as_bytes())?;
        let mut got = 0;
        while got < out.len() {
            match self.incoming()? {
                Incoming::Ready => self.mark_writable(),
                Incoming::Data(data) => {
                    let end = got + data.len();
                    if end > out.len() {
                        return Err(Error::Protocol(format!("block {index}: {end} bytes for {}", out.len())));
                    }
                    out[got..end].copy_from_slice(&data);
                    got = end;
                }
            }
        }
        Ok(())
    }

    /// Tell the computer the package has been read and close the stream,
    /// once it has taken the message.
    pub fn finish(&mut self) -> Result<(), Error> {
        self.write_stream(DONE)?;
        loop {
            match self.incoming()? {
                Incoming::Ready => break,
                Incoming::Data(_) => {}
            }
        }
        let stream = self.stream.take().expect("open until closed here");
        self.send(&Packet::new(CLSE, LOCAL_ID, stream.remote, []))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers_carry_the_inverted_command_and_a_checksum_only_on_the_first_version() {
        let p = Packet::new(WRTE, 1, 7, *b"00000042");
        let h = p.header(VERSION);
        assert_eq!(&h[..4], b"WRTE");
        assert_eq!(u32::from_le_bytes(h[12..16].try_into().unwrap()), 8);
        assert_eq!(u32::from_le_bytes(h[16..20].try_into().unwrap()), 0);
        assert_eq!(u32::from_le_bytes(h[20..24].try_into().unwrap()), !WRTE);
        let old = p.header(VERSION_MIN);
        assert_eq!(u32::from_le_bytes(old[16..20].try_into().unwrap()), 8 * u32::from(b'0') + 4 + 2);
        let back = Header::parse(&h).unwrap();
        assert_eq!(back, Header { command: WRTE, arg0: 1, arg1: 7, len: 8, check: 0 });
    }

    #[test]
    fn broken_headers_are_refused() {
        let mut h = Packet::new(OKAY, 1, 2, []).header(VERSION);
        h[20] ^= 1;
        assert!(matches!(Header::parse(&h), Err(Error::Protocol(_))));
        let mut big = Packet::new(WRTE, 1, 2, []).header(VERSION);
        big[12..16].copy_from_slice(&(MAX_PAYLOAD + 1).to_le_bytes());
        assert!(matches!(Header::parse(&big), Err(Error::Protocol(_))));
        let unknown = Packet::new(0x1234_5678, 0, 0, []).header(VERSION);
        assert!(matches!(Header::parse(&unknown), Err(Error::Protocol(_))));
    }

    #[test]
    fn the_banner_names_the_product_like_adbd_does() {
        assert_eq!(
            banner("ishtar", "2304FPN6DC", "ishtar"),
            b"sideload::ro.product.name=ishtar;ro.product.model=2304FPN6DC;ro.product.device=ishtar;features=".to_vec()
        );
    }

    #[test]
    fn services_are_read_as_adb_names_them() {
        assert_eq!(
            service(b"sideload-host:7340032000:65536\0"),
            Service::SideloadHost(Offer { size: 7_340_032_000, block_size: 65536 })
        );
        assert_eq!(service(b"sideload:123456\0"), Service::Sideload);
        assert_eq!(service(b"shell:ls\0"), Service::Other("shell:ls".into()));
        assert_eq!(service(b"sideload-host:x:1\0"), Service::Other("sideload-host:x:1".into()));
    }

    #[test]
    fn offers_are_cut_into_blocks_and_checked() {
        let offer = Offer { size: 65536 * 3 + 100, block_size: 65536 };
        assert_eq!(offer.blocks(), 4);
        assert_eq!(offer.block_len(0), 65536);
        assert_eq!(offer.block_len(3), 100);
        assert!(offer.check().is_ok());
        assert!(Offer { size: 0, block_size: 65536 }.check().is_err());
        assert!(Offer { size: 10, block_size: 512 }.check().is_err());
        assert!(Offer { size: 65536 * (MAX_BLOCKS + 1), block_size: 65536 }.check().is_err());
    }
}
