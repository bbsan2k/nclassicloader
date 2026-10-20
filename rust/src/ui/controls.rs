//! Shared keyboard/gamepad navigation. A loaded-but-paused core is not gameplay.
use super::*;

impl App {
    pub(super) fn handle_key(&mut self, code: Scancode, repeat: bool, canvas: &Canvas<Window>) {
        if self.playing {
            match code {
                Scancode::Escape if !repeat => self.leave_game(),
                Scancode::F5 if !repeat => self.save_slot(),
                Scancode::F8 if !repeat => self.resume_slot(canvas),
                _ => {}
            }
            return;
        }
        self.hover.clear();
        if code == Scancode::F2 && !repeat {
            self.activate("profiles", canvas);
            return;
        }
        if self.panel == "resume" {
            match code {
                Scancode::Escape | Scancode::Up if !self.suspend_controls => {
                    self.panel.clear();
                    self.menu_focus = MenuFocus::Games;
                }
                Scancode::Escape => {
                    self.panel.clear();
                    self.menu_focus = MenuFocus::Games;
                }
                Scancode::Up => self.suspend_controls = false,
                Scancode::Down | Scancode::Tab => {
                    self.suspend_controls = true;
                    self.focus = 0;
                }
                Scancode::Left | Scancode::Right => {
                    let direction = if code == Scancode::Left { -1 } else { 1 };
                    if self.suspend_controls {
                        self.focus = (self.focus as i32 + direction).rem_euclid(5) as usize;
                    } else {
                        self.slot = (self.slot as i32 + direction).rem_euclid(4) as usize;
                        let _ = self.save_settings();
                    }
                }
                Scancode::Return if !repeat => {
                    if self.suspend_controls {
                        let action =
                            ["save", "resume", "lock", "delete", "close"][self.focus.min(4)];
                        self.activate(action, canvas);
                    } else if self.has_pending() {
                        self.request_suspend_save();
                    } else {
                        self.resume_slot(canvas);
                    }
                }
                Scancode::S if !repeat => self.request_suspend_save(),
                Scancode::R if !repeat => self.resume_slot(canvas),
                Scancode::C if !repeat => self.resume_pending(canvas),
                Scancode::X if !repeat => self.toggle_suspend_lock(),
                Scancode::Delete if !repeat => self.request_suspend_delete(),
                _ => {}
            }
            return;
        }
        if self.toolbar_key(code, repeat, canvas) {
            return;
        }
        if !self.panel.is_empty() {
            if self.panel == "confirm-overwrite" || self.panel == "confirm-delete" {
                match code {
                    Scancode::Escape => {
                        self.confirm_target = None;
                        self.open_suspend();
                    }
                    Scancode::Left
                    | Scancode::Right
                    | Scancode::Up
                    | Scancode::Down
                    | Scancode::Tab => self.focus = (self.focus + 1) % 2,
                    Scancode::Return if !repeat => {
                        let action = if self.focus == 0 {
                            "cancel-suspend"
                        } else if self.panel == "confirm-delete" {
                            "confirm-delete"
                        } else {
                            "confirm-save"
                        };
                        self.activate(action, canvas);
                    }
                    _ => {}
                }
                return;
            }
            match code {
                Scancode::Escape => {
                    if self.panel.starts_with("confirm-") {
                        self.confirm_target = None;
                        self.open_suspend();
                    } else if self.panel == "emulator-settings" {
                        self.panel = "settings".into();
                        self.focus = 0;
                    } else if self.panel == "help" {
                        self.panel = "toolbar:4".into();
                        self.focus = 0;
                    } else {
                        self.panel.clear();
                    }
                }
                Scancode::Return if !repeat => {
                    if let Some(action) = self.actions.get(self.focus).cloned() {
                        self.activate(&action, canvas);
                    }
                }
                Scancode::Left
                | Scancode::Right
                | Scancode::Up
                | Scancode::Down
                | Scancode::Tab => {
                    let count = self.actions.len().max(1);
                    self.focus = if matches!(code, Scancode::Up | Scancode::Left) {
                        (self.focus + count - 1) % count
                    } else {
                        (self.focus + 1) % count
                    };
                }
                _ => {}
            }
            return;
        }
        match (self.menu_focus, code) {
            (MenuFocus::Games, Scancode::Up) => self.menu_focus = MenuFocus::Toolbar,
            (MenuFocus::Toolbar, Scancode::Down | Scancode::Escape) => {
                self.menu_focus = MenuFocus::Games
            }
            (MenuFocus::Toolbar, Scancode::Left) => {
                self.toolbar_index = (self.toolbar_index + 4) % 5
            }
            (MenuFocus::Toolbar, Scancode::Right) => {
                self.toolbar_index = (self.toolbar_index + 1) % 5
            }
            (MenuFocus::Toolbar, Scancode::Return) if !repeat => {
                self.activate(&format!("toolbar:{}", self.toolbar_index), canvas)
            }
            (MenuFocus::Games, Scancode::Down) => self.open_suspend(),
            (MenuFocus::Games, Scancode::Left) => self.move_selection(-1),
            (MenuFocus::Games, Scancode::Right) => self.move_selection(1),
            (MenuFocus::Games, Scancode::Return) if !repeat => {
                self.begin_game(canvas);
            }
            (_, Scancode::Tab) if !repeat => self.activate("sort", canvas),
            (_, Scancode::M) if !repeat => self.activate("sound", canvas),
            _ => {}
        }
    }
}
