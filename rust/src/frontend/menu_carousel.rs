use super::menu_scene::{self, Command, ImageSource, Rect, Rgba, SpritePlacement};
use super::menu_scene::{AtlasSprite, TextPosition, TextureId};
use std::collections::BTreeMap;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ResumeState {
    Empty,
    Saved,
    Locked,
}

#[derive(Copy, Clone, Debug)]
pub struct CardInput {
    pub nes: bool,
    pub x: f32,
    pub shift: f32,
    pub active: bool,
    pub firmware_locked: bool,
    pub players: u32,
    pub simultaneous: bool,
    pub slots: [ResumeState; 4],
}

#[derive(Clone, Debug, PartialEq)]
pub enum CardPart {
    Sprite(SpritePlacement),
    Cover { bounds: Rect, alpha: u8 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct CardPlan {
    pub parts: Vec<CardPart>,
    pub hit: Rect,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CardResources {
    pub sprites: BTreeMap<String, menu_scene::AtlasSprite>,
    pub cover: Option<ImageSource>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CardScene {
    pub commands: Vec<Command>,
    pub hit: Rect,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ArrowAction {
    Previous,
    Next,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ArrowHit {
    pub action: ArrowAction,
    pub rect: Rect,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArrowScene {
    pub commands: Vec<Command>,
    pub hits: [ArrowHit; 2],
}

pub fn arrows(
    card_shift: f32,
    left: Option<AtlasSprite>,
    right: Option<AtlasSprite>,
) -> ArrowScene {
    let mut commands = Vec::new();
    let white = Rgba {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };
    for (sprite, x) in [(left, 38.0), (right, 1242.0)] {
        if let Some(sprite) = sprite {
            commands.push(menu_scene::sprite(
                sprite.texture,
                sprite.source,
                Rect {
                    x,
                    y: 360.0 + card_shift,
                    w: 32.0,
                    h: 44.0,
                },
                white,
                sprite.flip_x,
            ));
        }
    }
    ArrowScene {
        commands,
        hits: [
            ArrowHit {
                action: ArrowAction::Previous,
                rect: Rect {
                    x: 0.0,
                    y: 240.0,
                    w: 65.0,
                    h: 255.0,
                },
            },
            ArrowHit {
                action: ArrowAction::Next,
                rect: Rect {
                    x: 1215.0,
                    y: 240.0,
                    w: 65.0,
                    h: 255.0,
                },
            },
        ],
    }
}

pub fn title_scene(
    nes: bool,
    title_shift: f32,
    layout: &super::font::TextLayout,
    atlases: &BTreeMap<String, TextureId>,
    caption: Option<AtlasSprite>,
    title_tint: Rgba,
) -> Vec<Command> {
    let mut commands = Vec::new();
    if let Some(caption) = caption {
        commands.push(menu_scene::sprite(
            caption.texture,
            caption.source,
            Rect {
                x: 640.0,
                y: 178.0 + title_shift,
                w: if nes { 1026.0 } else { 1044.0 },
                h: 66.0,
            },
            Rgba {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            },
            caption.flip_x,
        ));
    }
    commands.extend(menu_scene::text(
        layout,
        atlases,
        TextPosition {
            x: 640.0,
            y: 176.0 + title_shift,
            max_width: 990.0,
            line_anchor: true,
            tint: title_tint,
        },
    ));
    commands
}

pub fn card_scene(plan: &CardPlan, resources: &CardResources) -> CardScene {
    let mut commands = Vec::new();
    for part in &plan.parts {
        match part {
            CardPart::Sprite(placement) => {
                if let Some(sprite) = resources.sprites.get(&placement.name) {
                    commands.push(menu_scene::sprite(
                        sprite.texture,
                        sprite.source,
                        placement.centered,
                        placement.tint,
                        sprite.flip_x,
                    ));
                }
            }
            CardPart::Cover { bounds, alpha } => {
                if let Some(image) = resources.cover {
                    commands.push(menu_scene::fit_image(
                        image.texture,
                        image.width,
                        image.height,
                        *bounds,
                        *alpha,
                    ));
                }
            }
        }
    }
    CardScene {
        commands,
        hit: plan.hit,
    }
}

pub fn card(input: CardInput) -> CardPlan {
    let white = Rgba {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };
    let mut parts = vec![CardPart::Sprite(SpritePlacement {
        name: if input.active {
            "game_card_active"
        } else {
            "game_card"
        }
        .into(),
        centered: Rect {
            x: input.x + if input.nes { 3.0 } else { 0.0 },
            y: (if input.nes { 363.0 } else { 360.0 }) + input.shift,
            w: if input.nes { 246.0 } else { 252.0 },
            h: if input.nes { 270.0 } else { 276.0 },
        },
        tint: white,
    })];
    parts.push(CardPart::Cover {
        bounds: Rect {
            x: input.x - 114.0,
            y: 234.0 + input.shift,
            w: 228.0,
            h: 204.0,
        },
        alpha: if input.firmware_locked { 165 } else { 255 },
    });
    if input.firmware_locked {
        parts.push(CardPart::Sprite(SpritePlacement {
            name: "icon_lock".into(),
            centered: Rect {
                x: input.x + 98.0,
                y: 239.0 + input.shift,
                w: 28.0,
                h: 36.0,
            },
            tint: white,
        }));
    }
    let player = if input.players > 1 {
        if input.simultaneous {
            "icon_2P_simultaneous"
        } else {
            "icon_1P-2P"
        }
    } else {
        "icon_1P"
    };
    parts.push(CardPart::Sprite(SpritePlacement {
        name: if input.nes && input.active {
            format!("{player}_active")
        } else {
            player.into()
        },
        centered: Rect {
            x: input.x + if input.nes { 76.5 } else { 81.0 },
            y: (if input.nes { 463.5 } else { 462.0 }) + input.shift,
            w: if input.nes { 57.0 } else { 66.0 },
            h: if input.nes { 33.0 } else { 42.0 },
        },
        tint: white,
    }));
    for (slot, state) in input.slots.into_iter().enumerate() {
        let name = match state {
            ResumeState::Empty => "icon_Resume_off",
            ResumeState::Saved => "icon_Resume_on",
            ResumeState::Locked => "icon_Resume_lock",
        };
        parts.push(CardPart::Sprite(SpritePlacement {
            name: format!("{name}{}", if input.active { "_active" } else { "" }),
            centered: Rect {
                x: input.x - if input.nes { 88.0 } else { 96.0 } + slot as f32 * 24.0,
                y: 468.0 + input.shift,
                w: 18.0,
                h: 18.0,
            },
            tint: white,
        }));
    }
    CardPlan {
        parts,
        hit: Rect {
            x: input.x - 126.0,
            y: 222.0 + input.shift,
            w: 252.0,
            h: 276.0,
        },
    }
}
