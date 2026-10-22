//! Stock-style full-screen toolbar pages. Geometry follows defaultscene.scn;
//! custom emulator controls live on their own, explicitly named page.
use super::*;
use crate::frontend::menu_input::{Key, KeyEvent};
use crate::frontend::toolbar_navigation as navigation;

const LANGUAGES: [&str; 9] = [
    "usa_en", "eur_fr", "eur_de", "eur_es", "eur_it", "eur_nl", "eur_pt", "eur_ru", "jpn_ja",
];

impl App {
    pub(super) fn toolbar_page(&self) -> Option<usize> {
        if self.panel == "settings" {
            Some(1)
        } else {
            self.panel
                .strip_prefix("toolbar:")
                .and_then(|value| value.parse().ok())
        }
    }

    fn language_choices(&self) -> Vec<&str> {
        LANGUAGES
            .into_iter()
            .filter(|locale| self.art.languages.contains_key(*locale))
            .collect()
    }

    pub(super) fn display_mode(&self) -> usize {
        self.preferences
            .get("displayMode")
            .and_then(Value::as_u64)
            .unwrap_or(1)
            .min(2) as usize
    }

    pub(super) fn toolbar_initial_focus(&self, page: usize) -> usize {
        match page {
            0 => self.display_mode(),
            2 => self
                .language_choices()
                .iter()
                .position(|locale| *locale == self.art.locale)
                .unwrap_or(0),
            _ => 0,
        }
    }

    // Source sprites have independent flips/rotations for each frame edge.
    fn panel_sprite(
        &mut self,
        canvas: &mut Canvas<Window>,
        name: &str,
        rect: FRect,
        angle: f64,
        flip_x: bool,
        flip_y: bool,
    ) {
        let Some(source) = self.art.sprites.get(&format!("{name}.png")).copied() else {
            return;
        };
        if let Some(texture) = self.art.textures.get_mut("skin/packed.png") {
            texture.set_color_mod(255, 255, 255);
            texture.set_alpha_mod(255);
            texture.set_scale_mode(ScaleMode::Nearest);
            let _ = canvas.copy_ex(texture, Some(source), rect, angle, None, flip_x, flip_y);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn panel_text_left(
        &mut self,
        canvas: &mut Canvas<Window>,
        text: &str,
        left: f32,
        center_y: f32,
        font: &str,
        width: f32,
        color: Color,
    ) {
        let advance = self.art.measure(text, font).advance.min(width);
        self.art.text(
            canvas,
            text,
            left + advance * 0.5,
            center_y,
            font,
            color,
            width,
            true,
        );
    }

    fn toolbar_frame(&mut self, canvas: &mut Canvas<Window>, page: usize) {
        self.hits.clear();
        self.actions.clear();
        self.rect(canvas, FRect::new(0.0, 0.0, 1280.0, 720.0), Color::BLACK);
        for (x, y, flip_x, flip_y) in [
            (82.0, 54.0, false, false),
            (1198.0, 54.0, true, false),
            (82.0, 666.0, false, true),
            (1198.0, 666.0, true, true),
        ] {
            self.panel_sprite(
                canvas,
                "dialog_corner",
                FRect::new(x - 18.0, y - 18.0, 36.0, 36.0),
                0.0,
                flip_x,
                flip_y,
            );
        }
        for y in [42.0, 678.0] {
            self.art
                .sprite(canvas, "dialog_line", 640.0, y, 1080.0, 12.0, Color::WHITE);
        }
        for x in [70.0, 1210.0] {
            self.panel_sprite(
                canvas,
                "dialog_line",
                FRect::new(x - 288.0, 354.0, 576.0, 12.0),
                90.0,
                false,
                false,
            );
        }
        self.art.sprite(
            canvas,
            "dialog_line",
            640.0,
            108.0,
            1134.0,
            12.0,
            Color::WHITE,
        );
        let title = self.art.translate(
            [
                "sys_display_Title",
                "sys_option_Title",
                "sys_language_hud_Title",
                "sys_copyright_Title",
                "sys_manual_Title",
            ][page],
            [
                "Display",
                "Options",
                "Languages",
                "Legal Notices",
                "Manuals",
            ][page],
        );
        self.art.sprite(
            canvas,
            [
                "btn_display",
                "btn_option",
                "btn_language",
                "btn_copyright",
                "manual",
            ][page],
            128.0,
            75.0,
            72.0,
            72.0,
            Color::WHITE,
        );
        self.panel_text_left(canvas, &title, 174.0, 72.0, "l", 430.0, Color::WHITE);
        let prefix = [
            "sys_display_hud_",
            "sys_option_hud_",
            "sys_language_hud_",
            "sys_copyright_hud_",
            "sys_manual_hud_",
        ][page];
        let hints = if page == 4 {
            vec![(
                "icon_B_light",
                self.art.translate("sys_manual_hud_Back", "Back"),
            )]
        } else {
            vec![
                (
                    if page == 1 {
                        "Icon_direction_Lengthwise_light"
                    } else {
                        "icon_direction_lateral_light"
                    },
                    self.art.translate(
                        &format!("{prefix}{}", if page == 3 { "Page" } else { "Select" }),
                        "Select",
                    ),
                ),
                (
                    if page == 3 {
                        "Icon_direction_Lengthwise_light"
                    } else {
                        "icon_A_light"
                    },
                    self.art.translate(
                        &format!("{prefix}{}", if page == 3 { "Scroll" } else { "Decide" }),
                        if page == 3 { "Scroll" } else { "OK" },
                    ),
                ),
                (
                    "icon_B_light",
                    self.art.translate(&format!("{prefix}Return"), "Back"),
                ),
            ]
        };
        let total: f32 = hints
            .iter()
            .map(|(_, label)| 40.0 + self.art.measure(label, "s").advance)
            .sum::<f32>()
            + 30.0 * hints.len().saturating_sub(1) as f32;
        let mut left = 1180.0 - total;
        for (icon, label) in hints {
            let width = self.art.measure(&label, "s").advance;
            self.art
                .sprite(canvas, icon, left + 18.0, 74.0, 36.0, 36.0, Color::WHITE);
            self.panel_text_left(canvas, &label, left + 40.0, 74.0, "s", width, Color::WHITE);
            if icon == "icon_B_light" {
                self.hit("close", FRect::new(left - 4.0, 50.0, width + 48.0, 48.0));
            }
            left += 40.0 + width + 30.0;
        }
    }

    pub(super) fn render_toolbar(&mut self, canvas: &mut Canvas<Window>) {
        let page = self.toolbar_page().unwrap_or(1).min(4);
        self.toolbar_frame(canvas, page);
        match page {
            0 => self.display_page(canvas),
            1 => self.options_page(canvas),
            2 => self.language_page(canvas),
            3 => self.legal_page(canvas),
            _ => self.manual_page(canvas),
        }
    }

    fn display_page(&mut self, canvas: &mut Canvas<Window>) {
        let nes = self.art.nes;
        for (index, x) in [280.0, 640.0, 1000.0].into_iter().enumerate() {
            let action = format!("display:{index}");
            if self.focus == index || self.hover == action {
                self.art.sprite(
                    canvas,
                    "cursor_option",
                    x,
                    if nes { 396.0 } else { 307.0 },
                    360.0,
                    if nes { 360.0 } else { 297.0 },
                    Color::WHITE,
                );
            }
            if nes {
                self.art
                    .sprite(canvas, "tv_frame", x, 366.0, 318.0, 192.0, Color::WHITE);
            } else {
                self.rect(
                    canvas,
                    FRect::new(x - 160.0, 180.0, 320.0, 180.0),
                    Color::BLACK,
                );
            }
            self.art.sprite(
                canvas,
                if index == 0 {
                    "screen_analog_preview"
                } else {
                    "screen_pixel_perfect_preview"
                },
                x,
                if nes { 366.0 } else { 270.0 },
                if index == 2 {
                    192.0
                } else if nes {
                    240.0
                } else {
                    220.0
                },
                if nes { 180.0 } else { 168.0 },
                Color::WHITE,
            );
            let label = self.art.translate(
                [
                    "sys_display_item_CRTFilter",
                    "sys_display_item_4_3",
                    "sys_display_item_DotByDot",
                ][index],
                ["CRT Filter", "4:3", "Pixel Perfect"][index],
            );
            self.art.text(
                canvas,
                &label,
                x,
                if nes { 493.0 } else { 385.0 },
                "m",
                Color::WHITE,
                310.0,
                true,
            );
            self.art.sprite(
                canvas,
                if self.display_mode() == index {
                    "radiobtn_on"
                } else {
                    "radiobtn_off"
                },
                x,
                if nes { 540.0 } else { 426.0 },
                36.0,
                36.0,
                Color::WHITE,
            );
            self.hit(
                &action,
                FRect::new(
                    x - 180.0,
                    if nes { 216.0 } else { 158.0 },
                    360.0,
                    if nes { 360.0 } else { 297.0 },
                ),
            );
            self.actions.push(action);
        }
        if nes {
            self.art.text(
                canvas,
                "CRT uses a scanline approximation.",
                640.0,
                642.0,
                "s",
                Color::RGB(180, 180, 180),
                1000.0,
                false,
            );
            return;
        }
        self.art.sprite(
            canvas,
            "dialog_line",
            640.0,
            471.0,
            1134.0,
            12.0,
            Color::WHITE,
        );
        let label = self.art.translate("sys_display_frames_Title", "Frame");
        self.art
            .text(canvas, &label, 640.0, 503.0, "m", Color::WHITE, 900.0, true);
        if self.focus == 3 {
            self.art.sprite(
                canvas,
                "cursor_option",
                640.0,
                591.0,
                975.0,
                123.0,
                Color::WHITE,
            );
        }
        self.rect(canvas, FRect::new(554.0, 542.0, 172.0, 98.0), Color::BLACK);
        self.art.text(
            canvas,
            "No frame",
            640.0,
            591.0,
            "s",
            Color::WHITE,
            160.0,
            true,
        );
        self.hit("frame:none", FRect::new(152.0, 529.0, 975.0, 123.0));
        self.actions.push("frame:none".into());
        self.art.text(
            canvas,
            "CRT uses a scanline approximation. Decorative frames are not imported yet.",
            640.0,
            655.0,
            "s",
            Color::RGB(180, 180, 180),
            1000.0,
            false,
        );
    }

    fn options_page(&mut self, canvas: &mut Canvas<Window>) {
        let heading = self.art.translate("sys_option_settings_Title", "Settings");
        self.art.text(
            canvas,
            &heading,
            640.0,
            144.0,
            "m",
            Color::WHITE,
            950.0,
            true,
        );
        for (index, key) in [
            "sys_option_settings_MyPlayDemo",
            "sys_option_settings_AutoPlayDemo",
            "sys_option_settings_Burn-inPrevention",
        ]
        .into_iter()
        .enumerate()
        {
            let y = 228.0 + index as f32 * 72.0;
            let label = self.art.translate(
                key,
                [
                    "My Game Play Demo",
                    "Classic Demo",
                    "Screen Burn-In Reduction",
                ][index],
            );
            self.panel_text_left(
                canvas,
                &label,
                424.0,
                y,
                "m",
                350.0,
                Color::RGB(155, 155, 155),
            );
            self.art
                .sprite(canvas, "switch_disable", 811.0, y, 90.0, 48.0, Color::WHITE);
        }
        self.art.text(
            canvas,
            "Console demo playback and burn-in controls are not implemented.",
            640.0,
            430.0,
            "s",
            Color::RGB(180, 180, 180),
            1000.0,
            false,
        );
        self.art.sprite(
            canvas,
            "dialog_line",
            640.0,
            468.0,
            1134.0,
            12.0,
            Color::WHITE,
        );
        self.art.text(
            canvas,
            "Emulator / Image Library",
            640.0,
            504.0,
            "m",
            Color::WHITE,
            950.0,
            true,
        );
        self.art.sprite(
            canvas,
            if self.focus == 0 {
                "btn_init_on"
            } else {
                "btn_init_off"
            },
            640.0,
            588.0,
            if self.focus == 0 { 456.0 } else { 432.0 },
            if self.focus == 0 { 96.0 } else { 72.0 },
            Color::WHITE,
        );
        self.art.text(
            canvas,
            "Open emulator settings",
            640.0,
            588.0,
            "m",
            Color::WHITE,
            400.0,
            false,
        );
        self.hit("emulator-settings", FRect::new(412.0, 540.0, 456.0, 96.0));
        self.actions.push("emulator-settings".into());
    }

    fn language_page(&mut self, canvas: &mut Canvas<Window>) {
        let locales: Vec<_> = self
            .language_choices()
            .into_iter()
            .map(str::to_owned)
            .collect();
        for (index, locale) in locales.iter().enumerate() {
            let (column, row) = if index < 4 {
                (0, index)
            } else if index < 8 {
                (1, index - 4)
            } else {
                (0, 4)
            };
            let x = 400.0 + column as f32 * 528.0;
            let y = 216.0 + row as f32 * 96.0;
            let action = format!("language:{locale}");
            if self.focus == index || self.hover == action {
                self.art
                    .sprite(canvas, "cursor_option", x, y, 432.0, 72.0, Color::WHITE);
            }
            self.art.sprite(
                canvas,
                if *locale == self.art.locale {
                    "radiobtn_on"
                } else {
                    "radiobtn_off"
                },
                x - 180.0,
                y,
                36.0,
                36.0,
                Color::WHITE,
            );
            let fallback = match locale.as_str() {
                "usa_en" => "English",
                "eur_fr" => "Français",
                "eur_de" => "Deutsch",
                "eur_es" => "Español",
                "eur_it" => "Italiano",
                "eur_nl" => "Nederlands",
                "eur_pt" => "Português (Europeu)",
                "eur_ru" => "Русский",
                _ => "Japanese",
            };
            let key = format!(
                "sys_language_{}_{}",
                locale[..3].to_ascii_uppercase(),
                &locale[4..]
            );
            let label = self.art.translate(&key, fallback);
            self.panel_text_left(canvas, &label, x - 120.0, y, "m", 322.0, Color::WHITE);
            self.hit(&action, FRect::new(x - 216.0, y - 36.0, 432.0, 72.0));
            self.actions.push(action);
        }
    }

    fn legal_lines(&self) -> Vec<String> {
        let text = if self.focus == 1 {
            "Classic Menu — Rust reconstruction\n\nares: embedded NES/SNES emulation cores (ISC).\nSDL3: windowing, input, rendering and audio (zlib).\nRust frontend: Clover reconstruction contributors (MIT).\nBarlow launcher fonts: SIL Open Font License.\nXZ/liblzma: compressed image decoding (see packaged notices).\n\nFull third-party and Rust dependency license notices are included in this application's Resources/licenses folder.\n\nThe console artwork, fonts and language data are imported locally from the image selected by the user. They are not distributed with the application.".to_owned()
        } else {
            let mut text = self.art.translate("sys_copyright_games_HeadText", "");
            for game in &self.games {
                if !text.is_empty() {
                    text.push_str("\n\n");
                }
                text.push_str(if game.copyright.is_empty() {
                    &game.title
                } else {
                    &game.copyright
                });
            }
            text.push_str(&self.art.translate("sys_copyright_games_FootText", ""));
            text
        };
        text.split('\n')
            .flat_map(|paragraph| {
                if paragraph.is_empty() {
                    vec![String::new()]
                } else {
                    wrap_text(&self.art, paragraph, "s", 900.0)
                }
            })
            .collect()
    }

    fn legal_page(&mut self, canvas: &mut Canvas<Window>) {
        self.art.sprite(
            canvas,
            "dialog_line",
            640.0,
            180.0,
            1134.0,
            12.0,
            Color::WHITE,
        );
        for (index, (x, key, fallback)) in [
            (412.0, "sys_copyright_games_Title", "IP Notice"),
            (868.0, "sys_copyright_oss_Title", "Open Source Software"),
        ]
        .into_iter()
        .enumerate()
        {
            if self.focus == index {
                self.art
                    .sprite(canvas, "cursor_option", x, 144.0, 408.0, 60.0, Color::WHITE);
            }
            let label = self.art.translate(key, fallback);
            self.art
                .text(canvas, &label, x, 144.0, "m", Color::WHITE, 392.0, true);
            let action = format!("legal:{index}");
            self.hit(&action, FRect::new(x - 204.0, 114.0, 408.0, 60.0));
            self.actions.push(action);
        }
        let lines = self.legal_lines();
        self.panel_scroll = self.panel_scroll.min(lines.len().saturating_sub(16));
        for (row, line) in lines.iter().skip(self.panel_scroll).take(16).enumerate() {
            self.panel_text_left(
                canvas,
                line,
                168.0,
                240.0 + row as f32 * 26.0,
                "s",
                900.0,
                Color::WHITE,
            );
        }
        self.art.sprite(
            canvas,
            "scrollbar_Area",
            1144.0,
            432.0,
            6.0,
            336.0,
            Color::WHITE,
        );
        let fraction = if lines.len() > 16 {
            self.panel_scroll as f32 / (lines.len() - 16) as f32
        } else {
            0.0
        };
        self.art.sprite(
            canvas,
            "scrollbar_full",
            1144.0,
            300.0 + fraction * 264.0,
            30.0,
            72.0,
            Color::WHITE,
        );
        self.art.sprite(
            canvas,
            if self.panel_scroll > 0 {
                "scrollbar_allowup_on"
            } else {
                "scrollbar_allowup_off"
            },
            1144.0,
            240.0,
            48.0,
            48.0,
            Color::WHITE,
        );
        self.art.sprite(
            canvas,
            if self.panel_scroll + 16 < lines.len() {
                "scrollbar_allowdown_on"
            } else {
                "scrollbar_allowdown_off"
            },
            1144.0,
            624.0,
            48.0,
            48.0,
            Color::WHITE,
        );
        self.hit("legal-up", FRect::new(1120.0, 216.0, 48.0, 48.0));
        self.hit("legal-down", FRect::new(1120.0, 600.0, 48.0, 48.0));
    }

    pub(super) fn scroll_legal(&mut self, down: bool) {
        self.panel_scroll = if down {
            (self.panel_scroll + 1).min(self.legal_lines().len().saturating_sub(16))
        } else {
            self.panel_scroll.saturating_sub(1)
        };
    }

    fn manual_page(&mut self, canvas: &mut Canvas<Window>) {
        let text = self.art.translate(
            "sys_manual_Page",
            "View the original manuals on your smart device.",
        );
        for (row, line) in wrap_text(&self.art, &text, "m", 1000.0).iter().enumerate() {
            self.art.text(
                canvas,
                line,
                640.0,
                164.0 + row as f32 * 31.0,
                "m",
                Color::WHITE,
                1000.0,
                false,
            );
        }
        self.art
            .sprite(canvas, "qr_code1", 640.0, 393.0, 344.0, 344.0, Color::WHITE);
        let url = self.art.translate("sys_manual_URL", "");
        self.art
            .text(canvas, &url, 640.0, 602.0, "m", Color::WHITE, 1050.0, false);
        self.art.text(
            canvas,
            "Emulator controls (H)",
            640.0,
            648.0,
            "s",
            Color::RGB(180, 180, 180),
            900.0,
            false,
        );
        self.hit("help", FRect::new(440.0, 628.0, 400.0, 38.0));
        self.actions.push("help".into());
    }

    pub(super) fn toolbar_key(&mut self, event: KeyEvent, canvas: &Canvas<Window>) -> bool {
        let Some(page) = self.toolbar_page() else {
            return false;
        };
        let page0_display_up = page == 0 && event.key == Key::Up && !self.art.nes;
        let page2_direction = page == 2
            && matches!(
                event.key,
                Key::Left | Key::Right | Key::Up | Key::Down | Key::Next
            );
        let page3_scroll_down = page == 3 && event.key == Key::Down;
        let plan = navigation::transition(
            navigation::State {
                focus: self.focus,
                scroll: self.panel_scroll,
            },
            navigation::Context {
                page,
                nes: self.art.nes,
                display_mode: if page0_display_up {
                    self.display_mode()
                } else {
                    0
                },
                language_count: if page2_direction {
                    self.language_choices().len()
                } else {
                    0
                },
                legal_line_count: if page3_scroll_down {
                    self.legal_lines().len()
                } else {
                    0
                },
            },
            event,
        );
        self.focus = plan.state.focus;
        self.panel_scroll = plan.state.scroll;
        match plan.effect {
            None => {}
            Some(navigation::Effect::DismissPanel) => self.panel.clear(),
            Some(navigation::Effect::Help) => self.activate("help", canvas),
            Some(navigation::Effect::ActivateClose) => self.activate("close", canvas),
            Some(navigation::Effect::FrameNone) => self.activate("frame:none", canvas),
            Some(navigation::Effect::DisplayMode(mode)) => {
                self.activate(&format!("display:{mode}"), canvas)
            }
            Some(navigation::Effect::EmulatorSettings) => {
                self.activate("emulator-settings", canvas)
            }
            Some(navigation::Effect::ChooseLanguage(index)) => {
                let action = self
                    .language_choices()
                    .get(index)
                    .map(|locale| format!("language:{locale}"))
                    .unwrap_or_else(|| "close".into());
                self.activate(&action, canvas);
            }
        }
        true
    }
}
