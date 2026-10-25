//! Owned generic modal-panel presentation, without native action authority.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RowViewport {
    pub focus: usize,
    pub visible_count: usize,
    pub first_row: usize,
    pub row_height: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RowScene {
    pub commands: Vec<super::menu_scene::Command>,
    pub hit: super::menu_scene::Rect,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FooterScene {
    pub commands: Vec<super::menu_scene::Command>,
    pub hit: Option<super::menu_scene::Rect>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PanelTextScene {
    pub heading: Vec<super::menu_scene::Command>,
    pub body: Vec<super::menu_scene::Command>,
    pub rows_y: f32,
}

/// A captured presentation key; native code must validate its current binding.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PanelKey(pub String);

#[derive(Clone, Debug, PartialEq)]
pub enum PanelStep {
    Draw(super::menu_scene::Command),
    Dialog(super::menu_scene::Rect),
    Hit {
        key: PanelKey,
        rect: super::menu_scene::Rect,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct PanelInput {
    pub generation: usize,
    pub text: PanelTextScene,
    pub rows: Vec<(PanelKey, RowScene)>,
    pub footer: FooterScene,
    pub footer_key: Option<PanelKey>,
    pub actions: Vec<PanelKey>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PanelScene {
    pub generation: usize,
    pub steps: Vec<PanelStep>,
    pub actions: Vec<PanelKey>,
}

pub fn compose_panel(input: PanelInput) -> PanelScene {
    use super::menu_scene::{Command, Rect, Rgba};
    let mut steps = vec![
        PanelStep::Draw(Command::Fill {
            rect: Rect {
                x: 0.0,
                y: 96.0,
                w: 1280.0,
                h: 550.0,
            },
            color: Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 190,
            },
        }),
        PanelStep::Dialog(Rect {
            x: 200.0,
            y: 128.0,
            w: 880.0,
            h: 478.0,
        }),
    ];
    steps.extend(
        input
            .text
            .heading
            .into_iter()
            .chain(input.text.body)
            .map(PanelStep::Draw),
    );
    for (key, row) in input.rows {
        steps.extend(row.commands.into_iter().map(PanelStep::Draw));
        steps.push(PanelStep::Hit { key, rect: row.hit });
    }
    steps.extend(input.footer.commands.into_iter().map(PanelStep::Draw));
    if let (Some(key), Some(rect)) = (input.footer_key, input.footer.hit) {
        steps.push(PanelStep::Hit { key, rect });
    }
    PanelScene {
        generation: input.generation,
        steps,
        actions: input.actions,
    }
}

/// Places already-measured panel heading/body text while preserving row start.
pub fn text_scene(
    heading: &super::font::TextLayout,
    body: &[super::font::TextLayout],
    message: bool,
    atlases: &std::collections::BTreeMap<String, super::menu_scene::TextureId>,
) -> PanelTextScene {
    use super::menu_scene::{Rgba, TextPosition};

    let white = Rgba {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };
    let heading = super::menu_scene::text(
        heading,
        atlases,
        TextPosition {
            x: 640.0,
            y: 174.0,
            max_width: 790.0,
            line_anchor: true,
            tint: white,
        },
    );
    let mut commands = Vec::new();
    let mut y = 227.0;
    let step = if message { 32.0 } else { 38.0 };
    for layout in body {
        commands.extend(super::menu_scene::text(
            layout,
            atlases,
            TextPosition {
                x: 640.0,
                y,
                max_width: 790.0,
                line_anchor: !message,
                tint: white,
            },
        ));
        y += step;
    }
    PanelTextScene {
        heading,
        body: commands,
        rows_y: y,
    }
}

/// Builds generic-panel footer presentation; enabled controls only hit geometry.
pub fn footer(
    layout: &super::font::TextLayout,
    atlases: &std::collections::BTreeMap<String, super::menu_scene::TextureId>,
    icon: Option<super::menu_scene::AtlasSprite>,
    enabled: bool,
) -> FooterScene {
    use super::menu_scene::{Rect, Rgba};

    let back_width = layout.advance;
    let total = 40.0 + back_width;
    let white = Rgba {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };
    let mut commands = Vec::new();
    if let Some(icon) = icon {
        commands.push(super::menu_scene::sprite(
            icon.texture,
            icon.source,
            Rect {
                x: 640.0 - total * 0.5 + 18.0,
                y: 567.0,
                w: 36.0,
                h: 36.0,
            },
            white,
            icon.flip_x,
        ));
    }
    commands.extend(super::menu_scene::text(
        layout,
        atlases,
        super::menu_scene::TextPosition {
            x: 640.0 - total * 0.5 + 40.0 + back_width * 0.5,
            y: 567.0,
            max_width: 1200.0,
            line_anchor: true,
            tint: white,
        },
    ));
    FooterScene {
        commands,
        hit: enabled.then_some(Rect {
            x: 640.0 - total * 0.5 - 12.0,
            y: 543.0,
            w: total + 24.0,
            h: 48.0,
        }),
    }
}

/// Builds a panel row's optional highlight, measured label, and hit geometry.
pub fn row(
    y: f32,
    row_height: f32,
    highlighted: bool,
    highlight: Option<super::menu_scene::AtlasSprite>,
    label: &super::font::TextLayout,
    atlases: &std::collections::BTreeMap<String, super::menu_scene::TextureId>,
) -> RowScene {
    use super::menu_scene::{Rect, Rgba};

    let rect = Rect {
        x: 365.0,
        y: y - 14.0,
        w: 550.0,
        h: row_height,
    };
    let mut commands = Vec::new();
    if highlighted && let Some(highlight) = highlight {
        commands.push(super::menu_scene::sprite(
            highlight.texture,
            highlight.source,
            Rect {
                x: 640.0,
                y: rect.y + rect.h * 0.5,
                w: 570.0,
                h: row_height - 1.0,
            },
            Rgba {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            },
            highlight.flip_x,
        ));
    }
    commands.extend(super::menu_scene::text(
        label,
        atlases,
        super::menu_scene::TextPosition {
            x: 640.0,
            y: rect.y + rect.h * 0.5,
            max_width: 520.0,
            line_anchor: true,
            tint: Rgba {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            },
        },
    ));
    RowScene {
        commands,
        hit: rect,
    }
}

pub fn row_viewport(entry_count: usize, focus: usize, y: f32) -> RowViewport {
    let focus = focus.min(entry_count);
    let visible_count = entry_count.min(((530.0 - y) / 24.0).floor().max(1.0) as usize);
    let first_row = focus
        .saturating_add(1)
        .saturating_sub(visible_count)
        .min(entry_count.saturating_sub(visible_count));
    let row_height = 43.0_f32
        .min((530.0 - y) / visible_count.max(1) as f32)
        .max(24.0);
    RowViewport {
        focus,
        visible_count,
        first_row,
        row_height,
    }
}
