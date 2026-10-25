//! Plain App menu presentation identifiers and commands.

pub use super::font::Rect;

/// A process-local reference to a texture owned by one native assets owner.
/// This is not a path, persistent identifier, or authorization to open data.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct TextureId {
    pub generation: usize,
    pub slot: usize,
}

/// A snapshot of one atlas frame resolved by the current native assets owner.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct AtlasSprite {
    pub texture: TextureId,
    pub source: Rect,
    pub flip_x: bool,
}

/// A snapshot of one loaded image resolved by the current native assets owner.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ImageSource {
    pub texture: TextureId,
    pub width: u32,
    pub height: u32,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct TextPosition {
    pub x: f32,
    pub y: f32,
    pub max_width: f32,
    pub line_anchor: bool,
    pub tint: Rgba,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Filter {
    Nearest,
    Linear,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct TextureQuad {
    pub texture: TextureId,
    pub destination: Rect,
    pub source: Option<Rect>,
    pub tint: Rgba,
    pub filter: Filter,
    pub flip_x: bool,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Command {
    Texture(TextureQuad),
    Fill { rect: Rect, color: Rgba },
}

/// Captures measured bitmap glyphs as owned draw commands using existing placement policy.
pub fn text(
    layout: &super::font::TextLayout,
    atlases: &std::collections::BTreeMap<String, TextureId>,
    position: TextPosition,
) -> Vec<Command> {
    let placement = super::font::placement(
        layout,
        position.x,
        position.y,
        position.max_width,
        position.line_anchor,
    );
    layout
        .glyphs
        .iter()
        .filter_map(|glyph| {
            let texture = *atlases.get(&glyph.font)?;
            Some(Command::Texture(TextureQuad {
                texture,
                destination: placement.glyph_rect(glyph),
                source: Some(glyph.source),
                tint: position.tint,
                filter: Filter::Linear,
                flip_x: false,
            }))
        })
        .collect()
}

/// Applies the App atlas's legacy sprite name alias and suffix convention.
pub fn sprite_key(name: &str) -> (String, bool) {
    let flip_x = name == "caption_edge_left";
    let name = if flip_x { "caption_edge" } else { name };
    let key = if name.ends_with(".png") {
        name.to_owned()
    } else {
        format!("{name}.png")
    };
    (key, flip_x)
}

/// Builds an atlas sprite command from a center-positioned rectangle.
pub fn sprite(
    texture: TextureId,
    source: Rect,
    centered: Rect,
    tint: Rgba,
    flip_x: bool,
) -> Command {
    Command::Texture(TextureQuad {
        texture,
        destination: Rect {
            x: (centered.x - centered.w * 0.5).round(),
            y: (centered.y - centered.h * 0.5).round(),
            w: centered.w,
            h: centered.h,
        },
        source: Some(source),
        tint,
        filter: Filter::Nearest,
        flip_x,
    })
}

/// Binds layout-only sprite placements to supplied current-owner snapshots.
pub fn sprite_scene(
    plan: &[SpritePlacement],
    sources: &std::collections::BTreeMap<String, AtlasSprite>,
) -> Vec<Command> {
    plan.iter()
        .filter_map(|placement| {
            let source = sources.get(&placement.name)?;
            Some(sprite(
                source.texture,
                source.source,
                placement.centered,
                placement.tint,
                source.flip_x,
            ))
        })
        .collect()
}

/// Fits an image within bounds while preserving the legacy centered scaling math.
pub fn fit_image(
    texture: TextureId,
    image_width: u32,
    image_height: u32,
    bounds: Rect,
    alpha: u8,
) -> Command {
    let scale = (bounds.w / image_width as f32).min(bounds.h / image_height as f32);
    let width = image_width as f32 * scale;
    let height = image_height as f32 * scale;
    Command::Texture(TextureQuad {
        texture,
        destination: Rect {
            x: bounds.x + (bounds.w - width) * 0.5,
            y: bounds.y + (bounds.h - height) * 0.5,
            w: width,
            h: height,
        },
        source: None,
        tint: Rgba {
            r: 255,
            g: 255,
            b: 255,
            a: alpha,
        },
        filter: Filter::Linear,
        flip_x: false,
    })
}

/// Layout-only selection sprites; this does not retain or authorize textures.
#[derive(Clone, Debug, PartialEq)]
pub struct SpritePlacement {
    pub name: String,
    /// Center-positioned rectangle (x/y are the center, w/h are dimensions).
    pub centered: Rect,
    pub tint: Rgba,
}

/// Plans the App's eight atlas sprites around a selected item.
pub fn selection(nes: bool, centered: Rect, time: f32) -> Vec<SpritePlacement> {
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
    let color = (255.0 * tint) as u8;
    let tint = Rgba {
        r: color,
        g: color,
        b: 255,
        a: 255,
    };
    let widths = [32.0, centered.w - 64.0, 32.0];
    let heights = [32.0, centered.h - 64.0, 32.0];
    let left = centered.x - centered.w * 0.5;
    let top = centered.y - centered.h * 0.5;
    let mut result = Vec::with_capacity(8);
    for (row, &height) in heights.iter().enumerate() {
        for (column, &width) in widths.iter().enumerate() {
            if row == 1 && column == 1 {
                continue;
            }
            let tile = row * 3 + column + 1;
            let name = if !nes && row != 1 && column != 1 {
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
                    centered.w - 32.0
                }) + width * 0.5;
            let cy =
                top + (if row == 0 {
                    0.0
                } else if row == 1 {
                    32.0
                } else {
                    centered.h - 32.0
                }) + height * 0.5;
            result.push(SpritePlacement {
                name,
                centered: Rect {
                    x: cx,
                    y: cy,
                    w: width,
                    h: height,
                },
                tint,
            });
        }
    }
    result
}

/// Produces the initial App wallpaper operation(s), preserving native layout inputs.
pub fn wallpaper(
    nes: bool,
    background: Rgba,
    wall: Option<TextureId>,
    offset: f32,
) -> Vec<Command> {
    if nes {
        return vec![Command::Fill {
            rect: Rect {
                x: 0.0,
                y: 87.0,
                w: 1280.0,
                h: 552.0,
            },
            color: background,
        }];
    }
    let Some(texture) = wall else {
        return Vec::new();
    };
    (-1..=1)
        .map(|tile| {
            Command::Texture(TextureQuad {
                texture,
                destination: Rect {
                    x: -128.0 - offset + tile as f32 * 1536.0,
                    y: 87.0,
                    w: 1536.0,
                    h: 552.0,
                },
                source: None,
                tint: Rgba {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 255,
                },
                filter: Filter::Linear,
                flip_x: false,
            })
        })
        .collect()
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct HudMetrics {
    pub icon_width: f32,
    pub label_advance: f32,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct HudPlacement {
    /// Center-positioned icon rectangle.
    pub icon: Rect,
    pub label_x: f32,
    pub label_y: f32,
    /// Top-left positioned hit rectangle.
    pub hit: Rect,
}

/// Plans the four App HUD prompts without retaining texture or action authority.
pub fn hud(
    nes: bool,
    item_gap: f32,
    group_gap: f32,
    prompts: &[HudMetrics; 4],
) -> [HudPlacement; 4] {
    let widths = prompts.map(|prompt| prompt.icon_width + item_gap + prompt.label_advance);
    let total = widths.iter().sum::<f32>() + group_gap * 3.0;
    let mut x = 640.0 - total * 0.5;
    std::array::from_fn(|index| {
        let prompt = prompts[index];
        let whole = widths[index];
        let icon_height = if nes {
            if prompt.icon_width == 30.0 {
                30.0
            } else {
                24.0
            }
        } else if prompt.icon_width == 36.0 {
            36.0
        } else {
            30.0
        };
        let y = if nes { 612.0 } else { 609.0 };
        let placement = HudPlacement {
            icon: Rect {
                x: x + prompt.icon_width * 0.5,
                y,
                w: prompt.icon_width,
                h: icon_height,
            },
            label_x: x + prompt.icon_width + item_gap + prompt.label_advance * 0.5,
            label_y: y,
            hit: Rect {
                x: x - 5.0,
                y: 585.0,
                w: whole + 10.0,
                h: 48.0,
            },
        };
        x += whole + group_gap;
        placement
    })
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ThumbnailPlacement {
    pub index: usize,
    /// Top-left positioned cover rectangle.
    pub cover: Rect,
    pub alpha: u8,
    /// Center-positioned selected marker, if selected.
    pub marker: Option<Rect>,
    /// Center-positioned firmware lock icon.
    pub lock: Rect,
    /// Top-left positioned hit rectangle.
    pub hit: Rect,
}

/// Plans the App thumbnail strip without classifying games or authorizing actions.
pub fn thumbnails(game_count: usize, selected: usize) -> Vec<ThumbnailPlacement> {
    if game_count == 0 {
        return Vec::new();
    }
    let step = 42.0_f32.min(1080.0 / game_count as f32);
    let left = 640.0 - (game_count - 1) as f32 * step * 0.5;
    (0..game_count)
        .map(|index| {
            let x = left + index as f32 * step;
            let chosen = index == selected;
            ThumbnailPlacement {
                index,
                cover: Rect {
                    x: x - 16.0,
                    y: 535.0,
                    w: 32.0,
                    h: 32.0,
                },
                alpha: if chosen { 255 } else { 190 },
                marker: chosen.then_some(Rect {
                    x,
                    y: 522.0,
                    w: 24.0,
                    h: 16.0,
                }),
                lock: Rect {
                    x: x + 9.0,
                    y: 542.0,
                    w: 14.0,
                    h: 18.0,
                },
                hit: Rect {
                    x: x - step * 0.5,
                    y: 526.0,
                    w: step,
                    h: 55.0,
                },
            }
        })
        .collect()
}
