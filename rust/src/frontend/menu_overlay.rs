//! Closed display routing for native App menu overlays.

use super::{
    font::TextLayout,
    menu_scene::{self, Command, Rect, Rgba, TextPosition, TextureId},
};
use std::collections::BTreeMap;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum OverlayRoute {
    None,
    Resume,
    ResumeConfirmation,
    Toolbar,
    Panel,
    Notice,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NoticeScene {
    pub generation: usize,
    pub dialog: Rect,
    pub commands: Vec<Command>,
}

pub fn notice(
    generation: usize,
    layout: &TextLayout,
    atlases: &BTreeMap<String, TextureId>,
) -> NoticeScene {
    NoticeScene {
        generation,
        dialog: Rect {
            x: 304.0,
            y: 312.0,
            w: 672.0,
            h: 96.0,
        },
        commands: menu_scene::text(
            layout,
            atlases,
            TextPosition {
                x: 640.0,
                y: 356.0,
                max_width: 610.0,
                line_anchor: true,
                tint: Rgba {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 255,
                },
            },
        ),
    }
}

pub fn route(panel: &str) -> OverlayRoute {
    match panel {
        "resume" => OverlayRoute::Resume,
        "confirm-overwrite" | "confirm-delete" => OverlayRoute::ResumeConfirmation,
        "settings" => OverlayRoute::Toolbar,
        "emulator-settings" | "help" | "message" | "profiles" => OverlayRoute::Panel,
        "notice" => OverlayRoute::Notice,
        _ if panel.starts_with("toolbar:") => OverlayRoute::Toolbar,
        _ => OverlayRoute::None,
    }
}
