//! What the phone's GPU can be made to do, asked of its own libraries.
//!
//! Nothing here is linked against EGL or GLES: both are opened by name at the
//! moment they are asked, because the recovery builds before the libraries of
//! any one phone are known. What comes back says whether a program inside the
//! recovery can draw with the GPU at all, which is what a browser plugin would
//! have to do.

use std::ffi::{c_char, c_void, CString};
use std::path::Path;

/// What the GPU answered.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Gpu {
    /// The library the display came from, where one was found.
    pub library: Option<String>,
    /// Why there is no GPU to speak of, where there is none.
    pub unavailable: Option<String>,
    pub vendor: Option<String>,
    pub version: Option<String>,
    /// What the driver calls the chip, as GLES names it.
    pub renderer: Option<String>,
    pub gles: Option<String>,
    /// A context was made without a window, which is how a browser draws
    /// offscreen.
    pub surfaceless: bool,
    /// The extensions a browser's own backend leans on, by name, and whether
    /// each is there.
    pub extensions: Vec<(String, bool)>,
    /// The nodes and libraries a browser's backend needs, by name, and
    /// whether each is there.
    pub pieces: Vec<(String, bool)>,
}

/// The extensions a browser's backend leans on, in the order the page shows
/// them.
const WANTED: [&str; 8] = [
    "EGL_KHR_surfaceless_context",
    "EGL_KHR_image_base",
    "EGL_KHR_fence_sync",
    "EGL_EXT_image_dma_buf_import",
    "EGL_ANDROID_image_native_buffer",
    "EGL_KHR_platform_gbm",
    "EGL_EXT_platform_base",
    "EGL_MESA_platform_gbm",
];

/// The nodes and libraries a browser's backend needs.
const PIECES: [&str; 6] = [
    "/dev/dri/card0",
    "/dev/dri/renderD128",
    "/dev/dma_heap/system",
    "libgbm.so",
    "/vendor/lib64/libgbm.so",
    "/system/lib64/libgbm.so",
];

// EGL's own names, which its header would give, were one linked against.
const EGL_DEFAULT_DISPLAY: *mut c_void = std::ptr::null_mut();
const EGL_NO_CONTEXT: *mut c_void = std::ptr::null_mut();
const EGL_NO_SURFACE: *mut c_void = std::ptr::null_mut();
const EGL_OPENGL_ES_API: u32 = 0x30A0;
const EGL_CONTEXT_CLIENT_VERSION: i32 = 0x3098;
const EGL_VENDOR: i32 = 0x3053;
const EGL_VERSION: i32 = 0x3054;
const EGL_EXTENSIONS: i32 = 0x3055;
const EGL_NONE: i32 = 0x3038;

#[cfg(target_os = "android")]
mod imp {
    use super::*;

    type PfnGetDisplay = unsafe extern "C" fn(*mut c_void) -> *mut c_void;
    type PfnInitialize = unsafe extern "C" fn(*mut c_void, *mut i32, *mut i32) -> u32;
    type PfnQueryString = unsafe extern "C" fn(*mut c_void, i32) -> *const c_char;
    type PfnGetError = unsafe extern "C" fn() -> i32;
    type PfnBindApi = unsafe extern "C" fn(u32) -> u32;
    type PfnCreateContext = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *const i32) -> *mut c_void;
    type PfnMakeCurrent = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *mut c_void) -> u32;
    type PfnGetString = unsafe extern "C" fn(u32) -> *const c_char;

    extern "C" {
        fn dlopen(name: *const c_char, flags: i32) -> *mut c_void;
        fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
        fn dlclose(handle: *mut c_void) -> i32;
        fn open(path: *const c_char, flags: i32, ...) -> i32;
        fn close(fd: i32) -> i32;
    }
    const RTLD_NOW: i32 = 2;
    const O_RDONLY: i32 = 0;

    /// Open `name`, the paths it may be found at first.
    fn open_library(paths: &[&str]) -> Option<(String, *mut c_void)> {
        for path in paths {
            let Ok(name) = CString::new(*path) else { continue };
            let handle = unsafe { dlopen(name.as_ptr(), RTLD_NOW) };
            if !handle.is_null() {
                return Some(((*path).to_string(), handle));
            }
        }
        None
    }

    fn symbol<T: Copy>(handle: *mut c_void, name: &str) -> Option<T> {
        let name = CString::new(name).ok()?;
        let at = unsafe { dlsym(handle, name.as_ptr()) };
        if at.is_null() {
            return None;
        }
        Some(unsafe { std::mem::transmute_copy::<*mut c_void, T>(&at) })
    }

    fn text(at: *const c_char) -> Option<String> {
        if at.is_null() {
            return None;
        }
        Some(unsafe { std::ffi::CStr::from_ptr(at) }.to_string_lossy().into_owned())
    }

    /// Whether the path can be opened for reading, or the library opened.
    fn piece(name: &str) -> bool {
        if name.ends_with(".so") {
            return open_library(&[name]).is_some();
        }
        let Ok(path) = CString::new(name) else { return false };
        let fd = unsafe { open(path.as_ptr(), O_RDONLY) };
        if fd < 0 {
            return false;
        }
        unsafe { close(fd) };
        true
    }

    pub fn probe() -> Gpu {
        let mut gpu = Gpu {
            pieces: PIECES.iter().map(|p| ((*p).to_string(), piece(p))).collect(),
            extensions: WANTED.iter().map(|e| ((*e).to_string(), false)).collect(),
            ..Gpu::default()
        };

        let Some((library, egl)) = open_library(&["libEGL.so", "/system/lib64/libEGL.so", "/vendor/lib64/libEGL.so"]) else {
            gpu.unavailable = Some("libEGL cannot be opened".into());
            return gpu;
        };
        gpu.library = Some(library);

        let Some(get_display) = symbol::<PfnGetDisplay>(egl, "eglGetDisplay") else {
            gpu.unavailable = Some("the library has no eglGetDisplay".into());
            return gpu;
        };
        let Some(initialize) = symbol::<PfnInitialize>(egl, "eglInitialize") else {
            gpu.unavailable = Some("the library has no eglInitialize".into());
            return gpu;
        };
        let query = symbol::<PfnQueryString>(egl, "eglQueryString");
        let get_error = symbol::<PfnGetError>(egl, "eglGetError");

        let display = unsafe { get_display(EGL_DEFAULT_DISPLAY) };
        if display.is_null() {
            let code = get_error.map(|f| unsafe { f() }).unwrap_or(0);
            gpu.unavailable = Some(format!("no display (EGL error 0x{code:x})"));
            return gpu;
        }
        let (mut major, mut minor) = (0i32, 0i32);
        if unsafe { initialize(display, &mut major, &mut minor) } == 0 {
            let code = get_error.map(|f| unsafe { f() }).unwrap_or(0);
            gpu.unavailable = Some(format!("eglInitialize fails (0x{code:x})"));
            return gpu;
        }
        if let Some(query) = query {
            gpu.vendor = text(unsafe { query(display, EGL_VENDOR) });
            gpu.version = text(unsafe { query(display, EGL_VERSION) }).map(|v| format!("{v} ({major}.{minor})"));
            let extensions = text(unsafe { query(display, EGL_EXTENSIONS) }).unwrap_or_default();
            for (name, there) in gpu.extensions.iter_mut() {
                *there = extensions.contains(name.as_str());
            }
        }

        // A context with no window, and GLES through it.
        if let (Some(bind), Some(create), Some(make_current)) = (
            symbol::<PfnBindApi>(egl, "eglBindAPI"),
            symbol::<PfnCreateContext>(egl, "eglCreateContext"),
            symbol::<PfnMakeCurrent>(egl, "eglMakeCurrent"),
        ) {
            unsafe { bind(EGL_OPENGL_ES_API) };
            let attributes = [EGL_CONTEXT_CLIENT_VERSION, 3, EGL_NONE];
            let mut context = unsafe { create(display, std::ptr::null_mut(), EGL_NO_CONTEXT, attributes.as_ptr()) };
            if context.is_null() {
                let attributes = [EGL_CONTEXT_CLIENT_VERSION, 2, EGL_NONE];
                context = unsafe { create(display, std::ptr::null_mut(), EGL_NO_CONTEXT, attributes.as_ptr()) };
            }
            if !context.is_null() {
                gpu.surfaceless = unsafe { make_current(display, EGL_NO_SURFACE, EGL_NO_SURFACE, context) } != 0;
                if let Some((_, gles)) = open_library(&["libGLESv2.so", "/system/lib64/libGLESv2.so"]) {
                    if let Some(get_string) = symbol::<PfnGetString>(gles, "glGetString") {
                        gpu.renderer = text(unsafe { get_string(0x1F01) });
                        gpu.gles = text(unsafe { get_string(0x1F02) });
                    }
                    unsafe { dlclose(gles) };
                }
            }
        }
        unsafe { dlclose(egl) };
        gpu
    }
}

#[cfg(not(target_os = "android"))]
mod imp {
    use super::*;
    pub fn probe() -> Gpu {
        Gpu::default()
    }
}

pub use imp::probe;

/// Whether the GPU answered at all.
impl Gpu {
    pub fn present(&self) -> bool {
        self.unavailable.is_none() && self.version.is_some()
    }
}

/// A short line for the page to put beside the GPU's name.
pub fn summary(gpu: &Gpu) -> String {
    match (&gpu.unavailable, &gpu.renderer, &gpu.gles) {
        (Some(why), _, _) => why.clone(),
        (None, Some(renderer), Some(gles)) => format!("{renderer} · {gles}"),
        (None, Some(renderer), None) => renderer.clone(),
        _ => "no answer".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_gpu_that_answered_nothing_is_not_present() {
        assert!(!Gpu::default().present());
        assert_eq!(summary(&Gpu::default()), "no answer");
    }

    #[test]
    fn a_gpu_with_a_version_is_present() {
        let gpu = Gpu { version: Some("1.5".into()), ..Gpu::default() };
        assert!(gpu.present());
    }

    #[test]
    fn the_summary_says_why_there_is_no_gpu() {
        let gpu = Gpu { unavailable: Some("libEGL cannot be opened".into()), ..Gpu::default() };
        assert_eq!(summary(&gpu), "libEGL cannot be opened");
    }

    #[test]
    fn the_paths_that_are_files_are_told_apart_from_the_libraries() {
        assert!(PIECES.iter().any(|p| p.ends_with(".so")));
        assert!(PIECES.iter().any(|p| p.starts_with('/')));
        let _ = Path::new("/dev/dri/card0");
    }
}
