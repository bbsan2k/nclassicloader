use std::collections::HashSet;

use crate::frontend::menu_toolbar::ToolbarAction;
use crate::frontend::{menu_carousel::ArrowAction, menu_chrome::HudAction, menu_frame::HitAction};

pub(super) struct BindingContext {
    pub(super) generation: usize,
    game_keys: HashSet<String>,
}

impl BindingContext {
    pub(super) fn new(generation: usize, game_keys: impl IntoIterator<Item = String>) -> Self {
        Self {
            generation,
            game_keys: game_keys.into_iter().collect(),
        }
    }
}

pub(super) fn resolve_hit(action: &HitAction, context: &BindingContext) -> Option<String> {
    match action {
        HitAction::Game(key) if context.game_keys.contains(&key.0) => {
            Some(format!("game:{}", key.0))
        }
        HitAction::Game(_) => None,
        HitAction::Arrow(ArrowAction::Previous) => Some("previous".into()),
        HitAction::Arrow(ArrowAction::Next) => Some("next".into()),
        HitAction::Hud(HudAction::Settings) => Some("settings".into()),
        HitAction::Hud(HudAction::ResumePanel) => Some("resume-panel".into()),
        HitAction::Hud(HudAction::Sort) => Some("sort".into()),
        HitAction::Hud(HudAction::Start) => Some("start".into()),
        HitAction::Toolbar(action) => Some(
            match action {
                ToolbarAction::Display => "toolbar:0",
                ToolbarAction::Settings => "toolbar:1",
                ToolbarAction::Language => "toolbar:2",
                ToolbarAction::Copyright => "toolbar:3",
                ToolbarAction::Manual => "toolbar:4",
            }
            .into(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{BindingContext, resolve_hit};
    use crate::frontend::{
        menu_carousel::ArrowAction,
        menu_chrome::HudAction,
        menu_frame::{GameKey, HitAction},
        menu_toolbar::ToolbarAction,
    };

    #[test]
    fn resolves_current_keys_and_closed_native_action_tags() {
        let context = BindingContext::new(41, ["alpha".to_owned(), "sorted-key".to_owned()]);
        assert_eq!(context.generation, 41);
        assert_eq!(
            resolve_hit(&HitAction::Game(GameKey("alpha".into())), &context).as_deref(),
            Some("game:alpha")
        );
        assert_eq!(
            resolve_hit(&HitAction::Game(GameKey("removed".into())), &context),
            None
        );
        assert_eq!(
            resolve_hit(&HitAction::Game(GameKey("sorted-key".into())), &context).as_deref(),
            Some("game:sorted-key")
        );
        assert_eq!(
            resolve_hit(&HitAction::Arrow(ArrowAction::Previous), &context).as_deref(),
            Some("previous")
        );
        assert_eq!(
            resolve_hit(&HitAction::Arrow(ArrowAction::Next), &context).as_deref(),
            Some("next")
        );
        for (tag, native) in [
            (HudAction::Settings, "settings"),
            (HudAction::ResumePanel, "resume-panel"),
            (HudAction::Sort, "sort"),
            (HudAction::Start, "start"),
        ] {
            assert_eq!(
                resolve_hit(&HitAction::Hud(tag), &context).as_deref(),
                Some(native)
            );
        }
        for (action, native) in [
            (ToolbarAction::Display, "toolbar:0"),
            (ToolbarAction::Settings, "toolbar:1"),
            (ToolbarAction::Language, "toolbar:2"),
            (ToolbarAction::Copyright, "toolbar:3"),
            (ToolbarAction::Manual, "toolbar:4"),
        ] {
            assert_eq!(
                resolve_hit(&HitAction::Toolbar(action), &context).as_deref(),
                Some(native)
            );
        }
    }
}
