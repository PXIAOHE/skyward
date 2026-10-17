use crate::canvas::Canvas;
use anyhow::Result;

/// A present target. The device backend scans out through KMS; the host
/// preview backend encodes PNGs. Keeping the renderer behind this trait is
/// what lets the whole UI be built and reviewed without the phone.
pub trait Surface {
    fn size(&self) -> (u32, u32);
    fn present(&mut self, canvas: &Canvas) -> Result<()>;
}

#[cfg(target_os = "android")]
pub mod kms {
    use super::*;
    use anyhow::{anyhow, Context};
    use drm::buffer::{self, DrmFourcc, DrmModifier, PlanarBuffer};
    use drm::control::dumbbuffer::DumbBuffer;
    use drm::control::{
        atomic, connector, framebuffer, plane, property, AtomicCommitFlags, Device as ControlDevice, Event, FbCmd2Flags, ModeTypeFlags, PlaneType,
        ResourceHandle,
    };
    use drm::Device;
    use std::collections::HashMap;
    use std::fs::{File, OpenOptions};
    use std::os::fd::{AsFd, AsRawFd, BorrowedFd};
    use std::time::{Duration, Instant};

    /// How long the display may take to complete a commit.
    const COMMIT_TIMEOUT: Duration = Duration::from_secs(2);

    struct Card(File);

    impl AsFd for Card {
        fn as_fd(&self) -> BorrowedFd<'_> {
            self.0.as_fd()
        }
    }

    impl Device for Card {}
    impl ControlDevice for Card {}

    /// A single-plane dumb buffer presented to ADDFB2, so the framebuffer
    /// keeps the buffer's own pixel format.
    struct Planar<'a>(&'a DumbBuffer);

    impl PlanarBuffer for Planar<'_> {
        fn size(&self) -> (u32, u32) {
            buffer::Buffer::size(self.0)
        }
        fn format(&self) -> DrmFourcc {
            buffer::Buffer::format(self.0)
        }
        fn modifier(&self) -> Option<DrmModifier> {
            None
        }
        fn pitches(&self) -> [u32; 4] {
            [buffer::Buffer::pitch(self.0), 0, 0, 0]
        }
        fn handles(&self) -> [Option<buffer::Handle>; 4] {
            [Some(buffer::Buffer::handle(self.0)), None, None, None]
        }
        fn offsets(&self) -> [u32; 4] {
            [0; 4]
        }
    }

    /// Byte order of the scanout buffer against the canvas's RGBA.
    #[derive(Clone, Copy, Debug)]
    enum Order {
        Rgba,
        Bgra,
    }

    struct Buf {
        db: DumbBuffer,
        fb: framebuffer::Handle,
    }

    /// A KMS object's properties by name, with their current values.
    struct Props(HashMap<String, (property::Handle, u64)>);

    impl Props {
        fn read<T: ResourceHandle>(card: &Card, handle: T) -> Result<Self> {
            let values = card.get_properties(handle).context("read properties")?;
            let mut map = HashMap::new();
            for (&id, &value) in values.iter() {
                let info = card.get_property(id).context("read property")?;
                map.insert(info.name().to_string_lossy().into_owned(), (id, value));
            }
            Ok(Props(map))
        }

        fn get(&self, name: &str) -> Result<(property::Handle, u64)> {
            self.0.get(name).copied().ok_or_else(|| anyhow!("KMS property {name} missing"))
        }

        fn handle(&self, name: &str) -> Result<property::Handle> {
            Ok(self.get(name)?.0)
        }

        fn value(&self, name: &str) -> Result<u64> {
            Ok(self.get(name)?.1)
        }

        fn line(&self) -> String {
            let mut parts: Vec<String> = self.0.iter().map(|(name, (_, value))| format!("{name}={value}")).collect();
            parts.sort();
            parts.join(" ")
        }
    }

    fn open_card(path: &str) -> Result<Card> {
        let file = OpenOptions::new().read(true).write(true).open(path).with_context(|| format!("open {path}"))?;
        let card = Card(file);
        card.set_client_capability(drm::ClientCapability::UniversalPlanes, true).context("enable universal planes")?;
        card.set_client_capability(drm::ClientCapability::Atomic, true).context("enable atomic modesetting")?;
        Ok(card)
    }

    /// Built-in panel interfaces. Writeback and external outputs are never
    /// the recovery's screen.
    fn is_panel(interface: connector::Interface) -> bool {
        matches!(interface, connector::Interface::DSI | connector::Interface::EmbeddedDisplayPort | connector::Interface::LVDS)
    }

    /// Commit without blocking and wait for the flip event, so a commit the
    /// display never completes surfaces as an error instead of an endless
    /// wait.
    fn commit(card: &Card, req: atomic::AtomicModeReq, flags: AtomicCommitFlags) -> Result<()> {
        card.atomic_commit(flags | AtomicCommitFlags::NONBLOCK | AtomicCommitFlags::PAGE_FLIP_EVENT, req).context("atomic commit")?;
        let deadline = Instant::now() + COMMIT_TIMEOUT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let mut pfd = libc::pollfd { fd: card.as_fd().as_raw_fd(), events: libc::POLLIN, revents: 0 };
            let n = unsafe { libc::poll(&mut pfd, 1, left.as_millis() as i32) };
            if n < 0 {
                let err = std::io::Error::last_os_error();
                if err.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(err).context("wait for the display");
            }
            if n == 0 {
                return Err(anyhow!("the display did not complete the commit within {} ms", COMMIT_TIMEOUT.as_millis()));
            }
            for event in card.receive_events().context("read display events")? {
                if let Event::PageFlip(_) = event {
                    return Ok(());
                }
            }
        }
    }

    /// Connectors with their modes and properties, encoders with the CRTC
    /// they feed, CRTCs with their mode and properties, and the planes in
    /// use: what a device has for scanout and how it is wired right now.
    pub fn report(path: &str) -> Result<String> {
        use std::fmt::Write;
        let card = open_card(path)?;
        let res = card.resource_handles().context("read KMS resources")?;
        let mut out = String::new();
        for &h in res.connectors() {
            let c = card.get_connector(h, true).context("read connector")?;
            let encoder = c.current_encoder().map(u32::from);
            writeln!(out, "connector {} {:?} {:?} encoder {encoder:?} {} modes", u32::from(h), c.interface(), c.state(), c.modes().len())?;
            writeln!(out, "  {}", Props::read(&card, h)?.line())?;
            if is_panel(c.interface()) {
                for m in c.modes() {
                    let (w, hgt) = m.size();
                    writeln!(out, "  mode {:?} {w}x{hgt} {} Hz clock {} type {:?} flags {:?}", m.name(), m.vrefresh(), m.clock(), m.mode_type(), m.flags())?;
                }
            }
        }
        for &h in res.encoders() {
            let e = card.get_encoder(h).context("read encoder")?;
            let possible: Vec<u32> = res.filter_crtcs(e.possible_crtcs()).into_iter().map(u32::from).collect();
            writeln!(out, "encoder {} {:?} crtc {:?} possible {possible:?}", u32::from(h), e.kind(), e.crtc().map(u32::from))?;
        }
        for &h in res.crtcs() {
            let i = card.get_crtc(h).context("read CRTC")?;
            let mode = i.mode().map(|m| format!("{:?} {} Hz", m.name(), m.vrefresh()));
            writeln!(out, "crtc {} mode {mode:?} fb {:?}", u32::from(h), i.framebuffer().map(u32::from))?;
            writeln!(out, "  {}", Props::read(&card, h)?.line())?;
        }
        for p in card.plane_handles().context("list planes")? {
            let i = card.get_plane(p).context("read plane")?;
            let props = Props::read(&card, p)?;
            writeln!(
                out,
                "plane {} type {} crtc {:?} fb {:?} possible {} formats {}",
                u32::from(p),
                props.value("type")?,
                i.crtc().map(u32::from),
                i.framebuffer().map(u32::from),
                res.filter_crtcs(i.possible_crtcs()).len(),
                i.formats().len()
            )?;
            if i.crtc().is_some() {
                writeln!(out, "  {}", props.line())?;
            }
        }
        Ok(out)
    }

    /// Direct scanout on the built-in panel: its preferred mode on the CRTC
    /// driving it, two dumb buffers swapped on one primary plane, and every
    /// other connector and plane taken off that CRTC.
    pub struct KmsSurface {
        card: Card,
        bufs: [Buf; 2],
        back: usize,
        plane: plane::Handle,
        fb_prop: property::Handle,
        w: u32,
        h: u32,
        order: Order,
        summary: String,
    }

    impl KmsSurface {
        pub fn open(path: &str) -> Result<Self> {
            let card = open_card(path)?;
            let res = card.resource_handles().context("read KMS resources")?;

            let mut panel = None;
            for &h in res.connectors() {
                let info = card.get_connector(h, true).context("read connector")?;
                if is_panel(info.interface()) && info.state() == connector::State::Connected {
                    panel = Some(info);
                    break;
                }
            }
            let conn = panel.ok_or_else(|| anyhow!("no connected built-in panel"))?;
            let mode = *conn
                .modes()
                .iter()
                .find(|m| m.mode_type().contains(ModeTypeFlags::PREFERRED))
                .ok_or_else(|| anyhow!("the panel reports no preferred mode"))?;
            let (w, h) = (mode.size().0 as u32, mode.size().1 as u32);

            // The CRTC already driving the panel, or the first one an
            // encoder of this connector can drive.
            let current = match conn.current_encoder() {
                Some(e) => card.get_encoder(e).context("read encoder")?.crtc(),
                None => None,
            };
            let crtc = match current {
                Some(c) => c,
                None => {
                    let mut found = None;
                    for &e in conn.encoders() {
                        let enc = card.get_encoder(e).context("read encoder")?;
                        if let Some(&c) = res.filter_crtcs(enc.possible_crtcs()).first() {
                            found = Some(c);
                            break;
                        }
                    }
                    found.ok_or_else(|| anyhow!("no CRTC can drive the panel"))?
                }
            };

            // The primary plane already on the CRTC, else the first primary
            // plane that can drive it. Planes left on the CRTC by the boot
            // splash are switched off.
            let mut chosen: Option<(plane::Handle, Vec<u32>, Props, bool)> = None;
            let mut attached = Vec::new();
            for p in card.plane_handles().context("list planes")? {
                let info = card.get_plane(p).context("read plane")?;
                let props = Props::read(&card, p)?;
                let on_crtc = info.crtc() == Some(crtc);
                if on_crtc {
                    attached.push(p);
                }
                if props.value("type")? != PlaneType::Primary as u64 || !res.filter_crtcs(info.possible_crtcs()).contains(&crtc) {
                    continue;
                }
                if chosen.as_ref().is_none_or(|c| on_crtc && !c.3) {
                    chosen = Some((p, info.formats().to_vec(), props, on_crtc));
                }
            }
            let (plane, formats, plane_props, _) = chosen.ok_or_else(|| anyhow!("no primary plane can drive the CRTC"))?;

            let pick = [
                (DrmFourcc::Xbgr8888, Order::Rgba),
                (DrmFourcc::Abgr8888, Order::Rgba),
                (DrmFourcc::Xrgb8888, Order::Bgra),
                (DrmFourcc::Argb8888, Order::Bgra),
            ];
            let (fourcc, order) = pick
                .into_iter()
                .find(|(f, _)| formats.contains(&(*f as u32)))
                .ok_or_else(|| anyhow!("the primary plane takes none of the 32-bit RGB formats"))?;

            let make = || -> Result<Buf> {
                let db = card.create_dumb_buffer((w, h), fourcc, 32).context("create dumb buffer")?;
                let fb = card.add_planar_framebuffer(&Planar(&db), FbCmd2Flags::empty()).context("add framebuffer")?;
                Ok(Buf { db, fb })
            };
            let bufs = [make()?, make()?];

            let crtc_id = u64::from(u32::from(crtc));
            let mut req = atomic::AtomicModeReq::new();
            for &other in res.connectors().iter().filter(|&&c| c != conn.handle()) {
                let props = Props::read(&card, other)?;
                if props.value("CRTC_ID")? == crtc_id {
                    req.add_property(other, props.handle("CRTC_ID")?, property::Value::CRTC(None));
                }
            }
            let conn_props = Props::read(&card, conn.handle())?;
            let crtc_props = Props::read(&card, crtc)?;
            let blob = card.create_property_blob(&mode).context("create mode blob")?;
            req.add_property(conn.handle(), conn_props.handle("CRTC_ID")?, property::Value::CRTC(Some(crtc)));
            req.add_property(crtc, crtc_props.handle("MODE_ID")?, blob);
            req.add_property(crtc, crtc_props.handle("ACTIVE")?, property::Value::Boolean(true));
            for &other in attached.iter().filter(|&&p| p != plane) {
                let props = Props::read(&card, other)?;
                req.add_property(other, props.handle("FB_ID")?, property::Value::Framebuffer(None));
                req.add_property(other, props.handle("CRTC_ID")?, property::Value::CRTC(None));
            }
            let fb_prop = plane_props.handle("FB_ID")?;
            req.add_property(plane, fb_prop, property::Value::Framebuffer(Some(bufs[0].fb)));
            req.add_property(plane, plane_props.handle("CRTC_ID")?, property::Value::CRTC(Some(crtc)));
            req.add_property(plane, plane_props.handle("SRC_X")?, property::Value::UnsignedRange(0));
            req.add_property(plane, plane_props.handle("SRC_Y")?, property::Value::UnsignedRange(0));
            req.add_property(plane, plane_props.handle("SRC_W")?, property::Value::UnsignedRange((w as u64) << 16));
            req.add_property(plane, plane_props.handle("SRC_H")?, property::Value::UnsignedRange((h as u64) << 16));
            req.add_property(plane, plane_props.handle("CRTC_X")?, property::Value::SignedRange(0));
            req.add_property(plane, plane_props.handle("CRTC_Y")?, property::Value::SignedRange(0));
            req.add_property(plane, plane_props.handle("CRTC_W")?, property::Value::UnsignedRange(w as u64));
            req.add_property(plane, plane_props.handle("CRTC_H")?, property::Value::UnsignedRange(h as u64));
            commit(&card, req, AtomicCommitFlags::ALLOW_MODESET).context("set the panel mode")?;

            let summary = format!(
                "{:?} connector {} mode {:?} {w}x{h} {} Hz, crtc {}, plane {} {:?}",
                conn.interface(),
                u32::from(conn.handle()),
                mode.name(),
                mode.vrefresh(),
                u32::from(crtc),
                u32::from(plane),
                fourcc
            );
            Ok(KmsSurface { card, bufs, back: 1, plane, fb_prop, w, h, order, summary })
        }

        pub fn describe(&self) -> &str {
            &self.summary
        }
    }

    impl Surface for KmsSurface {
        fn size(&self) -> (u32, u32) {
            (self.w, self.h)
        }

        fn present(&mut self, canvas: &Canvas) -> Result<()> {
            if (canvas.w, canvas.h) != (self.w, self.h) {
                return Err(anyhow!("canvas {}x{} does not match the display {}x{}", canvas.w, canvas.h, self.w, self.h));
            }
            let order = self.order;
            let buf = &mut self.bufs[self.back];
            let pitch = buffer::Buffer::pitch(&buf.db) as usize;
            let row = self.w as usize * 4;
            {
                let mut map = self.card.map_dumb_buffer(&mut buf.db).context("map dumb buffer")?;
                let dst = map.as_mut();
                for y in 0..self.h as usize {
                    let src = &canvas.data[y * row..(y + 1) * row];
                    let out = &mut dst[y * pitch..y * pitch + row];
                    match order {
                        Order::Rgba => out.copy_from_slice(src),
                        Order::Bgra => {
                            for (o, s) in out.as_chunks_mut::<4>().0.iter_mut().zip(src.as_chunks::<4>().0) {
                                *o = [s[2], s[1], s[0], s[3]];
                            }
                        }
                    }
                }
            }
            let mut req = atomic::AtomicModeReq::new();
            req.add_property(self.plane, self.fb_prop, property::Value::Framebuffer(Some(buf.fb)));
            commit(&self.card, req, AtomicCommitFlags::empty()).context("flip")?;
            self.back = 1 - self.back;
            Ok(())
        }
    }
}
