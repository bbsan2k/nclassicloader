//! Visual suspend points backed by ares states, with screenshots kept in profile data.
use super::*;

impl App {
    fn pending_path(&self) -> PathBuf {
        self.data_path
            .join("saves")
            .join(&self.selected().id)
            .join("pending.state")
    }

    pub(super) fn has_pending(&self) -> bool {
        self.pending_path().is_file()
    }

    fn slot_locked(&self, slot: usize) -> bool {
        self.preferences
            .pointer(&format!("/suspendLocks/{}/{}", self.selected().id, slot))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    pub(super) fn capture_pending(&mut self) -> Result<(), String> {
        let path = self.pending_path();
        let id = self.selected().id.clone();
        let core = self
            .core
            .as_mut()
            .filter(|core| core.is_loaded() && self.running_game.as_deref() == Some(&id))
            .ok_or("No paused session for this game")?;
        let parent = path.parent().ok_or("Invalid pending state path")?;
        fs::create_dir_all(parent).map_err(|e| format!("Cannot create suspend folder: {e}"))?;
        core.save_state(&path).map_err(|e| e.to_string())?;
        if let Some(frame) = core.frame() {
            write_frame_png(&path.with_extension("png"), &frame)?;
        } else {
            let _ = fs::remove_file(path.with_extension("png"));
        }
        Ok(())
    }

    pub(super) fn open_suspend(&mut self) {
        self.playing = false;
        self.menu_focus = MenuFocus::Games;
        self.panel = "resume".into();
        self.focus = 0;
        self.suspend_controls = false;
        self.hover.clear();
        self.refresh_saves();
        self.reload_suspend_previews();
    }

    fn reload_suspend_previews(&mut self) {
        self.clear_suspend_previews();
        self.suspend_previews = std::array::from_fn(|slot| {
            let state = self.state_path(self.selected(), slot);
            if !state.is_file() {
                return None;
            }
            preview_texture(&self.art.creator, &state.with_extension("png")).ok()
        });
        self.pending_preview = if self.has_pending() {
            preview_texture(
                &self.art.creator,
                &self.pending_path().with_extension("png"),
            )
            .ok()
        } else {
            None
        };
    }

    pub(super) fn clear_suspend_previews(&mut self) {
        for texture in self
            .suspend_previews
            .iter_mut()
            .filter_map(Option::take)
            .chain(self.pending_preview.take())
        {
            // SAFETY: these textures have no aliases and belong to this app's live renderer.
            unsafe {
                texture.destroy();
            }
        }
    }

    pub(super) fn write_suspend_slot(&mut self) -> Result<(), String> {
        if self.slot_locked(self.slot) {
            return Err("This suspend point is locked. Unlock it before replacing it.".into());
        }
        let path = self.state_path(self.selected(), self.slot);
        let id = self.selected().id.clone();
        let pending = self.pending_path();
        // A paused point survives quitting or switching games before choosing a slot.
        if !self.playing && pending.is_file() {
            copy_state(&pending, &path)?;
            let source = pending.with_extension("png");
            if source.is_file() {
                copy_state(&source, &path.with_extension("png"))?;
            } else {
                let _ = fs::remove_file(path.with_extension("png"));
            }
        } else {
            let core = self
                .core
                .as_mut()
                .filter(|core| core.is_loaded() && self.running_game.as_deref() == Some(&id))
                .ok_or("Pause the selected game before creating a suspend point.")?;
            fs::create_dir_all(path.parent().ok_or("Invalid state path")?)
                .map_err(|e| e.to_string())?;
            core.save_state(&path).map_err(|e| e.to_string())?;
            if let Some(frame) = core.frame() {
                write_frame_png(&path.with_extension("png"), &frame)?;
            }
        }
        self.saved.insert((id, self.slot));
        if !self.playing && pending.is_file() {
            // The floating point has been placed in a numbered slot.
            fs::remove_file(&pending).map_err(|e| {
                format!("Saved point, but could not clear the temporary point: {e}")
            })?;
            let _ = fs::remove_file(pending.with_extension("png"));
        }
        self.reload_suspend_previews();
        self.save_settings()?;
        Ok(())
    }

    pub(super) fn request_suspend_save(&mut self) {
        if self.slot_locked(self.slot) {
            self.panel = "message".into();
            self.message = "This suspend point is locked. Unlock it before replacing it.".into();
            return;
        }
        if !self.has_pending() {
            self.panel = "message".into();
            self.message = self.art.translate(
                "sys_resume_explanation",
                "Pause a game to create a suspend point.",
            );
            return;
        }
        if self
            .saved
            .contains(&(self.selected().id.clone(), self.slot))
        {
            self.confirm_target = Some((self.selected().id.clone(), self.slot));
            self.panel = "confirm-overwrite".into();
            self.focus = 0;
        } else {
            self.save_slot();
            if self.panel != "message" {
                self.open_suspend();
                self.suspend_controls = true;
                self.focus = 1;
            }
        }
    }

    pub(super) fn confirm_suspend_save(&mut self) {
        if self.confirm_target.as_ref() != Some(&(self.selected().id.clone(), self.slot)) {
            return;
        }
        self.confirm_target = None;
        self.save_slot();
        if self.panel != "message" {
            self.open_suspend();
            self.suspend_controls = true;
            self.focus = 1;
        }
    }

    pub(super) fn toggle_suspend_lock(&mut self) {
        if !self
            .saved
            .contains(&(self.selected().id.clone(), self.slot))
        {
            return;
        }
        let locked = !self.slot_locked(self.slot);
        let id = self.selected().id.clone();
        self.preferences["suspendLocks"][id][self.slot.to_string()] = json!(locked);
        if let Err(error) = self.save_settings() {
            self.panel = "message".into();
            self.message = error;
        }
    }

    pub(super) fn request_suspend_delete(&mut self) {
        if !self
            .saved
            .contains(&(self.selected().id.clone(), self.slot))
        {
            return;
        }
        if self.slot_locked(self.slot) {
            self.panel = "message".into();
            self.message = "Unlock this suspend point before deleting it.".into();
            return;
        }
        self.confirm_target = Some((self.selected().id.clone(), self.slot));
        self.panel = "confirm-delete".into();
        self.focus = 0;
    }

    pub(super) fn confirm_suspend_delete(&mut self) {
        if self.confirm_target.as_ref() != Some(&(self.selected().id.clone(), self.slot))
            || self.slot_locked(self.slot)
        {
            return;
        }
        self.confirm_target = None;
        let state = self.state_path(self.selected(), self.slot);
        if let Err(error) = fs::remove_file(&state) {
            self.panel = "message".into();
            self.message = format!("Cannot delete suspend point: {error}");
            return;
        }
        let _ = fs::remove_file(state.with_extension("png"));
        self.open_suspend();
    }

    pub(super) fn resume_pending(&mut self, canvas: &Canvas<Window>) {
        let path = self.pending_path();
        if !path.is_file() || !self.begin_game(canvas) {
            return;
        }
        if let Some(core) = self.core.as_mut()
            && let Err(error) = core.load_state(&path)
        {
            self.playing = false;
            self.panel = "message".into();
            self.message = error.to_string();
        }
        if let Some(audio) = &self.core_audio {
            audio.clear();
        }
    }

    pub(super) fn render_suspend(&mut self, canvas: &mut Canvas<Window>) {
        self.hits.clear();
        self.actions.clear();
        self.art.sprite(
            canvas,
            "resumebg",
            640.0,
            546.0,
            1284.0,
            354.0,
            Color::WHITE,
        );
        self.art.sprite(
            canvas,
            "resumebg_tail",
            640.0,
            351.0,
            96.0,
            54.0,
            Color::WHITE,
        );
        let heading = self.art.translate("sys_resume_Title", "Suspend Point List");
        let heading_width = self.art.measure(&heading, "m").advance;
        let heading_left = 640.0 - (72.0 + 4.0 + heading_width) * 0.5;
        self.art.sprite(
            canvas,
            "icon_Resume",
            heading_left + 36.0,
            402.0,
            72.0,
            72.0,
            Color::WHITE,
        );
        self.art.text(
            canvas,
            &heading,
            heading_left + 76.0 + heading_width * 0.5,
            399.0,
            "m",
            Color::WHITE,
            840.0,
            true,
        );
        for slot in 0..4 {
            let x = 250.0 + slot as f32 * 260.0;
            let saved = self.saved.contains(&(self.selected().id.clone(), slot));
            let selected = slot == self.slot && !self.suspend_controls;
            if selected && !saved {
                self.art.sprite(
                    canvas,
                    "resume_card_active",
                    x,
                    540.0,
                    216.0,
                    192.0,
                    Color::WHITE,
                );
            }
            self.art.sprite(
                canvas,
                if saved {
                    if selected {
                        "resume_card_active"
                    } else {
                        "resume_card"
                    }
                } else {
                    "resume_card_empty"
                },
                x,
                540.0,
                if saved { 192.0 } else { 204.0 },
                if saved { 168.0 } else { 180.0 },
                Color::WHITE,
            );
            if let Some(texture) = &self.suspend_previews[slot] {
                // Original TextureComponent: 160 x 120, centered 12px above the card.
                let _ = canvas.copy(texture, None, FRect::new(x - 80.0, 468.0, 160.0, 120.0));
            } else {
                let label = if saved {
                    "Preview unavailable".into()
                } else {
                    self.art.translate("sys_resume_Empty", "No Data")
                };
                self.art.text(
                    canvas,
                    &label,
                    x,
                    if saved { 528.0 } else { 540.0 },
                    "s",
                    Color::WHITE,
                    172.0,
                    false,
                );
            }
            if !saved {
                self.art.sprite(
                    canvas,
                    &format!("icon_numer_{}", slot + 1),
                    x - 123.0,
                    474.0,
                    42.0,
                    42.0,
                    Color::WHITE,
                );
            }
            if self.slot_locked(slot) {
                self.art.sprite(
                    canvas,
                    "icon_lock",
                    x - 66.0,
                    588.0,
                    42.0,
                    54.0,
                    Color::WHITE,
                );
            }
            self.hit(
                format!("suspend-slot:{slot}"),
                FRect::new(x - 112.0, 442.0, 224.0, 196.0),
            );
        }
        if self.has_pending() {
            // The floating card is the current paused game, not an occupied save slot.
            self.art.sprite(
                canvas,
                "resume_card_float_home",
                180.0,
                270.0,
                162.0,
                150.0,
                Color::WHITE,
            );
            if let Some(texture) = &self.pending_preview {
                let _ = canvas.copy(texture, None, FRect::new(114.0, 209.0, 132.0, 99.0));
            }
            self.hit("resume-pending", FRect::new(96.0, 192.0, 168.0, 152.0));
        }
        let buttons = [
            (
                "save",
                self.art.translate("sys_resume_hud_Register", "Save"),
            ),
            (
                "resume",
                self.art.translate("sys_resume_hud_GameStart", "Start Game"),
            ),
            (
                "lock",
                self.art.translate("sys_resume_hud_Lock", "Lock/Unlock"),
            ),
            (
                "delete",
                self.art
                    .translate("sys_resume_select_delete_dialog_cm_Yes", "Delete"),
            ),
            ("close", self.art.translate("sys_resume_hud_Return", "Back")),
        ];
        // The stock HUD is a centered group of icon+label pairs, not five wide buttons.
        let icons = [
            ("icon_Y_resume", 30.0, 30.0),
            ("icon_start_resume", 78.0, 24.0),
            ("icon_X_resume", 30.0, 30.0),
            ("icon_select_resume", 90.0, 24.0),
            ("icon_B_resume", 30.0, 30.0),
        ];
        let widths: Vec<f32> = buttons
            .iter()
            .zip(icons)
            .map(|((_, label), (_, width, _))| width + 4.0 + self.art.measure(label, "s").advance)
            .collect();
        let total = widths.iter().sum::<f32>() + 30.0 * 4.0;
        let mut left = 640.0 - total * 0.5;
        for (index, ((action, label), (icon, icon_width, icon_height))) in
            buttons.into_iter().zip(icons).enumerate()
        {
            let width = widths[index];
            let x = left + width * 0.5;
            if (self.suspend_controls && self.focus == index) || self.hover == action {
                self.art.sprite(
                    canvas,
                    "btn_init_on",
                    x,
                    669.0,
                    width + 16.0,
                    42.0,
                    Color::WHITE,
                );
            }
            self.art.sprite(
                canvas,
                icon,
                left + icon_width * 0.5,
                669.0,
                icon_width,
                icon_height,
                Color::WHITE,
            );
            self.art.text(
                canvas,
                &label,
                left + icon_width + 4.0 + (width - icon_width - 4.0) * 0.5,
                669.0,
                "s",
                Color::WHITE,
                1000.0,
                true,
            );
            self.hit(action, FRect::new(left - 8.0, 645.0, width + 16.0, 48.0));
            self.actions.push(action.into());
            left += width + 30.0;
        }
    }

    pub(super) fn render_suspend_confirmation(&mut self, canvas: &mut Canvas<Window>) {
        self.hits.clear();
        self.actions.clear();
        self.rect(
            canvas,
            FRect::new(0.0, 96.0, 1280.0, 550.0),
            Color::RGBA(0, 0, 0, 190),
        );
        self.dialog(canvas, 240.0, 190.0, 800.0, 340.0);
        let deleting = self.panel == "confirm-delete";
        let text = if deleting {
            self.art.translate(
                "sys_resume_1_select_delete_dialog_Confitm",
                "Delete this suspend point? This cannot be undone.",
            )
        } else {
            "Replace the selected suspend point with the paused game?".into()
        };
        for (row, line) in wrap_text(&self.art, &text, "m", 720.0).iter().enumerate() {
            self.art.text(
                canvas,
                line,
                640.0,
                260.0 + row as f32 * 36.0,
                "m",
                Color::WHITE,
                720.0,
                false,
            );
        }
        for (index, (action, label)) in [
            ("cancel-suspend", "Cancel"),
            (
                if deleting {
                    "confirm-delete"
                } else {
                    "confirm-save"
                },
                if deleting { "Delete" } else { "Replace" },
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let x = 460.0 + index as f32 * 360.0;
            if self.focus == index {
                self.art
                    .sprite(canvas, "btn_init_on", x, 450.0, 270.0, 42.0, Color::WHITE);
            }
            self.art
                .text(canvas, label, x, 450.0, "m", Color::WHITE, 240.0, false);
            self.hit(action, FRect::new(x - 135.0, 426.0, 270.0, 48.0));
            self.actions.push(action.into());
        }
    }
}

fn copy_state(source: &Path, target: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(source).map_err(|e| e.to_string())?;
    if !metadata.file_type().is_file() || metadata.len() > 64 * 1024 * 1024 {
        return Err("Invalid or oversized suspend data".into());
    }
    let temporary = target.with_extension("copying");
    fs::copy(source, &temporary).map_err(|e| format!("Cannot copy suspend point: {e}"))?;
    publish_file(&temporary, target)
}

fn publish_file(temporary: &Path, target: &Path) -> Result<(), String> {
    match fs::rename(temporary, target) {
        Ok(()) => Ok(()),
        Err(error) => {
            #[cfg(target_os = "windows")]
            if target.is_file() {
                let unique = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos();
                let backup = target.with_extension(format!("previous-{unique}"));
                fs::rename(target, &backup).map_err(|e| e.to_string())?;
                if let Err(error) = fs::rename(temporary, target) {
                    fs::rename(&backup, target)
                        .map_err(|e| format!("Cannot restore previous point: {e}"))?;
                    return Err(error.to_string());
                }
                let _ = fs::remove_file(backup);
                return Ok(());
            }
            Err(format!("Cannot publish suspend data: {error}"))
        }
    }
}

fn write_frame_png(path: &Path, frame: &crate::ares::VideoFrame<'_>) -> Result<(), String> {
    let pixels: Vec<u8> = frame
        .pixels_argb
        .iter()
        .flat_map(|pixel| [(pixel >> 16) as u8, (pixel >> 8) as u8, *pixel as u8, 255])
        .collect();
    let temporary = path.with_extension("png.tmp");
    let file = File::create(&temporary).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(file, frame.width, frame.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer
        .write_image_data(&pixels)
        .map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    publish_file(&temporary, path)
}

fn preview_texture(
    creator: &TextureCreator<WindowContext>,
    path: &Path,
) -> Result<Texture, String> {
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !metadata.file_type().is_file() || metadata.len() > 8 * 1024 * 1024 {
        return Err("Invalid suspend preview".into());
    }
    let mut decoder =
        png::Decoder::new(BufReader::new(File::open(path).map_err(|e| e.to_string())?));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    if reader.info().width > 1024 || reader.info().height > 1024 {
        return Err("Suspend preview too large".into());
    }
    let mut data = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut data).map_err(|e| e.to_string())?;
    let mut pixels: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => data[..info.buffer_size()].to_vec(),
        png::ColorType::Rgb => data[..info.buffer_size()]
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        _ => return Err("Unsupported suspend preview pixels".into()),
    };
    let surface = Surface::from_data(
        &mut pixels,
        info.width,
        info.height,
        info.width * 4,
        PixelFormat::RGBA32,
    )
    .map_err(|e| e.to_string())?;
    let mut texture = creator
        .create_texture_from_surface(surface)
        .map_err(|e| e.to_string())?;
    texture.set_scale_mode(ScaleMode::Linear);
    Ok(texture)
}
