//! Opt-in end-to-end check using a supplied imported profile and disposable data.
use super::*;

impl App {
    pub(super) fn exercise_audio(
        &mut self,
        canvas: &Canvas<Window>,
        output: &Path,
    ) -> Result<(), String> {
        fs::create_dir_all(output).map_err(|e| e.to_string())?;
        if !self.begin_game(canvas) {
            return Err(self.message.clone());
        }
        if self.core_audio.is_none() {
            return Err("No SDL playback device for audio exercise".into());
        }
        let mut phases = Vec::new();
        for hz in [60, 120, 50] {
            let audio = self.core_audio.as_ref().expect("audio checked");
            audio.clear();
            if audio.queued() != 0 || audio.started.get() {
                return Err("Audio reset failed".into());
            }
            let priming_frames = self.pump_game_audio()?;
            if !self
                .core_audio
                .as_ref()
                .is_some_and(|audio| audio.started.get())
            {
                return Err("Audio was not primed before starting playback".into());
            }
            let start = Instant::now();
            let mut deadline = start;
            let mut frames = 0;
            let mut ticks = 0;
            let mut underruns = 0;
            let mut minimum = usize::MAX;
            let mut maximum = 0;
            while start.elapsed().as_secs_f64() < 1.5 {
                let audio = self.core_audio.as_ref().expect("audio checked");
                let queued = audio.queued();
                if queued == 0 {
                    underruns += 1;
                }
                minimum = minimum.min(queued);
                maximum = maximum.max(queued);
                frames += self.pump_game_audio()?;
                ticks += 1;
                // Inject short render/event stalls without altering the audio
                // device's clock. Production must catch up without queue clears.
                deadline += std::time::Duration::from_nanos(1_000_000_000 / hz);
                let jitter = if ticks % 19 == 0 {
                    std::time::Duration::from_millis(12)
                } else {
                    std::time::Duration::ZERO
                };
                std::thread::sleep(deadline.saturating_duration_since(Instant::now()) + jitter);
            }
            let elapsed = start.elapsed().as_secs_f64();
            let emulation_hz = frames as f64 / elapsed;
            phases.push(json!({"displayHz":hz,"emulatedFrames":frames,"elapsed":elapsed,"emulationHz":emulation_hz,"primingFrames":priming_frames,"underruns":underruns,"minimumQueuedBytes":minimum,"maximumQueuedBytes":maximum}));
        }
        self.core_audio.as_ref().expect("audio checked").clear();
        self.playing = false;
        atomic_json(&output.join("audio.json"), &json!({"phases":phases}))?;
        for phase in &phases {
            if phase["underruns"].as_u64() != Some(0) {
                return Err(format!(
                    "Audio underrun during controlled pacing exercise: {phase}"
                ));
            }
            let hz = phase["emulationHz"].as_f64().unwrap_or(0.0);
            if !(55.0..65.0).contains(&hz) {
                return Err(format!(
                    "Emulation clock followed display rather than audio: {phase}"
                ));
            }
        }
        Ok(())
    }

    // Explicit development audit. All states/SRAM go under the caller-selected
    // audit output, never into the profile's normal save directory.
    pub(super) fn audit_games(&mut self, output: &Path) -> Result<(), String> {
        fs::create_dir_all(output).map_err(|e| e.to_string())?;
        let mut results = Vec::new();
        let mut failures = Vec::new();
        for game in &self.games {
            let Some(rom) = self.rom_path(game) else {
                results.push(json!({"id": game.id, "blocked": true, "reason": game.import_error}));
                continue;
            };
            let run = || -> Result<Value, String> {
                let mut core = crate::ares::AresSession::new().map_err(|e| e.to_string())?;
                let saves = output.join("runtime").join(&game.id);
                fs::create_dir_all(&saves).map_err(|e| e.to_string())?;
                core.load(
                    &rom,
                    &saves,
                    Some(&self.firmware_path),
                    &self.assets_path.join("firmware"),
                )
                .map_err(|e| e.to_string())?;
                let mut audio_samples = 0;
                let mut peak = 0.0_f32;
                let mut max_colors = 0;
                for frame in 0..600 {
                    // Boot, then press Start and exercise both NES action buttons.
                    let buttons = if (180..190).contains(&frame) {
                        1 << 3
                    } else if frame > 240 && frame % 60 < 20 {
                        (1 << 8) | 1 | (1 << 7)
                    } else {
                        0
                    };
                    core.run_frame(buttons).map_err(|e| e.to_string())?;
                    for &sample in core.audio() {
                        if !sample.is_finite() {
                            return Err("Non-finite audio sample".into());
                        }
                        peak = peak.max(sample.abs());
                    }
                    audio_samples += core.audio().len();
                    if frame % 30 == 0 {
                        let video = core.frame().ok_or("Missing video frame")?;
                        max_colors = max_colors.max(
                            video
                                .pixels_argb
                                .iter()
                                .copied()
                                .collect::<HashSet<_>>()
                                .len(),
                        );
                    }
                }
                let frame = core.frame().ok_or("Missing video frame")?;
                let (width, height) = (frame.width, frame.height);
                let colors: HashSet<_> = frame.pixels_argb.iter().copied().collect();
                if max_colors < 2 || audio_samples == 0 {
                    return Err(format!(
                        "No rendered picture or audio samples (max colors={max_colors}, samples={audio_samples})"
                    ));
                }
                let state = saves.join("audit.state");
                core.save_state(&state).map_err(|e| e.to_string())?;
                for _ in 0..30 {
                    core.run_frame(0).map_err(|e| e.to_string())?;
                }
                let expected = core
                    .frame()
                    .ok_or("Missing replay frame")?
                    .pixels_argb
                    .to_vec();
                core.load_state(&state).map_err(|e| e.to_string())?;
                for _ in 0..30 {
                    core.run_frame(0).map_err(|e| e.to_string())?;
                }
                if expected != core.frame().ok_or("Missing replay frame")?.pixels_argb {
                    return Err("State replay diverged".into());
                }
                core.unload().map_err(|e| e.to_string())?;
                Ok(
                    json!({"id":game.id,"width":width,"height":height,"colors":colors.len(),"maxColors":max_colors,"audioSamples":audio_samples,"audioPeak":peak,"replay":true}),
                )
            };
            match run() {
                Ok(result) => {
                    eprintln!("Audited {}", game.title);
                    results.push(result);
                }
                Err(error) => {
                    let message = format!("{} ({}): {error}", game.title, game.id);
                    eprintln!("{message}");
                    failures.push(message);
                    results.push(json!({"id":game.id,"error":error}));
                }
            }
        }
        atomic_json(&output.join("games.json"), &json!(results))?;
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("\n"))
        }
    }

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
        self.handle_key(Scancode::F11, false, canvas);
        check(
            self.fullscreen && self.fullscreen_dirty,
            "F11 must request full screen",
        )?;
        self.handle_key(Scancode::F11, true, canvas);
        check(
            self.fullscreen,
            "Held F11 must not repeatedly toggle full screen",
        )?;
        self.handle_key(Scancode::F11, false, canvas);
        check(!self.fullscreen, "F11 must return to windowed mode")?;
        self.fullscreen_dirty = false;
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
        if self.art.nes {
            self.handle_key(Scancode::Down, false, canvas);
            check(self.focus == 1, "NES Display has no decorative-frame row")?;
        }
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
