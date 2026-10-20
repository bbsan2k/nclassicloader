//! Startup controller input. The state machine can be tested without SDL or hardware:
//! `rustc --edition=2024 --test rust/src/ui/launcher/controller.rs -o /path/to/test`.
//! See docs/controller-support.md for event-loop integration and mapping semantics.

use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    LibraryPrevious,
    LibraryNext,
    FocusPrevious,
    FocusNext,
    FilterPrevious,
    FilterNext,
    Confirm,
    Cancel,
    Import,
    OpenFolder,
}

impl Action {
    /// Apply navigation only. `count` is the number of *visible* library entries.
    /// Focus cycles library -> launch -> import -> folder -> library.
    pub fn navigate(self, selected: &mut usize, count: usize, focus: &mut Option<usize>) {
        match self {
            Self::LibraryPrevious | Self::LibraryNext => {
                *selected = if self == Self::LibraryPrevious {
                    selected.saturating_sub(1)
                } else {
                    selected.saturating_add(1)
                }
                .min(count.saturating_sub(1));
                *focus = None;
            }
            Self::FocusPrevious | Self::FocusNext => {
                let slot = focus.map_or(0, |index| index.min(2) + 1);
                let next = (slot + if self == Self::FocusNext { 1 } else { 3 }) % 4;
                *focus = next.checked_sub(1);
            }
            _ => {}
        }
    }

    /// Existing launcher action indices: 0 launch, 1 import, 2 open folder.
    pub fn activation(self, focus: Option<usize>) -> Option<i32> {
        match self {
            Self::Confirm => Some(focus.unwrap_or(0).min(2) as i32),
            Self::Import => Some(1),
            Self::OpenFolder => Some(2),
            _ => None,
        }
    }
}

// Keep this model independent of SDL so regressions need no native library.
#[derive(Clone, Copy, Debug)]
enum Button {
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
    L,
    R,
}
#[cfg(test)]
const BUTTONS: [Button; 12] = [
    Button::Up,
    Button::Down,
    Button::Left,
    Button::Right,
    Button::South,
    Button::East,
    Button::West,
    Button::North,
    Button::Start,
    Button::Back,
    Button::L,
    Button::R,
];

#[derive(Clone, Copy, Default)]
struct Snapshot {
    buttons: [bool; 12],
    x: i16,
    y: i16,
}

impl Snapshot {
    fn held(self, button: Button) -> bool {
        self.buttons[button as usize]
    }
}

const ENGAGE: i16 = 16_000;
const RELEASE: i16 = 9_000;
const REPEAT_DELAY: Duration = Duration::from_millis(350);
const REPEAT_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Default)]
struct Axis {
    direction: i8,
}

impl Axis {
    fn update(&mut self, value: i16) -> i8 {
        self.direction = if value >= ENGAGE {
            1
        } else if value <= -ENGAGE {
            -1
        } else if (self.direction == 1 && value > RELEASE)
            || (self.direction == -1 && value < -RELEASE)
        {
            self.direction
        } else {
            0
        };
        self.direction
    }
}

#[derive(Default)]
struct Repeat {
    direction: i8,
    next: Duration,
}

impl Repeat {
    fn step(&mut self, direction: i8, now: Duration) -> Option<i8> {
        if direction == 0 {
            self.direction = 0;
            return None;
        }
        if direction != self.direction {
            self.direction = direction;
            self.next = now.saturating_add(REPEAT_DELAY);
            return Some(direction);
        }
        if now >= self.next {
            // Never catch up after stalls: at most one repeat per axis per call.
            self.next = now.saturating_add(REPEAT_INTERVAL);
            return Some(direction);
        }
        None
    }
}

#[derive(Default)]
struct Input {
    previous: Snapshot,
    x: Axis,
    y: Axis,
    horizontal: Repeat,
    vertical: Repeat,
    blocked_navigation: bool,
}

impl Input {
    fn seed(&mut self, snapshot: Snapshot) {
        *self = Self::default();
        // Held controls on connection/focus regain cannot activate anything.
        self.advance(snapshot, Duration::ZERO, false);
    }

    fn advance(&mut self, snapshot: Snapshot, now: Duration, enabled: bool) -> Vec<Action> {
        let x = self.x.update(snapshot.x);
        let y = self.y.update(snapshot.y);
        let direction = |negative, positive, stick| {
            if negative || positive {
                i8::from(positive) - i8::from(negative)
            } else {
                stick
            }
        };
        let horizontal = direction(snapshot.held(Button::Left), snapshot.held(Button::Right), x);
        let vertical = direction(snapshot.held(Button::Up), snapshot.held(Button::Down), y);
        let mut actions = Vec::new();
        if !enabled {
            self.blocked_navigation = horizontal != 0 || vertical != 0;
            self.horizontal = Repeat::default();
            self.vertical = Repeat::default();
        } else {
            if horizontal == 0 && vertical == 0 {
                self.blocked_navigation = false;
            }
            if !self.blocked_navigation {
                if let Some(step) = self.vertical.step(vertical, now) {
                    actions.push(if step < 0 {
                        Action::LibraryPrevious
                    } else {
                        Action::LibraryNext
                    });
                }
                if let Some(step) = self.horizontal.step(horizontal, now) {
                    actions.push(if step < 0 {
                        Action::FocusPrevious
                    } else {
                        Action::FocusNext
                    });
                }
            }
            for (buttons, action) in [
                (&[Button::South, Button::Start][..], Action::Confirm),
                (&[Button::East, Button::Back][..], Action::Cancel),
                (&[Button::West][..], Action::Import),
                (&[Button::North][..], Action::OpenFolder),
                (&[Button::L][..], Action::FilterPrevious),
                (&[Button::R][..], Action::FilterNext),
            ] {
                // Aliases form one logical button; Start while holding South
                // cannot issue a second confirm.
                if buttons.iter().any(|b| snapshot.held(*b))
                    && !buttons.iter().any(|b| self.previous.held(*b))
                {
                    actions.push(action);
                }
            }
        }
        self.previous = snapshot;
        actions
    }
}

#[cfg(feature = "native-app")]
mod sdl {
    use super::*;
    use sdl3::{
        GamepadSubsystem,
        event::Event,
        gamepad::{Axis as PadAxis, Button as PadButton, Gamepad},
    };

    const PAD_BUTTONS: [PadButton; 12] = [
        PadButton::DPadUp,
        PadButton::DPadDown,
        PadButton::DPadLeft,
        PadButton::DPadRight,
        PadButton::South,
        PadButton::East,
        PadButton::West,
        PadButton::North,
        PadButton::Start,
        PadButton::Back,
        PadButton::LeftShoulder,
        PadButton::RightShoulder,
    ];

    /// Owns one controller and the subsystem; other controllers are ignored.
    pub struct Controller {
        // Drop the gamepad before its subsystem.
        gamepad: Option<Gamepad>,
        // Cache while connected: querying pad.id() after removal can fail.
        id: Option<u32>,
        subsystem: GamepadSubsystem,
        input: Input,
        snapshot: Snapshot,
    }

    impl Controller {
        pub fn new(subsystem: GamepadSubsystem) -> Self {
            let mut controller = Self {
                gamepad: None,
                id: None,
                subsystem,
                input: Input::default(),
                snapshot: Snapshot::default(),
            };
            controller.discover();
            controller
        }

        pub fn connected(&self) -> bool {
            self.gamepad.is_some()
        }

        fn sample(&self) -> Snapshot {
            self.gamepad
                .as_ref()
                .map_or_else(Snapshot::default, |pad| Snapshot {
                    buttons: PAD_BUTTONS.map(|button| pad.button(button)),
                    x: pad.axis(PadAxis::LeftX),
                    y: pad.axis(PadAxis::LeftY),
                })
        }

        fn discover(&mut self) {
            if self.gamepad.is_none() {
                self.gamepad = self.subsystem.gamepads().ok().and_then(|ids| {
                    ids.into_iter().find_map(|id| {
                        let pad = self.subsystem.open(id).ok()?;
                        if !pad.connected() {
                            return None;
                        }
                        self.id = Some(id.into());
                        Some(pad)
                    })
                });
                self.snapshot = self.sample();
                self.input.seed(self.snapshot);
            }
        }

        fn owns(&self, which: u32) -> bool {
            self.id == Some(which)
        }

        /// Call for EVERY SDL event, including while busy/unfocused (enabled=false).
        /// Processing edges here preserves even a down/up pair within one frame.
        pub fn event(&mut self, event: &Event, now: Duration, enabled: bool) -> Vec<Action> {
            match *event {
                Event::ControllerDeviceAdded { .. } => self.discover(),
                Event::ControllerDeviceRemoved { which, .. } if self.owns(which) => {
                    self.gamepad = None;
                    self.id = None;
                    self.snapshot = Snapshot::default();
                    self.input = Input::default();
                    self.discover(); // Fall back to another already-connected controller.
                }
                Event::ControllerButtonDown { which, button, .. }
                | Event::ControllerButtonUp { which, button, .. }
                    if self.owns(which) =>
                {
                    if let Some(index) = PAD_BUTTONS.iter().position(|b| *b == button) {
                        self.snapshot.buttons[index] =
                            matches!(event, Event::ControllerButtonDown { .. });
                        return self.input.advance(self.snapshot, now, enabled);
                    }
                }
                Event::ControllerAxisMotion {
                    which, axis, value, ..
                } if self.owns(which) => {
                    match axis {
                        PadAxis::LeftX => self.snapshot.x = value,
                        PadAxis::LeftY => self.snapshot.y = value,
                        _ => return Vec::new(),
                    }
                    return self.input.advance(self.snapshot, now, enabled);
                }
                _ => {}
            }
            Vec::new()
        }

        /// Call once each frame AFTER draining SDL events. Supplies bounded repeat
        /// and reconciles missed releases. Never stop pumping while importing.
        pub fn tick(&mut self, now: Duration, enabled: bool) -> Vec<Action> {
            self.snapshot = self.sample();
            self.input.advance(self.snapshot, now, enabled)
        }

        /// Call on focus transitions or immediately before opening a native dialog.
        /// Held buttons must be released before they can activate again.
        pub fn suspend(&mut self) {
            self.snapshot = self.sample();
            self.input.seed(self.snapshot);
        }
    }
}

#[cfg(feature = "native-app")]
pub use sdl::Controller;

#[cfg(test)]
mod tests {
    use super::*;
    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }
    fn pressed(button: Button) -> Snapshot {
        let mut s = Snapshot::default();
        s.buttons[button as usize] = true;
        s
    }

    #[test]
    fn labels_and_shortcuts() {
        for (b, a) in [
            (Button::South, Action::Confirm),
            (Button::Start, Action::Confirm),
            (Button::East, Action::Cancel),
            (Button::Back, Action::Cancel),
            (Button::West, Action::Import),
            (Button::North, Action::OpenFolder),
            (Button::L, Action::FilterPrevious),
            (Button::R, Action::FilterNext),
        ] {
            let mut input = Input::default();
            assert_eq!(input.advance(pressed(b), ms(0), true), vec![a]);
            assert!(input.advance(pressed(b), ms(10_000), true).is_empty());
            input.advance(Snapshot::default(), ms(10_001), true);
            assert_eq!(input.advance(pressed(b), ms(10_002), true), vec![a]);
        }
    }

    #[test]
    fn repeat_is_delayed_bounded_and_resets_on_release_or_reversal() {
        let mut input = Input::default();
        assert_eq!(
            input.advance(pressed(Button::Down), ms(0), true),
            vec![Action::LibraryNext]
        );
        assert!(
            input
                .advance(pressed(Button::Down), ms(349), true)
                .is_empty()
        );
        assert_eq!(
            input.advance(pressed(Button::Down), ms(350), true),
            vec![Action::LibraryNext]
        );
        assert_eq!(
            input.advance(pressed(Button::Down), ms(90_000), true),
            vec![Action::LibraryNext]
        );
        assert!(
            input
                .advance(pressed(Button::Down), ms(90_000), true)
                .is_empty()
        );
        assert_eq!(
            input.advance(pressed(Button::Up), ms(90_001), true),
            vec![Action::LibraryPrevious]
        );
        input.advance(Snapshot::default(), ms(90_002), true);
        assert_eq!(
            input.advance(pressed(Button::Up), ms(90_003), true),
            vec![Action::LibraryPrevious]
        );
    }

    #[test]
    fn stick_hysteresis_and_extremes() {
        let mut axis = Axis::default();
        for (value, expected) in [
            (15_999, 0),
            (16_000, 1),
            (10_000, 1),
            (9_000, 0),
            (-16_000, -1),
            (-9_001, -1),
            (-9_000, 0),
            (i16::MIN, -1),
            (i16::MAX, 1),
            (0, 0),
        ] {
            assert_eq!(axis.update(value), expected);
        }
        let mut input = Input::default();
        let s = Snapshot {
            y: -20_000,
            ..Snapshot::default()
        };
        assert_eq!(input.advance(s, ms(0), true), vec![Action::LibraryPrevious]);
        assert!(
            input
                .advance(Snapshot { y: -12_000, ..s }, ms(100), true)
                .is_empty()
        );
    }

    #[test]
    fn connect_busy_and_suspend_require_release() {
        let mut input = Input::default();
        for b in BUTTONS {
            let s = pressed(b);
            input.seed(s);
            assert!(input.advance(s, ms(1_000), true).is_empty());
            input.advance(Snapshot::default(), ms(1_001), true);
            assert!(!input.advance(s, ms(1_002), true).is_empty());
            assert!(input.advance(s, ms(1_003), false).is_empty());
            assert!(input.advance(s, ms(2_000), true).is_empty());
        }
        // Replacing/disconnecting a pad clears its held state. A neutral new
        // connection does not swallow the first deliberate navigation press.
        input.seed(Snapshot::default());
        assert_eq!(
            input.advance(pressed(Button::Down), ms(3_000), true),
            vec![Action::LibraryNext]
        );
        input.seed(Snapshot::default());
        assert_eq!(
            input.advance(pressed(Button::South), ms(3_001), true),
            vec![Action::Confirm]
        );
    }

    #[test]
    fn aliases_do_not_double_confirm_and_duplicate_down_is_ignored() {
        let mut input = Input::default();
        let mut s = pressed(Button::South);
        assert_eq!(input.advance(s, ms(0), true), vec![Action::Confirm]);
        s.buttons[Button::Start as usize] = true;
        assert!(input.advance(s, ms(1), true).is_empty());
        s.buttons[Button::South as usize] = false;
        assert!(input.advance(s, ms(2), true).is_empty());
        assert!(input.advance(s, ms(3), true).is_empty());
    }

    #[test]
    fn dpad_wins_over_stick_and_opposites_cancel() {
        let mut input = Input::default();
        let mut s = pressed(Button::Down);
        s.y = i16::MIN;
        assert_eq!(input.advance(s, ms(0), true), vec![Action::LibraryNext]);
        s.buttons[Button::Up as usize] = true;
        assert!(input.advance(s, ms(1_000), true).is_empty());
    }

    #[test]
    fn focus_and_empty_list_are_safe() {
        let (mut selected, mut focus) = (0, None);
        Action::LibraryNext.navigate(&mut selected, 0, &mut focus);
        assert_eq!(selected, 0);
        for expected in [Some(0), Some(1), Some(2), None] {
            Action::FocusNext.navigate(&mut selected, 0, &mut focus);
            assert_eq!(focus, expected);
        }
        Action::FocusPrevious.navigate(&mut selected, 0, &mut focus);
        assert_eq!(focus, Some(2));
        assert_eq!(Action::Confirm.activation(focus), Some(2));
        assert_eq!(Action::Confirm.activation(None), Some(0));
        assert_eq!(Action::Import.activation(None), Some(1));
        assert_eq!(Action::OpenFolder.activation(None), Some(2));
        Action::LibraryPrevious.navigate(&mut selected, 4, &mut focus);
        assert_eq!(focus, None);
    }
}
