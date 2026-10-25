//! Rust SDL3 frontend. Clover's packed atlas, bitmap fonts, imported covers,
//! regional layout, carousel controls, and ares session all stay on this thread.

mod app_import;
mod checks;
mod controls;
mod dialog_requests;
mod gamepad;
mod host;
mod icon;
mod input;
mod launcher;
#[cfg(test)]
mod menu_carousel_tests;
mod menu_frame;
mod menu_overlay;
mod menu_projection;
#[cfg(test)]
mod menu_resource_tests;
mod panel_projection;
#[cfg(test)]
mod panel_replay_tests;
mod panel_scene;
mod presentation;
mod suspend;
mod texture_resources;
mod timing;
mod toolbar;
mod toolbar_projection;

use crate::frontend::catalog::{self, Game, safe_id};
use crate::frontend::menu_input::Key;
use serde_json::{Value, json};
use std::{
    cell::Cell,
    collections::{BTreeMap, HashMap, HashSet},
    fs::{self, File},
    io::{BufReader, Read},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender},
    time::Instant,
};

use sdl3::{
    GamepadSubsystem,
    audio::{AudioFormat, AudioSpec, AudioSpecWAV, AudioStreamOwner},
    dialog::{self, DialogFileFilter},
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

use crate::frontend::font::{Font, TextLayout};

struct UiAssets {
    nes: bool,
    palette: Value,
    root: PathBuf,
    creator: TextureCreator<WindowContext>,
    textures: HashMap<String, Texture>,
    texture_bindings: texture_resources::TextureBindings,
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
    fn execute_base_menu(
        &mut self,
        canvas: &mut Canvas<Window>,
        scene: &crate::frontend::menu_frame::BaseMenuScene,
        context: &menu_frame::BindingContext,
        hits: &mut Vec<Hit>,
    ) -> bool {
        if scene.generation != context.generation
            || context.generation != self.texture_bindings.generation()
        {
            return false;
        }
        for step in &scene.steps {
            match step {
                crate::frontend::menu_frame::Step::Draw(command) => {
                    self.draw_command(canvas, command);
                }
                crate::frontend::menu_frame::Step::Hit(hit) => {
                    if let Some(action) = menu_frame::resolve_hit(&hit.action, context) {
                        hits.push(Hit {
                            action,
                            rect: to_frect(hit.rect),
                        });
                    }
                }
            }
        }
        true
    }

    fn execute_notice(
        &mut self,
        canvas: &mut Canvas<Window>,
        scene: &crate::frontend::menu_overlay::NoticeScene,
    ) -> bool {
        if scene.generation != self.texture_bindings.generation() {
            return false;
        }
        menu_overlay::draw_dialog(canvas, scene.dialog);
        self.draw_commands(canvas, scene.commands.iter().copied());
        true
    }

    fn execute_panel(
        &mut self,
        canvas: &mut Canvas<Window>,
        scene: &panel_scene::PanelScene,
        bindings: &panel_scene::PanelBindings,
        hits: &mut Vec<Hit>,
        actions: &mut Vec<String>,
    ) -> bool {
        if scene.generation != bindings.generation
            || bindings.generation != self.texture_bindings.generation()
        {
            return false;
        }
        hits.clear();
        actions.clear();
        for step in &scene.steps {
            match step {
                panel_scene::PanelStep::Draw(command) => {
                    self.draw_command(canvas, command);
                }
                panel_scene::PanelStep::Dialog(rect) => menu_overlay::draw_dialog(canvas, *rect),
                panel_scene::PanelStep::Hit { key, rect } => {
                    if let Some(action) = bindings.resolve(key) {
                        hits.push(Hit {
                            action,
                            rect: to_frect(*rect),
                        });
                    }
                }
            }
        }
        actions.extend(scene.actions.iter().filter_map(|key| bindings.resolve(key)));
        true
    }
}

impl UiAssets {
    fn card_resources(
        &self,
        plan: &crate::frontend::menu_carousel::CardPlan,
        cover_key: &str,
    ) -> crate::frontend::menu_carousel::CardResources {
        let mut resources = crate::frontend::menu_carousel::CardResources::default();
        for part in &plan.parts {
            if let crate::frontend::menu_carousel::CardPart::Sprite(sprite) = part
                && let Some(source) = self.sprite_source(&sprite.name)
            {
                resources.sprites.insert(sprite.name.clone(), source);
            }
        }
        resources.cover = self.image_source(cover_key);
        resources
    }

    fn sprite_source(&self, name: &str) -> Option<crate::frontend::menu_scene::AtlasSprite> {
        let (key, flip_x) = crate::frontend::menu_scene::sprite_key(name);
        let source = self.sprites.get(&key).copied()?;
        let texture = self.texture_bindings.id("skin/packed.png")?;
        self.texture_bindings.key(texture)?;
        self.textures.get("skin/packed.png")?;
        Some(crate::frontend::menu_scene::AtlasSprite {
            texture,
            source: from_frect(source),
            flip_x,
        })
    }

    fn image_source(&self, key: &str) -> Option<crate::frontend::menu_scene::ImageSource> {
        let texture_id = self.texture_bindings.id(key)?;
        self.texture_bindings.key(texture_id)?;
        let query = self.textures.get(key)?.query();
        Some(crate::frontend::menu_scene::ImageSource {
            texture: texture_id,
            width: query.width,
            height: query.height,
        })
    }

    fn palette_color(&self, key: &str, fallback: Color) -> Color {
        let channels = crate::frontend::skin_metadata::palette_color(
            &self.palette,
            key,
            [fallback.r, fallback.g, fallback.b, fallback.a],
        );
        Color::RGBA(channels[0], channels[1], channels[2], channels[3])
    }

    fn new(canvas: &Canvas<Window>, root: &Path, games: &[Game]) -> Result<Self, String> {
        let creator = canvas.texture_creator();
        let layout = read_json(&root.join("skin/layout.json"))?;
        let strings_file = read_json(&root.join("skin/strings.json"))?;
        let (locale, default_strings) = crate::frontend::languages::default_table(&strings_file);
        let theme = read_json(&root.join("skin/theme.json"))?;
        let theme_info = crate::frontend::skin_metadata::decode_theme(&theme);
        let packed = read_json(&root.join("skin/packed.json"))?;
        let sprites = crate::frontend::skin_metadata::decode_sprites(&packed)?
            .into_iter()
            .map(|(key, frame)| (key, FRect::new(frame.x, frame.y, frame.w, frame.h)))
            .collect();
        let title = crate::frontend::skin_metadata::initial_color(&theme, "title", [0, 0, 0, 255]);
        let hud =
            crate::frontend::skin_metadata::initial_color(&theme, "hud", [255, 255, 255, 255]);
        let mut result = Self {
            nes: theme_info.nes,
            palette: theme_info.palette,
            root: root.to_path_buf(),
            creator,
            textures: HashMap::new(),
            texture_bindings: texture_resources::TextureBindings::new()?,
            sprites,
            fonts: BTreeMap::new(),
            layout,
            strings: default_strings.clone(),
            default_strings,
            languages: BTreeMap::new(),
            title_color: Color::RGBA(title[0], title[1], title[2], title[3]),
            hud_color: Color::RGBA(hud[0], hud[1], hud[2], hud[3]),
            region: theme_info.region,
            locale,
        };
        let language_index = root.join("skin/languages.json");
        if language_index.is_file() {
            let index = read_json(&language_index)?;
            for locale in crate::frontend::languages::decode_index(&index)? {
                let table = read_json(&root.join("skin/languages").join(format!("{locale}.json")))?;
                result
                    .languages
                    .insert(locale, crate::frontend::languages::decode_table(&table)?);
            }
        }
        crate::frontend::languages::register_default(
            &mut result.languages,
            result.locale.clone(),
            &result.default_strings,
        );
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
            let font = crate::frontend::font::decode(&data);
            result.load_texture(&format!("skin/font-{name}.png"))?;
            result.fonts.insert(name.to_owned(), font);
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
                .as_chunks::<3>()
                .0
                .iter()
                .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255])
                .collect(),
            png::ColorType::GrayscaleAlpha => bytes
                .as_chunks::<2>()
                .0
                .iter()
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
        self.texture_bindings.bind(key);
        Ok(())
    }

    fn translate(&self, key: &str, fallback: &str) -> String {
        crate::frontend::languages::translate(&self.strings, &self.default_strings, key, fallback)
    }

    fn set_language(&mut self, locale: &str) -> Result<(), String> {
        let strings = self.languages.get(locale).ok_or_else(|| format!("Language {locale} is not available in this profile. Re-import its image to add all shipped languages."))?;
        self.strings = strings.clone();
        self.locale = locale.to_owned();
        Ok(())
    }

    fn measure(&self, value: &str, primary_name: &str) -> TextLayout {
        crate::frontend::font::measure(&self.fonts, value, primary_name)
    }

    fn texture_mut(&mut self, id: crate::frontend::menu_scene::TextureId) -> Option<&mut Texture> {
        let key = self.texture_bindings.key(id)?;
        self.textures.get_mut(key)
    }

    fn draw_command(
        &mut self,
        canvas: &mut Canvas<Window>,
        command: &crate::frontend::menu_scene::Command,
    ) -> bool {
        use crate::frontend::menu_scene::{Command, Filter};

        match command {
            Command::Fill { rect, color } => {
                canvas.set_draw_color(Color::RGBA(color.r, color.g, color.b, color.a));
                let _ = canvas.fill_rect(to_frect(*rect));
                true
            }
            Command::Texture(quad) => {
                let Some(texture) = self.texture_mut(quad.texture) else {
                    return false;
                };
                texture.set_color_mod(quad.tint.r, quad.tint.g, quad.tint.b);
                texture.set_alpha_mod(quad.tint.a);
                texture.set_scale_mode(match quad.filter {
                    Filter::Nearest => ScaleMode::Nearest,
                    Filter::Linear => ScaleMode::Linear,
                });
                let destination = to_frect(quad.destination);
                let source = quad.source.map(to_frect);
                if quad.flip_x {
                    let _ = canvas.copy_ex(texture, source, destination, 0.0, None, true, false);
                } else {
                    let _ = canvas.copy(texture, source, destination);
                }
                true
            }
        }
    }

    fn draw_commands(
        &mut self,
        canvas: &mut Canvas<Window>,
        commands: impl IntoIterator<Item = crate::frontend::menu_scene::Command>,
    ) {
        for command in commands {
            self.draw_command(canvas, &command);
        }
    }

    fn font_atlases(&self) -> BTreeMap<String, crate::frontend::menu_scene::TextureId> {
        self.fonts
            .keys()
            .filter_map(|name| {
                let key = format!("skin/font-{name}.png");
                Some((name.clone(), self.texture_bindings.id(&key)?))
            })
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn text_commands(
        &self,
        value: &str,
        x: f32,
        y: f32,
        font_name: &str,
        color: Color,
        max_width: f32,
        line_anchor: bool,
    ) -> Vec<crate::frontend::menu_scene::Command> {
        let layout = self.measure(value, font_name);
        crate::frontend::menu_scene::text(
            &layout,
            &self.font_atlases(),
            crate::frontend::menu_scene::TextPosition {
                x,
                y,
                max_width,
                line_anchor,
                tint: crate::frontend::menu_scene::Rgba {
                    r: color.r,
                    g: color.g,
                    b: color.b,
                    a: color.a,
                },
            },
        )
    }

    #[allow(dead_code)]
    fn draw_texture(
        &mut self,
        canvas: &mut Canvas<Window>,
        key: &str,
        dest: FRect,
        src: Option<FRect>,
        color: Color,
        smooth: bool,
    ) {
        use crate::frontend::menu_scene::{Command, Filter, Rgba, TextureQuad};

        let Some(texture) = self.texture_bindings.id(key) else {
            return;
        };
        self.draw_command(
            canvas,
            &Command::Texture(TextureQuad {
                texture,
                destination: from_frect(dest),
                source: src.map(from_frect),
                tint: Rgba {
                    r: color.r,
                    g: color.g,
                    b: color.b,
                    a: color.a,
                },
                filter: if smooth {
                    Filter::Linear
                } else {
                    Filter::Nearest
                },
                flip_x: false,
            }),
        );
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
        if let Some(source) = self.sprite_source(name) {
            let command = crate::frontend::menu_scene::sprite(
                source.texture,
                source.source,
                crate::frontend::menu_scene::Rect {
                    x,
                    y,
                    w: width,
                    h: height,
                },
                crate::frontend::menu_scene::Rgba {
                    r: color.r,
                    g: color.g,
                    b: color.b,
                    a: color.a,
                },
                source.flip_x,
            );
            self.draw_command(canvas, &command);
        }
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
        let commands = self.text_commands(value, x, y, font_name, color, max_width, line_anchor);
        self.draw_commands(canvas, commands);
    }

    #[allow(dead_code)]
    fn selection(
        &mut self,
        canvas: &mut Canvas<Window>,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        time: f32,
    ) {
        let plan = crate::frontend::menu_scene::selection(
            self.nes,
            crate::frontend::menu_scene::Rect {
                x,
                y,
                w: width,
                h: height,
            },
            time,
        );
        let mut sources = BTreeMap::new();
        for placement in &plan {
            if let Some(source) = self.sprite_source(&placement.name) {
                sources.insert(placement.name.clone(), source);
            }
        }
        let commands = crate::frontend::menu_scene::sprite_scene(&plan, &sources);
        self.draw_commands(canvas, commands);
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
    core_rate: Option<u32>,
    started: Cell<bool>,
}

enum DialogMessage {
    Picked(
        dialog_requests::Kind,
        crate::frontend::requests::RequestId,
        crate::frontend::requests::Outcome<PathBuf, String>,
    ),
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
        Some(Self {
            stream,
            data,
            core_rate: None,
            started: Cell::new(true),
        })
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
        Some(Self {
            stream,
            data: Vec::new(),
            core_rate: Some(rate),
            started: Cell::new(false),
        })
    }

    fn clear(&self) {
        if self.core_rate.is_some() {
            let _ = self.stream.pause();
            self.started.set(false);
        }
        let _ = self.stream.clear();
    }
    fn queued(&self) -> usize {
        self.stream.queued_bytes().unwrap_or(0).max(0) as usize
    }
    fn put_f32(&self, samples: &[f32]) -> Result<(), String> {
        self.stream
            .put_data_f32(samples)
            .map_err(|e| format!("Cannot queue game audio: {e}"))
    }
    fn needs_frames(&self) -> bool {
        self.core_rate
            .is_some_and(|rate| self.queued() < timing::audio_target_bytes(rate))
    }
    fn start_if_primed(&self) -> Result<(), String> {
        if !self.started.get() && !self.needs_frames() {
            self.stream
                .resume()
                .map_err(|e| format!("Cannot start game audio: {e}"))?;
            self.started.set(true);
        }
        Ok(())
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
    silent_frame_deadline: Instant,
    core: Option<crate::ares::AresSession>,
    frame_texture: Option<Texture>,
    held: HashSet<Scancode>,
    pad_held: HashSet<PadButton>,
    stick: gamepad::Stick,
    pad_repeat: gamepad::Repeat,
    gamepad_subsystem: GamepadSubsystem,
    gamepad: Option<Gamepad>,
    gamepad_id: Option<u32>,
    pad_blocked: bool,
    fullscreen: bool,
    fullscreen_dirty: bool,
    window_title: String,
    title_dirty: bool,
    dialog_sender: Sender<DialogMessage>,
    dialog_receiver: Receiver<DialogMessage>,
    picker_requests: dialog_requests::Requests,
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
    image_import: app_import::AppImport,
    last_notice: String,
    notice_owned: bool,
    preview_import_notice: Option<crate::ui::launcher::import_worker::CancellationView>,
}

type MenuFocus = crate::frontend::menu_navigation::MenuFocus;

fn from_frect(rect: FRect) -> crate::frontend::menu_scene::Rect {
    crate::frontend::menu_scene::Rect {
        x: rect.x,
        y: rect.y,
        w: rect.w,
        h: rect.h,
    }
}

fn to_frect(rect: crate::frontend::menu_scene::Rect) -> FRect {
    FRect::new(rect.x, rect.y, rect.w, rect.h)
}

impl Drop for App {
    fn drop(&mut self) {
        self.image_import.shutdown();
        self.picker_requests.close();
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
    fn dialog(&mut self, canvas: &mut Canvas<Window>, x: f32, y: f32, width: f32, height: f32) {
        menu_overlay::draw_dialog(
            canvas,
            crate::frontend::menu_scene::Rect {
                x,
                y,
                w: width,
                h: height,
            },
        );
    }

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
        let restored = crate::frontend::preferences::decode(&preferences)?;
        let firmware_path = restored
            .firmware
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(|| data_path.join("firmware"));
        let profiles = crate::profile::list_profiles(&profile_cache);
        let (dialog_sender, dialog_receiver) = mpsc::channel();
        let gamepad = gamepad_subsystem.gamepads().ok().and_then(|ids| {
            ids.into_iter()
                .find_map(|id| gamepad_subsystem.open(id).ok())
        });
        let gamepad_id = gamepad
            .as_ref()
            .and_then(|pad| pad.id().ok())
            .map(u32::from);
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
            silent_frame_deadline: Instant::now(),
            core: None,
            frame_texture: None,
            held: HashSet::new(),
            pad_held: HashSet::new(),
            stick: gamepad::Stick::default(),
            pad_repeat: gamepad::Repeat::default(),
            gamepad_subsystem,
            gamepad,
            gamepad_id,
            pad_blocked: gamepad_id.is_some(),
            fullscreen: false,
            fullscreen_dirty: false,
            window_title,
            title_dirty: false,
            dialog_sender,
            dialog_receiver,
            picker_requests: dialog_requests::Requests::default(),
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
            image_import: app_import::AppImport::default(),
            last_notice: String::new(),
            notice_owned: false,
            preview_import_notice: None,
        };
        app.selected = app
            .games
            .iter()
            .position(|g| restored.selected.as_deref() == Some(&g.id))
            .unwrap_or(0);
        app.sort = restored.sort;
        app.slot = restored.slot;
        app.sound = restored.sound;
        app.motion = restored.motion;
        if let Some(locale) = restored.language.as_deref() {
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
        let snapshot = crate::frontend::preferences::MenuSnapshot {
            selected: self.selected().id.clone(),
            sort: self.sort,
            slot: self.slot,
            sound: self.sound,
            motion: self.motion,
            language: self.art.locale.clone(),
            firmware: self.firmware_path.to_string_lossy().into_owned(),
        };
        save_preference_document(
            &self.data_path.join("preferences.json"),
            &mut self.preferences,
            &snapshot,
        )
    }

    fn switch_profile(&mut self, canvas: &Canvas<Window>, profile: crate::profile::Profile) {
        self.image_import.invalidate();
        self.picker_requests.invalidate();
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
            let restored = crate::frontend::preferences::decode(&preferences)?;
            Ok::<_, String>((games, art, preferences, restored))
        })();
        match result {
            Ok((games, art, preferences, restored)) => {
                self.assets_path = assets_path;
                self.data_path = data_path;
                self.games = games;
                self.art = art;
                self.preferences = preferences;
                if let Some(locale) = restored.language.as_deref() {
                    let _ = self.art.set_language(locale);
                }
                self.selected = self
                    .games
                    .iter()
                    .position(|game| restored.selected.as_deref() == Some(&game.id))
                    .unwrap_or(0);
                self.sort = restored.sort;
                self.slot = restored.slot;
                self.sound = restored.sound;
                self.motion = restored.motion;
                self.firmware_path = restored
                    .firmware
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

    fn choose_image(&mut self, canvas: &Canvas<Window>) {
        if self.image_import.is_busy() {
            return;
        }
        let context = dialog_requests::Context {
            profile: self.data_path.clone(),
            game: self.selected().id.clone(),
            start: false,
        };
        let Ok(id) = self.picker_requests.begin(
            dialog_requests::Kind::Image,
            context,
            crate::frontend::requests::Capability::Available,
        ) else {
            return;
        };
        let sender = self.dialog_sender.clone();
        let filters = [DialogFileFilter {
            name: "NES / SNES Classic image",
            pattern: "hsqs;gz;img",
        }];
        if let Err(error) = dialog::show_open_file_dialog(
            &filters,
            None::<&Path>,
            false,
            Some(canvas.window()),
            Box::new(move |result, _| {
                let _ = sender.send(DialogMessage::Picked(
                    dialog_requests::Kind::Image,
                    id,
                    dialog_requests::map_dialog(result),
                ));
            }),
        ) {
            let _ = self.dialog_sender.send(DialogMessage::Picked(
                dialog_requests::Kind::Image,
                id,
                crate::frontend::requests::Outcome::Failed(error.to_string()),
            ));
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
        self.image_import.invalidate();
        self.picker_requests.invalidate();
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
        if !self.rom_extension_allowed(&rom) {
            self.panel = "message".into();
            self.message = "The selected ROM belongs to a different console system. Choose a compatible ROM in emulator settings.".into();
            return false;
        }
        self.picker_requests.invalidate();
        self.image_import.invalidate();
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
        self.silent_frame_deadline = Instant::now();
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

    fn choose_rom(&mut self, canvas: &Canvas<Window>, start: bool) {
        let context = dialog_requests::Context {
            profile: self.data_path.clone(),
            game: self.selected().id.clone(),
            start,
        };
        let Ok(id) = self.picker_requests.begin(
            dialog_requests::Kind::Rom,
            context,
            crate::frontend::requests::Capability::Available,
        ) else {
            return;
        };
        let sender = self.dialog_sender.clone();
        let filters = [DialogFileFilter {
            name: if self.art.nes {
                "NES cartridge ROM"
            } else {
                "Super Nintendo ROM"
            },
            pattern: if self.art.nes { "nes" } else { "sfc;smc" },
        }];
        if let Err(error) = dialog::show_open_file_dialog(
            &filters,
            None::<&Path>,
            false,
            Some(canvas.window()),
            Box::new(move |result, _| {
                let _ = sender.send(DialogMessage::Picked(
                    dialog_requests::Kind::Rom,
                    id,
                    dialog_requests::map_dialog(result),
                ));
            }),
        ) {
            let _ = self.dialog_sender.send(DialogMessage::Picked(
                dialog_requests::Kind::Rom,
                id,
                crate::frontend::requests::Outcome::Failed(error.to_string()),
            ));
        }
    }

    fn rom_extension_allowed(&self, path: &Path) -> bool {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if self.art.nes {
            extension == "nes"
        } else {
            matches!(extension.as_str(), "sfc" | "smc")
        }
    }

    fn choose_firmware(&mut self, canvas: &Canvas<Window>) {
        let context = dialog_requests::Context {
            profile: self.data_path.clone(),
            game: self.selected().id.clone(),
            start: false,
        };
        let Ok(id) = self.picker_requests.begin(
            dialog_requests::Kind::Firmware,
            context,
            crate::frontend::requests::Capability::Available,
        ) else {
            return;
        };
        let sender = self.dialog_sender.clone();
        dialog::show_open_folder_dialog(
            None::<&Path>,
            false,
            Some(canvas.window()),
            Box::new(move |result, _| {
                let _ = sender.send(DialogMessage::Picked(
                    dialog_requests::Kind::Firmware,
                    id,
                    dialog_requests::map_dialog(result),
                ));
            }),
        );
    }

    fn process_dialogs(&mut self, canvas: &Canvas<Window>, closing: bool) {
        let messages = host::take_dialog_batch(self.dialog_receiver.try_iter());
        for message in messages {
            match message {
                DialogMessage::Picked(kind, id, outcome) => {
                    let current = dialog_requests::Context {
                        profile: self.data_path.clone(),
                        game: self.selected().id.clone(),
                        start: false,
                    };
                    let Some(action) = self.picker_requests.finish(id, kind, outcome, &current)
                    else {
                        continue;
                    };
                    let (context, path) = match action {
                        dialog_requests::Action::Selected(
                            dialog_requests::Kind::Image,
                            context,
                            path,
                        ) => {
                            debug_assert!(!context.start);
                            if self.image_import.is_busy() {
                                continue;
                            }
                            let target = app_import::Target {
                                profile: context.profile,
                                game: context.game,
                            };
                            match self
                                .image_import
                                .start(target, path, self.profile_cache.clone())
                            {
                                Ok(_) => {
                                    self.panel = "message".into();
                                    self.message = self.image_import.notice_text().into();
                                    self.last_notice = self.message.clone();
                                    self.notice_owned = true;
                                }
                                Err(error) => {
                                    self.panel = "message".into();
                                    self.message = error;
                                }
                            }
                            continue;
                        }
                        dialog_requests::Action::Selected(
                            dialog_requests::Kind::Rom,
                            context,
                            path,
                        ) => (context, path),
                        dialog_requests::Action::Selected(
                            dialog_requests::Kind::Firmware,
                            _context,
                            path,
                        ) => {
                            if fs::symlink_metadata(&path)
                                .is_ok_and(|metadata| metadata.file_type().is_dir())
                            {
                                self.firmware_path = path;
                                let _ = self.save_settings();
                            }
                            continue;
                        }
                        dialog_requests::Action::Error(error) => {
                            self.panel = "message".into();
                            self.message = error;
                            continue;
                        }
                        dialog_requests::Action::Done => continue,
                    };
                    let game = context.game;
                    let start = context.start;
                    if !safe_id(&game)
                        || !self.games.iter().any(|entry| entry.id == game)
                        || !self.rom_extension_allowed(&path)
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
            }
        }
        let current = app_import::Target {
            profile: self.data_path.clone(),
            game: self.selected().id.clone(),
        };
        let matches_notice = self.owns_notice(&self.panel, &self.message);
        if !matches_notice || self.playing {
            self.notice_owned = false;
        }
        let notice_owned = matches_notice && self.notice_owned;
        let update = self
            .image_import
            .update(&current, notice_owned, self.playing, closing);
        if update.progress.is_some() && update.completion.is_none() && notice_owned {
            self.message = self.image_import.notice_text().into();
            self.last_notice = self.message.clone();
        }
        if let Some(result) = update.completion {
            match result {
                app_import::Poll::Switch(profile) => {
                    self.profiles = crate::profile::list_profiles(&self.profile_cache);
                    self.switch_profile(canvas, profile);
                }
                app_import::Poll::CachedOnly => {
                    self.profiles = crate::profile::list_profiles(&self.profile_cache);
                }
                app_import::Poll::Cancelled => {
                    self.message = "Image import cancelled.".into();
                }
                app_import::Poll::Error(error) => {
                    self.message = error;
                }
                app_import::Poll::Silent | app_import::Poll::RetiredSilent => {}
            }
            self.notice_owned = false;
        }
    }

    fn owns_notice(&self, panel: &str, message: &str) -> bool {
        self.image_import.is_busy()
            && self.notice_owned
            && panel == "message"
            && message == self.last_notice
    }

    pub(super) fn owned_import_notice(&self) -> bool {
        let current = app_import::Target {
            profile: self.data_path.clone(),
            game: self.selected().id.clone(),
        };
        self.image_import.cancellation_view(
            &current,
            self.owns_notice(&self.panel, &self.message),
            self.playing,
        ) != crate::ui::launcher::import_worker::CancellationView::Unavailable
    }

    fn import_cancellation_view(&self) -> crate::ui::launcher::import_worker::CancellationView {
        if let Some(view) = self.preview_import_notice {
            return view;
        }
        let current = app_import::Target {
            profile: self.data_path.clone(),
            game: self.selected().id.clone(),
        };
        let owned = self.owns_notice(&self.panel, &self.message);
        self.image_import
            .cancellation_view(&current, owned, self.playing)
    }

    fn request_import_cancel(&mut self) {
        let current = app_import::Target {
            profile: self.data_path.clone(),
            game: self.selected().id.clone(),
        };
        let owned = self.owned_import_notice();
        if let Some(
            result @ (crate::frontend::import_lifecycle::CancelResult::Requested
            | crate::frontend::import_lifecycle::CancelResult::TooLate),
        ) = self
            .image_import
            .cancel_notice(&current, owned, self.playing)
        {
            self.message = self.image_import.notice_text().into();
            self.last_notice = self.message.clone();
            self.notice_owned = true;
            let _ = result;
        }
    }

    fn dismiss_import_notice(&mut self) {
        if !self.owned_import_notice() {
            return;
        }
        self.image_import.release_notice();
        self.notice_owned = false;
        self.panel.clear();
        self.message.clear();
        self.last_notice.clear();
        self.menu_focus = MenuFocus::Games;
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
        let keyboard = keyboard.map(|key| self.held.contains(&key));
        let gamepad = gamepad.map(|button| self.pad_held.contains(&button));
        crate::frontend::gameplay::button_mask(
            keyboard,
            gamepad,
            self.stick.mask(),
            self.pad_blocked,
        )
    }

    fn pad_button_down(&mut self, button: PadButton, canvas: &Canvas<Window>) {
        if !self.pad_held.insert(button) {
            return;
        }
        if self.pad_blocked {
            return;
        }
        if self.playing {
            if button == PadButton::Guide
                || (self.pad_held.contains(&PadButton::Back)
                    && self.pad_held.contains(&PadButton::Start))
            {
                self.leave_game();
            }
            return;
        }
        let button = input::native_button(button);
        if let Some(key) = crate::frontend::menu_input::menu_key(button, self.panel == "resume") {
            if matches!(key, Key::Left | Key::Right | Key::Up | Key::Down) {
                self.pad_repeat.pressed(key, Instant::now());
            }
            self.handle_menu_input(
                crate::frontend::menu_input::KeyEvent { key, repeat: false },
                canvas,
            );
        }
    }

    fn poll_gamepad_navigation(&mut self, canvas: &Canvas<Window>) {
        if self.pad_blocked {
            let neutral = self.gamepad.as_ref().is_none_or(|pad| {
                let buttons = [
                    PadButton::South,
                    PadButton::East,
                    PadButton::West,
                    PadButton::North,
                    PadButton::Start,
                    PadButton::Back,
                    PadButton::Guide,
                    PadButton::DPadLeft,
                    PadButton::DPadRight,
                    PadButton::DPadUp,
                    PadButton::DPadDown,
                    PadButton::LeftShoulder,
                    PadButton::RightShoulder,
                ];
                !buttons.into_iter().any(|button| pad.button(button))
                    && i32::from(pad.axis(sdl3::gamepad::Axis::LeftX)).abs() < 9000
                    && i32::from(pad.axis(sdl3::gamepad::Axis::LeftY)).abs() < 9000
            });
            if !neutral {
                return;
            }
            self.pad_blocked = false;
            self.pad_held.clear();
            self.stick.clear();
            self.pad_repeat.clear();
        }
        if self.playing {
            self.pad_repeat.clear();
            return;
        }
        let direction = [
            (PadButton::DPadUp, Key::Up),
            (PadButton::DPadDown, Key::Down),
            (PadButton::DPadLeft, Key::Left),
            (PadButton::DPadRight, Key::Right),
        ]
        .into_iter()
        .find_map(|(button, key)| self.pad_held.contains(&button).then_some(key))
        .or_else(|| self.stick.menu_key());
        if let Some((key, repeat)) = self.pad_repeat.poll(direction, Instant::now()) {
            self.handle_menu_input(
                crate::frontend::menu_input::KeyEvent { key, repeat },
                canvas,
            );
        }
    }

    fn leave_game(&mut self) {
        self.playing = false;
        self.pad_blocked = self.gamepad.is_some();
        self.held.clear();
        self.pad_held.clear();
        self.stick.clear();
        self.pad_repeat.clear();
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
        if let Err(error) = core.run_frame(buttons) {
            self.playing = false;
            self.panel = "message".into();
            self.message = error.to_string();
            self.core_audio.as_ref().map(AudioOut::clear);
            return Ok(());
        }
        if let Some(audio) = &self.core_audio {
            audio.put_f32(core.audio())?;
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

    fn pump_game_audio(&mut self) -> Result<usize, String> {
        if self.core_audio.is_none() {
            let frames = timing::silent_frames_due(&mut self.silent_frame_deadline, Instant::now());
            for _ in 0..frames {
                self.run_game_frame()?;
            }
            return Ok(frames);
        }
        let mut frames = 0;
        // The audio device is the emulation clock. NES runs near 60.10 Hz,
        // independent of whether the display is 50, 60, 120 or 144 Hz. Fill a
        // bounded reservoir, then present the newest frame; never clear or
        // truncate continuous audio to correct a clock mismatch.
        while frames < timing::MAX_AUDIO_CATCHUP_FRAMES
            && self.playing
            && self.core_audio.as_ref().is_some_and(AudioOut::needs_frames)
        {
            self.run_game_frame()?;
            frames += 1;
        }
        if let Some(audio) = &self.core_audio {
            audio.start_if_primed()?;
        }
        Ok(frames)
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
            "cancel-import" => self.request_import_cancel(),
            "dismiss-import" => self.dismiss_import_notice(),
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
            "choose-suspend" => {
                if self.has_pending() {
                    self.request_suspend_save();
                } else {
                    self.resume_slot(canvas);
                }
            }
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
                    if index != self.selected {
                        self.image_import.invalidate();
                        self.picker_requests.invalidate();
                    }
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
        let elapsed = self.animation_start.elapsed().as_secs_f32();
        let scroll_elapsed = self.scroll_started.elapsed().as_secs_f32();
        let wallpaper_duration = self
            .art
            .layout
            .get("wallpaperDuration")
            .and_then(Value::as_f64)
            .unwrap_or(1.0) as f32;
        let motion =
            crate::frontend::menu_motion::motion(crate::frontend::menu_motion::MotionInput {
                suspend_layout,
                motion: self.motion,
                animation_elapsed: elapsed,
                scroll_elapsed,
                scroll_from: self.scroll_from,
                scroll_to: self.scroll_to,
                wallpaper_duration,
            });
        self.scroll_current = motion.scroll_current;
        let frame = self.project_base_menu(motion, suspend_layout);
        let scene = crate::frontend::menu_frame::compose(frame);
        let context = menu_frame::BindingContext::new(
            self.art.texture_bindings.generation(),
            self.games.iter().map(|game| game.id.clone()),
        );
        let toolbar_scene = self.project_menu_toolbar();
        if let Some(scene) = crate::frontend::menu_frame::with_toolbar(scene, toolbar_scene) {
            self.art
                .execute_base_menu(canvas, &scene, &context, &mut self.hits);
        }
        match crate::frontend::menu_overlay::route(&self.panel) {
            crate::frontend::menu_overlay::OverlayRoute::None => {}
            crate::frontend::menu_overlay::OverlayRoute::Resume => self.render_suspend(canvas),
            crate::frontend::menu_overlay::OverlayRoute::ResumeConfirmation => {
                self.render_suspend(canvas);
                self.render_suspend_confirmation(canvas);
            }
            crate::frontend::menu_overlay::OverlayRoute::Toolbar => self.render_toolbar(canvas),
            crate::frontend::menu_overlay::OverlayRoute::Panel => self.render_panel(canvas),
            crate::frontend::menu_overlay::OverlayRoute::Notice => {
                let message = self.message.clone();
                let layout = self.art.measure(&message, "l");
                let scene = crate::frontend::menu_overlay::notice(
                    self.art.texture_bindings.generation(),
                    &layout,
                    &self.art.font_atlases(),
                );
                self.art.execute_notice(canvas, &scene);
            }
        }
    }

    fn render_panel(&mut self, canvas: &mut Canvas<Window>) {
        let projected = self.project_panel();
        let scene = crate::frontend::menu_panel::compose_panel(projected.input);
        if self.art.execute_panel(
            canvas,
            &scene,
            &projected.bindings,
            &mut self.hits,
            &mut self.actions,
        ) {
            self.focus = projected.focus;
        }
    }

    fn render_game(&mut self, canvas: &mut Canvas<Window>) {
        let mode = self.display_mode();
        let width = if mode == 2 {
            if self.art.nes {
                768.0
            } else {
                720.0 * 8.0 / 7.0
            }
        } else {
            960.0
        };
        let left = (1280.0 - width) * 0.5;
        if let Some(texture) = self.frame_texture.as_ref() {
            let _ = canvas.copy(texture, None, FRect::new(left, 0.0, width, 720.0));
            if mode == 0 {
                canvas.set_blend_mode(BlendMode::Blend);
                canvas.set_draw_color(Color::RGBA(0, 0, 0, 64));
                let rows = if self.art.nes { 240 } else { 224 };
                for row in 0..rows {
                    let _ = canvas.fill_rect(FRect::new(
                        left,
                        row as f32 * 720.0 / rows as f32,
                        width,
                        1.0,
                    ));
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
        let mut stage = self.create_native_stage()?;
        canvas.set_blend_mode(BlendMode::Blend);
        let mut pacer = timing::FramePacer::new(canvas, frame_limit == 0);
        let mut frame_limit = crate::frontend::frame_limit::FrameLimit::new(frame_limit);
        while running {
            for event in events.poll_iter().take(host::NATIVE_EVENT_BUDGET) {
                self.handle_host_event(event, canvas, &mut running);
            }
            self.update_native_frame(canvas, !running)?;
            self.draw_native_frame(canvas, &mut stage)?;
            if frame_limit.capture_due()
                && let Some(path) = screenshot
            {
                save_screenshot(canvas, path)?;
            }
            canvas.present();
            if frame_limit.record_presented() {
                running = false;
            }
            pacer.finish_frame();
            if self.panel == "notice" && self.scroll_started.elapsed().as_secs_f32() > 2.5 {
                self.panel.clear();
            }
        }
        self.finish_native_run()
    }
}

fn import_footer(
    view: crate::ui::launcher::import_worker::CancellationView,
) -> (&'static str, Option<&'static str>) {
    use crate::ui::launcher::import_worker::CancellationView;
    match view {
        CancellationView::Available => ("Cancel import", Some("cancel-import")),
        CancellationView::Requested => ("Cancelling…", None),
        CancellationView::Finishing => ("Finishing import…", None),
        CancellationView::Unavailable => ("Back", None),
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

fn required_firmware_available(required: &[String], directory: &Path) -> bool {
    required.iter().all(|name| directory.join(name).is_file())
}

fn read_games(assets_path: &Path) -> Result<Vec<Game>, String> {
    let path = assets_path.join("catalog.json");
    let bytes = read_json_bytes(&path)?;
    catalog::decode_bytes(&bytes).map_err(|error| match error {
        catalog::DecodeError::TooLarge => format!("Invalid JSON file: {}", path.display()),
        catalog::DecodeError::InvalidJson(error) => {
            format!("Invalid JSON {}: {error}", path.display())
        }
        catalog::DecodeError::Catalog(error) => error,
    })
}

fn read_json(path: &Path) -> Result<Value, String> {
    let bytes = read_json_bytes(path)?;
    serde_json::from_slice(&bytes).map_err(|e| format!("Invalid JSON {}: {e}", path.display()))
}

fn read_json_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    if !metadata.file_type().is_file() || metadata.len() > catalog::MAX_JSON_BYTES as u64 {
        return Err(format!("Invalid JSON file: {}", path.display()));
    }
    let file = File::open(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(catalog::MAX_JSON_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    if bytes.len() > catalog::MAX_JSON_BYTES {
        return Err(format!("Invalid JSON file: {}", path.display()));
    }
    Ok(bytes)
}

#[cfg(test)]
mod catalog_read_tests {
    use super::read_json_bytes;
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "clover-catalog-read-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn bounded_json_reader_accepts_regular_files_and_rejects_missing_symlink_and_oversized() {
        let directory = temp_dir();
        let regular = directory.join("catalog.json");
        fs::write(&regular, b"{}\n").unwrap();
        assert_eq!(read_json_bytes(&regular).unwrap(), b"{}\n");

        let missing = directory.join("missing.json");
        assert!(
            read_json_bytes(&missing)
                .unwrap_err()
                .starts_with("Cannot read ")
        );

        #[cfg(unix)]
        {
            let link = directory.join("link.json");
            std::os::unix::fs::symlink(&regular, &link).unwrap();
            assert_eq!(
                read_json_bytes(&link).unwrap_err(),
                format!("Invalid JSON file: {}", link.display())
            );
        }

        let oversized = directory.join("oversized.json");
        let file = fs::File::create(&oversized).unwrap();
        file.set_len(crate::frontend::catalog::MAX_JSON_BYTES as u64 + 1)
            .unwrap();
        assert_eq!(
            read_json_bytes(&oversized).unwrap_err(),
            format!("Invalid JSON file: {}", oversized.display())
        );
        fs::remove_dir_all(directory).unwrap();
    }
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

fn save_preference_document(
    path: &Path,
    existing: &mut Value,
    snapshot: &crate::frontend::preferences::MenuSnapshot,
) -> Result<(), String> {
    let merged = crate::frontend::preferences::merge_snapshot(existing, snapshot)?;
    atomic_json(path, &merged)?;
    *existing = merged;
    Ok(())
}

#[cfg(test)]
mod preference_save_tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "clover-preferences-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn preference_write_succeeds_before_memory_update() {
        let dir = temp_dir();
        let path = dir.join("preferences.json");
        let snapshot = crate::frontend::preferences::MenuSnapshot {
            selected: "game".into(),
            sort: 1,
            slot: 2,
            sound: true,
            motion: false,
            language: "en".into(),
            firmware: "/firmware".into(),
        };
        let mut existing = json!({"history":{"game":1}});
        save_preference_document(&path, &mut existing, &snapshot).unwrap();
        assert_eq!(read_json(&path).unwrap(), existing);
        let blocked = dir.join("regular-file");
        fs::write(&blocked, b"block").unwrap();
        let mut failed_doc = json!({"extension":true});
        assert!(
            save_preference_document(
                &blocked.join("preferences.json"),
                &mut failed_doc,
                &snapshot
            )
            .is_err()
        );
        assert_eq!(failed_doc, json!({"extension":true}));
        fs::remove_dir_all(dir).unwrap();
    }
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
    let mut audit_games = None;
    let mut exercise_audio = None;
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
            "--audit-games" => audit_games = Some(PathBuf::from(value)),
            "--exercise-audio" => exercise_audio = Some(PathBuf::from(value)),
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
    let _ = icon::install(canvas.window_mut());
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
    if let Some(output) = exercise_audio {
        return app.exercise_audio(&canvas, &output);
    }
    if let Some(output) = audit_games {
        return app.audit_games(&output);
    }
    if frame_limit != 0 {
        if let Some(panel) = preview_panel {
            if let Some((view, message)) = match panel.as_str() {
                "import-running" => Some((
                    launcher::import_worker::CancellationView::Available,
                    "Importing image... You may continue browsing.",
                )),
                "import-requested" => Some((
                    launcher::import_worker::CancellationView::Requested,
                    "Cancellation requested; waiting for a safe stopping point...",
                )),
                "import-finishing" => Some((
                    launcher::import_worker::CancellationView::Finishing,
                    "Finishing import; cancellation is too late.",
                )),
                _ => None,
            } {
                app.panel = "message".into();
                app.message = message.into();
                app.preview_import_notice = Some(view);
            } else if panel == "resume" {
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
    let _ = icon::install(canvas.window_mut());
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
    use super::{import_footer, required_firmware_available};
    use crate::ui::launcher::import_worker::CancellationView;
    use std::fs;

    #[test]
    fn import_footer_labels_and_actions_match_enabled_state() {
        assert_eq!(
            import_footer(CancellationView::Available),
            ("Cancel import", Some("cancel-import"))
        );
        assert_eq!(
            import_footer(CancellationView::Requested),
            ("Cancelling…", None)
        );
        assert_eq!(
            import_footer(CancellationView::Finishing),
            ("Finishing import…", None)
        );
    }

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
