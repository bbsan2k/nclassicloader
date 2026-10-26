//! Shared-scene probe with optional sandboxed generated-NES video/audio mode.
//! This is not a complete frontend or real-content importer.
//! The embedding app exclusively owns each handle on its creating OS thread.

#[cfg(feature = "apple-game")]
mod classic;
#[cfg(feature = "apple-game")]
mod game;
#[cfg(feature = "apple-game")]
mod library;

use crate::frontend::{
    font::{PlacedGlyph, TextLayout},
    menu_carousel::{self, CardInput, CardResources, ResumeState},
    menu_chrome::{self, HudPrompt},
    menu_frame::{self, BaseMenuInput, BaseMenuScene, GameKey, Step},
    menu_scene::{self, AtlasSprite, Command, ImageSource, Rect, Rgba, TextureId},
    menu_toolbar::{self, ToolbarInput, ToolbarItemResources},
};
use sdl3::{
    pixels::{Color, PixelFormat},
    render::{BlendMode, Canvas, FRect, ScaleMode, Texture, TextureAccess},
    video::Window,
};
use std::{
    collections::BTreeMap,
    ffi::{c_char, c_void},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::atomic::{AtomicUsize, Ordering},
    thread::ThreadId,
};

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 720;
static NEXT_GENERATION: AtomicUsize = AtomicUsize::new(1);
const WHITE: Rgba = Rgba {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
};
const LETTERS: &[(char, [u8; 7])] = &[
    ('A', [14, 17, 17, 31, 17, 17, 17]),
    ('C', [14, 17, 16, 16, 16, 17, 14]),
    ('E', [31, 16, 16, 30, 16, 16, 31]),
    ('I', [31, 4, 4, 4, 4, 4, 31]),
    ('L', [16, 16, 16, 16, 16, 16, 31]),
    ('M', [17, 27, 21, 21, 17, 17, 17]),
    ('N', [17, 25, 25, 21, 19, 19, 17]),
    ('O', [14, 17, 17, 17, 17, 17, 14]),
    ('P', [30, 17, 17, 30, 16, 16, 16]),
    ('R', [30, 17, 17, 30, 20, 18, 17]),
    ('S', [15, 16, 16, 14, 1, 1, 30]),
    ('T', [31, 4, 4, 4, 4, 4, 4]),
    ('U', [17, 17, 17, 17, 17, 17, 14]),
    ('V', [17, 17, 17, 17, 17, 10, 4]),
    ('W', [17, 17, 17, 21, 21, 27, 17]),
    ('Y', [17, 17, 10, 4, 4, 4, 4]),
];

// Covers construction failure as well as normal shutdown with unsafe_textures.
struct OwnedTexture(Option<Texture>);
impl std::ops::Deref for OwnedTexture {
    type Target = Texture;
    fn deref(&self) -> &Texture {
        self.0.as_ref().unwrap()
    }
}
impl std::ops::DerefMut for OwnedTexture {
    fn deref_mut(&mut self) -> &mut Texture {
        self.0.as_mut().unwrap()
    }
}
impl Drop for OwnedTexture {
    fn drop(&mut self) {
        // SAFETY: this wrapper owns the unique SDL texture and never aliases ownership.
        if let Some(texture) = self.0.take() {
            unsafe { texture.destroy() };
        }
    }
}

struct Host {
    #[cfg(feature = "apple-game")]
    classic: Option<crate::ui::apple::ClassicMenu>,
    #[cfg(feature = "apple-game")]
    library: Option<library::Catalog>,
    #[cfg(feature = "apple-game")]
    library_atlas: Option<OwnedTexture>,
    #[cfg(feature = "apple-game")]
    library_presented_revision: u64,
    #[cfg(feature = "apple-game")]
    game: Option<game::Game>,
    stage: OwnedTexture,
    atlas: OwnedTexture,
    canvas: Canvas<Window>,
    _video: sdl3::VideoSubsystem,
    _sdl: sdl3::Sdl,
    owner: ThreadId,
    generation: usize,
    scene: BaseMenuScene,
    frames: u32,
    paused: bool,
    audio_interrupted: bool,
}

fn pattern() -> Vec<u8> {
    let mut rgba = vec![0; 128 * 64 * 4];
    for (index, (_, rows)) in LETTERS.iter().enumerate() {
        for (y, mask) in rows.iter().enumerate() {
            for x in 0..5 {
                if mask & (1 << (4 - x)) != 0 {
                    let pixel = (y * 128 + index * 6 + x) * 4;
                    rgba[pixel..pixel + 4].copy_from_slice(&[255; 4]);
                }
            }
        }
    }
    for y in 16..64 {
        for x in 0..128 {
            let pixel = (y * 128 + x) * 4;
            rgba[pixel..pixel + 4].copy_from_slice(if (x / 8 + y / 8) % 2 == 0 {
                &[36, 80, 160, 255]
            } else {
                &[28, 150, 145, 255]
            });
        }
    }
    rgba
}

fn layout(text: &str) -> TextLayout {
    let mut glyphs = Vec::new();
    let mut x = 0.0;
    for letter in text.chars() {
        if let Some(index) = LETTERS.iter().position(|(ch, _)| *ch == letter) {
            glyphs.push(PlacedGlyph {
                font: "preview".into(),
                source: Rect {
                    x: (index * 6) as f32,
                    y: 0.0,
                    w: 5.0,
                    h: 7.0,
                },
                x,
                y: 0.0,
                width: 20.0,
                height: 28.0,
            });
        }
        x += 24.0;
    }
    TextLayout {
        glyphs,
        advance: x,
        line: 36.0,
        left: 0.0,
        top: 0.0,
        right: x,
        bottom: 28.0,
    }
}

fn scene(generation: usize) -> BaseMenuScene {
    let id = TextureId {
        generation,
        slot: 0,
    };
    let sprite = AtlasSprite {
        texture: id,
        source: Rect {
            x: 0.0,
            y: 16.0,
            w: 16.0,
            h: 16.0,
        },
        flip_x: false,
    };
    let image = ImageSource {
        texture: id,
        width: 128,
        height: 64,
    };
    let fonts = BTreeMap::from([("preview".into(), id)]);
    let cards = [340.0, 640.0, 940.0]
        .into_iter()
        .enumerate()
        .map(|(index, x)| {
            let plan = menu_carousel::card(CardInput {
                nes: false,
                x,
                shift: 0.0,
                active: index == 1,
                firmware_locked: false,
                players: 2,
                simultaneous: true,
                slots: [
                    ResumeState::Empty,
                    ResumeState::Saved,
                    ResumeState::Locked,
                    ResumeState::Empty,
                ],
            });
            let sprites = plan
                .parts
                .iter()
                .filter_map(|part| match part {
                    menu_carousel::CardPart::Sprite(placement) => {
                        Some((placement.name.clone(), sprite))
                    }
                    _ => None,
                })
                .collect();
            (
                GameKey(format!("synthetic:{index}")),
                menu_carousel::card_scene(
                    &plan,
                    &CardResources {
                        sprites,
                        cover: Some(image),
                    },
                ),
            )
        })
        .collect();
    let selection = menu_scene::selection(
        false,
        Rect {
            x: 640.0,
            y: 360.0,
            w: 278.0,
            h: 302.0,
        },
        0.5,
    );
    let sources = selection
        .iter()
        .map(|placement| (placement.name.clone(), sprite))
        .collect();
    let prompts = ["MENU", "SAVE", "SORT", "PLAY"].map(|label| HudPrompt {
        icon: Some(sprite),
        icon_width: 36.0,
        layout: layout(label),
    });
    let base = menu_frame::compose(BaseMenuInput {
        generation,
        wallpaper: menu_scene::wallpaper(
            false,
            Rgba {
                r: 18,
                g: 24,
                b: 42,
                a: 255,
            },
            None,
            0.0,
        ),
        title: menu_carousel::title_scene(
            false,
            0.0,
            &layout("CLOVER APPLE PREVIEW"),
            &fonts,
            Some(sprite),
            WHITE,
        ),
        cards,
        selection: menu_scene::sprite_scene(&selection, &sources),
        arrows: menu_carousel::arrows(
            0.0,
            Some(sprite),
            Some(AtlasSprite {
                flip_x: true,
                ..sprite
            }),
        ),
        thumbnails: menu_scene::thumbnails(3, 1)
            .into_iter()
            .map(|placement| {
                (
                    GameKey(format!("synthetic:{}", placement.index)),
                    menu_chrome::thumbnail_scene(placement, Some(image), Some(sprite), None),
                )
            })
            .collect(),
        hud: menu_chrome::hud_scene(false, 8.0, 24.0, &prompts, &fonts, WHITE),
        bars: menu_chrome::menu_bars(false, false, Some(sprite), Some(sprite), None),
    });
    let plans = menu_toolbar::toolbar(&ToolbarInput {
        hover: "toolbar:0".into(),
        panel: String::new(),
        keyboard_focus: false,
        keyboard_index: 0,
    });
    let resources = std::array::from_fn(|_| ToolbarItemResources {
        active: Some(sprite),
        icon: Some(sprite),
        tooltip: vec![],
    });
    menu_frame::with_toolbar(
        base,
        menu_toolbar::toolbar_scene(generation, &plans, &resources),
    )
    .unwrap()
}

fn build() -> Result<Host, String> {
    let generation = NEXT_GENERATION
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_add(1))
        .map_err(|_| "preview generation exhausted")?;
    let sdl = sdl3::init().map_err(|e| e.to_string())?;
    let video = sdl.video().map_err(|e| e.to_string())?;
    let window = video
        .window("Clover Apple shared-scene preview", WIDTH, HEIGHT)
        .resizable()
        .build()
        .map_err(|e| e.to_string())?;
    let mut canvas = window.into_canvas();
    canvas.set_blend_mode(BlendMode::Blend);
    let creator = canvas.texture_creator();
    let mut atlas = OwnedTexture(Some(
        creator
            .create_texture_static(PixelFormat::RGBA32, 128, 64)
            .map_err(|e| e.to_string())?,
    ));
    atlas
        .update(None, &pattern(), 128 * 4)
        .map_err(|e| e.to_string())?;
    atlas.set_blend_mode(BlendMode::Blend);
    let mut stage = OwnedTexture(Some(
        creator
            .create_texture(PixelFormat::RGBA8888, TextureAccess::Target, WIDTH, HEIGHT)
            .map_err(|e| e.to_string())?,
    ));
    stage.set_blend_mode(BlendMode::None);
    stage.set_scale_mode(ScaleMode::Linear);
    Ok(Host {
        #[cfg(feature = "apple-game")]
        classic: None,
        #[cfg(feature = "apple-game")]
        library: None,
        #[cfg(feature = "apple-game")]
        library_atlas: None,
        #[cfg(feature = "apple-game")]
        library_presented_revision: 0,
        #[cfg(feature = "apple-game")]
        game: None,
        stage,
        atlas,
        canvas,
        _video: video,
        _sdl: sdl,
        owner: std::thread::current().id(),
        generation,
        scene: scene(generation),
        frames: 0,
        paused: false,
        audio_interrupted: false,
    })
}

fn draw(host: &mut Host) -> Result<(), String> {
    #[cfg(feature = "apple-game")]
    if let Some(game) = &mut host.game {
        game.tick()?;
        let mut result = Ok(());
        host.canvas
            .with_texture_canvas(&mut host.stage, |canvas| result = game.draw(canvas))
            .map_err(|e| e.to_string())?;
        result?;
        return present(host);
    }
    #[cfg(feature = "apple-game")]
    if let Some(menu) = &mut host.classic {
        let mut result = Ok(());
        host.canvas
            .with_texture_canvas(&mut host.stage, |canvas| {
                canvas.set_draw_color(Color::BLACK);
                canvas.clear();
                result = menu.draw(canvas);
            })
            .map_err(|e| e.to_string())?;
        result?;
        return present(host);
    }
    if host.scene.generation != host.generation {
        return Err("foreign preview scene".into());
    }
    host.canvas
        .with_texture_canvas(&mut host.stage, |canvas| {
            canvas.set_draw_color(Color::BLACK);
            canvas.clear();
            for step in &host.scene.steps {
                if let Step::Draw(command) = step {
                    let texture = match command {
                        Command::Texture(quad)
                            if quad.texture
                                == (TextureId {
                                    generation: host.generation,
                                    slot: 0,
                                }) =>
                        {
                            Some(&mut *host.atlas)
                        }
                        #[cfg(feature = "apple-game")]
                        Command::Texture(quad)
                            if quad.texture
                                == (TextureId {
                                    generation: host.generation,
                                    slot: 1,
                                }) =>
                        {
                            host.library_atlas.as_deref_mut()
                        }
                        _ => None,
                    };
                    crate::sdl_renderer::draw_command(canvas, command, texture);
                }
            }
            // Hits are presentation only: this probe never binds or executes actions.
        })
        .map_err(|e| e.to_string())?;
    present(host)
}

fn present(host: &mut Host) -> Result<(), String> {
    let (width, height) = host.canvas.output_size().map_err(|e| e.to_string())?;
    let scale = (width as f32 / WIDTH as f32).min(height as f32 / HEIGHT as f32);
    let destination = FRect::new(
        (width as f32 - WIDTH as f32 * scale) * 0.5,
        (height as f32 - HEIGHT as f32 * scale) * 0.5,
        WIDTH as f32 * scale,
        HEIGHT as f32 * scale,
    );
    host.canvas.set_draw_color(Color::BLACK);
    host.canvas.clear();
    host.canvas
        .copy(&host.stage, None, destination)
        .map_err(|e| e.to_string())?;
    host.canvas.present();
    #[cfg(feature = "apple-game")]
    if host.game.is_none()
        && let Some(library) = &host.library
    {
        host.library_presented_revision = library.revision();
    }
    host.frames = host
        .frames
        .checked_add(1)
        .ok_or("preview frame counter exhausted")?;
    Ok(())
}

fn checksum(host: &mut Host) -> Result<u64, String> {
    let mut pixels = None;
    host.canvas
        .with_texture_canvas(&mut host.stage, |canvas| {
            pixels = Some(canvas.read_pixels(None))
        })
        .map_err(|e| e.to_string())?;
    let surface = pixels
        .ok_or("missing stage readback")?
        .map_err(|e| e.to_string())?
        .convert_format(PixelFormat::RGBA32)
        .map_err(|e| e.to_string())?;
    if (surface.width(), surface.height()) != (WIDTH, HEIGHT) {
        return Err("wrong stage size".into());
    }
    let pitch = surface.pitch() as usize;
    if pitch < WIDTH as usize * 4 {
        return Err("invalid readback pitch".into());
    }
    let mut hash = 14695981039346656037_u64;
    let mut nonblack = false;
    surface.with_lock(|bytes| {
        for y in 0..HEIGHT as usize {
            let row = &bytes[y * pitch..y * pitch + WIDTH as usize * 4];
            for pixel in row.chunks_exact(4) {
                nonblack |= pixel[..3].iter().any(|channel| *channel != 0);
                for byte in pixel {
                    hash = (hash ^ u64::from(*byte)).wrapping_mul(1099511628211);
                }
            }
        }
    });
    if !nonblack || hash == 0 {
        return Err("empty preview readback".into());
    }
    Ok(hash)
}

fn error_out(dst: *mut c_char, capacity: usize, message: &str) {
    if dst.is_null() || capacity == 0 {
        return;
    }
    let count = message.len().min(capacity - 1);
    // SAFETY: the ABI caller supplies a writable buffer of capacity bytes.
    unsafe {
        std::ptr::copy_nonoverlapping(message.as_ptr(), dst.cast(), count);
        *dst.add(count) = 0;
    }
}
fn guard(
    error: *mut c_char,
    capacity: usize,
    operation: impl FnOnce() -> Result<(), String>,
) -> i32 {
    error_out(error, capacity, "");
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(Ok(())) => 0,
        Ok(Err(message)) => {
            error_out(error, capacity, &message);
            1
        }
        Err(_) => {
            error_out(error, capacity, "Rust preview panic");
            1
        }
    }
}
unsafe fn host<'a>(state: *mut c_void) -> Result<&'a mut Host, String> {
    if state.is_null() {
        return Err("null preview handle".into());
    }
    // SAFETY: the caller supplies a live, exclusively accessed handle from init.
    let host = unsafe { &mut *state.cast::<Host>() };
    if host.owner != std::thread::current().id() {
        return Err("non-owner preview thread".into());
    }
    Ok(host)
}

/// # Safety
/// Outputs must be writable; every successful handle is exclusively owned by the
/// caller and must be used/destroyed on the creating thread, with no concurrent calls.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clover_apple_preview_init(
    out: *mut *mut c_void,
    error: *mut c_char,
    capacity: usize,
) -> i32 {
    guard(error, capacity, || {
        if out.is_null() {
            return Err("null output handle".into());
        }
        unsafe {
            *out = std::ptr::null_mut();
        }
        let value = build()?;
        unsafe {
            *out = Box::into_raw(Box::new(value)).cast();
        }
        Ok(())
    })
}

/// # Safety
/// State and error buffer must satisfy init's lifetime, thread and exclusivity contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clover_apple_preview_iterate(
    state: *mut c_void,
    error: *mut c_char,
    capacity: usize,
) -> i32 {
    guard(error, capacity, || {
        let host = unsafe { host(state)? };
        if !host.paused {
            draw(host)?;
        }
        Ok(())
    })
}

/// # Safety
/// State must satisfy init's lifetime, thread and exclusivity contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clover_apple_preview_event(state: *mut c_void, kind: u32) -> i32 {
    guard(std::ptr::null_mut(), 0, || {
        let host = unsafe { host(state)? };
        #[cfg(feature = "apple-game")]
        if let Some(game) = &mut host.game {
            match kind {
                1 | 4 => game.pause()?,
                3 if host.paused && !host.audio_interrupted => game.resume(),
                _ => {}
            }
        }
        match kind {
            1 => host.paused = true,
            2 => {}
            3 if !host.audio_interrupted => host.paused = false,
            3 => {}
            4 => {
                host.audio_interrupted = true;
                host.paused = true;
            }
            5 => host.audio_interrupted = false,
            _ => return Err("invalid preview event".into()),
        }
        #[cfg(feature = "apple-game")]
        if let Some(menu) = &mut host.classic {
            if matches!(kind, 1 | 4) {
                menu.block(true);
            } else if matches!(kind, 2 | 3 | 5) && !host.audio_interrupted {
                menu.block(false);
                if host.game.is_none() {
                    host.paused = false;
                }
            }
        }
        Ok(())
    })
}

/// # Safety
/// State must satisfy init's contract; output pointers and error buffer must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clover_apple_preview_probe(
    state: *mut c_void,
    frames: *mut u32,
    result: *mut u64,
    error: *mut c_char,
    capacity: usize,
) -> i32 {
    guard(error, capacity, || {
        if frames.is_null() || result.is_null() {
            return Err("null probe output".into());
        }
        let host = unsafe { host(state)? };
        if host.frames < 3 {
            return Err("three frames required".into());
        }
        let hash = checksum(host)?;
        unsafe {
            *frames = host.frames;
            *result = hash;
        }
        Ok(())
    })
}

/// # Safety
/// A non-null handle must be live and exclusively owned on its creating thread.
/// Consume it exactly once. Null is a no-op; a wrong-thread call does not destroy it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clover_apple_preview_quit(state: *mut c_void) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !state.is_null() && unsafe { host(state) }.is_ok() {
            unsafe {
                drop(Box::from_raw(state.cast::<Host>()));
            }
        }
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires caller-provided SDL_VIDEODRIVER=dummy and SDL_RENDER_DRIVER=software"]
    fn shared_scene_callback_abi_and_lifecycle() {
        assert_eq!(std::env::var("SDL_VIDEODRIVER").as_deref(), Ok("dummy"));
        assert_eq!(
            std::env::var("SDL_RENDER_DRIVER").as_deref(),
            Ok("software")
        );
        let mut state = std::ptr::null_mut();
        let mut error = [0; 256];
        let (mut frames, mut hash) = (0, 0);
        unsafe {
            assert_eq!(
                clover_apple_preview_init(std::ptr::null_mut(), error.as_mut_ptr(), error.len()),
                1
            );
            assert_eq!(
                clover_apple_preview_iterate(state, error.as_mut_ptr(), error.len()),
                1
            );
            assert_eq!(
                clover_apple_preview_init(&mut state, error.as_mut_ptr(), error.len()),
                0
            );
            assert_eq!(
                clover_apple_preview_probe(
                    state,
                    &mut frames,
                    &mut hash,
                    error.as_mut_ptr(),
                    error.len()
                ),
                1
            );
            for _ in 0..3 {
                assert_eq!(
                    clover_apple_preview_iterate(state, error.as_mut_ptr(), error.len()),
                    0
                );
            }
            assert_eq!(
                clover_apple_preview_probe(
                    state,
                    &mut frames,
                    &mut hash,
                    error.as_mut_ptr(),
                    error.len()
                ),
                0
            );
            assert_eq!(frames, 3);
            assert_ne!(hash, 0);
            let first_generation = host(state).unwrap().generation;
            for event in [1, 2] {
                assert_eq!(clover_apple_preview_event(state, event), 0);
                assert_eq!(
                    clover_apple_preview_iterate(state, error.as_mut_ptr(), error.len()),
                    0
                );
                assert_eq!(host(state).unwrap().frames, 3);
            }
            assert_eq!(clover_apple_preview_event(state, 99), 1);
            assert_eq!(clover_apple_preview_event(state, 3), 0);
            assert_eq!(
                clover_apple_preview_iterate(state, error.as_mut_ptr(), error.len()),
                0
            );
            host(state).unwrap().scene.generation = first_generation + 1;
            assert_eq!(
                clover_apple_preview_iterate(state, error.as_mut_ptr(), error.len()),
                1
            );
            clover_apple_preview_quit(state);
            clover_apple_preview_quit(std::ptr::null_mut());
            assert_eq!(
                clover_apple_preview_init(&mut state, error.as_mut_ptr(), error.len()),
                0
            );
            assert_ne!(host(state).unwrap().generation, first_generation);
            clover_apple_preview_quit(state);
        }
    }
}
