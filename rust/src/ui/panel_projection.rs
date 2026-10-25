use crate::frontend::menu_panel::{self, PanelInput, PanelKey};

pub(super) struct ProjectedPanel {
    pub(super) input: PanelInput,
    pub(super) bindings: super::panel_scene::PanelBindings,
    pub(super) focus: usize,
}

impl super::UiAssets {
    pub(super) fn project_panel_rows(
        &self,
        entries: Vec<(String, String)>,
        viewport: menu_panel::RowViewport,
        mut y: f32,
        hover: &str,
    ) -> Vec<(PanelKey, menu_panel::RowScene)> {
        let mut rows = Vec::new();
        for (index, (action, label)) in entries
            .into_iter()
            .enumerate()
            .skip(viewport.first_row)
            .take(viewport.visible_count)
        {
            let highlighted = viewport.focus == index || hover == action;
            let highlight = highlighted
                .then(|| self.sprite_source("btn_init_on"))
                .flatten();
            let layout = self.measure(&label, "m");
            rows.push((
                PanelKey(action),
                menu_panel::row(
                    y,
                    viewport.row_height,
                    highlighted,
                    highlight,
                    &layout,
                    &self.font_atlases(),
                ),
            ));
            y += viewport.row_height;
        }
        rows
    }
}

impl super::App {
    pub(super) fn project_panel(&self) -> ProjectedPanel {
        let page = if self.panel == "help" {
            Some(4)
        } else {
            self.panel
                .strip_prefix("toolbar:")
                .and_then(|value| value.parse::<usize>().ok())
        };
        let settings = self.panel == "emulator-settings" || page == Some(1);
        let heading = match (self.panel.as_str(), page) {
            ("message", _) => "Classic Menu".to_owned(),
            ("profiles", _) => "Image Library".to_owned(),
            (_, Some(0)) => self.art.translate("sys_display_Title", "Display"),
            (_, Some(1)) => self.art.translate("sys_option_Title", "Settings"),
            (_, Some(2)) => self.art.translate("sys_language_hud_Title", "Language"),
            (_, Some(3)) => self.selected().title.clone(),
            (_, Some(4)) => "Help".to_owned(),
            ("emulator-settings", _) => "Emulator / Image Library".to_owned(),
            _ => self.art.translate("sys_resume_Title", "Suspend points"),
        };
        let heading_layout = self.art.measure(&heading, "l");
        let import_notice = self.owned_import_notice() || self.preview_import_notice.is_some();
        let entries: Vec<(String, String)> = if import_notice {
            vec![("dismiss-import".into(), "Continue browsing".into())]
        } else if self.panel == "profiles" {
            let mut profiles: Vec<_> = self
                .profiles
                .iter()
                .map(|profile| {
                    let name = profile
                        .directory
                        .file_name()
                        .and_then(|value| value.to_str())
                        .unwrap_or_default()
                        .to_owned();
                    (format!("profile:{name}"), profile.label())
                })
                .collect();
            profiles.push((
                "import-image".into(),
                "Import image or switch version...".into(),
            ));
            profiles
        } else if page == Some(2) {
            self.art
                .languages
                .keys()
                .map(|locale| {
                    let label = match locale.as_str() {
                        "usa_en" => "English",
                        "eur_de" => "Deutsch",
                        "eur_fr" => "Français",
                        "eur_es" => "Español",
                        "eur_it" => "Italiano",
                        "eur_nl" => "Nederlands",
                        "eur_pt" => "Português (Europeu)",
                        "eur_ru" => "Русский",
                        "jpn_ja" => "Japanese",
                        other => other,
                    };
                    (
                        format!("language:{locale}"),
                        format!(
                            "{}{}",
                            if *locale == self.art.locale {
                                "[x] "
                            } else {
                                "[ ] "
                            },
                            label
                        ),
                    )
                })
                .collect()
        } else if self.panel == "message" || page == Some(3) || page == Some(4) {
            Vec::new()
        } else if settings {
            vec![
                ("rom".into(), "Choose ROM for selected game".into()),
                (
                    "firmware".into(),
                    "Choose coprocessor firmware folder".into(),
                ),
                (
                    "profiles".into(),
                    "Image Library / Switch Version (F2)".into(),
                ),
                (
                    "sound".into(),
                    format!("Menu sound: {}", if self.sound { "On" } else { "Off" }),
                ),
                (
                    "motion".into(),
                    format!("Animation: {}", if self.motion { "On" } else { "Off" }),
                ),
                ("slot".into(), format!("Suspend slot: {}", self.slot + 1)),
                ("fullscreen".into(), "Toggle full screen".into()),
            ]
        } else if page == Some(0) {
            vec![
                ("fullscreen".into(), "Toggle full screen".into()),
                (
                    "motion".into(),
                    format!("Animation: {}", if self.motion { "On" } else { "Off" }),
                ),
            ]
        } else {
            let is_saved = self
                .saved
                .contains(&(self.selected().id.clone(), self.slot));
            vec![
                ("slot".into(), "Choose next slot".into()),
                (
                    "resume".into(),
                    format!(
                        "Resume selected slot ({})",
                        if is_saved { "Saved" } else { "Empty" }
                    ),
                ),
                ("save".into(), "Save current session to this slot".into()),
            ]
        };
        let message = self.panel == "message";
        let body: Vec<String> = if message {
            super::wrap_text(&self.art, &self.message, "s", 770.0)
        } else {
            let selected = self.selected().clone();
            match page {
                Some(2) => vec![
                    if self.art.languages.len() > 1 {
                        "Choose the language for Clover menu labels.".into()
                    } else {
                        "Re-import this image to add all shipped language files.".into()
                    },
                    "Game titles, regional artwork and custom controls are unchanged.".into(),
                ],
                Some(3) => vec![
                    format!("{}    {}", selected.publisher, selected.release),
                    selected.copyright.clone(),
                    "Core: ares (embedded, no external emulator)".into(),
                ],
                Some(4) => vec![
                    "Arrows: select    Return: start    Tab: sort".into(),
                    "Up: toolbar    Down: suspend points".into(),
                    "Z B, X A, A Y, S X, Q L, W R".into(),
                    "Escape: pause and pick a suspend slot; F5/F8: save/load".into(),
                ],
                _ if self.panel == "profiles" => vec![
                    "Choose a verified cached profile, or import another NES / SNES Classic image."
                        .into(),
                    "The source image is read-only; extracted programs are never run.".into(),
                ],
                _ if settings => {
                    vec!["Keyboard: arrows select; Return chooses; Escape closes.".into()]
                }
                _ => vec![
                    format!(
                        "Slot {} - {}",
                        self.slot + 1,
                        if self.saved.contains(&(selected.id.clone(), self.slot)) {
                            "Saved"
                        } else {
                            "Empty"
                        }
                    ),
                    "Suspend points use this profile's data directory.".into(),
                ],
            }
        };
        let body_layouts: Vec<_> = body
            .iter()
            .map(|line| self.art.measure(line, if message { "s" } else { "m" }))
            .collect();
        let text = menu_panel::text_scene(
            &heading_layout,
            &body_layouts,
            message,
            &self.art.font_atlases(),
        );
        let viewport = menu_panel::row_viewport(entries.len(), self.focus, text.rows_y);
        let actions: Vec<_> = entries
            .iter()
            .map(|(action, _)| PanelKey(action.clone()))
            .collect();
        let rows = self
            .art
            .project_panel_rows(entries, viewport, text.rows_y, &self.hover);
        let cancellation_view = if import_notice {
            self.import_cancellation_view()
        } else {
            crate::ui::launcher::import_worker::CancellationView::Unavailable
        };
        let (import_footer_label, footer_action) = super::import_footer(cancellation_view);
        let back = if import_notice {
            import_footer_label.to_owned()
        } else {
            self.art.translate("sys_option_hud_Return", "Back")
        };
        let layout = self.art.measure(&back, "s");
        let footer = menu_panel::footer(
            &layout,
            &self.art.font_atlases(),
            self.art.sprite_source("icon_B_light"),
            footer_action.is_some() || !import_notice,
        );
        let footer_key = if let Some(action) = footer_action {
            Some(PanelKey(action.into()))
        } else if !import_notice {
            Some(PanelKey("close".into()))
        } else {
            None
        };
        let mut actions = actions;
        if let Some(key) = &footer_key {
            actions.push(key.clone());
        }
        let generation = self.art.texture_bindings.generation();
        let input = PanelInput {
            generation,
            text,
            rows,
            footer,
            footer_key,
            actions,
        };
        let bindings = super::panel_scene::PanelBindings::new(generation, input.actions.clone());
        ProjectedPanel {
            input,
            bindings,
            focus: viewport.focus,
        }
    }
}
