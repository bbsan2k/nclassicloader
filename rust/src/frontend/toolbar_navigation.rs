//! Host-neutral navigation and activation intents for toolbar panels.

use super::{
    language_navigation::{LanguageInput, next_index},
    menu_input::{Key, KeyEvent},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct State {
    pub focus: usize,
    pub scroll: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Context {
    pub page: usize,
    pub nes: bool,
    pub display_mode: usize,
    pub language_count: usize,
    pub legal_line_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    DismissPanel,
    Help,
    ActivateClose,
    FrameNone,
    DisplayMode(usize),
    EmulatorSettings,
    ChooseLanguage(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    pub state: State,
    pub effect: Option<Effect>,
}

/// Apply the legacy toolbar key policy using only metadata supplied by the host.
pub fn transition(mut state: State, context: Context, event: KeyEvent) -> Plan {
    let effect = match event.key {
        Key::Back => Some(Effect::DismissPanel),
        Key::Help if context.page == 4 => Some(Effect::Help),
        Key::Confirm if !event.repeat => Some(match context.page {
            0 if state.focus == 3 => Effect::FrameNone,
            0 => Effect::DisplayMode(state.focus.min(2)),
            1 => Effect::EmulatorSettings,
            2 => Effect::ChooseLanguage(state.focus),
            _ => Effect::ActivateClose,
        }),
        Key::Left | Key::Right | Key::Up | Key::Down | Key::Next => {
            let backwards = matches!(event.key, Key::Left | Key::Up);
            match context.page {
                0 => {
                    if event.key == Key::Down && !context.nes {
                        state.focus = 3;
                    } else if event.key == Key::Up && !context.nes {
                        state.focus = context.display_mode;
                    } else if state.focus < 3
                        && matches!(event.key, Key::Left | Key::Right | Key::Next)
                    {
                        state.focus = (state.focus + if backwards { 2 } else { 1 }) % 3;
                    }
                }
                2 => {
                    let input = match event.key {
                        Key::Up => LanguageInput::Up,
                        Key::Down => LanguageInput::Down,
                        Key::Left => LanguageInput::Left,
                        Key::Right => LanguageInput::Right,
                        Key::Next => LanguageInput::Next,
                        _ => LanguageInput::Stay,
                    };
                    state.focus = next_index(state.focus, context.language_count, input);
                }
                3 => {
                    if matches!(event.key, Key::Left | Key::Right | Key::Next) {
                        state.focus = 1 - state.focus.min(1);
                        state.scroll = 0;
                    } else if event.key == Key::Up {
                        state.scroll = state.scroll.saturating_sub(1);
                    } else {
                        state.scroll = state
                            .scroll
                            .saturating_add(1)
                            .min(context.legal_line_count.saturating_sub(16));
                    }
                }
                _ => {}
            }
            None
        }
        _ => None,
    };
    Plan { state, effect }
}

#[cfg(test)]
mod tests {
    use super::{Context, Effect, State, transition};
    use crate::frontend::menu_input::{Key, KeyEvent};

    fn event(key: Key, repeat: bool) -> KeyEvent {
        KeyEvent { key, repeat }
    }

    fn context(page: usize) -> Context {
        Context {
            page,
            nes: false,
            display_mode: 1,
            language_count: 9,
            legal_line_count: 24,
        }
    }

    fn state(focus: usize, scroll: usize) -> State {
        State { focus, scroll }
    }

    #[test]
    fn confirmation_effects_preserve_all_five_toolbar_pages_and_repeats() {
        assert_eq!(
            transition(state(3, 0), context(0), event(Key::Confirm, false)).effect,
            Some(Effect::FrameNone)
        );
        assert_eq!(
            transition(state(usize::MAX, 0), context(0), event(Key::Confirm, false)).effect,
            Some(Effect::DisplayMode(2))
        );
        assert_eq!(
            transition(state(0, 0), context(1), event(Key::Confirm, false)).effect,
            Some(Effect::EmulatorSettings)
        );
        assert_eq!(
            transition(state(8, 0), context(2), event(Key::Confirm, false)).effect,
            Some(Effect::ChooseLanguage(8))
        );
        for page in [3, 4, 5] {
            assert_eq!(
                transition(state(0, 0), context(page), event(Key::Confirm, false)).effect,
                Some(Effect::ActivateClose)
            );
        }
        for page in 0..=4 {
            assert_eq!(
                transition(state(0, 0), context(page), event(Key::Confirm, true)).effect,
                None
            );
        }
    }

    #[test]
    fn back_is_direct_dismiss_and_help_is_repeat_independent() {
        assert_eq!(
            transition(state(0, 0), context(4), event(Key::Back, false)).effect,
            Some(Effect::DismissPanel)
        );
        assert_eq!(
            transition(state(0, 0), context(4), event(Key::Confirm, false)).effect,
            Some(Effect::ActivateClose)
        );
        assert_eq!(
            transition(state(0, 0), context(4), event(Key::Help, true)).effect,
            Some(Effect::Help)
        );
        assert_eq!(
            transition(state(0, 0), context(3), event(Key::Help, false)).effect,
            None
        );
    }

    #[test]
    fn display_page_preserves_nes_snes_navigation_and_clamps_invalid_focus() {
        let mut snes = context(0);
        assert_eq!(
            transition(state(2, 0), snes, event(Key::Down, false))
                .state
                .focus,
            3
        );
        assert_eq!(
            transition(state(3, 0), snes, event(Key::Up, false))
                .state
                .focus,
            1
        );
        assert_eq!(
            transition(state(0, 0), snes, event(Key::Left, false))
                .state
                .focus,
            2
        );
        snes.nes = true;
        assert_eq!(
            transition(state(1, 0), snes, event(Key::Down, false))
                .state
                .focus,
            1
        );
        assert_eq!(
            transition(state(1, 0), snes, event(Key::Up, false))
                .state
                .focus,
            1
        );
        assert_eq!(
            transition(state(usize::MAX, 0), snes, event(Key::Next, false))
                .state
                .focus,
            usize::MAX
        );
    }

    #[test]
    fn language_grid_reuses_eight_and_nine_entry_navigation_and_host_lookup_intent() {
        let mut eight = context(2);
        eight.language_count = 8;
        assert_eq!(
            transition(state(0, 0), eight, event(Key::Down, false))
                .state
                .focus,
            1
        );
        assert_eq!(
            transition(state(3, 0), eight, event(Key::Down, false))
                .state
                .focus,
            3
        );
        let nine = context(2);
        assert_eq!(
            transition(state(8, 0), nine, event(Key::Right, false))
                .state
                .focus,
            7
        );
        assert_eq!(
            transition(state(8, 0), nine, event(Key::Up, false))
                .state
                .focus,
            3
        );
        assert_eq!(
            transition(state(4, 0), nine, event(Key::Up, false))
                .state
                .focus,
            4
        );
        assert_eq!(
            transition(state(8, 0), nine, event(Key::Next, false))
                .state
                .focus,
            0
        );
        assert_eq!(
            transition(state(0, 0), nine, event(Key::Next, false))
                .state
                .focus,
            1
        );
        // Resolution against the host's available language list, including fallback,
        // remains an activation-adapter responsibility.
        assert_eq!(
            transition(state(usize::MAX, 0), nine, event(Key::Confirm, false)).effect,
            Some(Effect::ChooseLanguage(usize::MAX))
        );
    }

    #[test]
    fn legal_page_focus_scroll_and_bounds_are_overflow_safe() {
        let p = transition(state(0, usize::MAX), context(3), event(Key::Down, false));
        assert_eq!(p.state.scroll, 8);
        let p = transition(state(0, 8), context(3), event(Key::Down, false));
        assert_eq!(p.state.scroll, 8);
        let p = transition(state(0, 1), context(3), event(Key::Up, false));
        assert_eq!(p.state.scroll, 0);
        let p = transition(state(0, 5), context(3), event(Key::Right, false));
        assert_eq!(p.state.focus, 1);
        assert_eq!(p.state.scroll, 0);
        let mut short = context(3);
        short.legal_line_count = 4;
        assert_eq!(
            transition(state(0, usize::MAX), short, event(Key::Down, false))
                .state
                .scroll,
            0
        );
    }
}
