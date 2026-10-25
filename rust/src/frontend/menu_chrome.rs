//! Bound thumbnail draw commands for the App menu.

use super::menu_scene::{self, AtlasSprite, Command, ImageSource, Rgba, TextureId};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub struct ThumbnailScene {
    pub commands: Vec<Command>,
    pub hit: menu_scene::Rect,
}

/// Binds available current-owner snapshots to one thumbnail placement.
pub fn thumbnail_scene(
    placement: menu_scene::ThumbnailPlacement,
    cover: Option<ImageSource>,
    marker: Option<AtlasSprite>,
    lock: Option<AtlasSprite>,
) -> ThumbnailScene {
    let mut commands = Vec::new();
    if let Some(cover) = cover {
        commands.push(menu_scene::fit_image(
            cover.texture,
            cover.width,
            cover.height,
            placement.cover,
            placement.alpha,
        ));
    }
    if let (Some(rect), Some(marker)) = (placement.marker, marker) {
        commands.push(menu_scene::sprite(
            marker.texture,
            marker.source,
            rect,
            Rgba {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            },
            marker.flip_x,
        ));
    }
    if let Some(lock) = lock {
        commands.push(menu_scene::sprite(
            lock.texture,
            lock.source,
            placement.lock,
            Rgba {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            },
            lock.flip_x,
        ));
    }
    ThumbnailScene {
        commands,
        hit: placement.hit,
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum HudAction {
    Settings,
    ResumePanel,
    Sort,
    Start,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HudPrompt {
    pub icon: Option<AtlasSprite>,
    pub icon_width: f32,
    pub layout: super::font::TextLayout,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HudItem {
    pub commands: Vec<Command>,
    pub hit: menu_scene::Rect,
    pub action: HudAction,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HudScene {
    pub items: [HudItem; 4],
}

/// Builds the four fixed-order HUD prompt groups from current-owner snapshots.
pub fn hud_scene(
    nes: bool,
    item_gap: f32,
    group_gap: f32,
    prompts: &[HudPrompt; 4],
    atlases: &BTreeMap<String, TextureId>,
    tint: Rgba,
) -> HudScene {
    let metrics = std::array::from_fn(|index| menu_scene::HudMetrics {
        icon_width: prompts[index].icon_width,
        label_advance: prompts[index].layout.advance,
    });
    let placements = menu_scene::hud(nes, item_gap, group_gap, &metrics);
    let actions = [
        HudAction::Settings,
        HudAction::ResumePanel,
        HudAction::Sort,
        HudAction::Start,
    ];
    let items = std::array::from_fn(|index| {
        let prompt = &prompts[index];
        let placement = placements[index];
        let mut commands = Vec::new();
        if let Some(icon) = prompt.icon {
            commands.push(menu_scene::sprite(
                icon.texture,
                icon.source,
                placement.icon,
                Rgba {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 255,
                },
                icon.flip_x,
            ));
        }
        commands.extend(menu_scene::text(
            &prompt.layout,
            atlases,
            menu_scene::TextPosition {
                x: placement.label_x,
                y: placement.label_y,
                max_width: 1200.0,
                line_anchor: true,
                tint,
            },
        ));
        HudItem {
            commands,
            hit: placement.hit,
            action: actions[index],
        }
    });
    HudScene { items }
}

/// Builds the existing top/bottom menu bars and eligible NES regional overlay.
pub fn menu_bars(
    nes: bool,
    japan: bool,
    upper: Option<AtlasSprite>,
    lower: Option<AtlasSprite>,
    overlay: Option<AtlasSprite>,
) -> Vec<Command> {
    let white = Rgba {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };
    let mut commands = Vec::new();
    if let Some(sprite) = upper {
        commands.push(menu_scene::sprite(
            sprite.texture,
            sprite.source,
            menu_scene::Rect {
                x: 640.0,
                y: 36.0,
                w: 1296.0,
                h: 120.0,
            },
            white,
            sprite.flip_x,
        ));
    }
    if let Some(sprite) = lower {
        commands.push(menu_scene::sprite(
            sprite.texture,
            sprite.source,
            menu_scene::Rect {
                x: 640.0,
                y: 690.0,
                w: 1296.0,
                h: 120.0,
            },
            white,
            sprite.flip_x,
        ));
    }
    if nes
        && !japan
        && let Some(sprite) = overlay
    {
        commands.push(menu_scene::sprite(
            sprite.texture,
            sprite.source,
            menu_scene::Rect {
                x: 640.0,
                y: 36.0,
                w: 480.0,
                h: 120.0,
            },
            white,
            sprite.flip_x,
        ));
    }
    commands
}
