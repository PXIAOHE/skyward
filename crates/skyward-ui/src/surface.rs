use crate::canvas::Canvas;
use anyhow::Result;

/// A present target. The device backend scans out to the DRM framebuffer; the
/// host preview backend encodes PNGs. Keeping the renderer behind this trait is
/// what lets the whole UI be built and reviewed without the phone.
pub trait Surface {
    fn size(&self) -> (u32, u32);
    fn present(&mut self, canvas: &Canvas) -> Result<()>;
}

#[cfg(target_os = "android")]
pub mod drm {
    use super::*;

    /// Direct scanout via `/dev/dri`. Wired up in the on-device bring-up; the
    /// renderer and page composition above are platform independent.
    pub struct DrmSurface {
        pub w: u32,
        pub h: u32,
    }

    impl Surface for DrmSurface {
        fn size(&self) -> (u32, u32) {
            (self.w, self.h)
        }

        fn present(&mut self, _canvas: &Canvas) -> Result<()> {
            anyhow::bail!("DRM scanout is wired up during on-device bring-up")
        }
    }
}
