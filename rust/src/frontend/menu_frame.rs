use super::{
    menu_carousel::{ArrowAction, ArrowScene, CardScene},
    menu_chrome::{HudAction, HudScene, ThumbnailScene},
    menu_scene::{Command, Rect},
    menu_toolbar::ToolbarAction,
};

pub fn last_hit<T>(hits: &[T], x: f32, y: f32, rect: impl Fn(&T) -> Rect) -> Option<&T> {
    hits.iter().rev().find(|hit| {
        let r = rect(hit);
        x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GameKey(pub String);

#[derive(Clone, Debug, PartialEq)]
pub enum HitAction {
    Game(GameKey),
    Arrow(ArrowAction),
    Hud(HudAction),
    Toolbar(ToolbarAction),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub action: HitAction,
    pub rect: Rect,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Draw(Command),
    Hit(Hit),
}

#[derive(Clone, Debug, PartialEq)]
pub struct BaseMenuInput {
    pub generation: usize,
    pub wallpaper: Vec<Command>,
    pub title: Vec<Command>,
    pub cards: Vec<(GameKey, CardScene)>,
    pub selection: Vec<Command>,
    pub arrows: ArrowScene,
    pub thumbnails: Vec<(GameKey, ThumbnailScene)>,
    pub hud: HudScene,
    pub bars: Vec<Command>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BaseMenuScene {
    pub generation: usize,
    pub steps: Vec<Step>,
}

/// Appends toolbar presentation only when both scenes belong to the same assets owner.
pub fn with_toolbar(
    mut base: BaseMenuScene,
    toolbar: super::menu_toolbar::ToolbarScene,
) -> Option<BaseMenuScene> {
    if base.generation != toolbar.generation {
        return None;
    }
    base.steps.extend(toolbar.steps);
    Some(base)
}

fn draws(commands: Vec<Command>, steps: &mut Vec<Step>) {
    steps.extend(commands.into_iter().map(Step::Draw));
}

pub fn compose(input: BaseMenuInput) -> BaseMenuScene {
    let mut steps = Vec::new();
    draws(input.wallpaper, &mut steps);
    draws(input.title, &mut steps);
    for (key, scene) in input.cards {
        draws(scene.commands, &mut steps);
        steps.push(Step::Hit(Hit {
            action: HitAction::Game(key),
            rect: scene.hit,
        }));
    }
    draws(input.selection, &mut steps);
    draws(input.arrows.commands, &mut steps);
    for hit in input.arrows.hits {
        steps.push(Step::Hit(Hit {
            action: HitAction::Arrow(hit.action),
            rect: hit.rect,
        }));
    }
    for (key, scene) in input.thumbnails {
        draws(scene.commands, &mut steps);
        steps.push(Step::Hit(Hit {
            action: HitAction::Game(key),
            rect: scene.hit,
        }));
    }
    for item in input.hud.items {
        let action = item.action;
        draws(item.commands, &mut steps);
        steps.push(Step::Hit(Hit {
            action: HitAction::Hud(action),
            rect: item.hit,
        }));
    }
    draws(input.bars, &mut steps);
    BaseMenuScene {
        generation: input.generation,
        steps,
    }
}
