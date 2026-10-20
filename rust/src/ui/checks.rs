//! Opt-in end-to-end check using a supplied imported profile and disposable data.
use super::*;

impl App {
    pub(super) fn exercise_suspend(
        &mut self,
        canvas: &mut Canvas<Window>,
        output: &Path,
    ) -> Result<(), String> {
        fs::create_dir_all(output).map_err(|e| e.to_string())?;
        let check = |condition: bool, message: &str| {
            if condition {
                Ok(())
            } else {
                Err(message.to_owned())
            }
        };
        let game = self.selected().id.clone();
        self.handle_key(Scancode::Up, false, canvas);
        check(
            self.menu_focus == MenuFocus::Toolbar && self.panel.is_empty(),
            "Up must focus toolbar, not open Settings",
        )?;
        self.handle_key(Scancode::Right, false, canvas);
        check(
            self.toolbar_index == 1 && self.selected().id == game,
            "Toolbar navigation changed selected game",
        )?;
        self.render_menu(canvas);
        save_screenshot(canvas, &output.join("keyboard-toolbar.png"))?;
        self.handle_key(Scancode::Return, false, canvas);
        check(
            self.panel == "settings",
            "Return must open the focused toolbar icon",
        )?;
        self.handle_key(Scancode::Escape, false, canvas);
        self.handle_key(Scancode::Down, false, canvas);
        check(
            self.menu_focus == MenuFocus::Games,
            "Down must return from toolbar to carousel",
        )?;
        self.pad_button_down(PadButton::DPadUp, canvas);
        check(
            self.menu_focus == MenuFocus::Toolbar,
            "Gamepad must share toolbar navigation",
        )?;
        self.pad_held.clear();
        self.handle_key(Scancode::Down, false, canvas);
        self.handle_key(Scancode::Down, false, canvas);
        check(self.panel == "resume", "Down must open suspend picker")?;
        let old_slot = self.slot;
        self.handle_key(Scancode::Right, false, canvas);
        check(
            self.slot == (old_slot + 1) % 4,
            "Left/right must choose suspend slots",
        )?;
        self.handle_key(Scancode::Up, false, canvas);
        check(
            self.panel.is_empty(),
            "Up must return from picker to carousel",
        )?;

        self.activate("toolbar:0", canvas);
        check(
            self.focus == 1,
            "Display must initially focus the current 4:3 mode",
        )?;
        self.handle_key(Scancode::Right, false, canvas);
        self.handle_key(Scancode::Return, false, canvas);
        check(
            self.preferences.get("displayMode").and_then(Value::as_u64) == Some(2),
            "Display choice was not applied",
        )?;
        self.activate("display:1", canvas);
        self.handle_key(Scancode::Escape, false, canvas);
        self.activate("toolbar:1", canvas);
        self.handle_key(Scancode::Return, false, canvas);
        check(
            self.panel == "emulator-settings",
            "Options must link to separate emulator controls",
        )?;
        self.handle_key(Scancode::Escape, false, canvas);
        check(self.panel == "settings", "Back must return to Options")?;
        self.activate("toolbar:3", canvas);
        self.handle_key(Scancode::Down, false, canvas);
        check(self.panel_scroll > 0, "Legal notices must scroll")?;
        self.handle_key(Scancode::Right, false, canvas);
        check(
            self.focus == 1 && self.panel_scroll == 0,
            "Legal tabs must reset scroll",
        )?;
        self.activate("toolbar:4", canvas);
        self.handle_key(Scancode::H, false, canvas);
        check(
            self.panel == "help",
            "Manuals must preserve access to emulator help",
        )?;
        self.handle_key(Scancode::Escape, false, canvas);
        self.handle_key(Scancode::Escape, false, canvas);
        self.menu_focus = MenuFocus::Games;

        check(
            self.begin_game(canvas),
            &format!("Cannot launch supplied test game: {}", self.message),
        )?;
        self.core_audio = None; // Run deterministic frames without a wall-clock audio queue.
        for _ in 0..600 {
            self.run_game_frame()?;
        }
        self.leave_game();
        check(
            !self.playing && self.panel == "resume" && self.has_pending(),
            "Reset must pause into picker and preserve a floating point",
        )?;
        let saves = self.data_path.join("saves").join(&game);
        check(
            saves.join("pending.png").is_file(),
            "Paused screenshot was not captured",
        )?;
        self.render_menu(canvas);
        save_screenshot(canvas, &output.join("paused-point.png"))?;
        let preview = fs::read(saves.join("pending.png")).map_err(|e| e.to_string())?;
        check(
            !saves.join("slot-2.state").exists(),
            "Pausing must not silently overwrite a numbered slot",
        )?;
        self.slot = 1;
        self.request_suspend_save();
        check(
            saves.join("slot-2.state").is_file() && saves.join("slot-2.png").is_file(),
            "Choosing slot 2 must save state and picture",
        )?;
        check(
            !self.has_pending(),
            "Saving must place, not duplicate, the floating point",
        )?;
        check(
            fs::read(saves.join("slot-2.png")).map_err(|e| e.to_string())? == preview,
            "Slot preview differs from paused frame",
        )?;
        let before = fs::read(saves.join("slot-2.state")).map_err(|e| e.to_string())?;
        self.toggle_suspend_lock();
        self.capture_pending()?;
        self.request_suspend_save();
        check(
            self.panel == "message",
            "Locked point must reject overwrite",
        )?;
        check(
            fs::read(saves.join("slot-2.state")).map_err(|e| e.to_string())? == before,
            "Locked point changed",
        )?;
        self.open_suspend();
        self.request_suspend_delete();
        check(
            self.panel == "message" && saves.join("slot-2.state").is_file(),
            "Locked state must reject deletion",
        )?;
        self.open_suspend();
        self.toggle_suspend_lock();
        self.request_suspend_save();
        check(
            self.panel == "confirm-overwrite",
            "Occupied slot must ask before overwrite",
        )?;
        self.render_menu(canvas);
        save_screenshot(canvas, &output.join("overwrite-confirmation.png"))?;
        self.activate("cancel-suspend", canvas);
        check(
            fs::read(saves.join("slot-2.state")).map_err(|e| e.to_string())? == before,
            "Cancel replaced a state",
        )?;
        self.resume_slot(canvas);
        self.core_audio = None;
        check(
            self.playing,
            "Loading a suspend point did not start emulation",
        )?;
        for _ in 0..12 {
            self.run_game_frame()?;
        }
        let first = self
            .core
            .as_ref()
            .and_then(|core| core.frame())
            .ok_or("No resumed frame")?
            .pixels_argb
            .to_vec();
        self.resume_slot(canvas);
        self.core_audio = None;
        for _ in 0..12 {
            self.run_game_frame()?;
        }
        let second = self
            .core
            .as_ref()
            .and_then(|core| core.frame())
            .ok_or("No replayed frame")?
            .pixels_argb
            .to_vec();
        check(
            first == second,
            "State load did not replay deterministically",
        )?;
        self.leave_game();
        self.request_suspend_save();
        check(
            self.panel == "confirm-overwrite",
            "Replacing a point must remain confirmed",
        )?;
        self.confirm_suspend_save();
        check(
            fs::read(saves.join("slot-2.state")).map_err(|e| e.to_string())? != before,
            "Confirmed overwrite did not save the new state",
        )?;
        self.capture_pending()?;
        self.slot = 0;
        self.request_suspend_save();
        self.capture_pending()?;
        self.slot = 2;
        self.request_suspend_save();
        self.slot = 1;
        self.request_suspend_delete();
        check(
            self.panel == "confirm-delete",
            "Delete must ask for confirmation",
        )?;
        self.confirm_suspend_delete();
        check(
            !saves.join("slot-2.state").exists() && !saves.join("slot-2.png").exists(),
            "Delete must remove only chosen state and picture",
        )?;
        check(
            saves.join("slot-1.state").is_file() && saves.join("slot-3.state").is_file(),
            "Delete affected another slot",
        )?;
        self.slot = 0;
        self.toggle_suspend_lock();
        self.open_suspend();
        self.render_menu(canvas);
        save_screenshot(canvas, &output.join("saved-picker.png"))?;
        // A real restart must reload pictures and lock preferences from data, not RAM.
        self.core
            .as_mut()
            .ok_or("Missing core")?
            .unload()
            .map_err(|e| e.to_string())?;
        self.running_game = None;
        self.running_rom = None;
        self.resume_slot(canvas);
        self.core_audio = None;
        check(
            self.playing,
            "Saved point did not load after unloading the core",
        )?;
        for _ in 0..12 {
            self.run_game_frame()?;
        }
        self.leave_game();
        self.save_settings()?;
        atomic_json(
            &output.join("result.json"),
            &json!({"toolbar_navigation": true, "gamepad_navigation": true,
            "screenshots": true, "slot_picker": true, "lock_protection": true, "overwrite_cancel": true, "confirmed_overwrite": true,
            "deterministic_state_replay": true, "delete_isolation": true, "reload_after_unload": true}),
        )?;
        Ok(())
    }
}
