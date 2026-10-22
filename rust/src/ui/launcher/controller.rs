pub use crate::frontend::launcher_controller::Action;
use crate::frontend::launcher_controller::{Input, Snapshot};
use std::time::Duration;

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

/// Owns one SDL gamepad and the subsystem; other controllers are ignored.
pub struct Controller {
    gamepad: Option<Gamepad>,
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

    /// Call for every SDL event, including while input is disabled.
    pub fn event(&mut self, event: &Event, now: Duration, enabled: bool) -> Vec<Action> {
        match *event {
            Event::ControllerDeviceAdded { .. } => self.discover(),
            Event::ControllerDeviceRemoved { which, .. } if self.owns(which) => {
                self.gamepad = None;
                self.id = None;
                self.snapshot = Snapshot::default();
                self.input = Input::default();
                self.discover();
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

    /// Call once each frame after draining events, including when input is disabled.
    pub fn tick(&mut self, now: Duration, enabled: bool) -> Vec<Action> {
        self.snapshot = self.sample();
        self.input.advance(self.snapshot, now, enabled)
    }

    /// Reseed held state on focus transitions or before opening a native dialog.
    pub fn suspend(&mut self) {
        self.snapshot = self.sample();
        self.input.seed(self.snapshot);
    }
}
