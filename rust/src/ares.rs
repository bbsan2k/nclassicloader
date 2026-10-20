//! Safe, thread-affine wrapper around the narrow native ares C ABI.

use std::{
    ffi::{CStr, CString, c_char, c_int},
    marker::PhantomData,
    path::Path,
    ptr::NonNull,
    rc::Rc,
};

#[repr(C)]
struct CloverAres {
    _opaque: [u8; 0],
}

unsafe extern "C" {
    fn clover_ares_create() -> *mut CloverAres;
    fn clover_ares_destroy(core: *mut CloverAres);
    fn clover_ares_load(
        core: *mut CloverAres,
        rom: *const c_char,
        saves: *const c_char,
        coprocessor_firmware: *const c_char,
        system_firmware: *const c_char,
    ) -> c_int;
    fn clover_ares_run_frame(core: *mut CloverAres, buttons: u16) -> c_int;
    fn clover_ares_unload(core: *mut CloverAres);
    fn clover_ares_loaded(core: *const CloverAres) -> c_int;
    fn clover_ares_frame(
        core: *const CloverAres,
        pixel_count: *mut usize,
        width: *mut u32,
        height: *mut u32,
    ) -> *const u32;
    fn clover_ares_audio(core: *const CloverAres, float_count: *mut usize) -> *const f32;
    fn clover_ares_audio_rate(core: *const CloverAres) -> u32;
    fn clover_ares_save_state(core: *mut CloverAres, filename: *const c_char) -> c_int;
    fn clover_ares_load_state(core: *mut CloverAres, filename: *const c_char) -> c_int;
    fn clover_ares_error(core: *const CloverAres) -> *const c_char;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AresError(pub String);

impl std::fmt::Display for AresError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for AresError {}

pub struct VideoFrame<'a> {
    pub pixels_argb: &'a [u32],
    pub width: u32,
    pub height: u32,
}

pub struct AresSession {
    handle: NonNull<CloverAres>,
    // ares's machine and save-state API are bound to the creating thread.
    _thread_affinity: PhantomData<Rc<()>>,
}

impl AresSession {
    pub fn new() -> Result<Self, AresError> {
        // SAFETY: creation owns a new opaque handle and does not retain pointers.
        let handle = unsafe { clover_ares_create() };
        let handle =
            NonNull::new(handle).ok_or_else(|| AresError("Cannot allocate ares session".into()))?;
        Ok(Self {
            handle,
            _thread_affinity: PhantomData,
        })
    }

    pub fn load(
        &mut self,
        rom: &Path,
        saves: &Path,
        coprocessor_firmware: Option<&Path>,
        system_firmware: &Path,
    ) -> Result<(), AresError> {
        let rom = utf8_path(rom)?;
        let saves = utf8_path(saves)?;
        let system = utf8_path(system_firmware)?;
        let coprocessor = coprocessor_firmware.map(utf8_path).transpose()?;
        // SAFETY: C strings live across the call; the native bridge copies paths
        // immediately and retains no Rust-owned pointers.
        let result = unsafe {
            clover_ares_load(
                self.handle.as_ptr(),
                rom.as_ptr(),
                saves.as_ptr(),
                coprocessor
                    .as_ref()
                    .map_or(std::ptr::null(), |v| v.as_ptr()),
                system.as_ptr(),
            )
        };
        self.result(result)
    }

    pub fn run_frame(&mut self, buttons: u16) -> Result<(), AresError> {
        // SAFETY: handle is exclusively borrowed and owned by this session.
        let result = unsafe { clover_ares_run_frame(self.handle.as_ptr(), buttons) };
        self.result(result)
    }

    pub fn is_loaded(&self) -> bool {
        // SAFETY: read-only query on the owned handle.
        unsafe { clover_ares_loaded(self.handle.as_ptr()) != 0 }
    }

    pub fn frame(&self) -> Option<VideoFrame<'_>> {
        let (mut count, mut width, mut height) = (0, 0, 0);
        // SAFETY: the returned view is tied to `&self`, preventing another
        // mutable session operation for its lifetime.
        let data =
            unsafe { clover_ares_frame(self.handle.as_ptr(), &mut count, &mut width, &mut height) };
        if data.is_null() || count == 0 || width == 0 || height == 0 {
            return None;
        }
        let expected = (width as usize).checked_mul(height as usize)?;
        if count != expected {
            return None;
        }
        // SAFETY: native storage owns `count` initialized pixels and remains
        // stable while this immutable borrow prevents stepping/unloading.
        let pixels_argb = unsafe { std::slice::from_raw_parts(data, count) };
        Some(VideoFrame {
            pixels_argb,
            width,
            height,
        })
    }

    pub fn audio(&self) -> &[f32] {
        let mut count = 0;
        // SAFETY: same stable-borrow rule as `frame`.
        let data = unsafe { clover_ares_audio(self.handle.as_ptr(), &mut count) };
        if data.is_null() || count == 0 {
            return &[];
        }
        // SAFETY: native storage owns `count` initialized floats until the next
        // mutable session operation.
        unsafe { std::slice::from_raw_parts(data, count) }
    }

    pub fn audio_rate(&self) -> u32 {
        // SAFETY: read-only query on the owned handle.
        unsafe { clover_ares_audio_rate(self.handle.as_ptr()) }
    }

    pub fn save_state(&mut self, filename: &Path) -> Result<(), AresError> {
        let filename = utf8_path(filename)?;
        // SAFETY: path is a valid NUL-terminated string for the duration of call.
        let result = unsafe { clover_ares_save_state(self.handle.as_ptr(), filename.as_ptr()) };
        self.result(result)
    }

    pub fn load_state(&mut self, filename: &Path) -> Result<(), AresError> {
        let filename = utf8_path(filename)?;
        // SAFETY: path is a valid NUL-terminated string for the duration of call.
        let result = unsafe { clover_ares_load_state(self.handle.as_ptr(), filename.as_ptr()) };
        self.result(result)
    }

    pub fn unload(&mut self) -> Result<(), AresError> {
        // SAFETY: handle is exclusively borrowed and owned by this session.
        unsafe { clover_ares_unload(self.handle.as_ptr()) };
        self.last_error()
    }

    fn result(&self, success: c_int) -> Result<(), AresError> {
        if success != 0 {
            Ok(())
        } else {
            Err(self
                .last_error()
                .err()
                .unwrap_or_else(|| AresError("ares operation failed".into())))
        }
    }

    fn last_error(&self) -> Result<(), AresError> {
        // SAFETY: native error text remains owned by handle and is NUL terminated.
        let message = unsafe { clover_ares_error(self.handle.as_ptr()) };
        if message.is_null() {
            return Err(AresError("Unknown native ares error".into()));
        }
        // SAFETY: the bridge returns a valid C string for the handle lifetime.
        let message = unsafe { CStr::from_ptr(message) }
            .to_string_lossy()
            .into_owned();
        if message.is_empty() {
            Ok(())
        } else {
            Err(AresError(message))
        }
    }
}

impl Drop for AresSession {
    fn drop(&mut self) {
        // SAFETY: uniquely owned and dropped exactly once. Native destructor
        // unloads the machine and persists save memory.
        unsafe { clover_ares_destroy(self.handle.as_ptr()) };
    }
}

fn utf8_path(path: &Path) -> Result<CString, AresError> {
    let path = path
        .to_str()
        .ok_or_else(|| AresError("Filesystem path cannot be represented as UTF-8".into()))?;
    CString::new(path).map_err(|_| AresError("Filesystem path contains a NUL byte".into()))
}
