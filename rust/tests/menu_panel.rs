use clover_portable::frontend::menu_panel::{
    FooterScene, PanelInput, PanelKey, PanelStep, RowViewport, compose_panel, footer, row_viewport,
};
use clover_portable::frontend::{
    font::{PlacedGlyph, Rect},
    menu_panel,
    menu_scene::{AtlasSprite, Command, Filter, Rgba, TextureId, TextureQuad},
};
use std::collections::BTreeMap;

#[test]
fn empty_and_focus_at_entry_count() {
    assert_eq!(
        row_viewport(0, 4, 227.0),
        RowViewport {
            focus: 0,
            visible_count: 0,
            first_row: 0,
            row_height: 43.0,
        }
    );
    assert_eq!(
        row_viewport(3, 3, 227.0),
        RowViewport {
            focus: 3,
            visible_count: 3,
            first_row: 0,
            row_height: 43.0,
        }
    );
}

#[test]
fn focus_past_end_is_clamped() {
    assert_eq!(row_viewport(3, 8, 227.0).focus, 3);
}

#[test]
fn large_catalog_scrolls_focus_into_visible_window() {
    assert_eq!(
        row_viewport(100, 99, 227.0),
        RowViewport {
            focus: 99,
            visible_count: 12,
            first_row: 88,
            row_height: 25.25,
        }
    );
}

#[test]
fn little_vertical_room_keeps_one_row_and_minimum_height() {
    assert_eq!(
        row_viewport(5, 4, 529.0),
        RowViewport {
            focus: 4,
            visible_count: 1,
            first_row: 4,
            row_height: 24.0,
        }
    );
}

#[test]
fn representative_float_arithmetic_is_preserved() {
    let viewport = row_viewport(4, 2, 391.5);
    assert_eq!(viewport.visible_count, 4);
    assert_eq!(viewport.first_row, 0);
    assert_eq!(viewport.row_height, 34.625);
}

fn label() -> clover_portable::frontend::font::TextLayout {
    clover_portable::frontend::font::TextLayout {
        glyphs: vec![PlacedGlyph {
            font: "m".into(),
            source: Rect {
                x: 1.0,
                y: 2.0,
                w: 3.0,
                h: 4.0,
            },
            x: 0.0,
            y: 0.0,
            width: 3.0,
            height: 4.0,
        }],
        advance: 10.0,
        line: 8.0,
        left: 0.0,
        top: 0.0,
        right: 10.0,
        bottom: 8.0,
    }
}

#[test]
fn highlighted_row_orders_highlight_then_text_and_always_returns_hit() {
    let atlas = TextureId {
        generation: 2,
        slot: 3,
    };
    let font = TextureId {
        generation: 2,
        slot: 4,
    };
    let sprite = AtlasSprite {
        texture: atlas,
        source: Rect {
            x: 5.0,
            y: 6.0,
            w: 7.0,
            h: 8.0,
        },
        flip_x: true,
    };
    let mut atlases = BTreeMap::new();
    atlases.insert("m".into(), font);
    let scene = menu_panel::row(100.0, 40.0, true, Some(sprite), &label(), &atlases);
    assert_eq!(
        scene.hit,
        Rect {
            x: 365.0,
            y: 86.0,
            w: 550.0,
            h: 40.0
        }
    );
    assert_eq!(
        scene.commands,
        vec![
            Command::Texture(TextureQuad {
                texture: atlas,
                destination: Rect {
                    x: 355.0,
                    y: 87.0,
                    w: 570.0,
                    h: 39.0
                },
                source: Some(sprite.source),
                tint: Rgba {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 255
                },
                filter: Filter::Nearest,
                flip_x: true,
            }),
            Command::Texture(TextureQuad {
                texture: font,
                destination: Rect {
                    x: 635.0,
                    y: 102.0,
                    w: 3.0,
                    h: 4.0
                },
                source: Some(Rect {
                    x: 1.0,
                    y: 2.0,
                    w: 3.0,
                    h: 4.0
                }),
                tint: Rgba {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 255
                },
                filter: Filter::Linear,
                flip_x: false,
            }),
        ]
    );
}

#[test]
fn absent_or_ignored_sprites_and_missing_font_only_omit_draws() {
    let atlas = TextureId {
        generation: 1,
        slot: 1,
    };
    let sprite = AtlasSprite {
        texture: atlas,
        source: Rect {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        },
        flip_x: false,
    };
    let empty = BTreeMap::new();
    let missing_font = menu_panel::row(30.0, 24.0, true, Some(sprite), &label(), &empty);
    assert_eq!(missing_font.commands.len(), 1);
    let unhighlighted = menu_panel::row(30.0, 24.0, false, Some(sprite), &label(), &empty);
    assert!(unhighlighted.commands.is_empty());
    assert_eq!(
        unhighlighted.hit,
        Rect {
            x: 365.0,
            y: 16.0,
            w: 550.0,
            h: 24.0
        }
    );
}

#[test]
fn footer_draws_icon_then_label_and_enabled_hit() {
    let icon_id = TextureId {
        generation: 4,
        slot: 1,
    };
    let font_id = TextureId {
        generation: 4,
        slot: 2,
    };
    let icon = AtlasSprite {
        texture: icon_id,
        source: Rect {
            x: 2.0,
            y: 3.0,
            w: 8.0,
            h: 9.0,
        },
        flip_x: true,
    };
    let mut atlases = BTreeMap::new();
    atlases.insert("m".into(), font_id);
    let scene = footer(&label(), &atlases, Some(icon), true);
    assert_eq!(
        scene.hit,
        Some(Rect {
            x: 603.0,
            y: 543.0,
            w: 74.0,
            h: 48.0
        })
    );
    assert_eq!(
        scene.commands,
        vec![
            Command::Texture(TextureQuad {
                texture: icon_id,
                destination: Rect {
                    x: 615.0,
                    y: 549.0,
                    w: 36.0,
                    h: 36.0
                },
                source: Some(icon.source),
                tint: Rgba {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 255
                },
                filter: Filter::Nearest,
                flip_x: true,
            }),
            Command::Texture(TextureQuad {
                texture: font_id,
                destination: Rect {
                    x: 655.0,
                    y: 563.0,
                    w: 3.0,
                    h: 4.0
                },
                source: Some(Rect {
                    x: 1.0,
                    y: 2.0,
                    w: 3.0,
                    h: 4.0
                }),
                tint: Rgba {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 255
                },
                filter: Filter::Linear,
                flip_x: false,
            }),
        ]
    );
}

#[test]
fn disabled_footer_still_draws_and_missing_resources_only_skip_draws() {
    let icon = AtlasSprite {
        texture: TextureId {
            generation: 1,
            slot: 2,
        },
        source: Rect {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        },
        flip_x: false,
    };
    let empty = BTreeMap::new();
    let disabled = footer(&label(), &empty, Some(icon), false);
    assert_eq!(disabled.commands.len(), 1);
    assert_eq!(disabled.hit, None);
    let absent = footer(&label(), &empty, None, true);
    assert!(absent.commands.is_empty());
    assert_eq!(
        absent.hit,
        Some(Rect {
            x: 603.0,
            y: 543.0,
            w: 74.0,
            h: 48.0
        })
    );
}

#[test]
fn panel_text_uses_message_and_regular_body_spacing_and_anchor_policy() {
    let id = TextureId {
        generation: 9,
        slot: 5,
    };
    let mut atlases = BTreeMap::new();
    atlases.insert("m".into(), id);
    let heading = label();
    let body = [label(), label()];
    let normal = menu_panel::text_scene(&heading, &body, false, &atlases);
    let message = menu_panel::text_scene(&heading, &body, true, &atlases);
    assert_eq!(normal.heading.len(), 1);
    assert_eq!(normal.body.len(), 2);
    assert_eq!(normal.rows_y, 303.0);
    assert_eq!(message.rows_y, 291.0);
    let Command::Texture(normal_first) = normal.body[0] else {
        panic!()
    };
    let Command::Texture(normal_second) = normal.body[1] else {
        panic!()
    };
    let Command::Texture(message_first) = message.body[0] else {
        panic!()
    };
    assert_eq!(normal_first.destination.y, 223.0);
    assert_eq!(normal_second.destination.y, 261.0);
    assert_eq!(message_first.destination.y, 223.0);
    assert_eq!(normal_first.destination.x, 635.0);
    assert_eq!(message_first.destination.x, 635.0);
    assert_eq!(normal_first.texture, id);
}

#[test]
fn panel_text_empty_or_unbound_glyphs_keep_rows_start_independent() {
    let atlases = BTreeMap::new();
    let heading = label();
    let body = [label(), label(), label()];
    let scene = menu_panel::text_scene(&heading, &body, true, &atlases);
    assert!(scene.heading.is_empty());
    assert!(scene.body.is_empty());
    assert_eq!(scene.rows_y, 323.0);
    let empty = menu_panel::text_scene(&heading, &[], false, &atlases);
    assert_eq!(empty.rows_y, 227.0);
}

#[test]
fn composition_orders_dimmer_dialog_text_rows_and_footer_and_preserves_full_actions() {
    let draw = Command::Fill {
        rect: Rect {
            x: 1.0,
            y: 2.0,
            w: 3.0,
            h: 4.0,
        },
        color: Rgba {
            r: 1,
            g: 2,
            b: 3,
            a: 4,
        },
    };
    let row_key = PanelKey("visible-row".into());
    let hidden_key = PanelKey("offscreen-row".into());
    let footer_key = PanelKey("close".into());
    let scene = compose_panel(PanelInput {
        generation: 22,
        text: clover_portable::frontend::menu_panel::PanelTextScene {
            heading: vec![draw],
            body: vec![draw],
            rows_y: 227.0,
        },
        rows: vec![(
            row_key.clone(),
            clover_portable::frontend::menu_panel::RowScene {
                commands: vec![draw],
                hit: Rect {
                    x: 4.0,
                    y: 5.0,
                    w: 6.0,
                    h: 7.0,
                },
            },
        )],
        footer: FooterScene {
            commands: vec![draw],
            hit: Some(Rect {
                x: 8.0,
                y: 9.0,
                w: 10.0,
                h: 11.0,
            }),
        },
        footer_key: Some(footer_key.clone()),
        actions: vec![hidden_key.clone(), row_key.clone(), footer_key.clone()],
    });
    assert_eq!(scene.generation, 22);
    assert_eq!(
        scene.actions,
        vec![hidden_key, row_key.clone(), footer_key.clone()]
    );
    assert_eq!(
        scene.steps,
        vec![
            PanelStep::Draw(Command::Fill {
                rect: Rect {
                    x: 0.0,
                    y: 96.0,
                    w: 1280.0,
                    h: 550.0
                },
                color: Rgba {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 190
                },
            }),
            PanelStep::Dialog(Rect {
                x: 200.0,
                y: 128.0,
                w: 880.0,
                h: 478.0
            }),
            PanelStep::Draw(draw),
            PanelStep::Draw(draw),
            PanelStep::Draw(draw),
            PanelStep::Hit {
                key: row_key,
                rect: Rect {
                    x: 4.0,
                    y: 5.0,
                    w: 6.0,
                    h: 7.0
                }
            },
            PanelStep::Draw(draw),
            PanelStep::Hit {
                key: footer_key,
                rect: Rect {
                    x: 8.0,
                    y: 9.0,
                    w: 10.0,
                    h: 11.0
                }
            },
        ]
    );
}

#[test]
fn composition_does_not_create_hit_for_disabled_footer() {
    let scene = compose_panel(PanelInput {
        generation: 1,
        text: clover_portable::frontend::menu_panel::PanelTextScene {
            heading: vec![],
            body: vec![],
            rows_y: 227.0,
        },
        rows: vec![],
        footer: FooterScene {
            commands: vec![],
            hit: Some(Rect {
                x: 1.0,
                y: 2.0,
                w: 3.0,
                h: 4.0,
            }),
        },
        footer_key: None,
        actions: vec![PanelKey("all-entry-action".into())],
    });
    assert!(
        !scene
            .steps
            .iter()
            .any(|step| matches!(step, PanelStep::Hit { .. }))
    );
    assert_eq!(scene.actions, vec![PanelKey("all-entry-action".into())]);
}
