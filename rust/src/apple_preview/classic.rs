use super::*;
use crate::frontend::menu_input::Key;
use std::{ffi::CStr, path::Path};

unsafe fn text<'a>(p: *const c_char) -> Result<&'a str, String> {
    if p.is_null() {
        return Err("Null Classic path".into());
    }
    unsafe { CStr::from_ptr(p) }
        .to_str()
        .map_err(|e| e.to_string())
}
/// # Safety
/// Owner-thread exclusive host; all paths are live UTF-8 C strings from a
/// freshly worker-validated profile. Buffers are writable and nonoverlapping.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clover_apple_classic_open(
    state: *mut c_void,
    assets: *const c_char,
    data: *const c_char,
    cache: *const c_char,
    home: *const c_char,
    error: *mut c_char,
    size: usize,
) -> i32 {
    guard(error, size, || {
        let host = unsafe { host(state)? };
        if host.game.is_some() || host.classic.is_some() {
            return Err("Close the current Classic collection first".into());
        }
        let [assets, data, cache, home] =
            unsafe { [text(assets)?, text(data)?, text(cache)?, text(home)?] }.map(Path::new);
        if [assets, data, cache, home].iter().any(|p| !p.is_absolute()) {
            return Err("Classic paths must be absolute".into());
        }
        host.classic = Some(crate::ui::apple::ClassicMenu::new(
            &host.canvas,
            &host._sdl,
            assets,
            data,
            cache,
            home,
        )?);
        host.paused = false;
        Ok(())
    })
}
/// # Safety
/// Live exclusive owner-thread host and writable error buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clover_apple_classic_close(
    state: *mut c_void,
    error: *mut c_char,
    size: usize,
) -> i32 {
    guard(error, size, || {
        let host = unsafe { host(state)? };
        if host.game.is_some() {
            return Err("Stop the game before closing the collection".into());
        }
        if let Some(menu) = &mut host.classic {
            menu.finish()?;
        }
        host.classic = None;
        host.paused = false;
        Ok(())
    })
}
/// # Safety
/// Live exclusive owner-thread host; the native input reducer already applies
/// foreground/focus/interruption blockers and merges independent sources.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clover_apple_classic_input(
    state: *mut c_void,
    mask: u16,
    key: u32,
) -> i32 {
    guard(std::ptr::null_mut(), 0, || {
        let host = unsafe { host(state)? };
        if host.game.is_some() || host.audio_interrupted {
            return Ok(());
        }
        if let Some(menu) = &mut host.classic {
            if host.paused {
                menu.block(false);
                host.paused = false;
            }
            menu.mask(mask, &host.canvas);
            if let Some(key) = match key {
                1 => Some(Key::Back),
                2 => Some(Key::Next),
                3 => Some(Key::Confirm),
                4 => Some(Key::Right),
                5 => Some(Key::Left),
                _ => None,
            } {
                menu.key(key, false, &host.canvas);
            }
        }
        Ok(())
    })
}
/// # Safety
/// Live owner-thread host and writable buffers; normalized coordinates belong
/// to the current SDL window. No OS path or worker state is captured in a hit.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clover_apple_classic_point(
    state: *mut c_void,
    window: u32,
    x: f32,
    y: f32,
) -> i32 {
    guard(std::ptr::null_mut(), 0, || {
        let host = unsafe { host(state)? };
        if host.game.is_some()
            || host.paused
            || window != host.canvas.window().id()
            || !x.is_finite()
            || !y.is_finite()
            || !(0.0..=1.0).contains(&x)
            || !(0.0..=1.0).contains(&y)
        {
            return Ok(());
        }
        let (w, h) = host.canvas.output_size().map_err(|e| e.to_string())?;
        let scale = (w as f32 / 1280.0).min(h as f32 / 720.0);
        if scale > 0.0
            && let Some(menu) = &mut host.classic
        {
            menu.point(
                (x * w as f32 - (w as f32 - 1280.0 * scale) * 0.5) / scale,
                (y * h as f32 - (h as f32 - 720.0 * scale) * 0.5) / scale,
                &host.canvas,
            );
        }
        Ok(())
    })
}
/// # Safety
/// Live owner-thread host, writable/exclusive output (at least 8192 bytes).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clover_apple_classic_action(
    state: *mut c_void,
    json: *mut c_char,
    size: usize,
    error: *mut c_char,
    capacity: usize,
) -> i32 {
    guard(error, capacity, || {
        if json.is_null() || size < 8192 {
            return Err("Short Classic action output".into());
        }
        let host = unsafe { host(state)? };
        let action = if host.game.is_none() && !host.paused {
            host.classic
                .as_mut()
                .map(|m| m.take_action())
                .unwrap_or_default()
        } else {
            serde_json::Value::Null
        };
        error_out(json, size, &action.to_string());
        Ok(())
    })
}
/// # Safety
/// Live exclusive owner-thread host; text is a live C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clover_apple_classic_error(
    state: *mut c_void,
    message: *const c_char,
) -> i32 {
    guard(std::ptr::null_mut(), 0, || {
        if let Some(menu) = &mut unsafe { host(state)? }.classic {
            menu.error(unsafe { text(message)? }.to_owned());
        }
        Ok(())
    })
}
/// # Safety
/// Live exclusive owner-thread host and writable output (at least 8192 bytes).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clover_apple_classic_report(
    state: *mut c_void,
    json: *mut c_char,
    size: usize,
    error: *mut c_char,
    capacity: usize,
) -> i32 {
    guard(error, capacity, || {
        if json.is_null() || size < 8192 {
            return Err("Short Classic report output".into());
        }
        let host = unsafe { host(state)? };
        let mut report = host
            .classic
            .as_ref()
            .map(|m| m.report())
            .unwrap_or_else(|| serde_json::json!({"kind":"none"}));
        report["visible"] =
            serde_json::json!(host.classic.is_some() && host.game.is_none() && !host.paused);
        report["checksum"] = serde_json::json!(if host.classic.is_some() {
            checksum(host)?.to_string()
        } else {
            "0".into()
        });
        let text = report.to_string();
        if text.len() >= size {
            return Err("Classic report too long".into());
        }
        error_out(json, size, &text);
        Ok(())
    })
}
