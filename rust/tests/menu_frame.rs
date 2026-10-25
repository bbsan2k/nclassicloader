use clover_portable::frontend::{
    font::{PlacedGlyph, TextLayout},
    menu_carousel::{self, ArrowAction, ArrowHit, ArrowScene, CardInput, CardScene, ResumeState},
    menu_chrome::{self, HudAction, HudItem, HudPrompt, HudScene, ThumbnailScene},
    menu_frame::*,
    menu_scene::{self, AtlasSprite, Command, ImageSource, Rect, Rgba, TextureId},
    menu_toolbar,
};
use std::collections::BTreeMap;

fn rect(x: f32) -> Rect {
    Rect {
        x,
        y: 0.0,
        w: 1.0,
        h: 1.0,
    }
}
fn rect_xy(x: f32, y: f32) -> Rect {
    Rect {
        x,
        y,
        w: 1.0,
        h: 1.0,
    }
}

#[test]
fn last_hit_uses_reverse_priority_half_open_bounds_and_preserves_identity() {
    let hits = [rect(0.0), rect(0.5)];
    let hit = last_hit(&hits, 0.75, 0.5, |r| *r).unwrap();
    assert!(std::ptr::eq(hit, &hits[1]));
    assert!(last_hit(&hits, 1.5, 0.5, |r| *r).is_none());
    assert!(last_hit(&hits, 0.0, 0.0, |r| *r).is_some());
    assert!(last_hit::<Rect>(&[], 0.0, 0.0, |r| *r).is_none());

    let odd = [
        Rect {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 1.0,
        },
        Rect {
            x: 0.0,
            y: 0.0,
            w: -1.0,
            h: 1.0,
        },
    ];
    assert!(last_hit(&odd, 0.0, 0.5, |r| *r).is_none());
    assert!(last_hit(&hits, f32::NAN, 0.5, |r| *r).is_none());
    assert!(last_hit(&hits, 0.5, f32::NAN, |r| *r).is_none());
}

#[test]
fn toolbar_combination_requires_generation_and_appends_owned_steps() {
    let id = TextureId {
        generation: 71,
        slot: 4,
    };
    let base = BaseMenuScene {
        generation: 71,
        steps: vec![Step::Draw(Command::Texture(menu_scene::TextureQuad {
            texture: id,
            destination: rect(1.0),
            source: None,
            tint: Rgba {
                r: 1,
                g: 2,
                b: 3,
                a: 4,
            },
            filter: menu_scene::Filter::Nearest,
            flip_x: false,
        }))],
    };
    let toolbar = menu_toolbar::ToolbarScene {
        generation: 71,
        steps: vec![Step::Hit(Hit {
            action: HitAction::Toolbar(menu_toolbar::ToolbarAction::Manual),
            rect: rect(9.0),
        })],
    };
    let combined = with_toolbar(base.clone(), toolbar.clone()).unwrap();
    assert_eq!(combined.generation, 71);
    assert_eq!(combined.steps.len(), 2);
    assert!(matches!(&combined.steps[0], Step::Draw(Command::Texture(q)) if q.texture == id));
    assert!(
        matches!(&combined.steps[1], Step::Hit(Hit { action: HitAction::Toolbar(menu_toolbar::ToolbarAction::Manual), rect: r }) if r.x == 9.0)
    );
    assert!(
        with_toolbar(
            base,
            menu_toolbar::ToolbarScene {
                generation: 72,
                ..toolbar
            }
        )
        .is_none()
    );
}
fn cmd(x: f32) -> Command {
    Command::Fill {
        rect: rect(x),
        color: super_color(),
    }
}
fn super_color() -> clover_portable::frontend::menu_scene::Rgba {
    clover_portable::frontend::menu_scene::Rgba {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    }
}

#[test]
fn compose_preserves_order_and_owned_game_keys() {
    let scene = compose(BaseMenuInput {
        generation: 7,
        wallpaper: vec![cmd(0.0)],
        title: vec![cmd(1.0)],
        cards: vec![
            (
                GameKey("stable".into()),
                CardScene {
                    commands: vec![cmd(2.0)],
                    hit: rect(3.0),
                },
            ),
            (
                GameKey("second".into()),
                CardScene {
                    commands: vec![cmd(12.0)],
                    hit: rect(13.0),
                },
            ),
        ],
        selection: vec![cmd(4.0)],
        arrows: ArrowScene {
            commands: vec![cmd(5.0)],
            hits: [
                ArrowHit {
                    action: ArrowAction::Previous,
                    rect: rect(6.0),
                },
                ArrowHit {
                    action: ArrowAction::Next,
                    rect: rect(7.0),
                },
            ],
        },
        thumbnails: vec![
            (
                GameKey("stable".into()),
                ThumbnailScene {
                    commands: vec![],
                    hit: rect(8.0),
                },
            ),
            (
                GameKey("thumb".into()),
                ThumbnailScene {
                    commands: vec![cmd(14.0)],
                    hit: rect(15.0),
                },
            ),
        ],
        hud: HudScene {
            items: std::array::from_fn(|i| HudItem {
                commands: if i == 2 { vec![cmd(9.0)] } else { vec![] },
                hit: rect(10.0),
                action: if i == 2 {
                    HudAction::Sort
                } else {
                    HudAction::Start
                },
            }),
        },
        bars: vec![cmd(11.0)],
    });
    assert_eq!(scene.generation, 7);
    assert_eq!(scene.steps.len(), 19);
    let order: Vec<_> = scene
        .steps
        .iter()
        .map(|step| match step {
            Step::Draw(Command::Fill { rect, .. }) => format!("D{}", rect.x),
            Step::Draw(Command::Texture(_)) => "T".into(),
            Step::Hit(Hit {
                action: HitAction::Game(key),
                rect,
            }) => format!("G{}:{}", key.0, rect.x),
            Step::Hit(Hit {
                action: HitAction::Arrow(ArrowAction::Previous),
                ..
            }) => "P".into(),
            Step::Hit(Hit {
                action: HitAction::Arrow(ArrowAction::Next),
                ..
            }) => "N".into(),
            Step::Hit(Hit {
                action: HitAction::Hud(action),
                ..
            }) => format!("H{action:?}"),
            Step::Hit(Hit {
                action: HitAction::Toolbar(action),
                ..
            }) => format!("B{action:?}"),
        })
        .collect();
    assert_eq!(
        order,
        [
            "D0",
            "D1",
            "D2",
            "Gstable:3",
            "D12",
            "Gsecond:13",
            "D4",
            "D5",
            "P",
            "N",
            "Gstable:8",
            "D14",
            "Gthumb:15",
            "HStart",
            "HStart",
            "D9",
            "HSort",
            "HStart",
            "D11"
        ]
    );
    assert!(
        matches!(&scene.steps[3], Step::Hit(Hit { action: HitAction::Game(GameKey(key)), .. }) if key == "stable")
    );
    assert!(
        matches!(scene.steps.last(), Some(Step::Draw(Command::Fill { rect, .. })) if rect.x == 11.0)
    );
}

fn typed_id(generation: usize, slot: usize) -> TextureId {
    TextureId { generation, slot }
}

fn atlas(generation: usize, slot: usize) -> AtlasSprite {
    AtlasSprite {
        texture: typed_id(generation, slot),
        source: rect_xy(slot as f32, 2.0),
        flip_x: false,
    }
}

fn layout() -> TextLayout {
    TextLayout {
        glyphs: vec![PlacedGlyph {
            font: "s".into(),
            source: rect_xy(1.0, 2.0),
            x: 0.0,
            y: 0.0,
            width: 4.0,
            height: 6.0,
        }],
        advance: 8.0,
        line: 10.0,
        left: 0.0,
        top: 0.0,
        right: 4.0,
        bottom: 6.0,
    }
}

fn public_scene(nes: bool, suspend: bool, generation: usize) -> BaseMenuScene {
    let shift = if suspend { -69.0 } else { 0.0 };
    let font_ids = BTreeMap::from([("s".to_owned(), typed_id(generation, 90))]);
    let title = menu_carousel::title_scene(
        nes,
        if suspend { -52.0 } else { 0.0 },
        &layout(),
        &font_ids,
        Some(atlas(generation, 91)),
        Rgba {
            r: 1,
            g: 2,
            b: 3,
            a: 255,
        },
    );
    let card_plan = menu_carousel::card(CardInput {
        nes,
        x: 640.0,
        shift,
        active: true,
        firmware_locked: false,
        players: 2,
        simultaneous: false,
        slots: [ResumeState::Empty; 4],
    });
    let sprites = card_plan
        .parts
        .iter()
        .enumerate()
        .filter_map(|(i, part)| match part {
            menu_carousel::CardPart::Sprite(p) => Some((p.name.clone(), atlas(generation, i))),
            menu_carousel::CardPart::Cover { .. } => None,
        })
        .collect();
    let card = menu_carousel::card_scene(
        &card_plan,
        &menu_carousel::CardResources {
            sprites,
            cover: Some(ImageSource {
                texture: typed_id(generation, 92),
                width: 64,
                height: 32,
            }),
        },
    );
    let selection_plan = menu_scene::selection(
        nes,
        Rect {
            x: 640.0,
            y: 360.0,
            w: 278.0,
            h: 302.0,
        },
        0.5,
    );
    let selection_sources = selection_plan
        .iter()
        .enumerate()
        .map(|(i, p)| (p.name.clone(), atlas(generation, i + 30)))
        .collect();
    let selection = menu_scene::sprite_scene(&selection_plan, &selection_sources);
    let arrows = menu_carousel::arrows(
        shift,
        Some(atlas(generation, 70)),
        Some(atlas(generation, 71)),
    );
    let thumb_plans = menu_scene::thumbnails(2, 0);
    let thumbnails = thumb_plans
        .into_iter()
        .enumerate()
        .map(|(i, p)| {
            let cover = ImageSource {
                texture: typed_id(generation, 100 + i),
                width: 32,
                height: 48,
            };
            (
                GameKey(format!("thumb{i}")),
                menu_chrome::thumbnail_scene(
                    p,
                    Some(cover),
                    p.marker.map(|_| atlas(generation, 80)),
                    None,
                ),
            )
        })
        .collect();
    let prompts = std::array::from_fn(|_| HudPrompt {
        icon: Some(atlas(generation, 110)),
        icon_width: 30.0,
        layout: layout(),
    });
    let hud = menu_chrome::hud_scene(
        nes,
        8.0,
        24.0,
        &prompts,
        &font_ids,
        Rgba {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        },
    );
    let bars = menu_chrome::menu_bars(
        nes,
        false,
        Some(atlas(generation, 120)),
        Some(atlas(generation, 121)),
        Some(atlas(generation, 122)),
    );
    compose(BaseMenuInput {
        generation,
        wallpaper: menu_scene::wallpaper(
            nes,
            Rgba {
                r: 0,
                g: 1,
                b: 2,
                a: 255,
            },
            Some(typed_id(generation, 1)),
            3.0,
        ),
        title,
        cards: vec![(GameKey("card".into()), card)],
        selection,
        arrows,
        thumbnails,
        hud,
        bars,
    })
}

#[test]
fn public_builders_preserve_system_suspend_geometry_hits_and_generation_resources() {
    let normal = public_scene(false, false, 22);
    let suspended = public_scene(false, true, 22);
    let nes = public_scene(true, false, 23);
    assert_eq!(normal.generation, 22);
    assert_eq!(suspended.generation, 22);
    assert_eq!(nes.generation, 23);
    assert!(
        matches!(normal.steps[0], Step::Draw(Command::Texture(q)) if q.texture == typed_id(22, 1))
    );
    assert!(matches!(nes.steps[0], Step::Draw(Command::Fill { .. })));
    let title_y = |scene: &BaseMenuScene| {
        scene
            .steps
            .iter()
            .find_map(|step| match step {
                Step::Draw(Command::Texture(q)) if q.texture == typed_id(22, 91) => {
                    Some(q.destination.y)
                }
                _ => None,
            })
            .unwrap()
    };
    assert_eq!(title_y(&normal), 145.0);
    assert_eq!(title_y(&suspended), 93.0);
    let card_top_left = |scene: &BaseMenuScene, generation| {
        scene
            .steps
            .iter()
            .find_map(|step| match step {
                Step::Draw(Command::Texture(q)) if q.texture == typed_id(generation, 0) => {
                    Some(q.destination)
                }
                _ => None,
            })
            .unwrap()
    };
    assert_eq!(
        card_top_left(&normal, 22),
        Rect {
            x: 514.0,
            y: 222.0,
            w: 252.0,
            h: 276.0
        }
    );
    assert_eq!(
        card_top_left(&nes, 23),
        Rect {
            x: 520.0,
            y: 228.0,
            w: 246.0,
            h: 270.0
        }
    );
    let normal_card = normal
        .steps
        .iter()
        .find_map(|step| match step {
            Step::Hit(Hit {
                action: HitAction::Game(GameKey(key)),
                rect,
            }) if key == "card" => Some(*rect),
            _ => None,
        })
        .unwrap();
    let suspended_card = suspended
        .steps
        .iter()
        .find_map(|step| match step {
            Step::Hit(Hit {
                action: HitAction::Game(GameKey(key)),
                rect,
            }) if key == "card" => Some(*rect),
            _ => None,
        })
        .unwrap();
    assert_eq!(normal_card.y, 222.0);
    assert_eq!(suspended_card.y, 153.0);
    let hud_icon_y = |scene: &BaseMenuScene, generation| {
        scene
            .steps
            .iter()
            .find_map(|step| match step {
                Step::Draw(Command::Texture(q)) if q.texture == typed_id(generation, 110) => {
                    Some(q.destination.y)
                }
                _ => None,
            })
            .unwrap()
    };
    assert_eq!(hud_icon_y(&normal, 22), 594.0);
    assert_eq!(hud_icon_y(&nes, 23), 597.0);
    let arrow_hits = |scene: &BaseMenuScene| {
        scene
            .steps
            .iter()
            .filter_map(|step| match step {
                Step::Hit(Hit {
                    action: HitAction::Arrow(action),
                    rect,
                }) => Some((*action, *rect)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(arrow_hits(&normal), arrow_hits(&suspended));
    assert_eq!(
        arrow_hits(&normal)[0].1,
        Rect {
            x: 0.0,
            y: 240.0,
            w: 65.0,
            h: 255.0
        }
    );
    assert_eq!(
        normal
            .steps
            .iter()
            .filter(|s| matches!(
                s,
                Step::Hit(Hit {
                    action: HitAction::Hud(_),
                    ..
                })
            ))
            .count(),
        4
    );
    assert!(
        normal
            .steps
            .iter()
            .any(|s| matches!(s, Step::Draw(Command::Texture(q)) if q.texture.generation == 22))
    );
}

#[test]
fn composed_scene_owns_keys_and_stale_texture_ids_without_resource_normalization() {
    let mut input = BaseMenuInput {
        generation: 31,
        wallpaper: vec![Command::Texture(menu_scene::TextureQuad {
            texture: typed_id(2, 999),
            destination: rect_xy(1.0, 0.0),
            source: None,
            tint: Rgba {
                r: 1,
                g: 2,
                b: 3,
                a: 4,
            },
            filter: menu_scene::Filter::Nearest,
            flip_x: false,
        })],
        title: vec![],
        cards: vec![(
            GameKey("captured".into()),
            CardScene {
                commands: vec![],
                hit: rect(7.0),
            },
        )],
        selection: vec![],
        arrows: menu_carousel::arrows(0.0, None, None),
        thumbnails: vec![(
            GameKey("thumb".into()),
            ThumbnailScene {
                commands: vec![],
                hit: rect(8.0),
            },
        )],
        hud: menu_chrome::hud_scene(
            true,
            8.0,
            24.0,
            &std::array::from_fn(|_| HudPrompt {
                icon: None,
                icon_width: 30.0,
                layout: layout(),
            }),
            &BTreeMap::new(),
            Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            },
        ),
        bars: vec![],
    };
    let scene = compose(input.clone());
    input.cards[0].0.0.replace_range(.., "reordered-catalog");
    input.generation = 99;
    assert_eq!(scene.generation, 31);
    assert!(
        matches!(scene.steps[0], Step::Draw(Command::Texture(q)) if q.texture == typed_id(2, 999))
    );
    assert!(
        matches!(&scene.steps[1], Step::Hit(Hit { action: HitAction::Game(GameKey(key)), rect }) if key == "captured" && rect.x == 7.0)
    );
    assert!(scene.steps.iter().any(|step| matches!(step, Step::Hit(Hit { action: HitAction::Game(GameKey(key)), rect }) if key == "thumb" && rect.x == 8.0)));
    assert!(scene.steps.iter().any(|s| matches!(
        s,
        Step::Hit(Hit {
            action: HitAction::Arrow(ArrowAction::Previous),
            ..
        })
    )));
    assert!(scene.steps.iter().any(|s| matches!(
        s,
        Step::Hit(Hit {
            action: HitAction::Hud(HudAction::Settings),
            ..
        })
    )));
}
