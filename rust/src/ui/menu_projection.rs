use super::*;

impl App {
    pub(super) fn project_base_menu(
        &self,
        motion: crate::frontend::menu_motion::MotionPlan,
        suspend_layout: bool,
    ) -> crate::frontend::menu_frame::BaseMenuInput {
        let title_shift = motion.title_shift;
        let card_shift = motion.card_shift;
        let offset = motion.wallpaper_offset;
        let background = if self.art.nes {
            self.art.palette_color(
                "background",
                if self.art.region == "Japan" {
                    Color::RGB(235, 226, 213)
                } else {
                    Color::RGB(0, 21, 61)
                },
            )
        } else {
            Color::WHITE
        };
        let wall = self.art.texture_bindings.id("skin/wall.png");
        let wallpaper = crate::frontend::menu_scene::wallpaper(
            self.art.nes,
            crate::frontend::menu_scene::Rgba {
                r: background.r,
                g: background.g,
                b: background.b,
                a: background.a,
            },
            wall,
            offset,
        );
        let mut frame = crate::frontend::menu_frame::BaseMenuInput {
            generation: self.art.texture_bindings.generation(),
            wallpaper,
            title: Vec::new(),
            cards: Vec::new(),
            selection: Vec::new(),
            arrows: crate::frontend::menu_carousel::ArrowScene {
                commands: Vec::new(),
                hits: [
                    crate::frontend::menu_carousel::ArrowHit {
                        action: crate::frontend::menu_carousel::ArrowAction::Previous,
                        rect: crate::frontend::menu_scene::Rect {
                            x: 0.0,
                            y: 0.0,
                            w: 0.0,
                            h: 0.0,
                        },
                    },
                    crate::frontend::menu_carousel::ArrowHit {
                        action: crate::frontend::menu_carousel::ArrowAction::Next,
                        rect: crate::frontend::menu_scene::Rect {
                            x: 0.0,
                            y: 0.0,
                            w: 0.0,
                            h: 0.0,
                        },
                    },
                ],
            },
            thumbnails: Vec::new(),
            hud: crate::frontend::menu_chrome::HudScene {
                items: std::array::from_fn(|_| crate::frontend::menu_chrome::HudItem {
                    commands: Vec::new(),
                    hit: crate::frontend::menu_scene::Rect {
                        x: 0.0,
                        y: 0.0,
                        w: 0.0,
                        h: 0.0,
                    },
                    action: crate::frontend::menu_chrome::HudAction::Start,
                }),
            },
            bars: Vec::new(),
        };

        let title = self.selected().title.clone();
        let title_layout = self.art.measure(&title, "title");
        frame.title = crate::frontend::menu_carousel::title_scene(
            self.art.nes,
            title_shift,
            &title_layout,
            &self.art.font_atlases(),
            self.art.sprite_source("caption_title"),
            crate::frontend::menu_scene::Rgba {
                r: self.art.title_color.r,
                g: self.art.title_color.g,
                b: self.art.title_color.b,
                a: self.art.title_color.a,
            },
        );
        let gap = self
            .art
            .layout
            .get("cardSpacing")
            .and_then(Value::as_f64)
            .unwrap_or(300.0) as f32;
        for i in self.scroll_current.floor() as i32 - 3..=self.scroll_current.floor() as i32 + 3 {
            let index = i.rem_euclid(self.games.len() as i32) as usize;
            let game = self.games[index].clone();
            let firmware_locked = !self.firmware_ready(&game) || self.rom_path(&game).is_none();
            let x = (640.0 + (i as f32 - self.scroll_current) * gap).round();
            if !(-150.0..=1430.0).contains(&x) {
                continue;
            }
            let active = index == self.selected;
            let slots = std::array::from_fn(|slot| {
                let saved = self.saved.contains(&(game.id.clone(), slot));
                let locked = saved
                    && self
                        .preferences
                        .pointer(&format!("/suspendLocks/{}/{slot}", game.id))
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                if locked {
                    crate::frontend::menu_carousel::ResumeState::Locked
                } else if saved {
                    crate::frontend::menu_carousel::ResumeState::Saved
                } else {
                    crate::frontend::menu_carousel::ResumeState::Empty
                }
            });
            let plan =
                crate::frontend::menu_carousel::card(crate::frontend::menu_carousel::CardInput {
                    nes: self.art.nes,
                    x,
                    shift: card_shift,
                    active,
                    firmware_locked,
                    players: game.players,
                    simultaneous: game.simultaneous,
                    slots,
                });
            let resources = self
                .art
                .card_resources(&plan, &format!("games/{}/{}.png", game.id, game.id));
            let scene = crate::frontend::menu_carousel::card_scene(&plan, &resources);
            frame
                .cards
                .push((crate::frontend::menu_frame::GameKey(game.id.clone()), scene));
        }
        if self.menu_focus == MenuFocus::Games && !suspend_layout {
            let plan = crate::frontend::menu_scene::selection(
                self.art.nes,
                crate::frontend::menu_scene::Rect {
                    x: 640.0 + (self.scroll_to - self.scroll_current) * gap,
                    y: 360.0,
                    w: 278.0,
                    h: 302.0,
                },
                motion.selection_time,
            );
            let mut sources = BTreeMap::new();
            for placement in &plan {
                if let Some(source) = self.art.sprite_source(&placement.name) {
                    sources.insert(placement.name.clone(), source);
                }
            }
            frame.selection = crate::frontend::menu_scene::sprite_scene(&plan, &sources);
        }
        frame.arrows = crate::frontend::menu_carousel::arrows(
            card_shift,
            self.art.sprite_source("arrow_left"),
            self.art.sprite_source("arrow_right"),
        );
        let thumbnails = crate::frontend::menu_scene::thumbnails(self.games.len(), self.selected);
        for thumbnail in thumbnails {
            let game = self.games[thumbnail.index].clone();
            let cover = self
                .art
                .image_source(&format!("games/{}/{}_small.png", game.id, game.id));
            let marker = thumbnail
                .marker
                .and_then(|_| self.art.sprite_source("curor_thumbnail_1"));
            let lock = if self.firmware_ready(&game) {
                None
            } else {
                self.art.sprite_source("icon_lock")
            };
            let scene =
                crate::frontend::menu_chrome::thumbnail_scene(thumbnail, cover, marker, lock);
            frame
                .thumbnails
                .push((crate::frontend::menu_frame::GameKey(game.id.clone()), scene));
        }
        let prompts = [
            (
                "icon_direction_up_light",
                self.art.translate("sys_gametitlelist_hud_Menu", "Menu"),
                if self.art.nes { 30.0 } else { 36.0 },
            ),
            (
                "icon_direction_down_light",
                self.art.translate("sys_gametitlelist_hud_Resume", "Resume"),
                if self.art.nes { 30.0 } else { 36.0 },
            ),
            (
                "icon_select_light",
                self.art.translate("sys_gametitlelist_hud_Sort", "Sort"),
                if self.art.nes { 90.0 } else { 96.0 },
            ),
            (
                "icon_start_light",
                self.art.translate("sys_gametitlelist_hud_Decide", "Start"),
                if self.art.nes { 78.0 } else { 84.0 },
            ),
        ];
        let item_gap = self
            .art
            .layout
            .get("hudItemGap")
            .and_then(Value::as_f64)
            .unwrap_or(8.0) as f32;
        let group_gap = self
            .art
            .layout
            .get("hudGroupGap")
            .and_then(Value::as_f64)
            .unwrap_or(24.0) as f32;
        let hud_prompts = std::array::from_fn(|index| {
            let (icon, label, icon_width) = &prompts[index];
            crate::frontend::menu_chrome::HudPrompt {
                icon: self.art.sprite_source(icon),
                icon_width: *icon_width,
                layout: self.art.measure(label, "s"),
            }
        });
        frame.hud = crate::frontend::menu_chrome::hud_scene(
            self.art.nes,
            item_gap,
            group_gap,
            &hud_prompts,
            &self.art.font_atlases(),
            crate::frontend::menu_scene::Rgba {
                r: self.art.hud_color.r,
                g: self.art.hud_color.g,
                b: self.art.hud_color.b,
                a: self.art.hud_color.a,
            },
        );
        let overlay = (self.art.nes && self.art.region != "Japan")
            .then(|| self.art.sprite_source("menubar_U_2"))
            .flatten();
        frame.bars = crate::frontend::menu_chrome::menu_bars(
            self.art.nes,
            self.art.region == "Japan",
            self.art.sprite_source("menubar_U"),
            self.art.sprite_source("menubar_L"),
            overlay,
        );
        frame
    }
}
