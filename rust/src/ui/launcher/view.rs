//! Launcher-only vector chrome and embedded typography; no console assets needed.
use super::import_worker::CancellationView;
use super::*;
use fontdue::{Font, FontSettings, Metrics};
#[path = "flags.rs"]
mod flags;
#[path = "projection.rs"]
mod projection;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PrimaryButton {
    label: &'static str,
    enabled: bool,
}

fn primary_button(
    busy: bool,
    cancel: CancellationView,
    entry_exists: bool,
    ready: bool,
) -> PrimaryButton {
    if busy {
        let (label, enabled) = match cancel {
            CancellationView::Available => ("Cancel import", true),
            CancellationView::Requested => ("Cancelling…", false),
            CancellationView::Finishing => ("Finishing import…", false),
            CancellationView::Unavailable => ("Preparing your Classic…", false),
        };
        PrimaryButton { label, enabled }
    } else {
        PrimaryButton {
            label: if !entry_exists {
                "Import an image"
            } else if ready {
                "Launch Classic Menu"
            } else {
                "Import & launch"
            },
            enabled: true,
        }
    }
}

pub use crate::frontend::launcher_scene::{HEIGHT, PAGE_SIZE, WIDTH};

pub const BUTTONS: [FRect; 3] = [
    sdl_rect(crate::frontend::launcher_scene::BUTTONS[0]),
    sdl_rect(crate::frontend::launcher_scene::BUTTONS[1]),
    sdl_rect(crate::frontend::launcher_scene::BUTTONS[2]),
];

pub fn row_rect(row: usize) -> FRect {
    sdl_rect(crate::frontend::launcher_scene::row_rect(row))
}

pub(super) const fn sdl_rect(rect: crate::frontend::launcher_scene::Rect) -> FRect {
    FRect {
        x: rect.x,
        y: rect.y,
        w: rect.w,
        h: rect.h,
    }
}

struct Glyph {
    metrics: Metrics,
    texture: Option<Texture>,
}

pub struct View {
    fonts: [Font; 2],
    creator: TextureCreator<WindowContext>,
    glyphs: HashMap<(char, u16, bool), Glyph>,
    flags: flags::Assets,
}

impl View {
    pub fn new(canvas: &Canvas<Window>) -> Result<Self, String> {
        let fonts = [
            Font::from_bytes(
                include_bytes!("../../../assets/fonts/Barlow-Regular.ttf") as &[u8],
                FontSettings::default(),
            )
            .map_err(str::to_owned)?,
            Font::from_bytes(
                include_bytes!("../../../assets/fonts/Barlow-SemiBold.ttf") as &[u8],
                FontSettings::default(),
            )
            .map_err(str::to_owned)?,
        ];
        let creator = canvas.texture_creator();
        let flags = flags::Assets::new(&creator)?;
        Ok(Self {
            fonts,
            creator,
            glyphs: HashMap::new(),
            flags,
        })
    }

    fn width(&self, value: &str, size: u16, bold: bool) -> f32 {
        let font = &self.fonts[usize::from(bold)];
        let mut previous = None;
        value
            .chars()
            .map(|character| {
                let kerning = previous
                    .and_then(|left| font.horizontal_kern(left, character, size as f32))
                    .unwrap_or(0.0);
                previous = Some(character);
                font.metrics(character, size as f32).advance_width + kerning
            })
            .sum()
    }

    fn fitted(&self, value: &str, size: u16, bold: bool, max_width: f32) -> String {
        if self.width(value, size, bold) <= max_width {
            return value.to_owned();
        }
        let mut result = String::new();
        for character in value.chars() {
            let candidate = format!("{result}{character}…");
            if self.width(&candidate, size, bold) > max_width {
                break;
            }
            result.push(character);
        }
        result.push('…');
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn text(
        &mut self,
        canvas: &mut Canvas<Window>,
        value: &str,
        x: f32,
        y: f32,
        size: u16,
        bold: bool,
        color: Color,
        max_width: f32,
    ) -> Result<(), String> {
        let value = self.fitted(value, size, bold, max_width);
        let font = &self.fonts[usize::from(bold)];
        let ascent = font
            .horizontal_line_metrics(size as f32 * 2.0)
            .ok_or("Missing font line metrics")?
            .ascent
            / 2.0;
        let mut pen = x;
        let mut previous = None;
        for character in value.chars() {
            pen += previous
                .and_then(|left| font.horizontal_kern(left, character, size as f32))
                .unwrap_or(0.0);
            previous = Some(character);
            let key = (character, size, bold);
            if !self.glyphs.contains_key(&key) {
                let (metrics, bitmap) = font.rasterize(character, size as f32 * 2.0);
                let texture = if metrics.width != 0 && metrics.height != 0 {
                    let mut pixels: Vec<u8> = bitmap
                        .iter()
                        .flat_map(|alpha| [255, 255, 255, *alpha])
                        .collect();
                    let surface = Surface::from_data(
                        &mut pixels,
                        metrics.width as u32,
                        metrics.height as u32,
                        metrics.width as u32 * 4,
                        PixelFormat::RGBA32,
                    )
                    .map_err(|e| e.to_string())?;
                    let mut texture = self
                        .creator
                        .create_texture_from_surface(surface)
                        .map_err(|e| e.to_string())?;
                    texture.set_blend_mode(BlendMode::Blend);
                    texture.set_scale_mode(ScaleMode::Linear);
                    Some(texture)
                } else {
                    None
                };
                self.glyphs.insert(key, Glyph { metrics, texture });
            }
            let glyph = self
                .glyphs
                .get_mut(&key)
                .ok_or("Missing rasterized glyph")?;
            let metrics = glyph.metrics;
            if let Some(texture) = glyph.texture.as_mut() {
                texture.set_color_mod(color.r, color.g, color.b);
                texture.set_alpha_mod(color.a);
                let dest = FRect::new(
                    pen + metrics.xmin as f32 / 2.0,
                    y + ascent - (metrics.height as f32 + metrics.ymin as f32) / 2.0,
                    metrics.width as f32 / 2.0,
                    metrics.height as f32 / 2.0,
                );
                canvas
                    .copy(texture, None, dest)
                    .map_err(|e| e.to_string())?;
            }
            pen += metrics.advance_width / 2.0;
        }
        Ok(())
    }

    fn draw_commands(
        &mut self,
        canvas: &mut Canvas<Window>,
        commands: impl IntoIterator<Item = crate::frontend::launcher_scene::DrawCommand>,
    ) -> Result<(), String> {
        use crate::frontend::launcher_scene::{DrawCommand, FontRole};
        for command in commands {
            match command {
                DrawCommand::Geometry(geometry) => draw_geometry(canvas, [geometry]),
                DrawCommand::Flag { region, rect } => {
                    self.flags.draw(canvas, region, sdl_rect(rect))
                }
                DrawCommand::Text(text) => self.text(
                    canvas,
                    &text.value,
                    text.x,
                    text.y,
                    text.size,
                    match text.font {
                        FontRole::Regular => false,
                        FontRole::Semibold => true,
                    },
                    Color::RGBA(text.color.r, text.color.g, text.color.b, text.color.a),
                    text.max_width,
                )?,
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        canvas: &mut Canvas<Window>,
        entries: &[&Entry],
        selected: usize,
        busy: bool,
        cancel: CancellationView,
        status: &str,
        error: bool,
        hover: Option<(f32, f32)>,
        focus: Option<usize>,
        filter_label: &str,
        controller_connected: bool,
        elapsed: f32,
    ) -> Result<(), String> {
        let presentation = projection::project(
            entries,
            selected,
            busy,
            cancel,
            status,
            error,
            hover,
            focus,
            filter_label,
            controller_connected,
            elapsed,
        );
        let scene = crate::frontend::launcher_presentation::compose(&presentation);
        self.draw_scene(canvas, &scene)
    }

    fn draw_scene(
        &mut self,
        canvas: &mut Canvas<Window>,
        scene: &crate::frontend::launcher_presentation::Scene,
    ) -> Result<(), String> {
        canvas.set_blend_mode(BlendMode::Blend);
        self.draw_commands(canvas, scene.before_status.iter().cloned())?;
        let mut status_text =
            crate::frontend::launcher_status::StatusText::new(&scene.status, scene.error);
        while let Some(candidate) = status_text.candidate() {
            let width = self.width(candidate, 15, false);
            if let Some(command) = status_text.advance(width) {
                self.draw_commands(canvas, [command])?;
            }
        }
        self.draw_commands(canvas, [status_text.finish()])?;
        self.draw_commands(canvas, scene.after_status.iter().cloned())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Console {
    Nes,
    Famicom,
    Snes,
    Unknown,
}

fn console(entry: &Entry) -> Console {
    match entry {
        Entry::Cached(profile) => {
            let system = profile.manifest.get("system").and_then(Value::as_str);
            let ui = profile
                .manifest
                .get("ui")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if ui == "hvc" || ui == "hvcj" {
                Console::Famicom
            } else if system == Some("nes") || ui == "nes" {
                Console::Nes
            } else if system == Some("snes") || matches!(ui, "snes-eur" | "snes-usa" | "shvc") {
                Console::Snes
            } else {
                Console::Unknown
            }
        }
        Entry::Image(path) => {
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_ascii_lowercase();
            if name.contains("dp-hvc-") || name.contains("dp-hvcj-") {
                Console::Famicom
            } else if name.contains("dp-nes-") {
                Console::Nes
            } else if name.contains("dp-snes") || name.contains("dp-shvc-") {
                Console::Snes
            } else {
                Console::Unknown
            }
        }
    }
}

pub(super) fn system_group(entry: &Entry) -> u8 {
    match console(entry) {
        Console::Nes | Console::Famicom => 0,
        Console::Snes => 1,
        Console::Unknown => 2,
    }
}

pub(super) fn preview_matches(entry: &Entry, name: &str) -> bool {
    match name {
        "nes" => console(entry) == Console::Nes,
        "famicom" => console(entry) == Console::Famicom,
        "snes" => console(entry) == Console::Snes,
        _ => false,
    }
}

fn system_label(entry: &Entry) -> &'static str {
    match console(entry) {
        Console::Nes | Console::Famicom => "NES",
        Console::Snes => "SNES",
        Console::Unknown => "IMAGE",
    }
}

fn identity(entry: &Entry) -> (String, String, String) {
    let system = console(entry);
    let (region, version) = match entry {
        Entry::Cached(profile) => (
            profile
                .manifest
                .get("region")
                .and_then(Value::as_str)
                .unwrap_or("Unknown")
                .to_owned(),
            profile
                .manifest
                .get("version")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_owned(),
        ),
        Entry::Image(path) => {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let region = if name.contains("sneseur") {
                "Europe"
            } else if name.contains("snesusa") || name.contains("dp-nes-") {
                "USA"
            } else if name.contains("shvc") || name.contains("dp-hvc-") || name.contains("dp-hvcj-")
            {
                "Japan"
            } else {
                "Unknown"
            };
            let version = name
                .split("release-v")
                .nth(1)
                .and_then(|part| part.split('-').next())
                .map(|value| format!("Version {value}"))
                .unwrap_or_else(|| "Custom image".into());
            (region.to_owned(), version)
        }
    };
    let chip = match region.as_str() {
        "Europe" => "EUR",
        "USA" => "USA",
        "Japan" => "JPN",
        _ => "IMG",
    };
    let title = if matches!(system, Console::Nes | Console::Famicom) {
        if system == Console::Famicom {
            "Famicom Classic".into()
        } else {
            "NES Classic".into()
        }
    } else if system == Console::Unknown {
        "Classic Menu image".into()
    } else if region == "Japan" {
        "Super Famicom".into()
    } else if region == "Unknown" {
        "SNES Classic image".into()
    } else {
        format!(
            "SNES Classic · {}",
            if region == "USA" {
                "United States"
            } else {
                region.as_str()
            }
        )
    };
    let version = if let Some(value) = version.strip_prefix("release-v") {
        format!("Version {}", value.split('-').next().unwrap_or(value))
    } else {
        version
    };
    (title, chip.into(), version)
}

/// A stable visible-release key for suppressing an image when its imported,
/// playable profile is already present in the launcher.
pub(super) fn release_key(entry: &Entry) -> Option<(String, String, String)> {
    let (title, region, _) = identity(entry);
    if region == "IMG" {
        return None;
    }
    let version = match entry {
        Entry::Cached(profile) => profile.manifest.get("version")?.as_str()?,
        Entry::Image(path) => path
            .file_name()?
            .to_str()?
            .split("release-v")
            .nth(1)?
            .split('-')
            .next()?,
    };
    if version.is_empty() || version == "unknown" || version == "Custom image" {
        return None;
    }
    Some((title, region, version.to_owned()))
}

fn draw_geometry(
    canvas: &mut Canvas<Window>,
    commands: impl IntoIterator<Item = crate::frontend::launcher_scene::GeometryCommand>,
) {
    use crate::frontend::launcher_scene::GeometryCommand;
    for command in commands {
        match command {
            GeometryCommand::FillRect { rect, color } => fill(
                canvas,
                sdl_rect(rect),
                Color::RGBA(color.r, color.g, color.b, color.a),
            ),
            GeometryCommand::RoundedRect {
                rect,
                color,
                radius,
            } => panel(
                canvas,
                sdl_rect(rect),
                Color::RGBA(color.r, color.g, color.b, color.a),
                radius,
            ),
        }
    }
}

fn fill(canvas: &mut Canvas<Window>, rect: FRect, color: Color) {
    canvas.set_draw_color(color);
    let _ = canvas.fill_rect(rect);
}

fn panel(canvas: &mut Canvas<Window>, rect: FRect, color: Color, radius: f32) {
    // Rounded scanlines stay portable across SDL render backends.
    for row in 0..rect.h.ceil() as i32 {
        let y = row as f32 + 0.5;
        let delta = if y < radius {
            radius - y
        } else if y > rect.h - radius {
            y - (rect.h - radius)
        } else {
            0.0
        };
        let inset = if delta > 0.0 {
            radius - (radius * radius - delta * delta).max(0.0).sqrt()
        } else {
            0.0
        };
        fill(
            canvas,
            FRect::new(
                rect.x + inset,
                rect.y + row as f32,
                rect.w - inset * 2.0,
                1.0,
            ),
            color,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_button_cancellation_states_match_enabled_and_label_contract() {
        assert_eq!(
            primary_button(true, CancellationView::Available, true, false),
            PrimaryButton {
                label: "Cancel import",
                enabled: true
            }
        );
        assert_eq!(
            primary_button(true, CancellationView::Requested, true, false),
            PrimaryButton {
                label: "Cancelling…",
                enabled: false
            }
        );
        assert_eq!(
            primary_button(true, CancellationView::Finishing, true, false),
            PrimaryButton {
                label: "Finishing import…",
                enabled: false
            }
        );
        assert_eq!(
            primary_button(true, CancellationView::Unavailable, true, false),
            PrimaryButton {
                label: "Preparing your Classic…",
                enabled: false
            }
        );
        assert_eq!(
            primary_button(false, CancellationView::Unavailable, false, false),
            PrimaryButton {
                label: "Import an image",
                enabled: true
            }
        );
        assert_eq!(
            primary_button(false, CancellationView::Unavailable, true, true),
            PrimaryButton {
                label: "Launch Classic Menu",
                enabled: true
            }
        );
    }

    #[test]
    fn busy_cancel_hit_target_is_the_existing_primary_button() {
        let rect = BUTTONS[0];
        assert!(point_inside(
            rect.x + rect.w / 2.0,
            rect.y + rect.h / 2.0,
            BUTTONS[0]
        ));
        assert!(!point_inside(
            rect.x + rect.w + 1.0,
            rect.y + rect.h / 2.0,
            BUTTONS[0]
        ));
    }

    #[test]
    fn friendly_image_identity() {
        let (title, region, version) = identity(&Entry::Image(PathBuf::from(
            "dp-sneseur-release-v2.0.14-0-gabc.hsqs",
        )));
        assert_eq!(title, "SNES Classic · Europe");
        assert_eq!(region, "EUR");
        assert_eq!(version, "Version 2.0.14");
    }
    #[test]
    fn cached_and_filename_regions_use_the_same_flags() {
        for (filename, region, label) in [
            ("dp-sneseur-release-v2.hsqs", "Europe", "Europe"),
            ("dp-snesusa-release-v2.hsqs", "USA", "United States"),
            ("dp-shvc-release-v2.hsqs", "Japan", "Japan"),
            ("custom.hsqs", "Unknown", "Unknown"),
        ] {
            let cached = Entry::Cached(crate::profile::Profile {
                directory: PathBuf::from("cache-hash"),
                manifest: json!({"system":"snes", "region":region}),
            });
            let image = Entry::Image(PathBuf::from(filename));
            let cached_region = flags::Region::from_marker(&identity(&cached).1);
            let image_region = flags::Region::from_marker(&identity(&image).1);
            assert_eq!(cached_region, image_region);
            assert_eq!(cached_region.label(), label);
        }
    }
    #[test]
    fn controllers_distinguish_both_systems_and_unknown_images() {
        for (name, expected) in [
            ("dp-nes-release-v1.0.7.hsqs", Console::Nes),
            ("kernel-dp-hvc-release-v1.0.5.img", Console::Famicom),
            ("dp-hvcj-release-v3.0.1.hsqs", Console::Famicom),
            ("dp-snesusa-release-v2.0.14.hsqs", Console::Snes),
            ("dp-shvc-release-v2.0.12.hsqs", Console::Snes),
            ("custom.hsqs", Console::Unknown),
        ] {
            assert_eq!(console(&Entry::Image(PathBuf::from(name))), expected);
        }
    }
    #[test]
    fn cached_controller_uses_manifest_not_profile_directory_name() {
        for (manifest, expected) in [
            (json!({"system":"nes","ui":"hvc"}), Console::Famicom),
            (
                json!({"system":"nes","ui":"nes","region":"Japan"}),
                Console::Nes,
            ),
            (json!({"ui":"shvc"}), Console::Snes),
            (json!({"system":"snes"}), Console::Snes),
            (json!({}), Console::Unknown),
        ] {
            let entry = Entry::Cached(crate::profile::Profile {
                directory: PathBuf::from("hash-v7"),
                manifest,
            });
            assert_eq!(console(&entry), expected);
        }
    }
    #[test]
    fn rows_and_buttons_do_not_overlap() {
        assert!(row_rect(PAGE_SIZE - 1).y + row_rect(0).h < BUTTONS[0].y);
        assert!(BUTTONS[1].x + BUTTONS[1].w < row_rect(0).x);
    }
}

#[cfg(test)]
#[path = "recreation_tests.rs"]
mod recreation_tests;
