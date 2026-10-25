use clover_portable::frontend::{
    font::{PlacedGlyph, TextLayout},
    menu_panel::{self, PanelInput, PanelKey, PanelStep},
    menu_scene::{AtlasSprite, Command, Filter, Rect, TextureId},
};
use std::collections::BTreeMap;

fn key(value: &str) -> PanelKey {
    PanelKey(value.into())
}

fn layout() -> TextLayout {
    TextLayout {
        glyphs: vec![PlacedGlyph {
            font: "test".into(),
            source: Rect {
                x: 0.0,
                y: 0.0,
                w: 8.0,
                h: 10.0,
            },
            x: 0.0,
            y: 0.0,
            width: 8.0,
            height: 10.0,
        }],
        advance: 12.0,
        line: 14.0,
        left: 0.0,
        top: 0.0,
        right: 8.0,
        bottom: 10.0,
    }
}

#[test]
fn whole_panel_orders_shell_text_visible_rows_and_footer_but_keeps_all_actions() {
    let font = TextureId {
        generation: 9,
        slot: 1,
    };
    let sprite = AtlasSprite {
        texture: TextureId {
            generation: 8,
            slot: 2,
        },
        source: Rect {
            x: 10.0,
            y: 20.0,
            w: 6.0,
            h: 7.0,
        },
        flip_x: true,
    };
    let mut fonts = BTreeMap::from([("test".into(), font)]);
    let mut measured = layout();
    let text = menu_panel::text_scene(&measured, &[measured.clone()], false, &fonts);
    assert_eq!(text.rows_y, 265.0);
    let viewport = menu_panel::row_viewport(20, 19, text.rows_y);
    assert_eq!((viewport.first_row, viewport.visible_count), (9, 11));
    let mut y = text.rows_y;
    let rows: Vec<_> = (viewport.first_row..20)
        .map(|index| {
            let row = menu_panel::row(
                y,
                viewport.row_height,
                index == 19,
                Some(sprite),
                &measured,
                &fonts,
            );
            y += viewport.row_height;
            (key(&format!("profile:{index}")), row)
        })
        .collect();
    let footer = menu_panel::footer(&measured, &fonts, Some(sprite), true);
    let mut actions: Vec<_> = (0..20).map(|i| key(&format!("profile:{i}"))).collect();
    actions.push(key("close"));
    let scene = menu_panel::compose_panel(PanelInput {
        generation: 9,
        text,
        rows,
        footer,
        footer_key: Some(key("close")),
        actions: actions.clone(),
    });
    measured.glyphs.clear();
    fonts.clear();
    drop((measured, fonts));
    assert_eq!(scene.generation, 9);
    assert_eq!(scene.actions, actions);
    assert!(
        matches!(scene.steps[0], PanelStep::Draw(Command::Fill { rect, color })
        if rect == Rect { x: 0.0, y: 96.0, w: 1280.0, h: 550.0 } && color.a == 190)
    );
    assert!(matches!(scene.steps[1], PanelStep::Dialog(rect)
        if rect == Rect { x: 200.0, y: 128.0, w: 880.0, h: 478.0 }));
    let mut cursor = 2;
    for expected_y in [174.0, 227.0] {
        let PanelStep::Draw(Command::Texture(quad)) = scene.steps[cursor] else {
            panic!("text draw")
        };
        assert_eq!(quad.texture, font);
        assert_eq!(quad.filter, Filter::Linear);
        assert_eq!(quad.destination.y, expected_y - 7.0);
        cursor += 1;
    }
    let mut expected_y = 265.0;
    for index in 9..20 {
        if index == 19 {
            let PanelStep::Draw(Command::Texture(quad)) = scene.steps[cursor] else {
                panic!("highlight")
            };
            assert_eq!(
                quad.texture.generation, 8,
                "stale IDs are retained, not rebound"
            );
            assert!(quad.flip_x);
            cursor += 1;
        }
        assert!(
            matches!(scene.steps[cursor], PanelStep::Draw(Command::Texture(q)) if q.texture == font)
        );
        cursor += 1;
        let PanelStep::Hit { key: hit_key, rect } = &scene.steps[cursor] else {
            panic!("row hit")
        };
        assert_eq!(hit_key, &key(&format!("profile:{index}")));
        assert_eq!(rect.y, expected_y - 14.0);
        expected_y += viewport.row_height;
        cursor += 1;
    }
    assert!(
        matches!(scene.steps[cursor], PanelStep::Draw(Command::Texture(q)) if q.texture == sprite.texture)
    );
    assert!(
        matches!(scene.steps[cursor + 1], PanelStep::Draw(Command::Texture(q)) if q.texture == font)
    );
    assert!(
        matches!(&scene.steps[cursor + 2], PanelStep::Hit { key: k, rect } if k == &key("close") && rect.y == 543.0)
    );
    assert_eq!(scene.steps.len(), cursor + 3);
}

#[test]
fn missing_resources_preserve_keys_and_disabled_footer_has_no_hit_or_action() {
    let measured = layout();
    let fonts = BTreeMap::new();
    let text = menu_panel::text_scene(
        &measured,
        &[measured.clone(), measured.clone()],
        true,
        &fonts,
    );
    assert_eq!(
        text.rows_y, 291.0,
        "missing fonts do not remove line spacing"
    );
    let row = menu_panel::row(text.rows_y, 24.0, true, None, &measured, &fonts);
    let scene = menu_panel::compose_panel(PanelInput {
        generation: 5,
        text,
        rows: vec![(key("dismiss-import"), row)],
        footer: menu_panel::footer(&measured, &fonts, None, false),
        footer_key: None,
        actions: vec![key("dismiss-import")],
    });
    assert_eq!(scene.steps.len(), 3);
    assert!(
        matches!(&scene.steps[2], PanelStep::Hit { key: k, .. } if k == &key("dismiss-import"))
    );
    assert_eq!(scene.actions, [key("dismiss-import")]);
}

#[test]
fn keys_are_captured_not_resolved_by_catalog_position_and_empty_panel_keeps_shell() {
    let measured = layout();
    let fonts = BTreeMap::new();
    let mut catalog = ["language:jpn_ja", "language:usa_en"];
    let row = menu_panel::row(227.0, 43.0, false, None, &measured, &fonts);
    let scene = menu_panel::compose_panel(PanelInput {
        generation: 12,
        text: menu_panel::text_scene(&measured, &[], false, &fonts),
        rows: vec![(key(catalog[0]), row)],
        footer: menu_panel::footer(&measured, &fonts, None, false),
        footer_key: None,
        actions: catalog.iter().map(|value| key(value)).collect(),
    });
    catalog.reverse();
    assert_eq!(
        scene.actions,
        [key("language:jpn_ja"), key("language:usa_en")]
    );
    assert!(
        matches!(&scene.steps[2], PanelStep::Hit { key: k, .. } if k == &key("language:jpn_ja"))
    );
    let empty = menu_panel::compose_panel(PanelInput {
        generation: 12,
        text: menu_panel::text_scene(&measured, &[], false, &fonts),
        rows: vec![],
        footer: menu_panel::footer(&measured, &fonts, None, false),
        footer_key: Some(key("close")),
        actions: vec![],
    });
    assert_eq!(empty.steps.len(), 2);
    assert!(empty.actions.is_empty());
}
