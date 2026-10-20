//! Rust SDL3 frontend. Clover's packed atlas, bitmap fonts, imported covers,
//! regional layout, carousel controls, and ares session all stay on this thread.

mod checks;
mod controls;
mod launcher;
mod suspend;
mod toolbar;

use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs::{self, File},
    io::BufReader,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender},
    time::Instant,
};

use sdl3::{
    GamepadSubsystem,
    audio::{AudioFormat, AudioSpec, AudioSpecWAV, AudioStreamOwner},
    dialog::{self, DialogError, DialogFileFilter},
    event::Event,
    gamepad::{Button as PadButton, Gamepad},
    keyboard::Scancode,
    pixels::{Color, PixelFormat},
    render::{BlendMode, Canvas, FRect, ScaleMode, Texture, TextureAccess, TextureCreator},
    surface::Surface,
    video::{Window, WindowContext},
};

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn clover_install_native_menu(in_launcher: i32);
    fn clover_native_menu_event() -> u32;
}

const STAGE_W: f32 = 1280.0;

#[derive(Clone)]
struct Game {
    id: String,
    title: String,
    sort_title: String,
    publisher: String,
    release: String,
    copyright: String,
    rom: String,
    import_error: String,
    players: u32,
    simultaneous: bool,
    firmware_required: Vec<String>,
}

#[derive(Clone, Copy)]
struct Glyph {
    source: FRect,
    x: f32,
    y: f32,
    advance: f32,
}

struct Font {
    size: f32,
    line: f32,
    baseline: f32,
    glyphs: BTreeMap<char, Glyph>,
    kernings: BTreeMap<(char, char), f32>,
}

#[derive(Clone)]
struct PlacedGlyph {
    font: String,
    source: FRect,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

struct TextLayout {
    glyphs: Vec<PlacedGlyph>,
    advance: f32,
    line: f32,
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

struct UiAssets {
    root: PathBuf,
    creator: TextureCreator<WindowContext>,
    textures: HashMap<String, Texture>,
    sprites: HashMap<String, FRect>,
    fonts: BTreeMap<String, Font>,
    layout: Value,
    strings: Value,
    default_strings: Value,
    languages: BTreeMap<String, Value>,
    title_color: Color,
    hud_color: Color,
    region: String,
    locale: String,
}

impl UiAssets {
    fn new(canvas: &Canvas<Window>, root: &Path, games: &[Game]) -> Result<Self, String> {
        let creator = canvas.texture_creator();
        let layout = read_json(&root.join("skin/layout.json"))?;
        let strings_file = read_json(&root.join("skin/strings.json"))?;
        let theme = read_json(&root.join("skin/theme.json"))?;
        let packed = read_json(&root.join("skin/packed.json"))?;
        let sprites = packed
            .get("sprites")
            .and_then(Value::as_object)
            .ok_or("Clover packed atlas has no sprite table")?
            .iter()
            .filter_map(|(key, item)| {
                let frame = item.get("textureFrame")?.as_array()?;
                if frame.len() != 4 {
                    return None;
                }
                Some((
                    key.clone(),
                    FRect::new(
                        frame[0].as_f64()? as f32,
                        frame[1].as_f64()? as f32,
                        frame[2].as_f64()? as f32,
                        frame[3].as_f64()? as f32,
                    ),
                ))
            })
            .collect();
        let color = |name: &str, fallback: [u8; 4]| -> Color {
            theme
                .pointer(&format!("/colors/{name}"))
                .and_then(Value::as_array)
                .filter(|v| v.len() == 4)
                .map(|v| {
                    Color::RGBA(
                        v[0].as_u64().unwrap_or(fallback[0] as u64) as u8,
                        v[1].as_u64().unwrap_or(fallback[1] as u64) as u8,
                        v[2].as_u64().unwrap_or(fallback[2] as u64) as u8,
                        v[3].as_u64().unwrap_or(fallback[3] as u64) as u8,
                    )
                })
                .unwrap_or(Color::RGBA(
                    fallback[0],
                    fallback[1],
                    fallback[2],
                    fallback[3],
                ))
        };
        let mut result = Self {
            root: root.to_path_buf(),
            creator,
            textures: HashMap::new(),
            sprites,
            fonts: BTreeMap::new(),
            layout,
            strings: strings_file
                .get("strings")
                .cloned()
                .unwrap_or_else(|| json!({})),
            default_strings: strings_file
                .get("strings")
                .cloned()
                .unwrap_or_else(|| json!({})),
            languages: BTreeMap::new(),
            title_color: color("title", [0, 0, 0, 255]),
            hud_color: color("hud", [255, 255, 255, 255]),
            region: theme
                .get("region")
                .and_then(Value::as_str)
                .unwrap_or("Europe")
                .to_owned(),
            locale: strings_file
                .get("locale")
                .and_then(Value::as_str)
                .unwrap_or("usa_en")
                .to_owned(),
        };
        let language_index = root.join("skin/languages.json");
        if language_index.is_file() {
            let index = read_json(&language_index)?;
            let locales = index
                .get("locales")
                .and_then(Value::as_array)
                .ok_or("Invalid language index")?;
            if locales.len() > 64 {
                return Err("Too many language tables".into());
            }
            for locale in locales {
                let locale = locale
                    .as_str()
                    .filter(|locale| crate::skin::valid_locale(locale))
                    .ok_or("Invalid language identifier")?;
                let table = read_json(&root.join("skin/languages").join(format!("{locale}.json")))?;
                let strings = table
                    .get("strings")
                    .and_then(Value::as_object)
                    .ok_or("Invalid language table")?;
                if strings.len() >= 5000 || !strings.values().all(Value::is_string) {
                    return Err("Invalid language strings".into());
                }
                result
                    .languages
                    .insert(locale.to_owned(), Value::Object(strings.clone()));
            }
        }
        result
            .languages
            .entry(result.locale.clone())
            .or_insert_with(|| result.default_strings.clone());
        result.load_texture("skin/packed.png")?;
        result.load_texture("skin/wall.png")?;
        for game in games {
            result.load_texture(&format!("games/{}/{}.png", game.id, game.id))?;
            result.load_texture(&format!("games/{}/{}_small.png", game.id, game.id))?;
        }
        for name in ["s", "m", "l", "title", "copyright"] {
            let path = root.join(format!("skin/font-{name}.json"));
            if !path.is_file() && name == "copyright" {
                continue;
            }
            let data = read_json(&path)?;
            let mut glyphs = BTreeMap::new();
            if let Some(items) = data.get("glyphs").and_then(Value::as_object) {
                for (code, item) in items {
                    let Ok(code) = code.parse::<u32>() else {
                        continue;
                    };
                    let Some(character) = char::from_u32(code) else {
                        continue;
                    };
                    let n = |key: &str| item.get(key).and_then(Value::as_f64).unwrap_or(0.0) as f32;
                    glyphs.insert(
                        character,
                        Glyph {
                            source: FRect::new(n("x"), n("y"), n("width"), n("height")),
                            x: n("xOffset"),
                            y: n("yOffset"),
                            advance: n("advance"),
                        },
                    );
                }
            }
            let mut kernings = BTreeMap::new();
            if let Some(items) = data.get("kernings").and_then(Value::as_object) {
                for (key, value) in items {
                    let Some((left, right)) = key.split_once(':') else {
                        continue;
                    };
                    let (Ok(left), Ok(right)) = (left.parse::<u32>(), right.parse::<u32>()) else {
                        continue;
                    };
                    if let (Some(left), Some(right), Some(amount)) =
                        (char::from_u32(left), char::from_u32(right), value.as_f64())
                    {
                        kernings.insert((left, right), amount as f32);
                    }
                }
            }
            result.load_texture(&format!("skin/font-{name}.png"))?;
            result.fonts.insert(
                name.to_owned(),
                Font {
                    size: data.get("fontSize").and_then(Value::as_f64).unwrap_or(16.0) as f32,
                    line: data
                        .get("lineHeight")
                        .and_then(Value::as_f64)
                        .unwrap_or(20.0) as f32,
                    baseline: data.get("baseline").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                    glyphs,
                    kernings,
                },
            );
        }
        Ok(result)
    }

    fn load_texture(&mut self, key: &str) -> Result<(), String> {
        if self.textures.contains_key(key) {
            return Ok(());
        }
        let path = self.root.join(key);
        let mut decoder = png::Decoder::new(BufReader::new(
            File::open(&path).map_err(|e| format!("Cannot open {}: {e}", path.display()))?,
        ));
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder
            .read_info()
            .map_err(|e| format!("Invalid PNG {}: {e}", path.display()))?;
        let mut bytes = vec![0; reader.output_buffer_size()];
        let info = reader
            .next_frame(&mut bytes)
            .map_err(|e| format!("Cannot decode {}: {e}", path.display()))?;
        bytes.truncate(info.buffer_size());
        let width = info.width;
        let height = info.height;
        let rgba = match info.color_type {
            png::ColorType::Rgba => bytes,
            png::ColorType::Rgb => bytes
                .chunks_exact(3)
                .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255])
                .collect(),
            png::ColorType::GrayscaleAlpha => bytes
                .chunks_exact(2)
                .flat_map(|pixel| [pixel[0], pixel[0], pixel[0], pixel[1]])
                .collect(),
            png::ColorType::Grayscale => bytes
                .into_iter()
                .flat_map(|value| [value, value, value, 255])
                .collect(),
            png::ColorType::Indexed => {
                return Err(format!("Indexed PNG was not expanded: {}", path.display()));
            }
        };
        if width == 0
            || height == 0
            || width > 8192
            || height > 8192
            || rgba.len() != width as usize * height as usize * 4
        {
            return Err(format!("Invalid PNG dimensions: {}", path.display()));
        }
        let mut rgba = rgba;
        let surface = Surface::from_data(&mut rgba, width, height, width * 4, PixelFormat::RGBA32)
            .map_err(|e| format!("Cannot create image surface: {e}"))?;
        let mut texture = self
            .creator
            .create_texture_from_surface(surface)
            .map_err(|e| e.to_string())?;
        texture.set_blend_mode(BlendMode::Blend);
        texture.set_scale_mode(ScaleMode::Nearest);
        self.textures.insert(key.to_owned(), texture);
        Ok(())
    }

    fn translate(&self, key: &str, fallback: &str) -> String {
        self.strings
            .get(key)
            .and_then(Value::as_str)
            .or_else(|| self.default_strings.get(key).and_then(Value::as_str))
            .unwrap_or(fallback)
            .to_owned()
    }

    fn set_language(&mut self, locale: &str) -> Result<(), String> {
        let strings = self.languages.get(locale).ok_or_else(|| format!("Language {locale} is not available in this profile. Re-import its image to add all shipped languages."))?;
        self.strings = strings.clone();
        self.locale = locale.to_owned();
        Ok(())
    }

    fn measure(&self, value: &str, primary_name: &str) -> TextLayout {
        let Some(primary) = self.fonts.get(primary_name) else {
            return TextLayout {
                glyphs: Vec::new(),
                advance: 0.0,
                line: 0.0,
                left: 0.0,
                top: 0.0,
                right: 0.0,
                bottom: 0.0,
            };
        };
        let mut layout = TextLayout {
            glyphs: Vec::new(),
            advance: 0.0,
            line: primary.line,
            left: 0.0,
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
        };
        let mut first = true;
        let mut previous: Option<(String, char)> = None;
        for character in value.chars().take(4096) {
            let chosen = if primary.glyphs.contains_key(&character) {
                Some(primary_name.to_owned())
            } else {
                self.fonts.iter().find_map(|(name, font)| {
                    font.glyphs.contains_key(&character).then(|| name.clone())
                })
            };
            let mut character = character;
            let name = chosen.or_else(|| {
                character = '?';
                if primary.glyphs.contains_key(&character) {
                    Some(primary_name.to_owned())
                } else {
                    self.fonts.iter().find_map(|(name, font)| {
                        font.glyphs.contains_key(&character).then(|| name.clone())
                    })
                }
            });
            let Some(name) = name else { continue };
            let Some(font) = self.fonts.get(&name) else {
                continue;
            };
            let Some(glyph) = font.glyphs.get(&character).copied() else {
                continue;
            };
            let scale = primary.size / font.size.max(1.0);
            if let Some((_, previous_char)) = previous
                .as_ref()
                .filter(|(previous_name, _)| previous_name == &name)
            {
                layout.advance += font
                    .kernings
                    .get(&(*previous_char, character))
                    .copied()
                    .unwrap_or(0.0)
                    * scale;
            }
            let x = layout.advance + glyph.x * scale;
            let y = primary.baseline - font.baseline * scale + glyph.y * scale;
            let width = glyph.source.w * scale;
            let height = glyph.source.h * scale;
            layout.glyphs.push(PlacedGlyph {
                font: name.clone(),
                source: glyph.source,
                x,
                y,
                width,
                height,
            });
            if character != ' ' && width > 0.0 && height > 0.0 {
                if first {
                    layout.left = x;
                    layout.top = y;
                    layout.right = x + width;
                    layout.bottom = y + height;
                    first = false;
                } else {
                    layout.left = layout.left.min(x);
                    layout.top = layout.top.min(y);
                    layout.right = layout.right.max(x + width);
                    layout.bottom = layout.bottom.max(y + height);
                }
            }
            layout.advance += glyph.advance * scale;
            previous = Some((name, character));
        }
        layout
    }

    fn draw_texture(
        &mut self,
        canvas: &mut Canvas<Window>,
        key: &str,
        dest: FRect,
        src: Option<FRect>,
        color: Color,
        smooth: bool,
    ) {
        if let Some(texture) = self.textures.get_mut(key) {
            texture.set_color_mod(color.r, color.g, color.b);
            texture.set_alpha_mod(color.a);
            texture.set_scale_mode(if smooth {
                ScaleMode::Linear
            } else {
                ScaleMode::Nearest
            });
            let _ = canvas.copy(texture, src, dest);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn sprite(
        &mut self,
        canvas: &mut Canvas<Window>,
        name: &str,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: Color,
    ) {
        let flip = name == "caption_edge_left";
        let name = if flip { "caption_edge" } else { name };
        let key = if name.ends_with(".png") {
            name.to_owned()
        } else {
            format!("{name}.png")
        };
        if let Some(src) = self.sprites.get(&key).copied() {
            let dest = FRect::new(
                (x - width * 0.5).round(),
                (y - height * 0.5).round(),
                width,
                height,
            );
            if flip {
                if let Some(texture) = self.textures.get_mut("skin/packed.png") {
                    texture.set_color_mod(color.r, color.g, color.b);
                    texture.set_alpha_mod(color.a);
                    texture.set_scale_mode(ScaleMode::Nearest);
                    let _ = canvas.copy_ex(texture, Some(src), dest, 0.0, None, true, false);
                }
            } else {
                self.draw_texture(canvas, "skin/packed.png", dest, Some(src), color, false);
            }
        }
    }

    fn fit_image(&mut self, canvas: &mut Canvas<Window>, key: &str, rect: FRect, alpha: u8) {
        let Some(texture) = self.textures.get_mut(key) else {
            return;
        };
        let query = texture.query();
        let scale = (rect.w / query.width as f32).min(rect.h / query.height as f32);
        let width = query.width as f32 * scale;
        let height = query.height as f32 * scale;
        texture.set_color_mod(255, 255, 255);
        texture.set_alpha_mod(alpha);
        texture.set_scale_mode(ScaleMode::Linear);
        let dest = FRect::new(
            rect.x + (rect.w - width) * 0.5,
            rect.y + (rect.h - height) * 0.5,
            width,
            height,
        );
        let _ = canvas.copy(texture, None, dest);
    }

    #[allow(clippy::too_many_arguments)]
    fn text(
        &mut self,
        canvas: &mut Canvas<Window>,
        value: &str,
        x: f32,
        y: f32,
        font_name: &str,
        color: Color,
        max_width: f32,
        line_anchor: bool,
    ) {
        let layout = self.measure(value, font_name);
        let extent = if line_anchor {
            layout.advance.max(layout.right)
        } else {
            layout.right - layout.left
        };
        let scale = (max_width / extent.max(1.0)).min(1.0);
        let left = x
            - (if line_anchor {
                layout.advance * 0.5
            } else {
                (layout.left + layout.right) * 0.5
            }) * scale;
        let top = y
            - (if line_anchor {
                layout.line * 0.5
            } else {
                (layout.top + layout.bottom) * 0.5
            }) * scale;
        for glyph in layout.glyphs {
            let key = format!("skin/font-{}.png", glyph.font);
            let dest = FRect::new(
                left + glyph.x * scale,
                top + glyph.y * scale,
                glyph.width * scale,
                glyph.height * scale,
            );
            self.draw_texture(canvas, &key, dest, Some(glyph.source), color, true);
        }
    }

    fn selection(
        &mut self,
        canvas: &mut Canvas<Window>,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        time: f32,
    ) {
        let phase = time.rem_euclid(2.25);
        let tint = if phase < 0.5 {
            0.75 + 0.5 * phase
        } else if phase < 1.25 {
            1.0
        } else if phase < 2.0 {
            1.0 - (phase - 1.25) / 3.0
        } else {
            0.75
        };
        let color = Color::RGB((255.0 * tint) as u8, (255.0 * tint) as u8, 255);
        let widths = [32.0, width - 64.0, 32.0];
        let heights = [32.0, height - 64.0, 32.0];
        let left = x - width * 0.5;
        let top = y - height * 0.5;
        for (row, &row_height) in heights.iter().enumerate() {
            for (column, &column_width) in widths.iter().enumerate() {
                if row == 1 && column == 1 {
                    continue;
                }
                let tile = row * 3 + column + 1;
                let name = if row != 1 && column != 1 {
                    format!("cursor_{tile}")
                } else {
                    format!("cursor_game_{tile}")
                };
                let cx =
                    left + (if column == 0 {
                        0.0
                    } else if column == 1 {
                        32.0
                    } else {
                        width - 32.0
                    }) + column_width * 0.5;
                let cy =
                    top + (if row == 0 {
                        0.0
                    } else if row == 1 {
                        32.0
                    } else {
                        height - 32.0
                    }) + row_height * 0.5;
                self.sprite(canvas, &name, cx, cy, column_width, row_height, color);
            }
        }
    }
}

#[derive(Clone)]
struct Hit {
    action: String,
    rect: FRect,
}

struct AudioOut {
    stream: AudioStreamOwner,
    data: Vec<u8>,
}

enum DialogMessage {
    Rom {
        game: String,
        path: PathBuf,
        start: bool,
    },
    Firmware(PathBuf),
    Image(PathBuf),
    Error(String),
}

impl AudioOut {
    fn wav(path: &Path) -> Option<Self> {
        let wav = AudioSpecWAV::load_wav(path).ok()?;
        let spec = AudioSpec {
            freq: Some(wav.freq),
            channels: Some(wav.channels as i32),
            format: Some(wav.format),
        };
        let audio = sdl3::init().ok()?.audio().ok()?;
        let stream = audio
            .default_playback_device()
            .open_device_stream(Some(&spec))
            .ok()?;
        let data = wav.buffer().to_vec();
        stream.resume().ok()?;
        Some(Self { stream, data })
    }

    fn core(rate: u32) -> Option<Self> {
        let spec = AudioSpec {
            freq: Some(rate as i32),
            channels: Some(2),
            format: Some(AudioFormat::f32_sys()),
        };
        let audio = sdl3::init().ok()?.audio().ok()?;
        let stream = audio
            .default_playback_device()
            .open_device_stream(Some(&spec))
            .ok()?;
        stream.resume().ok()?;
        Some(Self {
            stream,
            data: Vec::new(),
        })
    }

    fn clear(&self) {
        let _ = self.stream.clear();
    }
    fn queued(&self) -> usize {
        self.stream.queued_bytes().unwrap_or(0).max(0) as usize
    }
    fn put_f32(&self, samples: &[f32]) {
        let _ = self.stream.put_data_f32(samples);
    }
    fn play_loop(&self) {
        if !self.data.is_empty() && self.queued() < self.data.len() / 2 {
            let _ = self.stream.put_data(&self.data);
        }
    }
    fn play_once(&self) {
        if !self.data.is_empty() {
            let _ = self.stream.clear();
            let _ = self.stream.put_data(&self.data);
        }
    }
}

struct App {
    assets_path: PathBuf,
    data_path: PathBuf,
    profile_cache: PathBuf,
    profile_home: PathBuf,
    profiles: Vec<crate::profile::Profile>,
    firmware_path: PathBuf,
    games: Vec<Game>,
    art: UiAssets,
    preferences: Value,
    selected: usize,
    sort: usize,
    slot: usize,
    sound: bool,
    motion: bool,
    panel: String,
    message: String,
    focus: usize,
    hits: Vec<Hit>,
    actions: Vec<String>,
    hover: String,
    saved: HashSet<(String, usize)>,
    scroll_from: f32,
    scroll_to: f32,
    scroll_current: f32,
    scroll_started: Instant,
    animation_start: Instant,
    music: Option<AudioOut>,
    click: Option<AudioOut>,
    core_audio: Option<AudioOut>,
    core: Option<crate::ares::AresSession>,
    frame_texture: Option<Texture>,
    held: HashSet<Scancode>,
    pad_held: HashSet<PadButton>,
    gamepad_subsystem: GamepadSubsystem,
    gamepad: Option<Gamepad>,
    fullscreen: bool,
    fullscreen_dirty: bool,
    window_title: String,
    title_dirty: bool,
    dialog_sender: Sender<DialogMessage>,
    dialog_receiver: Receiver<DialogMessage>,
    running_game: Option<String>,
    playing: bool,
    menu_focus: MenuFocus,
    toolbar_index: usize,
    suspend_controls: bool,
    suspend_previews: [Option<Texture>; 4],
    pending_preview: Option<Texture>,
    confirm_target: Option<(String, usize)>,
    running_rom: Option<PathBuf>,
    panel_scroll: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MenuFocus {
    Games,
    Toolbar,
}

impl Drop for App {
    fn drop(&mut self) {
        self.clear_suspend_previews();
    }
}

impl Drop for UiAssets {
    fn drop(&mut self) {
        for (_, texture) in self.textures.drain() {
            // SAFETY: atlas/font/image textures are owned here; the creator keeps the renderer alive.
            unsafe {
                texture.destroy();
            }
        }
    }
}

impl App {
    fn new(
        canvas: &Canvas<Window>,
        assets_path: PathBuf,
        data_path: PathBuf,
        gamepad_subsystem: GamepadSubsystem,
        profile_cache: PathBuf,
        profile_home: PathBuf,
    ) -> Result<Self, String> {
        fs::create_dir_all(&data_path)
            .map_err(|e| format!("Cannot create profile data directory: {e}"))?;
        let games = read_games(&assets_path)?;
        let art = UiAssets::new(canvas, &assets_path, &games)?;
        let window_title = format!("Classic Menu - {} / {} - ares", art.region, art.locale);
        let preferences_path = data_path.join("preferences.json");
        let preferences = if preferences_path.exists() {
            read_json(&preferences_path)?
        } else {
            json!({})
        };
        let firmware_path = preferences
            .get("firmware")
            .and_then(Value::as_str)
            .map(PathBuf::from)
            .unwrap_or_else(|| data_path.join("firmware"));
        let profiles = crate::profile::list_profiles(&profile_cache);
        let (dialog_sender, dialog_receiver) = mpsc::channel();
        let gamepad = gamepad_subsystem.gamepads().ok().and_then(|ids| {
            ids.into_iter()
                .find_map(|id| gamepad_subsystem.open(id).ok())
        });
        let mut app = Self {
            assets_path: assets_path.clone(),
            data_path: data_path.clone(),
            profile_cache,
            profile_home,
            profiles,
            firmware_path,
            games,
            art,
            preferences,
            selected: 0,
            sort: 0,
            slot: 0,
            sound: false,
            motion: true,
            panel: String::new(),
            message: String::new(),
            focus: 0,
            hits: Vec::new(),
            actions: Vec::new(),
            hover: String::new(),
            saved: HashSet::new(),
            scroll_from: 0.0,
            scroll_to: 0.0,
            scroll_current: 0.0,
            scroll_started: Instant::now(),
            animation_start: Instant::now(),
            music: None,
            click: None,
            core_audio: None,
            core: None,
            frame_texture: None,
            held: HashSet::new(),
            pad_held: HashSet::new(),
            gamepad_subsystem,
            gamepad,
            fullscreen: false,
            fullscreen_dirty: false,
            window_title,
            title_dirty: false,
            dialog_sender,
            dialog_receiver,
            running_game: None,
            playing: false,
            menu_focus: MenuFocus::Games,
            toolbar_index: 0,
            suspend_controls: false,
            suspend_previews: std::array::from_fn(|_| None),
            pending_preview: None,
            confirm_target: None,
            running_rom: None,
            panel_scroll: 0,
        };
        app.selected = app
            .games
            .iter()
            .position(|g| app.preferences.get("selected").and_then(Value::as_str) == Some(&g.id))
            .unwrap_or(0);
        app.sort = app
            .preferences
            .get("sort")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            .min(5) as usize;
        app.slot = app
            .preferences
            .get("slot")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            .min(3) as usize;
        app.sound = app
            .preferences
            .get("sound")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        app.motion = app
            .preferences
            .get("motion")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        if let Some(locale) = app.preferences.get("language").and_then(Value::as_str) {
            let _ = app.art.set_language(locale);
        }
        app.sort_games();
        app.refresh_saves();
        app.music = AudioOut::wav(&assets_path.join("skin/bgm_home.wav"));
        app.click = AudioOut::wav(&assets_path.join("skin/se_sys_cursor.wav"));
        Ok(app)
    }

    fn selected(&self) -> &Game {
        &self.games[self.selected.min(self.games.len() - 1)]
    }

    fn record(&self, id: &str, key: &str) -> i64 {
        self.preferences
            .pointer(&format!("/history/{id}/{key}"))
            .and_then(Value::as_i64)
            .unwrap_or(0)
    }

    fn sort_games(&mut self) {
        let selected_id = self.selected().id.clone();
        let sort = self.sort;
        let history = self
            .preferences
            .get("history")
            .cloned()
            .unwrap_or_else(|| json!({}));
        self.games.sort_by(|a, b| {
            match sort {
                1 if a.players != b.players => return b.players.cmp(&a.players),
                2 | 3 => {
                    let key = if sort == 2 { "recent" } else { "count" };
                    let value = |id: &str| {
                        history
                            .pointer(&format!("/{id}/{key}"))
                            .and_then(Value::as_i64)
                            .unwrap_or(0)
                    };
                    if value(&a.id) != value(&b.id) {
                        return value(&b.id).cmp(&value(&a.id));
                    }
                }
                4 if a.release != b.release => return a.release.cmp(&b.release),
                5 if a.publisher != b.publisher => return a.publisher.cmp(&b.publisher),
                _ => {}
            }
            a.sort_title
                .cmp(&b.sort_title)
                .then_with(|| a.id.cmp(&b.id))
        });
        self.selected = self
            .games
            .iter()
            .position(|game| game.id == selected_id)
            .unwrap_or(0);
        self.scroll_current = self.selected as f32;
        self.scroll_to = self.scroll_current;
        self.scroll_from = self.scroll_current;
    }

    fn save_settings(&mut self) -> Result<(), String> {
        self.preferences["schema"] = json!(1);
        self.preferences["selected"] = json!(self.selected().id);
        self.preferences["sort"] = json!(self.sort);
        self.preferences["slot"] = json!(self.slot);
        self.preferences["sound"] = json!(self.sound);
        self.preferences["motion"] = json!(self.motion);
        self.preferences["language"] = json!(self.art.locale);
        self.preferences["firmware"] = json!(self.firmware_path.to_string_lossy());
        atomic_json(&self.data_path.join("preferences.json"), &self.preferences)
    }

    fn switch_profile(&mut self, canvas: &Canvas<Window>, profile: crate::profile::Profile) {
        let _ = self.save_settings();
        if let Some(core) = self.core.as_mut()
            && core.is_loaded()
            && let Err(error) = core.unload()
        {
            self.panel = "message".into();
            self.message = error.to_string();
            return;
        }
        self.running_game = None;
        self.running_rom = None;
        self.playing = false;
        self.menu_focus = MenuFocus::Games;
        self.clear_suspend_previews();
        self.core_audio = None;
        let assets_path = profile.assets();
        let data_path = profile.data();
        let result = (|| {
            fs::create_dir_all(&data_path)
                .map_err(|e| format!("Cannot create profile data directory: {e}"))?;
            let games = read_games(&assets_path)?;
            let art = UiAssets::new(canvas, &assets_path, &games)?;
            let preference_path = data_path.join("preferences.json");
            let preferences = if preference_path.exists() {
                read_json(&preference_path)?
            } else {
                json!({})
            };
            Ok::<_, String>((games, art, preferences))
        })();
        match result {
            Ok((games, art, preferences)) => {
                self.assets_path = assets_path;
                self.data_path = data_path;
                self.games = games;
                self.art = art;
                self.preferences = preferences;
                if let Some(locale) = self.preferences.get("language").and_then(Value::as_str) {
                    let _ = self.art.set_language(locale);
                }
                self.selected = self
                    .games
                    .iter()
                    .position(|game| {
                        self.preferences.get("selected").and_then(Value::as_str) == Some(&game.id)
                    })
                    .unwrap_or(0);
                self.sort = self
                    .preferences
                    .get("sort")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
                    .min(5) as usize;
                self.slot = self
                    .preferences
                    .get("slot")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
                    .min(3) as usize;
                self.sound = self
                    .preferences
                    .get("sound")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                self.motion = self
                    .preferences
                    .get("motion")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                self.firmware_path = self
                    .preferences
                    .get("firmware")
                    .and_then(Value::as_str)
                    .map(PathBuf::from)
                    .unwrap_or_else(|| self.data_path.join("firmware"));
                self.frame_texture = None;
                self.sort_games();
                self.refresh_saves();
                self.music = AudioOut::wav(&self.assets_path.join("skin/bgm_home.wav"));
                self.click = AudioOut::wav(&self.assets_path.join("skin/se_sys_cursor.wav"));
                self.panel.clear();
                self.profiles = crate::profile::list_profiles(&self.profile_cache);
                let region = profile
                    .manifest
                    .get("region")
                    .and_then(Value::as_str)
                    .unwrap_or("SNES Classic");
                let version = profile
                    .manifest
                    .get("version")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                self.window_title = format!("Classic Menu - {region} / {version} - ares");
                self.title_dirty = true;
                let directory = profile
                    .directory
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default();
                let marker = json!({"directory": directory});
                let _ = atomic_json(&self.profile_home.join("last-profile.json"), &marker);
            }
            Err(error) => {
                self.panel = "message".into();
                self.message = error;
            }
        }
    }

    fn choose_image(&self, canvas: &Canvas<Window>) {
        let sender = self.dialog_sender.clone();
        let filters = [DialogFileFilter {
            name: "SNES Classic image",
            pattern: "hsqs;gz;img",
        }];
        if let Err(error) = dialog::show_open_file_dialog(
            &filters,
            None::<&Path>,
            false,
            Some(canvas.window()),
            Box::new(move |result, _| match result {
                Ok(paths) => {
                    if let Some(path) = paths.into_iter().next() {
                        let _ = sender.send(DialogMessage::Image(path));
                    }
                }
                Err(DialogError::Canceled) => {}
                Err(error) => {
                    let _ = sender.send(DialogMessage::Error(error.to_string()));
                }
            }),
        ) {
            let _ = self
                .dialog_sender
                .send(DialogMessage::Error(error.to_string()));
        }
    }

    fn refresh_saves(&mut self) {
        self.saved.clear();
        for game in &self.games {
            for slot in 0..4 {
                if self.state_path(game, slot).is_file() {
                    self.saved.insert((game.id.clone(), slot));
                }
            }
        }
    }

    fn state_path(&self, game: &Game, slot: usize) -> PathBuf {
        self.data_path
            .join("saves")
            .join(&game.id)
            .join(format!("slot-{}.state", slot + 1))
    }

    fn move_selection(&mut self, direction: i32) {
        let n = self.games.len() as i32;
        self.selected = (self.selected as i32 + direction).rem_euclid(n) as usize;
        let mut target = self.selected as f32;
        let count = n as f32;
        target += ((self.scroll_current - target) / count).round() * count;
        self.scroll_from = self.scroll_current;
        self.scroll_to = target;
        self.scroll_started = Instant::now();
        if self.sound
            && let Some(click) = &self.click
        {
            click.play_once();
        }
    }

    fn begin_game(&mut self, canvas: &Canvas<Window>) -> bool {
        let game = self.selected().clone();
        if !self.firmware_ready(&game) {
            self.panel = "message".into();
            self.message = format!(
                "{} requires user-supplied coprocessor firmware. Choose its folder in Settings or place the required files in {}.",
                game.title,
                self.firmware_path.display()
            );
            return false;
        }
        let Some(rom) = self.rom_path(&game) else {
            if !game.import_error.is_empty() {
                self.panel = "message".into();
                self.message = format!(
                    "Image import: {}. Choose a compatible ROM in Settings.",
                    game.import_error
                );
            } else {
                self.choose_rom(canvas, true);
            }
            return false;
        };
        let Some(core) = self.core.as_mut() else {
            match crate::ares::AresSession::new() {
                Ok(core) => self.core = Some(core),
                Err(error) => {
                    self.panel = "message".into();
                    self.message = error.to_string();
                    return false;
                }
            }
            return self.begin_game(canvas);
        };
        if !core.is_loaded()
            || self.running_game.as_deref() != Some(&game.id)
            || self.running_rom.as_ref() != Some(&rom)
        {
            let saves = self.data_path.join("saves").join(&game.id);
            let bundled_system = self.assets_path.join("firmware");
            let system = if bundled_system.join("ipl.rom").is_file() {
                &bundled_system
            } else {
                &self.firmware_path
            };
            if let Err(error) = core.load(&rom, &saves, Some(&self.firmware_path), system) {
                self.panel = "message".into();
                self.message = error.to_string();
                return false;
            }
            self.core_audio = AudioOut::core(core.audio_rate());
            self.running_game = Some(game.id.clone());
            self.running_rom = Some(rom);
        }
        if let Some(audio) = &self.core_audio {
            audio.clear();
        }
        if let Some(music) = &self.music {
            music.clear();
        }
        self.panel.clear();
        self.playing = true;
        self.preferences["history"][&game.id]["count"] = json!(self.record(&game.id, "count") + 1);
        self.preferences["history"][&game.id]["recent"] = json!(chrono_now());
        let _ = self.save_settings();
        true
    }

    fn firmware_ready(&self, game: &Game) -> bool {
        required_firmware_available(&game.firmware_required, &self.firmware_path)
    }

    fn rom_path(&self, game: &Game) -> Option<PathBuf> {
        if let Some(path) = self
            .preferences
            .pointer(&format!("/roms/{}", game.id))
            .and_then(Value::as_str)
        {
            let path = PathBuf::from(path);
            if fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_file()) {
                return Some(path);
            }
        }
        if game.rom.is_empty() {
            return None;
        }
        let relative = Path::new(&game.rom);
        if relative.is_absolute()
            || relative
                .components()
                .any(|part| !matches!(part, std::path::Component::Normal(_)))
        {
            return None;
        }
        let path = self.assets_path.join(relative);
        fs::symlink_metadata(&path)
            .is_ok_and(|metadata| metadata.file_type().is_file())
            .then_some(path)
    }

    fn choose_rom(&self, canvas: &Canvas<Window>, start: bool) {
        let game = self.selected().id.clone();
        let sender = self.dialog_sender.clone();
        let filters = [DialogFileFilter {
            name: "Super Nintendo ROM",
            pattern: "sfc;smc",
        }];
        if let Err(error) = dialog::show_open_file_dialog(
            &filters,
            None::<&Path>,
            false,
            Some(canvas.window()),
            Box::new(move |result, _| match result {
                Ok(paths) => {
                    if let Some(path) = paths.into_iter().next() {
                        let _ = sender.send(DialogMessage::Rom {
                            game: game.clone(),
                            path,
                            start,
                        });
                    }
                }
                Err(DialogError::Canceled) => {}
                Err(error) => {
                    let _ = sender.send(DialogMessage::Error(error.to_string()));
                }
            }),
        ) {
            let _ = self
                .dialog_sender
                .send(DialogMessage::Error(error.to_string()));
        }
    }

    fn choose_firmware(&self, canvas: &Canvas<Window>) {
        let sender = self.dialog_sender.clone();
        dialog::show_open_folder_dialog(
            None::<&Path>,
            false,
            Some(canvas.window()),
            Box::new(move |result, _| match result {
                Ok(paths) => {
                    if let Some(path) = paths.into_iter().next() {
                        let _ = sender.send(DialogMessage::Firmware(path));
                    }
                }
                Err(DialogError::Canceled) => {}
                Err(error) => {
                    let _ = sender.send(DialogMessage::Error(error.to_string()));
                }
            }),
        );
    }

    fn process_dialogs(&mut self, canvas: &Canvas<Window>) {
        while let Ok(message) = self.dialog_receiver.try_recv() {
            match message {
                DialogMessage::Rom { game, path, start } => {
                    if !safe_id(&game)
                        || !fs::symlink_metadata(&path)
                            .is_ok_and(|metadata| metadata.file_type().is_file())
                    {
                        continue;
                    }
                    self.preferences["roms"][&game] = json!(path.to_string_lossy());
                    self.selected = self
                        .games
                        .iter()
                        .position(|entry| entry.id == game)
                        .unwrap_or(self.selected);
                    let _ = self.save_settings();
                    if start {
                        self.begin_game(canvas);
                    }
                }
                DialogMessage::Firmware(path) => {
                    if fs::symlink_metadata(&path)
                        .is_ok_and(|metadata| metadata.file_type().is_dir())
                    {
                        self.firmware_path = path;
                        let _ = self.save_settings();
                    }
                }
                DialogMessage::Image(path) => {
                    self.panel = "message".into();
                    self.message = "Importing image...".into();
                    match crate::profile::import_image(&path, &self.profile_cache, None) {
                        Ok(profile) => self.switch_profile(canvas, profile),
                        Err(error) => {
                            self.message = error;
                        }
                    }
                }
                DialogMessage::Error(error) => {
                    self.panel = "message".into();
                    self.message = error;
                }
            }
        }
    }

    fn button_mask(&self) -> u16 {
        let keyboard = [
            Scancode::Z,
            Scancode::A,
            Scancode::Backspace,
            Scancode::Return,
            Scancode::Up,
            Scancode::Down,
            Scancode::Left,
            Scancode::Right,
            Scancode::X,
            Scancode::S,
            Scancode::Q,
            Scancode::W,
        ];
        let gamepad = [
            PadButton::South,
            PadButton::West,
            PadButton::Back,
            PadButton::Start,
            PadButton::DPadUp,
            PadButton::DPadDown,
            PadButton::DPadLeft,
            PadButton::DPadRight,
            PadButton::East,
            PadButton::North,
            PadButton::LeftShoulder,
            PadButton::RightShoulder,
        ];
        keyboard
            .iter()
            .zip(gamepad)
            .enumerate()
            .fold(0, |mask, (index, (key, button))| {
                if self.held.contains(key) || self.pad_held.contains(&button) {
                    mask | (1 << index)
                } else {
                    mask
                }
            })
    }

    fn pad_button_down(&mut self, button: PadButton, canvas: &Canvas<Window>) {
        self.pad_held.insert(button);
        if self.playing {
            if button == PadButton::Guide {
                self.leave_game();
            }
            return;
        }
        let key = match button {
            PadButton::DPadLeft => Some(Scancode::Left),
            PadButton::DPadRight => Some(Scancode::Right),
            PadButton::DPadUp => Some(Scancode::Up),
            PadButton::DPadDown => Some(Scancode::Down),
            PadButton::Start if self.panel == "resume" => Some(Scancode::R),
            PadButton::South | PadButton::Start => Some(Scancode::Return),
            PadButton::East => Some(Scancode::Escape),
            PadButton::North => Some(Scancode::X),
            PadButton::West => Some(Scancode::S),
            PadButton::Back => Some(Scancode::Delete),
            _ => None,
        };
        if let Some(key) = key {
            self.handle_key(key, false, canvas);
        }
    }

    fn leave_game(&mut self) {
        self.playing = false;
        if let Some(audio) = &self.core_audio {
            audio.clear();
        }
        if let Err(error) = self.capture_pending() {
            self.panel = "message".into();
            self.message = error;
        } else {
            self.open_suspend();
        }
    }

    fn run_game_frame(&mut self) -> Result<(), String> {
        let buttons = self.button_mask();
        let Some(core) = self.core.as_mut() else {
            return Ok(());
        };
        if let Some(audio) = &self.core_audio
            && audio.queued() > core.audio_rate() as usize * 2 * 4 * 80 / 1000
        {
            return Ok(());
        }
        if let Err(error) = core.run_frame(buttons) {
            self.playing = false;
            self.panel = "message".into();
            self.message = error.to_string();
            self.core_audio.as_ref().map(AudioOut::clear);
            return Ok(());
        }
        if let Some(audio) = &self.core_audio {
            audio.put_f32(core.audio());
        }
        let Some(frame) = core.frame() else {
            return Ok(());
        };
        let width = frame.width;
        let height = frame.height;
        if self.frame_texture.as_ref().is_none_or(|texture| {
            texture.query().width != width || texture.query().height != height
        }) {
            self.frame_texture = Some(
                self.art
                    .creator
                    .create_texture(
                        PixelFormat::ARGB8888,
                        TextureAccess::Streaming,
                        width,
                        height,
                    )
                    .map_err(|e| e.to_string())?,
            );
        }
        let bytes: Vec<u8> = frame
            .pixels_argb
            .iter()
            .flat_map(|pixel| pixel.to_ne_bytes())
            .collect();
        if let Some(texture) = self.frame_texture.as_mut() {
            texture.set_scale_mode(ScaleMode::Nearest);
            texture
                .update(None, &bytes, width as usize * 4)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn activate(&mut self, action: &str, canvas: &Canvas<Window>) {
        if self.sound
            && let Some(click) = &self.click
        {
            click.play_once();
        }
        if let Some(index) = action
            .strip_prefix("toolbar:")
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|index| *index < 5)
        {
            self.toolbar_index = index;
            self.menu_focus = MenuFocus::Toolbar;
            self.panel_scroll = 0;
        }
        match action {
            "previous" => self.move_selection(-1),
            "next" => self.move_selection(1),
            "start" => {
                self.begin_game(canvas);
            }
            "sort" => {
                self.sort = (self.sort + 1) % 6;
                self.sort_games();
                self.panel = "notice".into();
                self.message = self.sort_name();
                self.scroll_started = Instant::now();
                let _ = self.save_settings();
            }
            "settings" => {
                self.panel = "settings".into();
                self.focus = 0;
            }
            "emulator-settings" | "help" => {
                self.panel = action.into();
                self.focus = 0;
            }
            "toolbar:0" | "toolbar:2" | "toolbar:3" | "toolbar:4" => {
                self.panel = action.to_owned();
                self.focus = self.toolbar_initial_focus(self.toolbar_index);
            }
            "toolbar:1" => {
                self.panel = "settings".into();
                self.focus = 0;
            }
            "frame:none" => {
                self.preferences["displayFrame"] = json!(0);
                let _ = self.save_settings();
            }
            "legal-up" => self.scroll_legal(false),
            "legal-down" => self.scroll_legal(true),
            _ if action.starts_with("display:") => {
                if let Ok(mode) = action[8..].parse::<usize>()
                    && mode < 3
                {
                    self.preferences["displayMode"] = json!(mode);
                    self.focus = mode;
                    let _ = self.save_settings();
                }
            }
            _ if action.starts_with("legal:") => {
                if let Ok(tab) = action[6..].parse::<usize>()
                    && tab < 2
                {
                    self.focus = tab;
                    self.panel_scroll = 0;
                }
            }
            "resume-panel" => {
                self.open_suspend();
            }
            "profiles" => {
                self.panel = "profiles".into();
                self.focus = 0;
            }
            "rom" => self.choose_rom(canvas, false),
            "firmware" => self.choose_firmware(canvas),
            "import-image" => self.choose_image(canvas),
            "close" => {
                if self.panel == "emulator-settings" {
                    self.panel = "settings".into();
                } else if self.panel == "help" {
                    self.panel = "toolbar:4".into();
                } else {
                    self.panel.clear();
                }
                self.hover.clear();
                self.confirm_target = None;
            }
            "sound" => {
                self.sound = !self.sound;
                if !self.sound {
                    self.music.as_ref().map(AudioOut::clear);
                }
                let _ = self.save_settings();
            }
            "motion" => {
                self.motion = !self.motion;
                let _ = self.save_settings();
            }
            "slot" => {
                self.slot = (self.slot + 1) % 4;
                let _ = self.save_settings();
            }
            "fullscreen" => {
                self.fullscreen = !self.fullscreen;
                self.fullscreen_dirty = true;
            }
            "save" => self.request_suspend_save(),
            "resume" => self.resume_slot(canvas),
            "lock" => self.toggle_suspend_lock(),
            "delete" => self.request_suspend_delete(),
            "resume-pending" => self.resume_pending(canvas),
            "confirm-save" => self.confirm_suspend_save(),
            "confirm-delete" => self.confirm_suspend_delete(),
            "cancel-suspend" => {
                self.confirm_target = None;
                self.open_suspend();
            }
            _ if action.starts_with("suspend-slot:") => {
                if let Ok(slot) = action[13..].parse::<usize>()
                    && slot < 4
                {
                    self.slot = slot;
                    self.suspend_controls = false;
                    let _ = self.save_settings();
                }
            }
            _ if action.starts_with("language:") => match self.art.set_language(&action[9..]) {
                Ok(()) => {
                    self.hover.clear();
                    let _ = self.save_settings();
                }
                Err(error) => {
                    self.panel = "message".into();
                    self.message = error;
                }
            },
            _ if action.starts_with("profile:") => {
                let name = &action[8..];
                if let Some(profile) = self
                    .profiles
                    .iter()
                    .find(|profile| {
                        profile
                            .directory
                            .file_name()
                            .and_then(|value| value.to_str())
                            == Some(name)
                    })
                    .cloned()
                {
                    self.switch_profile(canvas, profile);
                }
            }
            _ if action.starts_with("game:") => {
                self.menu_focus = MenuFocus::Games;
                if let Some(index) = self.games.iter().position(|g| g.id == action[5..]) {
                    self.selected = index;
                    self.scroll_current = index as f32;
                    self.scroll_from = self.scroll_current;
                    self.scroll_to = self.scroll_current;
                }
            }
            _ => {}
        }
    }

    fn save_slot(&mut self) {
        if let Err(error) = self.write_suspend_slot() {
            self.playing = false;
            if let Some(audio) = &self.core_audio {
                audio.clear();
            }
            self.panel = "message".into();
            self.message = error;
        }
    }

    fn resume_slot(&mut self, canvas: &Canvas<Window>) {
        let path = self.state_path(self.selected(), self.slot);
        if !path.is_file() {
            self.panel = "message".into();
            self.message = "There is no suspend point in this slot.".into();
            return;
        }
        if !self.begin_game(canvas) {
            return;
        }
        if let Some(core) = self.core.as_mut()
            && let Err(error) = core.load_state(&path)
        {
            self.playing = false;
            self.panel = "message".into();
            self.message = error.to_string();
        }
        if let Some(audio) = &self.core_audio {
            audio.clear();
        }
    }

    fn sort_name(&self) -> String {
        let key = [
            "TitleName",
            "Players",
            "RecentPlay",
            "PlayCount",
            "ReleaseDate",
            "Manufacturer",
        ][self.sort];
        self.art.translate(
            &format!("sys_sort_order_{key}"),
            [
                "Title",
                "Players",
                "Recently played",
                "Play count",
                "Release date",
                "Publisher",
            ][self.sort],
        )
    }

    fn hit(&mut self, action: impl Into<String>, rect: FRect) {
        self.hits.push(Hit {
            action: action.into(),
            rect,
        });
    }

    fn rect(&self, canvas: &mut Canvas<Window>, rect: FRect, color: Color) {
        canvas.set_draw_color(color);
        let _ = canvas.fill_rect(rect);
    }

    fn render_menu(&mut self, canvas: &mut Canvas<Window>) {
        self.hits.clear();
        self.actions.clear();
        // The stock resume scene repositions these two home-menu nodes before
        // drawing its shelf: gametitle Y 182 -> 234, cardlist Y 0 -> 69.
        let suspend_layout = matches!(
            self.panel.as_str(),
            "resume" | "confirm-overwrite" | "confirm-delete"
        );
        let title_shift = if suspend_layout { -52.0 } else { 0.0 };
        let card_shift = if suspend_layout { -69.0 } else { 0.0 };
        let elapsed = self.animation_start.elapsed().as_secs_f32();
        let progress = (self.scroll_started.elapsed().as_secs_f32() / 0.24).clamp(0.0, 1.0);
        self.scroll_current = if self.motion {
            self.scroll_from + (self.scroll_to - self.scroll_from) * progress
        } else {
            self.scroll_to
        };
        let wallpaper_duration = self
            .art
            .layout
            .get("wallpaperDuration")
            .and_then(Value::as_f64)
            .unwrap_or(1.0) as f32;
        let offset = if self.motion {
            elapsed.rem_euclid(wallpaper_duration.max(0.1)) / wallpaper_duration.max(0.1) * 1536.0
        } else {
            0.0
        };
        for tile in -1..=1 {
            let dest = FRect::new(-128.0 - offset + tile as f32 * 1536.0, 87.0, 1536.0, 552.0);
            self.art
                .draw_texture(canvas, "skin/wall.png", dest, None, Color::WHITE, true);
        }
        self.art.sprite(
            canvas,
            "caption_title",
            640.0,
            178.0 + title_shift,
            1044.0,
            66.0,
            Color::WHITE,
        );
        let title = self.selected().title.clone();
        self.art.text(
            canvas,
            &title,
            640.0,
            176.0 + title_shift,
            "title",
            self.art.title_color,
            990.0,
            true,
        );
        let gap = self
            .art
            .layout
            .get("cardSpacing")
            .and_then(Value::as_f64)
            .unwrap_or(300.0) as f32;
        for i in self.scroll_current.floor() as i32 - 3..=self.scroll_current.floor() as i32 + 3 {
            let index = i.rem_euclid(self.games.len() as i32) as usize;
            let game = self.games[index].clone();
            let firmware_locked = !self.firmware_ready(&game);
            let x = (640.0 + (i as f32 - self.scroll_current) * gap).round();
            if !(-150.0..=1430.0).contains(&x) {
                continue;
            }
            let active = index == self.selected;
            self.art.sprite(
                canvas,
                if active {
                    "game_card_active"
                } else {
                    "game_card"
                },
                x,
                360.0 + card_shift,
                252.0,
                276.0,
                Color::WHITE,
            );
            self.art.fit_image(
                canvas,
                &format!("games/{}/{}.png", game.id, game.id),
                FRect::new(x - 114.0, 234.0 + card_shift, 228.0, 204.0),
                if firmware_locked { 165 } else { 255 },
            );
            if firmware_locked {
                self.art.sprite(
                    canvas,
                    "icon_lock",
                    x + 98.0,
                    239.0 + card_shift,
                    28.0,
                    36.0,
                    Color::WHITE,
                );
            }
            let player_icon = if game.players > 1 {
                if game.simultaneous {
                    "icon_2P_simultaneous"
                } else {
                    "icon_1P-2P"
                }
            } else {
                "icon_1P"
            };
            self.art.sprite(
                canvas,
                player_icon,
                x + 81.0,
                462.0 + card_shift,
                66.0,
                42.0,
                Color::WHITE,
            );
            for slot in 0..4 {
                let saved = self.saved.contains(&(game.id.clone(), slot));
                let locked = saved
                    && self
                        .preferences
                        .pointer(&format!("/suspendLocks/{}/{slot}", game.id))
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                let name = format!(
                    "{}{}",
                    if locked {
                        "icon_Resume_lock"
                    } else if saved {
                        "icon_Resume_on"
                    } else {
                        "icon_Resume_off"
                    },
                    if active { "_active" } else { "" }
                );
                self.art.sprite(
                    canvas,
                    &name,
                    x - 96.0 + slot as f32 * 24.0,
                    468.0 + card_shift,
                    18.0,
                    18.0,
                    Color::WHITE,
                );
            }
            self.hit(
                format!("game:{}", game.id),
                FRect::new(x - 126.0, 222.0 + card_shift, 252.0, 276.0),
            );
        }
        if self.menu_focus == MenuFocus::Games && !suspend_layout {
            self.art.selection(
                canvas,
                640.0 + (self.scroll_to - self.scroll_current) * gap,
                360.0,
                278.0,
                302.0,
                if self.motion { elapsed } else { 0.5 },
            );
        }
        self.art.sprite(
            canvas,
            "arrow_left",
            38.0,
            360.0 + card_shift,
            32.0,
            44.0,
            Color::WHITE,
        );
        self.art.sprite(
            canvas,
            "arrow_right",
            1242.0,
            360.0 + card_shift,
            32.0,
            44.0,
            Color::WHITE,
        );
        self.hit("previous", FRect::new(0.0, 240.0, 65.0, 255.0));
        self.hit("next", FRect::new(1215.0, 240.0, 65.0, 255.0));
        let step = 42.0_f32.min(1080.0 / self.games.len() as f32);
        let left = 640.0 - (self.games.len() - 1) as f32 * step * 0.5;
        for index in 0..self.games.len() {
            let game = self.games[index].clone();
            let x = left + index as f32 * step;
            self.art.fit_image(
                canvas,
                &format!("games/{}/{}_small.png", game.id, game.id),
                FRect::new(x - 16.0, 535.0, 32.0, 32.0),
                if index == self.selected { 255 } else { 190 },
            );
            if index == self.selected {
                self.art.sprite(
                    canvas,
                    "curor_thumbnail_1",
                    x,
                    522.0,
                    24.0,
                    16.0,
                    Color::WHITE,
                );
            }
            if !self.firmware_ready(&game) {
                self.art.sprite(
                    canvas,
                    "icon_lock",
                    x + 9.0,
                    542.0,
                    14.0,
                    18.0,
                    Color::WHITE,
                );
            }
            self.hit(
                format!("game:{}", game.id),
                FRect::new(x - step * 0.5, 526.0, step, 55.0),
            );
        }
        let prompts = [
            (
                "icon_direction_up_light",
                self.art.translate("sys_gametitlelist_hud_Menu", "Menu"),
                36.0,
                "settings",
            ),
            (
                "icon_direction_down_light",
                self.art.translate("sys_gametitlelist_hud_Resume", "Resume"),
                36.0,
                "resume-panel",
            ),
            (
                "icon_select_light",
                self.art.translate("sys_gametitlelist_hud_Sort", "Sort"),
                96.0,
                "sort",
            ),
            (
                "icon_start_light",
                self.art.translate("sys_gametitlelist_hud_Decide", "Start"),
                84.0,
                "start",
            ),
        ];
        let item_gap = self
            .art
            .layout
            .get("hudItemGap")
            .and_then(Value::as_f64)
            .unwrap_or(8.0) as f32;
        let group_gap = self
            .art
            .layout
            .get("hudGroupGap")
            .and_then(Value::as_f64)
            .unwrap_or(24.0) as f32;
        let widths: Vec<_> = prompts
            .iter()
            .map(|(_, label, icon, _)| *icon + item_gap + self.art.measure(label, "s").advance)
            .collect();
        let width: f32 = widths.iter().sum::<f32>() + group_gap * 3.0;
        let mut x = 640.0 - width * 0.5;
        for ((icon, label, icon_width, action), whole) in prompts.iter().zip(widths) {
            let label_width = self.art.measure(label, "s").advance;
            self.art.sprite(
                canvas,
                icon,
                x + icon_width * 0.5,
                609.0,
                *icon_width,
                if *icon_width == 36.0 { 36.0 } else { 30.0 },
                Color::WHITE,
            );
            self.art.text(
                canvas,
                label,
                x + icon_width + item_gap + label_width * 0.5,
                609.0,
                "s",
                self.art.hud_color,
                1200.0,
                true,
            );
            self.hit(*action, FRect::new(x - 5.0, 585.0, whole + 10.0, 48.0));
            x += whole + group_gap;
        }
        self.art.sprite(
            canvas,
            "menubar_U",
            640.0,
            36.0,
            1296.0,
            120.0,
            Color::WHITE,
        );
        self.art.sprite(
            canvas,
            "menubar_L",
            640.0,
            690.0,
            1296.0,
            120.0,
            Color::WHITE,
        );
        let icons = [
            "btn_display",
            "btn_option",
            "btn_language",
            "btn_copyright",
            "manual",
        ];
        for (index, icon) in icons.iter().enumerate() {
            let bx = 640.0 + (index as f32 - 2.0) * 96.0;
            let keyboard_focus = self.menu_focus == MenuFocus::Toolbar
                && self.toolbar_index == index
                && self.panel.is_empty();
            if self.hover == format!("toolbar:{index}")
                || self.panel == format!("toolbar:{index}")
                || keyboard_focus
            {
                self.art.sprite(
                    canvas,
                    "btn_menubar_active",
                    bx,
                    62.0,
                    96.0,
                    70.0,
                    Color::WHITE,
                );
            }
            self.art
                .sprite(canvas, icon, bx, 62.0, 72.0, 72.0, Color::WHITE);
            self.hit(
                format!("toolbar:{index}"),
                FRect::new(bx - 46.0, 27.0, 92.0, 70.0),
            );
            if (self.hover == format!("toolbar:{index}") || keyboard_focus) && self.panel.is_empty()
            {
                let key = [
                    "sys_menubarU_cm_Display",
                    "sys_menubarU_cm_Option",
                    "sys_menubarU_cm_Language",
                    "sys_menubarU_cm_Copyright",
                    "sys_menubarU_cm_Manual",
                ][index];
                let label = self.art.translate(
                    key,
                    ["Display", "Settings", "Language", "Game info", "Help"][index],
                );
                let style = self
                    .art
                    .layout
                    .get("tooltip")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                let body_y = style.get("bodyY").and_then(Value::as_f64).unwrap_or(122.0) as f32;
                let height = style.get("height").and_then(Value::as_f64).unwrap_or(66.0) as f32;
                let edge = style
                    .get("edgeWidth")
                    .and_then(Value::as_f64)
                    .unwrap_or(12.0) as f32;
                let minimum = style
                    .get("minimumWidth")
                    .and_then(Value::as_f64)
                    .unwrap_or(144.0) as f32;
                let body = minimum.max(self.art.measure(&label, "s").advance);
                self.art
                    .sprite(canvas, "caption", bx, body_y, body, height, Color::WHITE);
                self.art.sprite(
                    canvas,
                    "caption_edge_left",
                    bx - body * 0.5 - edge * 0.5,
                    body_y,
                    edge,
                    height,
                    Color::WHITE,
                );
                self.art.sprite(
                    canvas,
                    "caption_edge",
                    bx + body * 0.5 + edge * 0.5,
                    body_y,
                    edge,
                    height,
                    Color::WHITE,
                );
                self.art.sprite(
                    canvas,
                    "caption_arrow",
                    bx,
                    body_y - height * 0.5 + 9.0,
                    24.0,
                    24.0,
                    Color::WHITE,
                );
                let position = self
                    .art
                    .layout
                    .pointer("/labels/tooltip/position/1")
                    .and_then(Value::as_f64)
                    .unwrap_or(128.0) as f32;
                self.art.text(
                    canvas,
                    &label,
                    bx,
                    position,
                    "s",
                    Color::WHITE,
                    1200.0,
                    true,
                );
            }
        }
        if self.panel == "resume" {
            self.render_suspend(canvas);
        } else if self.panel == "confirm-overwrite" || self.panel == "confirm-delete" {
            self.render_suspend(canvas);
            self.render_suspend_confirmation(canvas);
        } else if self.panel == "settings" || self.panel.starts_with("toolbar:") {
            self.render_toolbar(canvas);
        } else if self.panel == "emulator-settings"
            || self.panel == "help"
            || self.panel == "message"
            || self.panel == "profiles"
        {
            self.render_panel(canvas);
        } else if self.panel == "notice" {
            self.dialog(canvas, 304.0, 312.0, 672.0, 96.0);
            let message = self.message.clone();
            self.art.text(
                canvas,
                &message,
                640.0,
                356.0,
                "l",
                Color::WHITE,
                610.0,
                true,
            );
        }
    }

    fn dialog(&mut self, canvas: &mut Canvas<Window>, x: f32, y: f32, width: f32, height: f32) {
        self.rect(canvas, FRect::new(x, y, width, height), Color::BLACK);
        canvas.set_draw_color(Color::WHITE);
        for inset in 0..3 {
            let _ = canvas.draw_rect(FRect::new(
                x + inset as f32,
                y + inset as f32,
                width - inset as f32 * 2.0,
                height - inset as f32 * 2.0,
            ));
        }
    }

    fn render_panel(&mut self, canvas: &mut Canvas<Window>) {
        self.hits.clear();
        self.actions.clear();
        self.rect(
            canvas,
            FRect::new(0.0, 96.0, STAGE_W, 550.0),
            Color::RGBA(0, 0, 0, 190),
        );
        self.dialog(canvas, 200.0, 128.0, 880.0, 478.0);
        let page = if self.panel == "help" {
            Some(4)
        } else {
            self.panel
                .strip_prefix("toolbar:")
                .and_then(|v| v.parse::<usize>().ok())
        };
        let settings = self.panel == "emulator-settings" || page == Some(1);
        let heading = match (self.panel.as_str(), page) {
            ("message", _) => "Classic Menu".to_owned(),
            ("profiles", _) => "Image Library".to_owned(),
            (_, Some(0)) => self.art.translate("sys_display_Title", "Display"),
            (_, Some(1)) => self.art.translate("sys_option_Title", "Settings"),
            (_, Some(2)) => self.art.translate("sys_language_hud_Title", "Language"),
            (_, Some(3)) => self.selected().title.clone(),
            (_, Some(4)) => "Help".to_owned(),
            ("emulator-settings", _) => "Emulator / Image Library".to_owned(),
            _ => self.art.translate("sys_resume_Title", "Suspend points"),
        };
        self.art.text(
            canvas,
            &heading,
            640.0,
            174.0,
            "l",
            Color::WHITE,
            790.0,
            true,
        );
        let mut y = 227.0;
        let entries: Vec<(String, String)> = if self.panel == "profiles" {
            let mut profiles: Vec<_> = self
                .profiles
                .iter()
                .map(|profile| {
                    let name = profile
                        .directory
                        .file_name()
                        .and_then(|value| value.to_str())
                        .unwrap_or_default()
                        .to_owned();
                    (format!("profile:{name}"), profile.label())
                })
                .collect();
            profiles.push((
                "import-image".into(),
                "Import image or switch version...".into(),
            ));
            profiles
        } else if page == Some(2) {
            self.art
                .languages
                .keys()
                .map(|locale| {
                    let label = match locale.as_str() {
                        "usa_en" => "English",
                        "eur_de" => "Deutsch",
                        "eur_fr" => "Français",
                        "eur_es" => "Español",
                        "eur_it" => "Italiano",
                        "eur_nl" => "Nederlands",
                        "eur_pt" => "Português (Europeu)",
                        "eur_ru" => "Русский",
                        "jpn_ja" => "Japanese",
                        other => other,
                    };
                    (
                        format!("language:{locale}"),
                        format!(
                            "{}{}",
                            if *locale == self.art.locale {
                                "[x] "
                            } else {
                                "[ ] "
                            },
                            label
                        ),
                    )
                })
                .collect()
        } else if self.panel == "message" || page == Some(3) || page == Some(4) {
            Vec::new()
        } else if settings {
            vec![
                ("rom".into(), "Choose ROM for selected game".into()),
                (
                    "firmware".into(),
                    "Choose coprocessor firmware folder".into(),
                ),
                (
                    "profiles".into(),
                    "Image Library / Switch Version (F2)".into(),
                ),
                (
                    "sound".into(),
                    format!("Menu sound: {}", if self.sound { "On" } else { "Off" }),
                ),
                (
                    "motion".into(),
                    format!("Animation: {}", if self.motion { "On" } else { "Off" }),
                ),
                ("slot".into(), format!("Suspend slot: {}", self.slot + 1)),
                ("fullscreen".into(), "Toggle full screen".into()),
            ]
        } else if page == Some(0) {
            vec![
                ("fullscreen".into(), "Toggle full screen".into()),
                (
                    "motion".into(),
                    format!("Animation: {}", if self.motion { "On" } else { "Off" }),
                ),
            ]
        } else {
            let is_saved = self
                .saved
                .contains(&(self.selected().id.clone(), self.slot));
            vec![
                ("slot".into(), "Choose next slot".into()),
                (
                    "resume".into(),
                    format!(
                        "Resume selected slot ({})",
                        if is_saved { "Saved" } else { "Empty" }
                    ),
                ),
                ("save".into(), "Save current session to this slot".into()),
            ]
        };
        if self.panel == "message" {
            let message = self.message.clone();
            for line in wrap_text(&self.art, &message, "s", 770.0) {
                self.art
                    .text(canvas, &line, 640.0, y, "s", Color::WHITE, 790.0, false);
                y += 32.0;
            }
        } else {
            let selected = self.selected().clone();
            let body: Vec<String> = match page {
                Some(2) => vec![
                    if self.art.languages.len() > 1 {
                        "Choose the language for Clover menu labels.".into()
                    } else {
                        "Re-import this image to add all shipped language files.".into()
                    },
                    "Game titles, regional artwork and custom controls are unchanged.".into(),
                ],
                Some(3) => vec![
                    format!("{}    {}", selected.publisher, selected.release),
                    selected.copyright.clone(),
                    "Core: ares (embedded, no external emulator)".into(),
                ],
                Some(4) => vec![
                    "Arrows: select    Return: start    Tab: sort".into(),
                    "Up: toolbar    Down: suspend points".into(),
                    "Z B, X A, A Y, S X, Q L, W R".into(),
                    "Escape: pause and pick a suspend slot; F5/F8: save/load".into(),
                ],
                _ if self.panel == "profiles" => vec![
                    "Choose a verified cached profile, or import another SNES Classic image."
                        .into(),
                    "The source image is read-only; extracted programs are never run.".into(),
                ],
                _ if settings => {
                    vec!["Keyboard: arrows select; Return chooses; Escape closes.".into()]
                }
                _ => vec![
                    format!(
                        "Slot {} - {}",
                        self.slot + 1,
                        if self.saved.contains(&(selected.id.clone(), self.slot)) {
                            "Saved"
                        } else {
                            "Empty"
                        }
                    ),
                    "Suspend points use this profile's data directory.".into(),
                ],
            };
            for line in body {
                self.art
                    .text(canvas, &line, 640.0, y, "m", Color::WHITE, 790.0, true);
                y += 38.0;
            }
        }
        self.focus = self.focus.min(entries.len());
        self.actions = entries.iter().map(|(action, _)| action.clone()).collect();
        let visible_count = entries
            .len()
            .min(((530.0 - y) / 24.0).floor().max(1.0) as usize);
        let first_row = self
            .focus
            .saturating_add(1)
            .saturating_sub(visible_count)
            .min(entries.len().saturating_sub(visible_count));
        let row_height = 43.0_f32
            .min((530.0 - y) / visible_count.max(1) as f32)
            .max(24.0);
        for (index, (action, label)) in entries
            .into_iter()
            .enumerate()
            .skip(first_row)
            .take(visible_count)
        {
            let rect = FRect::new(365.0, y - 14.0, 550.0, row_height);
            if self.focus == index || self.hover == action {
                self.art.sprite(
                    canvas,
                    "btn_init_on",
                    640.0,
                    rect.y + rect.h * 0.5,
                    570.0,
                    row_height - 1.0,
                    Color::WHITE,
                );
            }
            self.art.text(
                canvas,
                &label,
                640.0,
                rect.y + rect.h * 0.5,
                "m",
                Color::WHITE,
                520.0,
                true,
            );
            self.hit(action.clone(), rect);
            y += row_height;
        }
        let back = self.art.translate("sys_option_hud_Return", "Back");
        let back_width = self.art.measure(&back, "s").advance;
        let total = 40.0 + back_width;
        self.art.sprite(
            canvas,
            "icon_B_light",
            640.0 - total * 0.5 + 18.0,
            567.0,
            36.0,
            36.0,
            Color::WHITE,
        );
        self.art.text(
            canvas,
            &back,
            640.0 - total * 0.5 + 40.0 + back_width * 0.5,
            567.0,
            "s",
            Color::WHITE,
            1200.0,
            true,
        );
        self.hit(
            "close",
            FRect::new(640.0 - total * 0.5 - 12.0, 543.0, total + 24.0, 48.0),
        );
        self.actions.push("close".into());
    }

    fn render_game(&mut self, canvas: &mut Canvas<Window>) {
        let mode = self.display_mode();
        let width = if mode == 2 { 720.0 * 8.0 / 7.0 } else { 960.0 };
        let left = (1280.0 - width) * 0.5;
        if let Some(texture) = self.frame_texture.as_ref() {
            let _ = canvas.copy(texture, None, FRect::new(left, 0.0, width, 720.0));
            if mode == 0 {
                canvas.set_blend_mode(BlendMode::Blend);
                canvas.set_draw_color(Color::RGBA(0, 0, 0, 64));
                for row in 0..224 {
                    let _ =
                        canvas.fill_rect(FRect::new(left, row as f32 * 720.0 / 224.0, width, 1.0));
                }
            }
        }
    }

    fn run(
        &mut self,
        canvas: &mut Canvas<Window>,
        frame_limit: usize,
        screenshot: Option<&Path>,
    ) -> Result<(), String> {
        let mut events = sdl3::init()
            .map_err(|e| e.to_string())?
            .event_pump()
            .map_err(|e| e.to_string())?;
        let mut running = true;
        let mut stage = self
            .art
            .creator
            .create_texture(PixelFormat::RGBA8888, TextureAccess::Target, 1280, 720)
            .map_err(|e| e.to_string())?;
        stage.set_blend_mode(BlendMode::None);
        stage.set_scale_mode(ScaleMode::Linear);
        canvas.set_blend_mode(BlendMode::Blend);
        let mut next_frame = Instant::now();
        let mut rendered = 0usize;
        while running {
            for event in events.poll_iter() {
                match event {
                    Event::Quit { .. } => running = false,
                    #[cfg(target_os = "macos")]
                    Event::User { type_, code, .. }
                        if type_ == unsafe { clover_native_menu_event() } =>
                    {
                        if self.playing {
                            self.leave_game();
                        }
                        match code {
                            1 => self.activate("profiles", canvas),
                            2 => self.choose_image(canvas),
                            3 => self.activate("toolbar:1", canvas),
                            4 => self.choose_rom(canvas, false),
                            5 => self.choose_firmware(canvas),
                            6 => self.activate("sort", canvas),
                            7 => self.activate("sound", canvas),
                            8 => self.activate("fullscreen", canvas),
                            9 => self.activate("toolbar:4", canvas),
                            10 => running = false,
                            _ => {}
                        }
                    }
                    Event::KeyDown {
                        scancode: Some(code),
                        repeat,
                        ..
                    } => {
                        self.held.insert(code);
                        self.handle_key(code, repeat, canvas);
                    }
                    Event::KeyUp {
                        scancode: Some(code),
                        ..
                    } => {
                        self.held.remove(&code);
                    }
                    Event::MouseMotion { x, y, .. } if !self.playing => {
                        self.menu_focus = MenuFocus::Games;
                        let (x, y) = to_stage(canvas, x, y);
                        self.hover.clear();
                        for hit in self.hits.iter().rev() {
                            if point_inside(x, y, hit.rect) {
                                self.hover = hit.action.clone();
                                break;
                            }
                        }
                    }
                    Event::MouseWheel { y, .. } if !self.playing && self.panel == "toolbar:3" => {
                        for _ in 0..(y.abs().ceil() as usize).min(8) {
                            self.scroll_legal(y < 0.0);
                        }
                    }
                    Event::MouseButtonDown {
                        mouse_btn: sdl3::mouse::MouseButton::Left,
                        clicks,
                        x,
                        y,
                        ..
                    } if !self.playing => {
                        let (x, y) = to_stage(canvas, x, y);
                        let action = self
                            .hits
                            .iter()
                            .rev()
                            .find(|hit| point_inside(x, y, hit.rect))
                            .map(|hit| hit.action.clone());
                        if let Some(action) = action {
                            self.activate(&action, canvas);
                            if clicks >= 2 && action.starts_with("game:") {
                                self.begin_game(canvas);
                            }
                        }
                    }
                    Event::ControllerDeviceAdded { which, .. } if self.gamepad.is_none() => {
                        self.gamepad = self
                            .gamepad_subsystem
                            .open(sdl3::sys::joystick::SDL_JoystickID(which))
                            .ok();
                    }
                    Event::ControllerDeviceRemoved { which, .. }
                        if self
                            .gamepad
                            .as_ref()
                            .is_some_and(|pad| pad.id().is_ok_and(|id| id == which)) =>
                    {
                        self.gamepad = None;
                        self.pad_held.clear();
                    }
                    Event::ControllerButtonDown { which, button, .. }
                        if self
                            .gamepad
                            .as_ref()
                            .is_some_and(|pad| pad.id().is_ok_and(|id| id == which)) =>
                    {
                        self.pad_button_down(button, canvas);
                    }
                    Event::ControllerButtonUp { which, button, .. }
                        if self
                            .gamepad
                            .as_ref()
                            .is_some_and(|pad| pad.id().is_ok_and(|id| id == which)) =>
                    {
                        self.pad_held.remove(&button);
                    }
                    Event::DropFile { filename, .. } => {
                        let path = PathBuf::from(filename);
                        let extension = path
                            .extension()
                            .and_then(|value| value.to_str())
                            .unwrap_or_default();
                        if matches!(extension.to_ascii_lowercase().as_str(), "sfc" | "smc") {
                            let game = self.selected().id.clone();
                            self.preferences["roms"][game] = json!(path.to_string_lossy());
                            let _ = self.save_settings();
                            self.begin_game(canvas);
                        } else {
                            self.panel = "message".into();
                            self.message = "Drop a compatible .sfc or .smc ROM.".into();
                        }
                    }
                    _ => {}
                }
            }
            self.process_dialogs(canvas);
            if self.title_dirty {
                self.title_dirty = false;
                if let Err(error) = canvas.window_mut().set_title(&self.window_title) {
                    self.panel = "message".into();
                    self.message = format!("Cannot update window title: {error}");
                }
            }
            if self.fullscreen_dirty {
                self.fullscreen_dirty = false;
                if let Err(error) = canvas.window_mut().set_fullscreen(self.fullscreen) {
                    self.panel = "message".into();
                    self.message = format!("Cannot change full-screen mode: {error}");
                }
            }
            if self.playing && self.panel.is_empty() {
                self.run_game_frame()?;
            } else if self.sound
                && let Some(music) = &self.music
            {
                music.play_loop();
            }
            // Resolve atlas pixels on a fixed stage before scaling to Retina/window size.
            canvas
                .with_texture_canvas(&mut stage, |canvas| {
                    canvas.set_draw_color(Color::BLACK);
                    canvas.clear();
                    if self.playing && self.panel.is_empty() {
                        self.render_game(canvas);
                    } else {
                        self.render_menu(canvas);
                    }
                })
                .map_err(|e| e.to_string())?;
            canvas.set_draw_color(Color::BLACK);
            canvas.clear();
            canvas
                .copy(&stage, None, FRect::new(0.0, 0.0, 1280.0, 720.0))
                .map_err(|e| e.to_string())?;
            if frame_limit != 0
                && rendered + 1 >= frame_limit
                && let Some(path) = screenshot
            {
                save_screenshot(canvas, path)?;
            }
            canvas.present();
            rendered += 1;
            if frame_limit != 0 && rendered >= frame_limit {
                running = false;
            }
            next_frame += std::time::Duration::from_micros(16_667);
            let now = Instant::now();
            if next_frame > now {
                std::thread::sleep(next_frame - now);
            } else {
                next_frame = now;
            }
            if self.panel == "notice" && self.scroll_started.elapsed().as_secs_f32() > 2.5 {
                self.panel.clear();
            }
        }
        if self.playing {
            self.playing = false;
            self.capture_pending()?;
        }
        self.save_settings()?;
        if let Some(core) = self.core.as_mut()
            && core.is_loaded()
        {
            core.unload()
                .map_err(|error| format!("Cannot flush game save memory: {error}"))?;
        }
        Ok(())
    }
}

fn point_inside(x: f32, y: f32, rect: FRect) -> bool {
    x >= rect.x && y >= rect.y && x < rect.x + rect.w && y < rect.y + rect.h
}

fn to_stage(canvas: &Canvas<Window>, x: f32, y: f32) -> (f32, f32) {
    let (width, height) = canvas.window().size();
    if width == 0 || height == 0 {
        return (x, y);
    }
    let scale = (width as f32 / STAGE_W).min(height as f32 / 720.0);
    let left = (width as f32 - STAGE_W * scale) * 0.5;
    let top = (height as f32 - 720.0 * scale) * 0.5;
    ((x - left) / scale, (y - top) / scale)
}

fn wrap_text(art: &UiAssets, text: &str, font: &str, width: f32) -> Vec<String> {
    let mut result = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let candidate = if line.is_empty() {
            word.to_owned()
        } else {
            format!("{line} {word}")
        };
        if !line.is_empty() && art.measure(&candidate, font).advance > width {
            result.push(std::mem::take(&mut line));
            line = word.to_owned();
        } else {
            line = candidate;
        }
    }
    if !line.is_empty() {
        result.push(line);
    }
    result
}

fn safe_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() < 100
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn required_firmware_available(required: &[String], directory: &Path) -> bool {
    required.iter().all(|name| directory.join(name).is_file())
}

fn read_games(assets_path: &Path) -> Result<Vec<Game>, String> {
    let catalog = read_json(&assets_path.join("catalog.json"))?;
    if catalog.get("schema").and_then(Value::as_u64) != Some(1) {
        return Err("Unsupported catalog schema".into());
    }
    let entries = catalog
        .get("games")
        .and_then(Value::as_array)
        .ok_or("Catalog has no games")?;
    let mut ids = HashSet::new();
    let mut games = Vec::new();
    for item in entries {
        let id = item
            .get("id")
            .and_then(Value::as_str)
            .ok_or("Catalog game has no id")?
            .to_owned();
        if !safe_id(&id) || !ids.insert(id.clone()) {
            return Err("Invalid or duplicate catalog game ID".into());
        }
        let string = |key: &str, fallback: &str| {
            item.get(key)
                .and_then(Value::as_str)
                .unwrap_or(fallback)
                .to_owned()
        };
        games.push(Game {
            title: string("title", &id),
            sort_title: string("sortTitle", &string("title", &id)),
            publisher: string("publisher", ""),
            release: string("releaseDate", ""),
            copyright: string("copyright", ""),
            rom: string("rom", ""),
            import_error: string("importError", ""),
            players: item.get("players").and_then(Value::as_u64).unwrap_or(1) as u32,
            simultaneous: item
                .get("simultaneous")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            firmware_required: item
                .get("firmwareRequired")
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
            id,
        });
    }
    if games.is_empty() {
        return Err("Empty game catalog".into());
    }
    Ok(games)
}

fn read_json(path: &Path) -> Result<Value, String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    if !metadata.file_type().is_file() || metadata.len() > 32 * 1024 * 1024 {
        return Err(format!("Invalid JSON file: {}", path.display()));
    }
    let bytes = fs::read(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("Invalid JSON {}: {e}", path.display()))
}

fn atomic_json(path: &Path, value: &Value) -> Result<(), String> {
    let parent = path.parent().ok_or("Settings path has no parent")?;
    fs::create_dir_all(parent).map_err(|e| format!("Cannot create settings directory: {e}"))?;
    let temporary = path.with_extension("json.tmp");
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    fs::write(&temporary, bytes).map_err(|e| format!("Cannot write settings: {e}"))?;
    if path.exists() {
        let backup = path.with_extension("json.previous");
        let _ = fs::remove_file(&backup);
        fs::rename(path, backup).map_err(|e| format!("Cannot preserve previous settings: {e}"))?;
    }
    fs::rename(&temporary, path).map_err(|e| format!("Cannot publish settings: {e}"))
}

fn chrono_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn default_data_dir() -> PathBuf {
    if cfg!(target_os = "macos") {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Library/Application Support/ClassicMenu/ClassicMenu")
    } else if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ClassicMenu/ClassicMenu")
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(".local/share")
            })
            .join("classicmenu")
    }
}

/// Start the imported-profile application. Use `run --image FILE` to import and
/// launch in one step, or `run --assets DIR --data-dir DIR` for an existing profile.
pub fn run(args: &[std::ffi::OsString]) -> Result<(), String> {
    let mut image = None;
    let mut assets = None;
    let mut data = None;
    let mut screenshot = None;
    let mut launcher_preview = None;
    let mut preview_panel = None;
    let mut preview_hover = None;
    let mut language = None;
    let mut exercise_ui = None;
    let mut frame_limit = std::env::var("CLOVER_RUST_WINDOW_FRAMES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let mut at = 0;
    while at < args.len() {
        let option = args[at].to_str().ok_or("Argument is not valid UTF-8")?;
        at += 1;
        let value = args
            .get(at)
            .ok_or_else(|| format!("Missing value for {option}"))?;
        at += 1;
        match option {
            "--image" => image = Some(PathBuf::from(value)),
            "--assets" => assets = Some(PathBuf::from(value)),
            "--data-dir" => data = Some(PathBuf::from(value)),
            "--screenshot" => screenshot = Some(PathBuf::from(value)),
            "--launcher-preview" => launcher_preview = Some(PathBuf::from(value)),
            "--preview-panel" => preview_panel = value.to_str().map(str::to_owned),
            "--preview-hover" => preview_hover = value.to_str().map(str::to_owned),
            "--language" => language = value.to_str().map(str::to_owned),
            "--exercise-ui" => exercise_ui = Some(PathBuf::from(value)),
            "--frames" => {
                frame_limit = value
                    .to_str()
                    .ok_or("Frame count is not UTF-8")?
                    .parse()
                    .map_err(|_| "Invalid frame count")?;
            }
            _ => return Err(format!("Unknown run option: {option}")),
        }
    }
    if image.is_some() && assets.is_some() {
        return Err("Choose --image or --assets, not both".into());
    }
    let data = data.unwrap_or_else(default_data_dir);
    let profile_home = data.clone();
    let profile_cache = data.join("profiles");
    if let Some(path) = launcher_preview {
        launcher::select(&data, Some(&path))?;
        return Ok(());
    }
    let (assets, profile_data) = if let Some(image) = image {
        let profile = crate::profile::import_image(&image, &profile_cache, None)?;
        let last = json!({"directory": profile.directory.file_name().and_then(|v| v.to_str()).unwrap_or_default()});
        atomic_json(&data.join("last-profile.json"), &last)?;
        (profile.assets(), profile.data())
    } else if let Some(assets) = assets {
        (assets, data)
    } else {
        let Some(selected) = launcher::select(&data, None)? else {
            return Ok(());
        };
        (selected.assets, selected.data)
    };
    let context = sdl3::init().map_err(|e| e.to_string())?;
    let video = context.video().map_err(|e| e.to_string())?;
    let gamepad_subsystem = context.gamepad().map_err(|e| e.to_string())?;
    let window = video
        .window("Classic Menu - Rust / ares", 1280, 720)
        .position_centered()
        .resizable()
        .high_pixel_density()
        .build()
        .map_err(|e| e.to_string())?;
    let mut canvas = window.into_canvas();
    canvas
        .set_logical_size(
            1280,
            720,
            sdl3::sys::render::SDL_LOGICAL_PRESENTATION_LETTERBOX,
        )
        .map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    unsafe {
        clover_install_native_menu(0);
    }
    let mut app = App::new(
        &canvas,
        assets,
        profile_data,
        gamepad_subsystem,
        profile_cache,
        profile_home,
    )?;
    if let Some(locale) = language {
        app.art.set_language(&locale)?;
    }
    if let Some(output) = exercise_ui {
        return app.exercise_suspend(&mut canvas, &output);
    }
    if frame_limit != 0 {
        if let Some(panel) = preview_panel {
            if panel == "resume" {
                app.open_suspend();
            } else if panel.starts_with("toolbar:")
                || matches!(panel.as_str(), "settings" | "emulator-settings" | "help")
            {
                app.activate(&panel, &canvas);
            } else {
                app.panel = panel;
            }
        }
        if let Some(hover) = preview_hover {
            app.hover = hover;
        }
    }
    canvas
        .window_mut()
        .set_title(&app.window_title)
        .map_err(|e| e.to_string())?;
    app.run(&mut canvas, frame_limit, screenshot.as_deref())
}

fn save_screenshot(canvas: &Canvas<Window>, path: &Path) -> Result<(), String> {
    let surface = canvas
        .read_pixels(None)
        .map_err(|e| format!("Cannot read rendered frame: {e}"))?;
    let rgba = surface
        .convert_format(PixelFormat::RGBA32)
        .map_err(|e| format!("Cannot convert rendered frame: {e}"))?;
    let (width, height, pitch) = (rgba.width(), rgba.height(), rgba.pitch() as usize);
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
    rgba.with_lock(|source| {
        for row in 0..height as usize {
            let start = row * pitch;
            pixels.extend_from_slice(&source[start..start + width as usize * 4]);
        }
    });
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Cannot create screenshot directory: {e}"))?;
    }
    let file = File::create(path).map_err(|e| format!("Cannot create screenshot: {e}"))?;
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .map_err(|e| format!("Cannot encode screenshot: {e}"))?
        .write_image_data(&pixels)
        .map_err(|e| format!("Cannot write screenshot: {e}"))
}

/// Kept as a display/driver smoke test for headless build verification.
pub fn smoke_test() -> Result<(), String> {
    let context = sdl3::init().map_err(|e| e.to_string())?;
    let video = context.video().map_err(|e| e.to_string())?;
    let window = video
        .window("Classic Menu - Rust SDL3 shell", 1280, 720)
        .position_centered()
        .resizable()
        .high_pixel_density()
        .build()
        .map_err(|e| e.to_string())?;
    let mut canvas = window.into_canvas();
    canvas
        .set_logical_size(
            1280,
            720,
            sdl3::sys::render::SDL_LOGICAL_PRESENTATION_LETTERBOX,
        )
        .map_err(|e| e.to_string())?;
    let frames = std::env::var("CLOVER_RUST_WINDOW_FRAMES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    let mut count = 0;
    loop {
        canvas.set_draw_color(Color::RGB(12, 19, 38));
        canvas.clear();
        canvas.present();
        count += 1;
        if frames != 0 && count >= frames {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::required_firmware_available;
    use std::fs;

    #[test]
    fn missing_coprocessor_firmware_fails_closed() {
        let directory = std::env::temp_dir().join(format!(
            "clover-rust-firmware-test-{}-{}",
            std::process::id(),
            super::chrono_now()
        ));
        fs::create_dir_all(&directory).unwrap();
        let required = vec!["dsp1.program.rom".to_owned(), "dsp1.data.rom".to_owned()];
        assert!(!required_firmware_available(&required, &directory));
        fs::write(directory.join(&required[0]), b"test firmware").unwrap();
        assert!(!required_firmware_available(&required, &directory));
        fs::write(directory.join(&required[1]), b"test firmware").unwrap();
        assert!(required_firmware_available(&required, &directory));
        assert!(required_firmware_available(&[], &directory));
        fs::remove_dir_all(directory).unwrap();
    }
}
