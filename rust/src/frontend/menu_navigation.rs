//! Host-neutral main-menu and general-panel keyboard transitions.

use super::menu_input::{Key, KeyEvent};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuFocus {
    Games,
    Toolbar,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    None,
    Resume,
    Toolbar,
    ConfirmOverwrite,
    ConfirmDelete,
    ConfirmOther,
    EmulatorSettings,
    Help,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct State {
    pub menu_focus: MenuFocus,
    pub focus: usize,
    pub toolbar_index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Context {
    pub playing: bool,
    pub owned_import_notice: bool,
    pub panel: Panel,
    pub action_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Fullscreen,
    LeaveGame,
    SaveSlot,
    LoadSlot,
    CancelImport,
    OpenProfiles,
    DelegateSuspend,
    DelegateToolbar,
    CancelConfirmationAndOpenSuspend,
    CancelSuspend,
    ConfirmDelete,
    ConfirmSave,
    SetSettingsPanel,
    SetHelpToolbarPanel,
    ClosePanel,
    ActivateFocused(usize),
    OpenToolbar(usize),
    OpenSuspend,
    MoveGame(i32),
    BeginGame,
    Sort,
    ToggleSound,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    pub state: State,
    pub clear_hover: bool,
    pub effect: Option<Effect>,
}

fn plan(state: State, clear_hover: bool, effect: Option<Effect>) -> Plan {
    Plan {
        state,
        clear_hover,
        effect,
    }
}

/// Apply the legacy menu-input priority without accessing native resources.
pub fn transition(mut state: State, context: Context, event: KeyEvent) -> Plan {
    if event.key == Key::Fullscreen {
        return plan(state, false, (!event.repeat).then_some(Effect::Fullscreen));
    }
    if context.playing {
        let effect = if event.repeat {
            None
        } else {
            match event.key {
                Key::Back => Some(Effect::LeaveGame),
                Key::SaveSlot => Some(Effect::SaveSlot),
                Key::LoadSlot => Some(Effect::LoadSlot),
                _ => None,
            }
        };
        return plan(state, false, effect);
    }
    if context.owned_import_notice && event.key == Key::Back {
        return plan(
            state,
            false,
            (!event.repeat).then_some(Effect::CancelImport),
        );
    }

    let clear_hover = true;
    if event.key == Key::Profiles && !event.repeat {
        return plan(state, clear_hover, Some(Effect::OpenProfiles));
    }
    if context.panel == Panel::Resume {
        return plan(state, clear_hover, Some(Effect::DelegateSuspend));
    }
    if context.panel == Panel::Toolbar {
        return plan(state, clear_hover, Some(Effect::DelegateToolbar));
    }
    if matches!(
        context.panel,
        Panel::ConfirmOverwrite | Panel::ConfirmDelete
    ) {
        let effect = match event.key {
            Key::Back => Some(Effect::CancelConfirmationAndOpenSuspend),
            Key::Left | Key::Right | Key::Up | Key::Down | Key::Next => {
                state.focus = (state.focus % 2 + 1) % 2;
                None
            }
            Key::Confirm if !event.repeat && state.focus == 0 => Some(Effect::CancelSuspend),
            Key::Confirm if !event.repeat && state.focus == 1 => {
                Some(if context.panel == Panel::ConfirmDelete {
                    Effect::ConfirmDelete
                } else {
                    Effect::ConfirmSave
                })
            }
            _ => None,
        };
        return plan(state, clear_hover, effect);
    }
    if context.panel != Panel::None {
        let effect = match event.key {
            Key::Back if context.panel == Panel::ConfirmOther => {
                Some(Effect::CancelConfirmationAndOpenSuspend)
            }
            Key::Back if context.panel == Panel::EmulatorSettings => Some(Effect::SetSettingsPanel),
            Key::Back if context.panel == Panel::Help => Some(Effect::SetHelpToolbarPanel),
            Key::Back => Some(Effect::ClosePanel),
            Key::Confirm if !event.repeat && state.focus < context.action_count => {
                Some(Effect::ActivateFocused(state.focus))
            }
            Key::Left | Key::Right | Key::Up | Key::Down | Key::Next => {
                let count = context.action_count.max(1);
                let focus = state.focus % count;
                state.focus = if matches!(event.key, Key::Up | Key::Left) {
                    if focus == 0 { count - 1 } else { focus - 1 }
                } else if focus == count - 1 {
                    0
                } else {
                    focus + 1
                };
                None
            }
            _ => None,
        };
        return plan(state, clear_hover, effect);
    }

    let effect = match (state.menu_focus, event.key) {
        (MenuFocus::Games, Key::Up) => {
            state.menu_focus = MenuFocus::Toolbar;
            None
        }
        (MenuFocus::Toolbar, Key::Down | Key::Back) => {
            state.menu_focus = MenuFocus::Games;
            None
        }
        (MenuFocus::Toolbar, Key::Left) => {
            let index = state.toolbar_index % 5;
            state.toolbar_index = if index == 0 { 4 } else { index - 1 };
            None
        }
        (MenuFocus::Toolbar, Key::Right) => {
            state.toolbar_index = (state.toolbar_index % 5 + 1) % 5;
            None
        }
        (MenuFocus::Toolbar, Key::Confirm) if !event.repeat && state.toolbar_index < 5 => {
            Some(Effect::OpenToolbar(state.toolbar_index))
        }
        (MenuFocus::Games, Key::Down) => Some(Effect::OpenSuspend),
        (MenuFocus::Games, Key::Left) => Some(Effect::MoveGame(-1)),
        (MenuFocus::Games, Key::Right) => Some(Effect::MoveGame(1)),
        (MenuFocus::Games, Key::Confirm) if !event.repeat => Some(Effect::BeginGame),
        (_, Key::Next) if !event.repeat => Some(Effect::Sort),
        (_, Key::ToggleSound) if !event.repeat => Some(Effect::ToggleSound),
        _ => None,
    };
    plan(state, clear_hover, effect)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(key: Key, repeat: bool) -> KeyEvent {
        KeyEvent { key, repeat }
    }

    fn state() -> State {
        State {
            menu_focus: MenuFocus::Games,
            focus: 0,
            toolbar_index: 0,
        }
    }

    fn context(panel: Panel) -> Context {
        Context {
            playing: false,
            owned_import_notice: false,
            panel,
            action_count: 3,
        }
    }

    #[test]
    fn priority_fullscreen_playing_and_import_back() {
        let mut playing = context(Panel::Resume);
        playing.playing = true;
        let p = transition(state(), playing, event(Key::Fullscreen, false));
        assert_eq!(p.effect, Some(Effect::Fullscreen));
        assert!(!p.clear_hover);
        assert_eq!(
            transition(state(), playing, event(Key::Fullscreen, true)).effect,
            None
        );
        assert_eq!(
            transition(state(), playing, event(Key::Back, true)).effect,
            None
        );
        assert_eq!(
            transition(state(), playing, event(Key::SaveSlot, false)).effect,
            Some(Effect::SaveSlot)
        );
        let mut import = context(Panel::Other);
        import.owned_import_notice = true;
        assert_eq!(
            transition(state(), import, event(Key::Back, false)).effect,
            Some(Effect::CancelImport)
        );
        let p = transition(state(), import, event(Key::Back, true));
        assert_eq!(p.effect, None);
        assert!(!p.clear_hover);
    }

    #[test]
    fn delegates_and_menu_navigation_preserve_priority() {
        assert_eq!(
            transition(state(), context(Panel::Resume), event(Key::Other, false)).effect,
            Some(Effect::DelegateSuspend)
        );
        assert_eq!(
            transition(state(), context(Panel::Toolbar), event(Key::Other, false)).effect,
            Some(Effect::DelegateToolbar)
        );
        assert_eq!(
            transition(state(), context(Panel::Resume), event(Key::Profiles, false)).effect,
            Some(Effect::OpenProfiles)
        );
        assert_eq!(
            transition(
                state(),
                context(Panel::ConfirmOther),
                event(Key::Back, false)
            )
            .effect,
            Some(Effect::CancelConfirmationAndOpenSuspend)
        );
        assert_eq!(
            transition(
                state(),
                context(Panel::EmulatorSettings),
                event(Key::Back, true)
            )
            .effect,
            Some(Effect::SetSettingsPanel)
        );
        assert_eq!(
            transition(state(), context(Panel::Help), event(Key::Back, false)).effect,
            Some(Effect::SetHelpToolbarPanel)
        );
        assert_eq!(
            transition(state(), context(Panel::None), event(Key::Down, false)).effect,
            Some(Effect::OpenSuspend)
        );
        let p = transition(state(), context(Panel::None), event(Key::Up, false));
        assert_eq!(p.state.menu_focus, MenuFocus::Toolbar);
        let mut toolbar = state();
        toolbar.menu_focus = MenuFocus::Toolbar;
        toolbar.toolbar_index = 4;
        assert_eq!(
            transition(toolbar, context(Panel::None), event(Key::Right, false))
                .state
                .toolbar_index,
            0
        );
        assert_eq!(
            transition(toolbar, context(Panel::None), event(Key::Confirm, false)).effect,
            Some(Effect::OpenToolbar(4))
        );
        toolbar.toolbar_index = usize::MAX;
        assert_eq!(
            transition(toolbar, context(Panel::None), event(Key::Confirm, false)).effect,
            None
        );
        let normalized = usize::MAX % 5;
        let expected = if normalized == 0 { 4 } else { normalized - 1 };
        assert_eq!(
            transition(toolbar, context(Panel::None), event(Key::Left, false))
                .state
                .toolbar_index,
            expected
        );
        assert_eq!(
            transition(state(), context(Panel::None), event(Key::Confirm, false)).effect,
            Some(Effect::BeginGame)
        );
    }

    #[test]
    fn confirmations_fail_closed_and_normalize_before_direction() {
        for panel in [Panel::ConfirmDelete, Panel::ConfirmOverwrite] {
            let mut malformed = state();
            malformed.focus = usize::MAX;
            assert_eq!(
                transition(malformed, context(panel), event(Key::Confirm, false)).effect,
                None
            );
            let p = transition(malformed, context(panel), event(Key::Right, false));
            assert_eq!(p.state.focus, 0);
            let mut confirm = state();
            confirm.focus = 1;
            assert_eq!(
                transition(confirm, context(panel), event(Key::Confirm, false)).effect,
                Some(if panel == Panel::ConfirmDelete {
                    Effect::ConfirmDelete
                } else {
                    Effect::ConfirmSave
                })
            );
        }
    }

    #[test]
    fn general_panel_focus_empty_actions_and_other_panel_hover() {
        let mut c = context(Panel::Other);
        c.action_count = 2;
        let mut s = state();
        s.focus = usize::MAX;
        let p = transition(s, c, event(Key::Right, false));
        assert_eq!(p.state.focus, 0);
        assert_eq!(
            transition(p.state, c, event(Key::Confirm, false)).effect,
            Some(Effect::ActivateFocused(0))
        );
        c.action_count = 0;
        assert_eq!(transition(s, c, event(Key::Confirm, false)).effect, None);
        assert_eq!(transition(s, c, event(Key::Down, false)).state.focus, 0);
        let p = transition(state(), c, event(Key::Other, false));
        assert!(p.clear_hover);
        assert_eq!(p.effect, None);
    }
}
