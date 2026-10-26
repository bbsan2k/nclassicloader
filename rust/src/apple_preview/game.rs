//! Generated NES vertical slice. Core lifetime and all borrowed buffers stay on
//! the host thread. SDL copies audio; video is copied before the next core call.
use super::{Host, OwnedTexture, guard, host};
use crate::ares::AresSession;
use sdl3::{
    audio::{AudioFormat, AudioSpec, AudioStreamOwner},
    pixels::{Color, PixelFormat},
    render::{Canvas, FRect, ScaleMode},
    video::Window,
};
use std::{
    ffi::{CStr, c_char, c_void},
    path::Path,
    time::{Duration, Instant},
};

const FPS: f64 = 60.0988;
const MAX_STEPS: u32 = 3;

#[derive(Default)]
struct Clock {
    debt: f64,
}
impl Clock {
    fn advance(&mut self, elapsed: Duration) -> u32 {
        self.debt = (self.debt + elapsed.as_secs_f64() * FPS).min(f64::from(MAX_STEPS));
        let steps = self.debt.floor() as u32;
        self.debt -= f64::from(steps);
        steps
    }
}

pub(super) struct Game {
    texture: OwnedTexture,
    stream: AudioStreamOwner,
    _audio: sdl3::AudioSubsystem,
    session: AresSession,
    clock: Clock,
    last: Instant,
    started: bool,
    buttons: u16,
    frames: u64,
    changes: u64,
    hash: u64,
    samples: u64,
    nonzero: bool,
    max_queue: u32,
    dropped_audio: u64,
    drained_bytes: u64,
    previous_queue: Option<u32>,
    max_steps: u32,
    pauses: u32,
    rate: u32,
    pixels: Vec<u8>,
    width: u32,
    height: u32,
    nes: bool,
}
impl Game {
    fn new(host: &Host, rom: &Path, root: &Path) -> Result<Self, String> {
        Self::with_firmware(host, rom, root, root)
    }
    fn with_firmware(
        host: &Host,
        rom: &Path,
        root: &Path,
        firmware: &Path,
    ) -> Result<Self, String> {
        if !rom.is_absolute() || !root.is_absolute() {
            return Err("game paths must be absolute".into());
        }
        std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
        let mut session = AresSession::new().map_err(|e| e.to_string())?;
        session
            .load(rom, root, None, firmware)
            .map_err(|e| e.to_string())?;
        let rate = session.audio_rate();
        if rate != 48000 {
            return Err("unexpected core audio rate".into());
        }
        let audio = host._sdl.audio().map_err(|e| e.to_string())?;
        let spec = AudioSpec {
            freq: Some(rate as i32),
            channels: Some(2),
            format: Some(AudioFormat::f32_sys()),
        };
        let stream = audio
            .default_playback_device()
            .open_device_stream(Some(&spec))
            .map_err(|e| e.to_string())?;
        let nes = rom.extension().is_some_and(|ext| ext == "nes");
        let mut texture = OwnedTexture(Some(
            host.canvas
                .texture_creator()
                .create_texture_streaming(
                    PixelFormat::RGBA32,
                    if nes { 256 } else { 1024 },
                    if nes { 240 } else { 1024 },
                )
                .map_err(|e| e.to_string())?,
        ));
        texture.set_scale_mode(ScaleMode::Nearest);
        Ok(Self {
            texture,
            stream,
            _audio: audio,
            session,
            clock: Clock::default(),
            last: Instant::now(),
            started: false,
            buttons: 0,
            frames: 0,
            changes: 0,
            hash: 0,
            samples: 0,
            nonzero: false,
            max_queue: 0,
            dropped_audio: 0,
            drained_bytes: 0,
            previous_queue: None,
            max_steps: 0,
            pauses: 0,
            rate,
            pixels: vec![0; 256 * 240 * 4],
            width: 256,
            height: 240,
            nes,
        })
    }
    fn queued(&self) -> Result<u32, String> {
        self.stream
            .queued_bytes()
            .map(|n| n.max(0) as u32)
            .map_err(|e| e.to_string())
    }
    pub(super) fn tick(&mut self) -> Result<(), String> {
        let queued = self.queued()?;
        if self.started
            && let Some(previous) = self.previous_queue
        {
            self.drained_bytes += u64::from(previous.saturating_sub(queued));
        }
        let now = Instant::now();
        let steps = self.clock.advance(now.duration_since(self.last));
        self.last = now;
        self.max_steps = self.max_steps.max(steps);
        for _ in 0..steps {
            self.session
                .run_frame(self.buttons)
                .map_err(|e| e.to_string())?;
            self.frames += 1;
            let audio = self.session.audio();
            if !audio.len().is_multiple_of(2)
                || audio.iter().any(|v| !v.is_finite() || v.abs() > 1.0)
            {
                return Err("invalid core audio".into());
            }
            self.samples += audio.len() as u64;
            self.nonzero |= audio.iter().any(|v| v.abs() > 0.001);
            // Bound input-format buffering to 100 ms, even if playback stalls.
            let queued = self.queued()?;
            if queued as usize + std::mem::size_of_val(audio) <= self.rate as usize * 8 / 10 {
                self.stream.put_data_f32(audio).map_err(|e| e.to_string())?;
                self.max_queue = self.max_queue.max(self.queued()?);
            } else {
                self.dropped_audio += audio.len() as u64;
            }
            if !self.started && self.queued()? >= self.rate * 8 / 30 {
                self.stream.resume().map_err(|e| e.to_string())?;
                self.started = true;
            }
            let frame = self.session.frame().ok_or("missing core video")?;
            if frame.width == 0
                || frame.height == 0
                || frame.width > 1024
                || frame.height > 1024
                || (self.nes && (frame.width, frame.height) != (256, 240))
            {
                return Err("unexpected core geometry".into());
            }
            self.width = frame.width;
            self.height = frame.height;
            self.pixels
                .resize(self.width as usize * self.height as usize * 4, 0);
            let mut hash = 14695981039346656037_u64;
            for (word, rgba) in frame
                .pixels_argb
                .iter()
                .zip(self.pixels.chunks_exact_mut(4))
            {
                rgba.copy_from_slice(&[(word >> 16) as u8, (word >> 8) as u8, *word as u8, 255]);
                hash = (hash ^ u64::from(*word)).wrapping_mul(1099511628211);
            }
            if self.hash != 0 && self.hash != hash {
                self.changes += 1;
            }
            self.hash = hash;
        }
        if steps > 0 {
            self.texture
                .update(
                    Some(sdl3::rect::Rect::new(0, 0, self.width, self.height)),
                    &self.pixels,
                    self.width as usize * 4,
                )
                .map_err(|e| e.to_string())?;
        }
        self.previous_queue = Some(self.queued()?);
        Ok(())
    }
    pub(super) fn draw(&self, canvas: &mut Canvas<Window>) -> Result<(), String> {
        canvas.set_draw_color(Color::BLACK);
        canvas.clear();
        // NES 4:3 display aspect, centered in the shared 1280x720 stage.
        if self.frames > 0 {
            canvas
                .copy(
                    &self.texture,
                    sdl3::rect::Rect::new(0, 0, self.width, self.height),
                    FRect::new(160.0, 0.0, 960.0, 720.0),
                )
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    pub(super) fn pause(&mut self) -> Result<(), String> {
        self.buttons = 0;
        self.stream.pause().map_err(|e| e.to_string())?;
        self.stream.clear().map_err(|e| e.to_string())?;
        self.started = false;
        self.previous_queue = None;
        self.clock = Clock::default();
        self.pauses += 1;
        Ok(())
    }
    pub(super) fn resume(&mut self) {
        self.last = Instant::now();
        self.clock = Clock::default();
    }
    fn report(&self, host: &Host) -> Result<String, String> {
        Ok(serde_json::json!({"emulated_frames": self.frames, "video_changes": self.changes,
            "audio_samples": self.samples, "audio_nonzero": self.nonzero, "audio_started": self.started,
            "max_audio_queue_bytes": self.max_queue, "audio_queue_bytes": self.queued()?,
            "audio_limit_bytes": self.rate * 8 / 10, "dropped_audio_samples": self.dropped_audio,
            "audio_drained_bytes": self.drained_bytes,
            "paused": host.paused, "buttons": self.buttons, "generation": host.generation,
            "audio_interrupted": host.audio_interrupted,
            "presented_frames": host.frames,
            "max_steps_per_callback": self.max_steps, "pauses": self.pauses}).to_string())
    }
}

/// # Safety
/// Same handle/buffer contract as preview init. Paths are live UTF-8 C strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clover_apple_game_start(
    state: *mut c_void,
    rom: *const c_char,
    root: *const c_char,
    error: *mut c_char,
    capacity: usize,
) -> i32 {
    guard(error, capacity, || {
        if rom.is_null() || root.is_null() {
            return Err("null game path".into());
        }
        let host = unsafe { host(state)? };
        if host.game.is_some() {
            return Err("game already started".into());
        }
        let rom = unsafe { CStr::from_ptr(rom) }
            .to_str()
            .map_err(|e| e.to_string())?;
        let root = unsafe { CStr::from_ptr(root) }
            .to_str()
            .map_err(|e| e.to_string())?;
        host.game = Some(Game::new(host, Path::new(rom), Path::new(root))?);
        Ok(())
    })
}

/// # Safety
/// Host is live/exclusive on its creating thread; paths are live C strings and
/// the error buffer is writable. Only worker-validated profile paths are passed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clover_apple_profile_game_start(
    state: *mut c_void,
    rom: *const c_char,
    saves: *const c_char,
    firmware: *const c_char,
    error: *mut c_char,
    size: usize,
) -> i32 {
    guard(error, size, || {
        if rom.is_null() || saves.is_null() || firmware.is_null() {
            return Err("Null profile game path".into());
        }
        let host = unsafe { host(state)? };
        if host.game.is_some() {
            return Err("Stop the current game first".into());
        }
        let path = |p| {
            unsafe { CStr::from_ptr(p) }
                .to_str()
                .map(Path::new)
                .map_err(|e| e.to_string())
        };
        host.game = Some(Game::with_firmware(
            host,
            path(rom)?,
            path(saves)?,
            path(firmware)?,
        )?);
        host.paused = true;
        Ok(())
    })
}

/// # Safety
/// The handle is live, exclusive and on its creating thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clover_apple_game_stop(
    state: *mut c_void,
    error: *mut c_char,
    capacity: usize,
) -> i32 {
    guard(error, capacity, || {
        let host = unsafe { host(state)? };
        if let Some(game) = &mut host.game {
            game.pause()?;
            game.session.unload().map_err(|e| e.to_string())?;
        }
        host.game = None;
        host.paused = host.audio_interrupted;
        Ok(())
    })
}

/// # Safety
/// Paths are live UTF-8 C strings. Output/error buffers are writable, exclusive,
/// and nonoverlapping. This function has no access to the live host/session.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clover_apple_import_nes(
    source: *const c_char,
    library: *const c_char,
    json: *mut c_char,
    size: usize,
    error: *mut c_char,
    capacity: usize,
) -> i32 {
    guard(error, capacity, || {
        if source.is_null() || library.is_null() || json.is_null() || size < 4096 {
            return Err("import requires paths and a 4096-byte output buffer".into());
        }
        let source = unsafe { CStr::from_ptr(source) }
            .to_str()
            .map_err(|e| e.to_string())?;
        let library = unsafe { CStr::from_ptr(library) }
            .to_str()
            .map_err(|e| e.to_string())?;
        // Reject paths whose result cannot fit before publishing anything.
        if library.len() > 1024 {
            return Err("import library path is too long".into());
        }
        let placeholder = "0".repeat(64);
        let directory = Path::new(library).join(&placeholder);
        let largest_report = serde_json::json!({"rom": directory.join("game.nes"),
            "saves": directory.join("saves"), "digest": placeholder, "mapper": 184})
        .to_string();
        if largest_report.len() >= size {
            return Err("import report does not fit output buffer".into());
        }
        let prepared =
            crate::apple_import::import_cartridge(Path::new(source), Path::new(library))?;
        let report = serde_json::json!({"rom": prepared.rom, "saves": prepared.saves,
            "digest": prepared.digest, "mapper": prepared.mapper})
        .to_string();
        if report.len() >= size {
            return Err("import report does not fit output buffer".into());
        }
        super::error_out(json, size, &report);
        Ok(())
    })
}

/// # Safety
/// The handle is live, exclusive and on its creating thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clover_apple_game_buttons(state: *mut c_void, buttons: u16) -> i32 {
    guard(std::ptr::null_mut(), 0, || {
        let host = unsafe { host(state)? };
        let game = host.game.as_mut().ok_or("no game")?;
        game.buttons = if host.paused { 0 } else { buttons };
        Ok(())
    })
}

/// # Safety
/// Same handle/buffer contract as preview init; JSON output must fit the buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clover_apple_game_report(
    state: *mut c_void,
    json: *mut c_char,
    size: usize,
    error: *mut c_char,
    capacity: usize,
) -> i32 {
    guard(error, capacity, || {
        let host = unsafe { host(state)? };
        let report = host.game.as_ref().ok_or("no game")?.report(host)?;
        if json.is_null() || size <= report.len() {
            return Err("short game report buffer".into());
        }
        super::error_out(json, size, &report);
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pacing_is_refresh_independent_and_drops_suspend_debt() {
        for hz in [50, 60, 120] {
            let mut clock = Clock::default();
            let frames: u32 = (0..hz * 10)
                .map(|_| clock.advance(Duration::from_secs_f64(1.0 / f64::from(hz))))
                .sum();
            assert!((600..=601).contains(&frames));
        }
        let mut clock = Clock::default();
        assert_eq!(clock.advance(Duration::from_secs(60)), MAX_STEPS);
        assert_eq!(clock.advance(Duration::ZERO), 0);
    }

    #[test]
    #[ignore = "requires dummy/software SDL and caller-generated CLOVER_GENERATED_NES fixture"]
    fn generated_game_video_audio_lifecycle_and_teardown() {
        assert_eq!(std::env::var("SDL_VIDEODRIVER").as_deref(), Ok("dummy"));
        assert_eq!(std::env::var("SDL_AUDIODRIVER").as_deref(), Ok("dummy"));
        let rom = std::env::var("CLOVER_GENERATED_NES").expect("fixture path");
        let root = Path::new(&rom).parent().unwrap().join("game-test-saves");
        let mut host = super::super::build().unwrap();
        host.game = Some(Game::new(&host, Path::new(&rom), &root).unwrap());
        unsafe {
            let ptr = (&mut host as *mut Host).cast();
            let mut tiny = [42_i8; 1];
            assert_eq!(
                clover_apple_game_start(
                    ptr,
                    std::ptr::null(),
                    std::ptr::null(),
                    tiny.as_mut_ptr(),
                    1
                ),
                1
            );
            assert_eq!(tiny[0], 0);
            assert_eq!(
                clover_apple_game_report(ptr, tiny.as_mut_ptr(), 1, std::ptr::null_mut(), 0),
                1
            );
            assert_eq!(clover_apple_game_buttons(std::ptr::null_mut(), 1), 1);
        }
        let game = host.game.as_mut().unwrap();
        for index in 0..90 {
            game.buttons = if index / 10 % 2 == 0 { 0 } else { 1 };
            game.last = Instant::now() - Duration::from_millis(17);
            game.tick().unwrap();
        }
        assert!(game.frames >= 90);
        assert!(game.changes >= 2);
        assert!(game.nonzero && game.started);
        assert!(game.samples > 0);
        assert!(game.max_queue <= game.rate * 8 / 10);
        assert!(game.max_steps <= MAX_STEPS);
        game.pause().unwrap();
        assert_eq!(game.queued().unwrap(), 0);
        assert_eq!(game.buttons, 0);
        let frames = game.frames;
        host.paused = true;
        // Foreground notification alone neither draws nor restarts the core.
        unsafe {
            let ptr = (&mut host as *mut Host).cast();
            assert_eq!(super::super::clover_apple_preview_event(ptr, 2), 0);
            assert_eq!(
                super::super::clover_apple_preview_iterate(ptr, std::ptr::null_mut(), 0),
                0
            );
            assert_eq!(host.game.as_ref().unwrap().frames, frames);
            assert_eq!(super::super::clover_apple_preview_event(ptr, 3), 0);
        }
        let game = host.game.as_mut().unwrap();
        assert_eq!(game.clock.advance(Duration::ZERO), 0);
        game.buttons = 1;
        game.last = Instant::now() - Duration::from_secs(60);
        game.tick().unwrap();
        assert_eq!(game.frames - frames, u64::from(MAX_STEPS));
        super::super::draw(&mut host).unwrap();
        let b_stage = super::super::checksum(&mut host).unwrap();
        for _ in 0..4 {
            let game = host.game.as_mut().unwrap();
            game.buttons = 1 << 8;
            game.last = Instant::now() - Duration::from_millis(17);
            super::super::draw(&mut host).unwrap();
        }
        assert_ne!(super::super::checksum(&mut host).unwrap(), b_stage);
        let frozen = host.game.as_ref().unwrap().frames;
        unsafe {
            let ptr = (&mut host as *mut Host).cast();
            for kind in [4, 4, 3, 2, 3] {
                assert_eq!(super::super::clover_apple_preview_event(ptr, kind), 0);
                assert!(host.paused && host.audio_interrupted);
                assert_eq!(host.game.as_ref().unwrap().queued().unwrap(), 0);
                assert_eq!(
                    super::super::clover_apple_preview_iterate(ptr, std::ptr::null_mut(), 0),
                    0
                );
                assert_eq!(host.game.as_ref().unwrap().frames, frozen);
            }
            for _ in 0..2 {
                assert_eq!(super::super::clover_apple_preview_event(ptr, 5), 0);
                assert!(host.paused && !host.audio_interrupted);
            }
            assert_eq!(super::super::clover_apple_preview_event(ptr, 3), 0);
            assert!(!host.paused && !host.audio_interrupted);
            assert_eq!(host.game.as_mut().unwrap().clock.advance(Duration::ZERO), 0);
        }
        drop(host);
        // The singleton native core must have been released on the owner thread.
        let mut session = AresSession::new().unwrap();
        session.load(Path::new(&rom), &root, None, &root).unwrap();
    }
}
