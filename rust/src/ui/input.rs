//! Native keyboard translation into the shared contextual menu vocabulary.

use crate::frontend::menu_input::{Button, Key, KeyEvent};
use sdl3::{gamepad::Button as PadButton, keyboard::Scancode};

pub(super) fn native_button(button: PadButton) -> Button {
    match button {
        PadButton::DPadUp => Button::Up,
        PadButton::DPadDown => Button::Down,
        PadButton::DPadLeft => Button::Left,
        PadButton::DPadRight => Button::Right,
        PadButton::South => Button::South,
        PadButton::East => Button::East,
        PadButton::West => Button::West,
        PadButton::North => Button::North,
        PadButton::Start => Button::Start,
        PadButton::Back => Button::Back,
        _ => Button::Other,
    }
}

pub(super) fn native_event(code: Scancode, repeat: bool) -> KeyEvent {
    let key = match code {
        Scancode::Up => Key::Up,
        Scancode::Down => Key::Down,
        Scancode::Left => Key::Left,
        Scancode::Right => Key::Right,
        Scancode::Return => Key::Confirm,
        Scancode::Escape => Key::Back,
        Scancode::Tab => Key::Next,
        Scancode::F2 => Key::Profiles,
        Scancode::F11 => Key::Fullscreen,
        Scancode::F5 => Key::SaveSlot,
        Scancode::F8 => Key::LoadSlot,
        Scancode::S => Key::SaveSuspend,
        Scancode::R => Key::ResumeSuspend,
        Scancode::C => Key::ContinueSuspend,
        Scancode::X => Key::ToggleLock,
        Scancode::Delete => Key::Delete,
        Scancode::M => Key::ToggleSound,
        Scancode::H => Key::Help,
        _ => Key::Other,
    };
    KeyEvent { key, repeat }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_native_gamepad_buttons_and_keeps_non_menu_buttons_other() {
        let cases = [
            (PadButton::DPadUp, Button::Up),
            (PadButton::DPadDown, Button::Down),
            (PadButton::DPadLeft, Button::Left),
            (PadButton::DPadRight, Button::Right),
            (PadButton::South, Button::South),
            (PadButton::East, Button::East),
            (PadButton::West, Button::West),
            (PadButton::North, Button::North),
            (PadButton::Start, Button::Start),
            (PadButton::Back, Button::Back),
        ];
        for (button, expected) in cases {
            assert_eq!(native_button(button), expected, "{button:?}");
        }
        for button in [
            PadButton::Guide,
            PadButton::LeftShoulder,
            PadButton::RightShoulder,
            PadButton::LeftStick,
            PadButton::RightStick,
        ] {
            assert_eq!(native_button(button), Button::Other, "{button:?}");
        }
    }

    #[test]
    fn maps_each_contextual_menu_key_explicitly() {
        let cases = [
            (Scancode::Up, Key::Up),
            (Scancode::Down, Key::Down),
            (Scancode::Left, Key::Left),
            (Scancode::Right, Key::Right),
            (Scancode::Return, Key::Confirm),
            (Scancode::Escape, Key::Back),
            (Scancode::Tab, Key::Next),
            (Scancode::F2, Key::Profiles),
            (Scancode::F11, Key::Fullscreen),
            (Scancode::F5, Key::SaveSlot),
            (Scancode::F8, Key::LoadSlot),
            (Scancode::S, Key::SaveSuspend),
            (Scancode::R, Key::ResumeSuspend),
            (Scancode::C, Key::ContinueSuspend),
            (Scancode::X, Key::ToggleLock),
            (Scancode::Delete, Key::Delete),
            (Scancode::M, Key::ToggleSound),
            (Scancode::H, Key::Help),
        ];
        for (scancode, expected) in cases {
            assert_eq!(native_event(scancode, false).key, expected, "{scancode:?}");
        }
    }

    #[test]
    fn unknown_letters_modifiers_and_other_function_keys_are_other() {
        for scancode in [
            Scancode::A,
            Scancode::Q,
            Scancode::LShift,
            Scancode::LCtrl,
            Scancode::LAlt,
            Scancode::F1,
            Scancode::F3,
            Scancode::F12,
        ] {
            assert_eq!(
                native_event(scancode, false).key,
                Key::Other,
                "{scancode:?}"
            );
        }
    }

    #[test]
    fn native_event_preserves_repeat_bit() {
        assert_eq!(
            native_event(Scancode::F11, true),
            KeyEvent {
                key: Key::Fullscreen,
                repeat: true,
            }
        );
    }
}
