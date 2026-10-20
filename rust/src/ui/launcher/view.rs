//! Launcher-only vector chrome and embedded typography; no console assets needed.
use super::*;
use fontdue::{Font, FontSettings, Metrics};

pub const WIDTH: u32 = 1200;
pub const HEIGHT: u32 = 760;
pub const PAGE_SIZE: usize = 6;
const INK: Color = Color::RGB(236, 242, 245);
const MUTED: Color = Color::RGB(156, 171, 186);
const ACCENT: Color = Color::RGB(94, 234, 210);

pub const BUTTONS: [FRect; 3] = [
    FRect {
        x: 866.0,
        y: 650.0,
        w: 294.0,
        h: 58.0,
    },
    FRect {
        x: 24.0,
        y: 285.0,
        w: 192.0,
        h: 48.0,
    },
    FRect {
        x: 24.0,
        y: 342.0,
        w: 192.0,
        h: 48.0,
    },
];

pub fn row_rect(row: usize) -> FRect {
    FRect::new(264.0, 194.0 + row as f32 * 72.0, 556.0, 62.0)
}

struct Glyph {
    metrics: Metrics,
    texture: Option<Texture>,
}

pub struct View {
    fonts: [Font; 2],
    creator: TextureCreator<WindowContext>,
    glyphs: HashMap<(char, u16, bool), Glyph>,
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
        Ok(Self {
            fonts,
            creator: canvas.texture_creator(),
            glyphs: HashMap::new(),
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

    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        canvas: &mut Canvas<Window>,
        entries: &[Entry],
        selected: usize,
        busy: bool,
        status: &str,
        error: bool,
        hover: Option<(f32, f32)>,
        focus: Option<usize>,
        elapsed: f32,
    ) -> Result<(), String> {
        canvas.set_blend_mode(BlendMode::Blend);
        // A restrained diagonal wash, rendered as geometry rather than an image.
        for y in 0..HEIGHT {
            let t = y as f32 / HEIGHT as f32;
            fill(
                canvas,
                FRect::new(0.0, y as f32, WIDTH as f32, 1.0),
                Color::RGB(
                    (20.0 - 7.0 * t) as u8,
                    (30.0 - 10.0 * t) as u8,
                    (43.0 - 13.0 * t) as u8,
                ),
            );
        }
        fill(
            canvas,
            FRect::new(0.0, 0.0, 240.0, HEIGHT as f32),
            Color::RGBA(10, 16, 24, 225),
        );
        fill(
            canvas,
            FRect::new(239.0, 0.0, 1.0, HEIGHT as f32),
            Color::RGB(39, 52, 65),
        );
        self.text(canvas, "CLOVER", 28.0, 35.0, 32, true, INK, 180.0)?;
        self.text(
            canvas,
            "CLASSIC MENU / RUST",
            29.0,
            77.0,
            12,
            true,
            ACCENT,
            190.0,
        )?;
        fill(canvas, FRect::new(28.0, 122.0, 36.0, 3.0), ACCENT);
        self.text(
            canvas,
            "A classic, reimagined.",
            28.0,
            146.0,
            17,
            false,
            MUTED,
            190.0,
        )?;

        panel(
            canvas,
            FRect::new(16.0, 215.0, 208.0, 48.0),
            Color::RGB(28, 46, 55),
            8.0,
        );
        fill(canvas, FRect::new(16.0, 230.0, 3.0, 18.0), ACCENT);
        self.text(canvas, "Your library", 38.0, 226.0, 19, true, ACCENT, 164.0)?;
        for (index, label) in [(1, "+  Import image"), (2, "Open data folder")] {
            let active = hover.is_some_and(|(x, y)| point_inside(x, y, BUTTONS[index]))
                || focus == Some(index);
            if active && !busy {
                panel(canvas, BUTTONS[index], Color::RGB(35, 49, 64), 8.0);
            }
            self.text(
                canvas,
                label,
                38.0,
                BUTTONS[index].y + 12.0,
                18,
                false,
                if busy { MUTED } else { INK },
                172.0,
            )?;
        }
        self.text(
            canvas,
            "LOCAL. PRIVATE. YOURS.",
            28.0,
            590.0,
            12,
            true,
            ACCENT,
            190.0,
        )?;
        self.text(
            canvas,
            "Your images stay read-only.",
            28.0,
            618.0,
            15,
            false,
            MUTED,
            192.0,
        )?;
        self.text(
            canvas,
            "Saves live separately.",
            28.0,
            640.0,
            15,
            false,
            MUTED,
            192.0,
        )?;
        self.text(canvas, "SDL3 + ares", 28.0, 714.0, 14, false, MUTED, 192.0)?;

        self.text(
            canvas,
            "MAKE YOURSELF AT HOME",
            272.0,
            35.0,
            13,
            true,
            ACCENT,
            700.0,
        )?;
        self.text(
            canvas,
            "Choose your Classic.",
            270.0,
            59.0,
            42,
            true,
            INK,
            850.0,
        )?;
        self.text(
            canvas,
            "Pick a region and version, or bring your own filesystem image.",
            272.0,
            118.0,
            18,
            false,
            MUTED,
            870.0,
        )?;
        self.text(
            canvas,
            &format!("YOUR LIBRARY  /  {} SOURCES", entries.len()),
            272.0,
            163.0,
            12,
            true,
            MUTED,
            450.0,
        )?;
        let page = selected / PAGE_SIZE;
        for (row, entry) in entries
            .iter()
            .skip(page * PAGE_SIZE)
            .take(PAGE_SIZE)
            .enumerate()
        {
            let rect = row_rect(row);
            let chosen = page * PAGE_SIZE + row == selected;
            let hot = hover.is_some_and(|(x, y)| point_inside(x, y, rect));
            if chosen {
                panel(canvas, rect, ACCENT, 9.0);
            }
            let inside = if chosen {
                FRect::new(rect.x + 1.0, rect.y + 1.0, rect.w - 2.0, rect.h - 2.0)
            } else {
                rect
            };
            panel(
                canvas,
                inside,
                if chosen {
                    Color::RGB(28, 51, 60)
                } else if hot && !busy {
                    Color::RGB(36, 49, 65)
                } else {
                    Color::RGB(25, 36, 49)
                },
                8.0,
            );
            let (title, region, version) = identity(entry);
            panel(
                canvas,
                FRect::new(rect.x + 14.0, rect.y + 12.0, 48.0, 38.0),
                if chosen {
                    Color::RGB(54, 83, 88)
                } else {
                    Color::RGB(41, 54, 69)
                },
                6.0,
            );
            self.text(
                canvas,
                &region,
                rect.x + 21.0,
                rect.y + 24.0,
                13,
                true,
                if chosen { ACCENT } else { MUTED },
                42.0,
            )?;
            self.text(
                canvas,
                &title,
                rect.x + 78.0,
                rect.y + 8.0,
                20,
                true,
                INK,
                340.0,
            )?;
            let kind = if matches!(entry, Entry::Cached(_)) {
                "Ready to play"
            } else {
                "Filesystem image"
            };
            self.text(
                canvas,
                &format!("{version}  ·  {kind}"),
                rect.x + 78.0,
                rect.y + 35.0,
                14,
                false,
                MUTED,
                445.0,
            )?;
            if chosen {
                self.text(
                    canvas,
                    "›",
                    rect.x + 523.0,
                    rect.y + 13.0,
                    27,
                    true,
                    ACCENT,
                    22.0,
                )?;
            }
        }
        if entries.is_empty() {
            panel(
                canvas,
                FRect::new(264.0, 194.0, 556.0, 422.0),
                Color::RGB(25, 36, 49),
                10.0,
            );
            self.text(
                canvas,
                "Your library starts here.",
                294.0,
                265.0,
                28,
                true,
                INK,
                490.0,
            )?;
            self.text(
                canvas,
                "Import a supplied .hsqs or .tar.gz image to begin.",
                294.0,
                317.0,
                18,
                false,
                MUTED,
                490.0,
            )?;
            self.text(
                canvas,
                "Already have game data? Open its assets folder.",
                294.0,
                347.0,
                18,
                false,
                MUTED,
                490.0,
            )?;
            self.text(
                canvas,
                "No game files are included or downloaded.",
                294.0,
                530.0,
                16,
                false,
                MUTED,
                490.0,
            )?;
        }

        panel(
            canvas,
            FRect::new(844.0, 194.0, 332.0, 422.0),
            Color::RGB(23, 34, 47),
            12.0,
        );
        controller(canvas, 1010.0, 272.0);
        let entry = entries.get(selected);
        let (title, region, version) = entry.map(identity).unwrap_or_else(|| {
            (
                "SNES Classic".into(),
                "ALL".into(),
                "Your region. Your version.".into(),
            )
        });
        self.text(canvas, &title, 866.0, 353.0, 25, true, INK, 290.0)?;
        self.text(canvas, &version, 866.0, 390.0, 16, false, MUTED, 290.0)?;
        let ready = matches!(entry, Some(Entry::Cached(_)));
        let badge = if ready {
            "VERIFIED CACHE"
        } else if entry.is_some() {
            "READY TO IMPORT"
        } else {
            "AWAITING GAME DATA"
        };
        self.text(canvas, badge, 866.0, 433.0, 12, true, ACCENT, 290.0)?;
        fill(
            canvas,
            FRect::new(866.0, 464.0, 286.0, 1.0),
            Color::RGB(48, 62, 76),
        );
        self.text(canvas, "REGION", 866.0, 482.0, 11, true, MUTED, 120.0)?;
        self.text(canvas, &region, 866.0, 503.0, 18, true, INK, 120.0)?;
        self.text(canvas, "SOURCE", 1014.0, 482.0, 11, true, MUTED, 130.0)?;
        self.text(
            canvas,
            if ready {
                "Local cache"
            } else if entry.is_some() {
                "Local image"
            } else {
                "Not selected"
            },
            1014.0,
            503.0,
            18,
            true,
            INK,
            140.0,
        )?;
        let source = entry
            .map(|entry| match entry {
                Entry::Cached(profile) => profile.directory.display().to_string(),
                Entry::Image(path) => path.display().to_string(),
            })
            .unwrap_or_else(|| "Use Import image or Open data folder.".into());
        self.text(canvas, &source, 866.0, 550.0, 13, false, MUTED, 286.0)?;
        self.text(
            canvas,
            if ready {
                "No original image needed."
            } else {
                "Imported locally. Never executed."
            },
            866.0,
            576.0,
            14,
            false,
            MUTED,
            286.0,
        )?;

        let primary_hot =
            hover.is_some_and(|(x, y)| point_inside(x, y, BUTTONS[0])) || focus == Some(0);
        let enabled = !busy;
        panel(
            canvas,
            BUTTONS[0],
            if !enabled {
                Color::RGB(43, 57, 68)
            } else if primary_hot {
                Color::RGB(143, 249, 226)
            } else {
                ACCENT
            },
            9.0,
        );
        self.text(
            canvas,
            if busy {
                "Preparing your Classic…"
            } else if entry.is_none() {
                "Import an image"
            } else if ready {
                "Launch Classic Menu"
            } else {
                "Import & launch"
            },
            890.0,
            666.0,
            21,
            true,
            if enabled {
                Color::RGB(12, 35, 38)
            } else {
                MUTED
            },
            245.0,
        )?;
        self.text(
            canvas,
            &format!(
                "{} / {}",
                if entries.is_empty() { 0 } else { page + 1 },
                entries.len().div_ceil(PAGE_SIZE)
            ),
            272.0,
            626.0,
            13,
            false,
            MUTED,
            100.0,
        )?;
        let color = if error {
            Color::RGB(255, 163, 156)
        } else {
            MUTED
        };
        // Wrap status by actual glyph widths rather than losing import errors.
        let mut line = String::new();
        let mut y = 657.0;
        for word in status.split_whitespace() {
            let candidate = format!("{line} {word}");
            if self.width(&candidate, 15, false) > 550.0 && !line.is_empty() {
                self.text(canvas, &line, 272.0, y, 15, false, color, 550.0)?;
                y += 21.0;
                line.clear();
                if y > 700.0 {
                    break;
                }
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        self.text(canvas, &line, 272.0, y, 15, false, color, 550.0)?;
        if busy {
            panel(
                canvas,
                FRect::new(272.0, 640.0, 548.0, 3.0),
                Color::RGB(43, 61, 74),
                1.0,
            );
            let offset = (elapsed * 170.0).rem_euclid(420.0);
            fill(
                canvas,
                FRect::new(272.0 + offset, 640.0, 128.0, 3.0),
                ACCENT,
            );
        }
        self.text(
            canvas,
            "Up / Down  Select     Enter  Launch     I  Import     D  Data folder     Esc  Quit",
            272.0,
            728.0,
            13,
            false,
            MUTED,
            880.0,
        )?;
        Ok(())
    }
}

fn identity(entry: &Entry) -> (String, String, String) {
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
            } else if name.contains("snesusa") {
                "USA"
            } else if name.contains("shvc") {
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
    let title = if region == "Japan" {
        "Super Famicom".into()
    } else if region == "Unknown" {
        "SNES Classic image".into()
    } else {
        format!("SNES Classic · {region}")
    };
    let version = if let Some(value) = version.strip_prefix("release-v") {
        format!("Version {}", value.split('-').next().unwrap_or(value))
    } else {
        version
    };
    (title, chip.into(), version)
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

fn controller(canvas: &mut Canvas<Window>, x: f32, y: f32) {
    panel(
        canvas,
        FRect::new(x - 118.0, y - 42.0, 236.0, 94.0),
        Color::RGB(13, 24, 35),
        42.0,
    );
    panel(
        canvas,
        FRect::new(x - 114.0, y - 48.0, 228.0, 94.0),
        Color::RGB(59, 76, 94),
        42.0,
    );
    panel(
        canvas,
        FRect::new(x - 106.0, y - 40.0, 212.0, 78.0),
        Color::RGB(43, 59, 77),
        34.0,
    );
    panel(
        canvas,
        FRect::new(x - 84.0, y - 8.0, 48.0, 16.0),
        Color::RGB(18, 29, 41),
        3.0,
    );
    panel(
        canvas,
        FRect::new(x - 68.0, y - 24.0, 16.0, 48.0),
        Color::RGB(18, 29, 41),
        3.0,
    );
    for (dx, dy, color) in [
        (60.0, -18.0, Color::RGB(146, 142, 221)),
        (81.0, 1.0, Color::RGB(94, 234, 210)),
        (39.0, 1.0, Color::RGB(91, 170, 194)),
        (60.0, 20.0, Color::RGB(214, 147, 179)),
    ] {
        panel(
            canvas,
            FRect::new(x + dx - 10.0, y + dy - 10.0, 20.0, 20.0),
            color,
            10.0,
        );
    }
    for dx in [-14.0, 9.0] {
        panel(
            canvas,
            FRect::new(x + dx, y + 4.0, 17.0, 6.0),
            Color::RGB(131, 151, 168),
            3.0,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn rows_and_buttons_do_not_overlap() {
        assert!(row_rect(PAGE_SIZE - 1).y + row_rect(0).h < BUTTONS[0].y);
        assert!(BUTTONS[1].x + BUTTONS[1].w < row_rect(0).x);
    }
}
