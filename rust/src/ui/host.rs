use super::*;

pub(super) const NATIVE_EVENT_BUDGET: usize = 128;
pub(super) const DIALOG_MESSAGE_BUDGET: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeUpdateStep {
    GamepadNavigation,
    ShutdownRequests,
    DialogCompletions,
    WindowTitle,
    Fullscreen,
    GameAudio,
}

const OPEN_UPDATE_STEPS: [NativeUpdateStep; 5] = [
    NativeUpdateStep::GamepadNavigation,
    NativeUpdateStep::DialogCompletions,
    NativeUpdateStep::WindowTitle,
    NativeUpdateStep::Fullscreen,
    NativeUpdateStep::GameAudio,
];
const CLOSING_UPDATE_STEPS: [NativeUpdateStep; 5] = [
    NativeUpdateStep::ShutdownRequests,
    NativeUpdateStep::DialogCompletions,
    NativeUpdateStep::WindowTitle,
    NativeUpdateStep::Fullscreen,
    NativeUpdateStep::GameAudio,
];

fn native_update_steps(closing: bool) -> &'static [NativeUpdateStep] {
    if closing {
        &CLOSING_UPDATE_STEPS
    } else {
        &OPEN_UPDATE_STEPS
    }
}

pub(super) fn take_dialog_batch<I: Iterator>(messages: I) -> Vec<I::Item> {
    messages.take(DIALOG_MESSAGE_BUDGET).collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClosingEvent {
    KeyRelease,
    PadRelease,
    FocusLost,
    Ignore,
}

fn closing_event(event: &Event) -> ClosingEvent {
    match event {
        Event::KeyUp { .. } => ClosingEvent::KeyRelease,
        Event::ControllerButtonUp { .. } => ClosingEvent::PadRelease,
        Event::Window {
            win_event: sdl3::event::WindowEvent::FocusLost,
            ..
        } => ClosingEvent::FocusLost,
        _ => ClosingEvent::Ignore,
    }
}

impl App {
    pub(super) fn finish_native_run(&mut self) -> Result<(), String> {
        if self.playing {
            self.playing = false;
            self.capture_pending()?;
        }
        self.save_settings()?;
        if let Some(core) = self.core.as_mut()
            && core.is_loaded()
        {
            core.unload()
                .map_err(|error| format!("Cannot flush game save memory: {error}"))?;
        }
        Ok(())
    }

    pub(super) fn update_native_frame(
        &mut self,
        canvas: &mut Canvas<Window>,
        closing: bool,
    ) -> Result<(), String> {
        for step in native_update_steps(closing) {
            match step {
                NativeUpdateStep::GamepadNavigation => self.poll_gamepad_navigation(canvas),
                NativeUpdateStep::ShutdownRequests => {
                    self.image_import.shutdown();
                    self.picker_requests.close();
                }
                NativeUpdateStep::DialogCompletions => self.process_dialogs(canvas, closing),
                NativeUpdateStep::WindowTitle => {
                    if self.title_dirty {
                        self.title_dirty = false;
                        if let Err(error) = canvas.window_mut().set_title(&self.window_title) {
                            self.panel = "message".into();
                            self.message = format!("Cannot update window title: {error}");
                        }
                    }
                }
                NativeUpdateStep::Fullscreen => {
                    if self.fullscreen_dirty {
                        self.fullscreen_dirty = false;
                        if let Err(error) = canvas.window_mut().set_fullscreen(self.fullscreen) {
                            self.panel = "message".into();
                            self.message = format!("Cannot change full-screen mode: {error}");
                        }
                    }
                }
                NativeUpdateStep::GameAudio => {
                    if self.playing && self.panel.is_empty() {
                        self.pump_game_audio()?;
                    } else if self.sound
                        && let Some(music) = &self.music
                    {
                        music.play_loop();
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn handle_host_event(
        &mut self,
        event: Event,
        canvas: &mut Canvas<Window>,
        running: &mut bool,
    ) {
        if !*running && closing_event(&event) == ClosingEvent::Ignore {
            return;
        }

        match event {
            Event::Quit { .. } => *running = false,
            Event::Window {
                win_event: sdl3::event::WindowEvent::FocusLost,
                ..
            } => {
                self.held.clear();
                self.pad_held.clear();
                self.stick.clear();
                self.pad_repeat.clear();
                self.pad_blocked = self.gamepad.is_some();
            }
            #[cfg(target_os = "macos")]
            Event::User { type_, code, .. } if type_ == unsafe { clover_native_menu_event() } => {
                if self.playing && code != 8 {
                    self.leave_game();
                }
                match code {
                    1 => self.activate("profiles", canvas),
                    2 => self.choose_image(canvas),
                    3 => self.activate("toolbar:1", canvas),
                    4 => self.choose_rom(canvas, false),
                    5 => self.choose_firmware(canvas),
                    6 => self.activate("sort", canvas),
                    7 => self.activate("sound", canvas),
                    8 => self.activate("fullscreen", canvas),
                    9 => self.activate("toolbar:4", canvas),
                    10 => *running = false,
                    _ => {}
                }
            }
            Event::KeyDown {
                scancode: Some(code),
                repeat,
                ..
            } => {
                self.held.insert(code);
                self.handle_key(code, repeat, canvas);
            }
            Event::KeyUp {
                scancode: Some(code),
                ..
            } => {
                self.held.remove(&code);
            }
            Event::MouseMotion { x, y, .. } if !self.playing => {
                self.menu_focus = MenuFocus::Games;
                let (x, y) = to_stage(canvas, x, y);
                self.hover.clear();
                for hit in self.hits.iter().rev() {
                    if point_inside(x, y, hit.rect) {
                        self.hover = hit.action.clone();
                        break;
                    }
                }
            }
            Event::MouseWheel { y, .. } if !self.playing && self.panel == "toolbar:3" => {
                for _ in 0..(y.abs().ceil() as usize).min(8) {
                    self.scroll_legal(y < 0.0);
                }
            }
            Event::MouseButtonDown {
                mouse_btn: sdl3::mouse::MouseButton::Left,
                clicks,
                x,
                y,
                ..
            } if !self.playing => {
                let (x, y) = to_stage(canvas, x, y);
                let action = self
                    .hits
                    .iter()
                    .rev()
                    .find(|hit| point_inside(x, y, hit.rect))
                    .map(|hit| hit.action.clone());
                if let Some(action) = action {
                    self.activate(&action, canvas);
                    if clicks >= 2 && action.starts_with("game:") {
                        self.begin_game(canvas);
                    }
                }
            }
            Event::ControllerDeviceAdded { which, .. } if self.gamepad.is_none() => {
                self.gamepad = self
                    .gamepad_subsystem
                    .open(sdl3::sys::joystick::SDL_JoystickID(which))
                    .ok();
                self.gamepad_id = self
                    .gamepad
                    .as_ref()
                    .and_then(|pad| pad.id().ok())
                    .map(u32::from);
                self.pad_blocked = self.gamepad.is_some();
            }
            Event::ControllerDeviceRemoved { which, .. } if self.gamepad_id == Some(which) => {
                if self.playing {
                    self.leave_game();
                }
                self.gamepad = None;
                self.pad_held.clear();
                self.stick.clear();
                self.pad_repeat.clear();
                self.gamepad = self.gamepad_subsystem.gamepads().ok().and_then(|ids| {
                    ids.into_iter()
                        .filter(|id| id.0 != which)
                        .find_map(|id| self.gamepad_subsystem.open(id).ok())
                });
                self.gamepad_id = self
                    .gamepad
                    .as_ref()
                    .and_then(|pad| pad.id().ok())
                    .map(u32::from);
                self.pad_blocked = self.gamepad.is_some();
            }
            Event::ControllerAxisMotion {
                which, axis, value, ..
            } if self.gamepad_id == Some(which) => match axis {
                sdl3::gamepad::Axis::LeftX => self.stick.update(0, value),
                sdl3::gamepad::Axis::LeftY => self.stick.update(1, value),
                _ => {}
            },
            Event::ControllerButtonDown { which, button, .. } if self.gamepad_id == Some(which) => {
                self.pad_button_down(button, canvas);
            }
            Event::ControllerButtonUp { which, button, .. } if self.gamepad_id == Some(which) => {
                self.pad_held.remove(&button);
            }
            Event::DropFile { filename, .. } => {
                let path = PathBuf::from(filename);
                if self.rom_extension_allowed(&path) {
                    let game = self.selected().id.clone();
                    self.preferences["roms"][game] = json!(path.to_string_lossy());
                    let _ = self.save_settings();
                    self.begin_game(canvas);
                } else {
                    self.panel = "message".into();
                    self.message = if self.art.nes {
                        "Drop a compatible .nes cartridge ROM."
                    } else {
                        "Drop a compatible .sfc or .smc ROM."
                    }
                    .into();
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_latch_only_allows_release_and_focus_cleanup() {
        use sdl3::{event::WindowEvent, keyboard::Mod};

        let queued_after_quit = [
            Event::Quit { timestamp: 0 },
            Event::KeyDown {
                timestamp: 0,
                window_id: 0,
                keycode: None,
                scancode: Some(Scancode::Return),
                keymod: Mod::empty(),
                repeat: false,
                which: 0,
                raw: 0,
            },
            Event::User {
                timestamp: 0,
                window_id: 0,
                type_: 1,
                code: 4,
                data1: std::ptr::null_mut(),
                data2: std::ptr::null_mut(),
            },
            Event::DropFile {
                timestamp: 0,
                window_id: 0,
                filename: "game.sfc".into(),
            },
            Event::KeyUp {
                timestamp: 0,
                window_id: 0,
                keycode: None,
                scancode: Some(Scancode::Return),
                keymod: Mod::empty(),
                repeat: false,
                which: 0,
                raw: 0,
            },
            Event::ControllerButtonUp {
                timestamp: 0,
                which: 7,
                button: sdl3::gamepad::Button::South,
            },
            Event::Window {
                timestamp: 0,
                window_id: 0,
                win_event: WindowEvent::FocusLost,
            },
        ];
        let mut running = true;
        for (index, event) in queued_after_quit.iter().enumerate() {
            let allowed = if matches!(event, Event::Quit { .. }) {
                running = false;
                true
            } else {
                running || closing_event(event) != ClosingEvent::Ignore
            };
            assert_eq!(allowed, matches!(index, 0 | 4..=6));
        }
    }

    #[test]
    fn native_budget_leaves_lazy_fifo_tail_unconsumed() {
        let mut next = 0..NATIVE_EVENT_BUDGET + 3;
        let processed: Vec<_> = (&mut next).take(NATIVE_EVENT_BUDGET).collect();
        assert_eq!(processed.len(), NATIVE_EVENT_BUDGET);
        assert_eq!(next.next(), Some(NATIVE_EVENT_BUDGET));
    }

    #[test]
    fn native_update_plan_orders_shutdown_before_notifications_and_skips_gamepad() {
        assert_eq!(
            native_update_steps(false),
            &[
                NativeUpdateStep::GamepadNavigation,
                NativeUpdateStep::DialogCompletions,
                NativeUpdateStep::WindowTitle,
                NativeUpdateStep::Fullscreen,
                NativeUpdateStep::GameAudio,
            ]
        );
        assert_eq!(
            native_update_steps(true),
            &[
                NativeUpdateStep::ShutdownRequests,
                NativeUpdateStep::DialogCompletions,
                NativeUpdateStep::WindowTitle,
                NativeUpdateStep::Fullscreen,
                NativeUpdateStep::GameAudio,
            ]
        );
    }

    #[test]
    fn dialog_budget_keeps_completion_release_and_close_fifo_tail() {
        #[derive(Clone, Debug, PartialEq, Eq)]
        enum QueuedMessage {
            Owned,
            Completion,
            Release,
            Close,
        }

        let mut queue = std::iter::repeat_n(QueuedMessage::Owned, DIALOG_MESSAGE_BUDGET)
            .chain([
                QueuedMessage::Completion,
                QueuedMessage::Release,
                QueuedMessage::Close,
            ])
            .peekable();
        let batch = take_dialog_batch(&mut queue);
        assert_eq!(batch.len(), DIALOG_MESSAGE_BUDGET);
        assert!(batch.iter().all(|message| *message == QueuedMessage::Owned));
        assert_eq!(
            queue.collect::<Vec<_>>(),
            [
                QueuedMessage::Completion,
                QueuedMessage::Release,
                QueuedMessage::Close,
            ]
        );
    }

    #[test]
    fn shutdown_closes_picker_requests_before_queued_completion_can_activate() {
        use crate::{
            frontend::requests::{Capability, Outcome},
            ui::dialog_requests::{Action, Context, Kind, Requests},
        };

        let context = Context {
            profile: "profile".into(),
            game: "game".into(),
            start: true,
        };
        let mut requests = Requests::default();
        let id = requests
            .begin(Kind::Rom, context.clone(), Capability::Available)
            .unwrap();
        let mut fake_owner_started_game = false;

        // This models the production closing plan: shutdown/close runs before
        // processing the queued picker completion notification.
        requests.close();
        if let Some(action) = requests.finish(
            id,
            Kind::Rom,
            Outcome::Selected("queued.sfc".into()),
            &context,
        ) {
            fake_owner_started_game = matches!(action, Action::Selected(_, _, _) if context.start);
        }
        assert!(!fake_owner_started_game);
    }
}
