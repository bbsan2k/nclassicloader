//! Shared keyboard/gamepad navigation. A loaded-but-paused core is not gameplay.
use super::input;
use super::*;
use crate::frontend::{
    menu_input::{KeyEvent, suspend_input},
    menu_navigation as navigation,
};

impl App {
    pub(super) fn handle_key(&mut self, code: Scancode, repeat: bool, canvas: &Canvas<Window>) {
        self.handle_menu_input(input::native_event(code, repeat), canvas);
    }

    pub(super) fn handle_menu_input(&mut self, event: KeyEvent, canvas: &Canvas<Window>) {
        let panel = if self.panel.is_empty() {
            navigation::Panel::None
        } else if self.panel == "resume" {
            navigation::Panel::Resume
        } else if self.toolbar_page().is_some() {
            navigation::Panel::Toolbar
        } else if self.panel == "confirm-overwrite" {
            navigation::Panel::ConfirmOverwrite
        } else if self.panel == "confirm-delete" {
            navigation::Panel::ConfirmDelete
        } else if self.panel.starts_with("confirm-") {
            navigation::Panel::ConfirmOther
        } else if self.panel == "emulator-settings" {
            navigation::Panel::EmulatorSettings
        } else if self.panel == "help" {
            navigation::Panel::Help
        } else {
            navigation::Panel::Other
        };
        let state = navigation::State {
            menu_focus: self.menu_focus,
            focus: self.focus,
            toolbar_index: self.toolbar_index,
        };
        let context = navigation::Context {
            playing: self.playing,
            owned_import_notice: self.owned_import_notice(),
            panel,
            action_count: self.actions.len(),
        };
        let plan = navigation::transition(state, context, event);
        self.menu_focus = plan.state.menu_focus;
        self.focus = plan.state.focus;
        self.toolbar_index = plan.state.toolbar_index;
        if plan.clear_hover {
            self.hover.clear();
        }
        let Some(effect) = plan.effect else {
            return;
        };
        match effect {
            navigation::Effect::Fullscreen => self.activate("fullscreen", canvas),
            navigation::Effect::LeaveGame => self.leave_game(),
            navigation::Effect::SaveSlot => self.save_slot(),
            navigation::Effect::LoadSlot => self.resume_slot(canvas),
            navigation::Effect::CancelImport => self.request_import_cancel(),
            navigation::Effect::OpenProfiles => self.activate("profiles", canvas),
            navigation::Effect::DelegateSuspend => self.handle_suspend_input(event, canvas),
            navigation::Effect::DelegateToolbar => {
                let _ = self.toolbar_key(event, canvas);
            }
            navigation::Effect::CancelConfirmationAndOpenSuspend => {
                self.confirm_target = None;
                self.open_suspend();
            }
            navigation::Effect::CancelSuspend => self.activate("cancel-suspend", canvas),
            navigation::Effect::ConfirmDelete => self.activate("confirm-delete", canvas),
            navigation::Effect::ConfirmSave => self.activate("confirm-save", canvas),
            navigation::Effect::SetSettingsPanel => {
                self.panel = "settings".into();
                self.focus = 0;
            }
            navigation::Effect::SetHelpToolbarPanel => {
                self.panel = "toolbar:4".into();
                self.focus = 0;
            }
            navigation::Effect::ClosePanel => self.panel.clear(),
            navigation::Effect::ActivateFocused(index) => {
                if let Some(action) = self.actions.get(index).cloned() {
                    self.activate(&action, canvas);
                }
            }
            navigation::Effect::OpenToolbar(index) => {
                self.activate(&format!("toolbar:{index}"), canvas)
            }
            navigation::Effect::OpenSuspend => self.open_suspend(),
            navigation::Effect::MoveGame(direction) => self.move_selection(direction),
            navigation::Effect::BeginGame => {
                self.begin_game(canvas);
            }
            navigation::Effect::Sort => self.activate("sort", canvas),
            navigation::Effect::ToggleSound => self.activate("sound", canvas),
        }
    }

    fn handle_suspend_input(&mut self, event: KeyEvent, canvas: &Canvas<Window>) {
        use crate::frontend::suspend_navigation::{Effect, State, transition};
        let pending = event.key == crate::frontend::menu_input::Key::Confirm
            && !event.repeat
            && !self.suspend_controls
            && self.has_pending();
        let input = suspend_input(event, pending, self.suspend_controls, self.art.nes);
        let (state, effects) = transition(
            State {
                slot: self.slot,
                focus: self.focus,
                controls: self.suspend_controls,
            },
            input,
        );
        self.slot = state.slot;
        self.focus = state.focus;
        self.suspend_controls = state.controls;
        for effect in effects {
            match effect {
                Effect::SaveSettings => {
                    let _ = self.save_settings();
                }
                Effect::Dismiss => {
                    self.panel.clear();
                    self.menu_focus = MenuFocus::Games;
                }
                Effect::ActivateSave => self.activate("save", canvas),
                Effect::ActivateResume => self.activate("resume", canvas),
                Effect::ActivateLock => self.activate("lock", canvas),
                Effect::ActivateDelete => self.activate("delete", canvas),
                Effect::ActivateClose => self.activate("close", canvas),
                Effect::ActivateChooseSuspend => self.activate("choose-suspend", canvas),
                Effect::RequestSave => self.request_suspend_save(),
                Effect::ResumeSlot => self.resume_slot(canvas),
                Effect::ResumePending => self.resume_pending(canvas),
                Effect::ToggleLock => self.toggle_suspend_lock(),
                Effect::RequestDelete => self.request_suspend_delete(),
            }
        }
    }
}
