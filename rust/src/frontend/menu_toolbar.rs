//! Pure layout and focus policy for the App toolbar.

use super::menu_frame::{Hit, HitAction, Step};
use super::{
    font::TextLayout,
    menu_scene::{self, AtlasSprite, Command, Rect, Rgba, TextPosition, TextureId},
};
use std::collections::BTreeMap;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ToolbarAction {
    Display,
    Settings,
    Language,
    Copyright,
    Manual,
}

pub const ALL: [ToolbarAction; 5] = [
    ToolbarAction::Display,
    ToolbarAction::Settings,
    ToolbarAction::Language,
    ToolbarAction::Copyright,
    ToolbarAction::Manual,
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolbarInput {
    pub hover: String,
    pub panel: String,
    pub keyboard_focus: bool,
    pub keyboard_index: usize,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ToolbarItemPlan {
    pub action: ToolbarAction,
    pub center_x: f32,
    pub active: bool,
    pub tooltip: bool,
    pub hit: Rect,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct TooltipStyle {
    pub body_y: f32,
    pub height: f32,
    pub edge_width: f32,
    pub minimum_width: f32,
    pub text_y: f32,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct TooltipSources {
    pub body: Option<AtlasSprite>,
    pub left: Option<AtlasSprite>,
    pub right: Option<AtlasSprite>,
    pub arrow: Option<AtlasSprite>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ToolbarItemResources {
    pub active: Option<AtlasSprite>,
    pub icon: Option<AtlasSprite>,
    pub tooltip: Vec<Command>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToolbarScene {
    pub generation: usize,
    pub steps: Vec<Step>,
}

pub fn toolbar_scene(
    generation: usize,
    plans: &[ToolbarItemPlan; 5],
    resources: &[ToolbarItemResources; 5],
) -> ToolbarScene {
    let mut steps = Vec::new();
    for (plan, resources) in plans.iter().zip(resources) {
        if plan.active
            && let Some(sprite) = resources.active
        {
            steps.push(Step::Draw(menu_scene::sprite(
                sprite.texture,
                sprite.source,
                Rect {
                    x: plan.center_x,
                    y: 62.0,
                    w: 96.0,
                    h: 70.0,
                },
                Rgba {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 255,
                },
                sprite.flip_x,
            )));
        }
        if let Some(sprite) = resources.icon {
            steps.push(Step::Draw(menu_scene::sprite(
                sprite.texture,
                sprite.source,
                Rect {
                    x: plan.center_x,
                    y: 62.0,
                    w: 72.0,
                    h: 72.0,
                },
                Rgba {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 255,
                },
                sprite.flip_x,
            )));
        }
        steps.push(Step::Hit(Hit {
            action: HitAction::Toolbar(plan.action),
            rect: plan.hit,
        }));
        if plan.tooltip {
            steps.extend(resources.tooltip.iter().cloned().map(Step::Draw));
        }
    }
    ToolbarScene { generation, steps }
}

pub fn tooltip(
    center_x: f32,
    style: TooltipStyle,
    layout: &TextLayout,
    atlases: &BTreeMap<String, TextureId>,
    sources: &TooltipSources,
    tint: Rgba,
) -> Vec<Command> {
    let width = style.minimum_width.max(layout.advance);
    let placements = [
        (
            sources.body,
            Rect {
                x: center_x,
                y: style.body_y,
                w: width,
                h: style.height,
            },
        ),
        (
            sources.left,
            Rect {
                x: center_x - width * 0.5 - style.edge_width * 0.5,
                y: style.body_y,
                w: style.edge_width,
                h: style.height,
            },
        ),
        (
            sources.right,
            Rect {
                x: center_x + width * 0.5 + style.edge_width * 0.5,
                y: style.body_y,
                w: style.edge_width,
                h: style.height,
            },
        ),
        (
            sources.arrow,
            Rect {
                x: center_x,
                y: style.body_y - style.height * 0.5 + 9.0,
                w: 24.0,
                h: 24.0,
            },
        ),
    ];
    let mut commands = Vec::new();
    for (source, centered) in placements {
        if let Some(source) = source {
            commands.push(menu_scene::sprite(
                source.texture,
                source.source,
                centered,
                Rgba {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 255,
                },
                source.flip_x,
            ));
        }
    }
    commands.extend(menu_scene::text(
        layout,
        atlases,
        TextPosition {
            x: center_x,
            y: style.text_y,
            max_width: 1200.0,
            line_anchor: true,
            tint,
        },
    ));
    commands
}

pub fn toolbar(input: &ToolbarInput) -> [ToolbarItemPlan; 5] {
    std::array::from_fn(|index| {
        let target = format!("toolbar:{index}");
        let panel_empty = input.panel.is_empty();
        let eligible_keyboard =
            input.keyboard_focus && input.keyboard_index == index && panel_empty;
        let center_x = 640.0 + (index as f32 - 2.0) * 96.0;
        ToolbarItemPlan {
            action: ALL[index],
            center_x,
            active: input.hover == target || input.panel == target || eligible_keyboard,
            tooltip: (input.hover == target || eligible_keyboard) && panel_empty,
            hit: Rect {
                x: center_x - 46.0,
                y: 27.0,
                w: 92.0,
                h: 70.0,
            },
        }
    })
}
