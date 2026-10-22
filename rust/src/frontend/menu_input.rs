//! Host-neutral vocabulary for contextual menu keyboard input.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    Confirm,
    Back,
    Next,
    Profiles,
    Fullscreen,
    SaveSlot,
    LoadSlot,
    SaveSuspend,
    ResumeSuspend,
    ContinueSuspend,
    ToggleLock,
    Delete,
    ToggleSound,
    Help,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Button {
    Up,
    Down,
    Left,
    Right,
    South,
    East,
    West,
    North,
    Start,
    Back,
    Other,
}

/// Translate a menu button according to the active menu context.
pub fn menu_key(button: Button, resume_panel: bool) -> Option<Key> {
    Some(match button {
        Button::Up => Key::Up,
        Button::Down => Key::Down,
        Button::Left => Key::Left,
        Button::Right => Key::Right,
        Button::South => Key::Confirm,
        Button::East => Key::Back,
        Button::West => Key::SaveSuspend,
        Button::North => Key::ToggleLock,
        Button::Start if resume_panel => Key::ResumeSuspend,
        Button::Start => Key::Confirm,
        Button::Back => Key::Delete,
        Button::Other => return None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyEvent {
    pub key: Key,
    pub repeat: bool,
}

/// Translate shared menu keys into the suspend panel's existing policy input.
pub fn suspend_input(
    event: KeyEvent,
    pending: bool,
    controls: bool,
    nes: bool,
) -> super::suspend_navigation::Input {
    use super::suspend_navigation::Input;
    match event.key {
        Key::Left => Input::Left,
        Key::Right => Input::Right,
        Key::Up => Input::Up,
        Key::Down => Input::Down,
        Key::Next => Input::Tab,
        Key::Back => Input::Escape,
        Key::Confirm => Input::Confirm {
            repeat: event.repeat,
            pending: pending && !event.repeat && !controls,
            nes,
        },
        Key::SaveSuspend if !event.repeat => Input::Save,
        Key::ResumeSuspend if !event.repeat => Input::Resume,
        Key::ContinueSuspend if !event.repeat => Input::Continue,
        Key::ToggleLock if !event.repeat => Input::Lock,
        Key::Delete if !event.repeat => Input::Delete,
        _ => Input::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::{Button, Key, KeyEvent, menu_key, suspend_input};
    use crate::frontend::suspend_navigation::Input;

    #[test]
    fn every_menu_button_maps_in_both_contexts() {
        let cases = [
            (Button::Up, Key::Up),
            (Button::Down, Key::Down),
            (Button::Left, Key::Left),
            (Button::Right, Key::Right),
            (Button::South, Key::Confirm),
            (Button::East, Key::Back),
            (Button::West, Key::SaveSuspend),
            (Button::North, Key::ToggleLock),
            (Button::Back, Key::Delete),
        ];
        for (button, expected) in cases {
            assert_eq!(menu_key(button, false), Some(expected), "{button:?}");
            assert_eq!(menu_key(button, true), Some(expected), "{button:?}");
        }
        assert_eq!(menu_key(Button::Start, false), Some(Key::Confirm));
        assert_eq!(menu_key(Button::Start, true), Some(Key::ResumeSuspend));
    }

    #[test]
    fn unknown_menu_button_has_no_action() {
        assert_eq!(menu_key(Button::Other, false), None);
        assert_eq!(menu_key(Button::Other, true), None);
    }

    #[test]
    fn suspend_input_maps_navigation_and_repeat_gated_commands() {
        let cases = [
            (Key::Left, Input::Left),
            (Key::Right, Input::Right),
            (Key::Up, Input::Up),
            (Key::Down, Input::Down),
            (Key::Next, Input::Tab),
            (Key::Back, Input::Escape),
            (Key::SaveSuspend, Input::Save),
            (Key::ResumeSuspend, Input::Resume),
            (Key::ContinueSuspend, Input::Continue),
            (Key::ToggleLock, Input::Lock),
            (Key::Delete, Input::Delete),
            (Key::Other, Input::Other),
            (Key::Profiles, Input::Other),
        ];
        for (key, expected) in cases {
            for repeat in [false, true] {
                let input = suspend_input(KeyEvent { key, repeat }, true, false, true);
                let expected = if repeat
                    && matches!(
                        key,
                        Key::SaveSuspend
                            | Key::ResumeSuspend
                            | Key::ContinueSuspend
                            | Key::ToggleLock
                            | Key::Delete
                    ) {
                    Input::Other
                } else {
                    expected
                };
                assert_eq!(input, expected, "{key:?}, repeat={repeat}");
            }
        }
    }

    #[test]
    fn suspend_confirm_preserves_repeat_pending_controls_and_system_context() {
        for repeat in [false, true] {
            for pending in [false, true] {
                for controls in [false, true] {
                    for nes in [false, true] {
                        assert_eq!(
                            suspend_input(
                                KeyEvent {
                                    key: Key::Confirm,
                                    repeat,
                                },
                                pending,
                                controls,
                                nes,
                            ),
                            Input::Confirm {
                                repeat,
                                pending: pending && !repeat && !controls,
                                nes,
                            }
                        );
                    }
                }
            }
        }
    }
}
