use clover_portable::frontend::font::{PlacedGlyph, TextLayout};
use clover_portable::frontend::menu_chrome::{
    HudAction, HudPrompt, hud_scene, menu_bars, thumbnail_scene,
};
use clover_portable::frontend::menu_scene::{
    self, AtlasSprite, Command, Filter, ImageSource, Rect, Rgba, TextureId, TextureQuad,
};
use std::collections::BTreeMap;

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect { x, y, w, h }
}
fn sprite(slot: usize, flip_x: bool) -> AtlasSprite {
    AtlasSprite {
        texture: TextureId {
            generation: 7,
            slot,
        },
        source: rect(3.0, 4.0, 5.0, 6.0),
        flip_x,
    }
}

#[test]
fn binds_order_geometry_alpha_ids_and_flip_without_inventing_marker() {
    let placement = menu_scene::thumbnails(2, 0)[0];
    let cover = ImageSource {
        texture: TextureId {
            generation: 7,
            slot: 1,
        },
        width: 64,
        height: 32,
    };
    let scene = thumbnail_scene(
        placement,
        Some(cover),
        Some(sprite(2, true)),
        Some(sprite(3, false)),
    );
    assert_eq!(scene.hit, placement.hit);
    assert_eq!(
        scene.commands,
        vec![
            menu_scene::fit_image(cover.texture, 64, 32, placement.cover, placement.alpha),
            menu_scene::sprite(
                sprite(2, true).texture,
                sprite(2, true).source,
                placement.marker.unwrap(),
                Rgba {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 255
                },
                true
            ),
            menu_scene::sprite(
                sprite(3, false).texture,
                sprite(3, false).source,
                placement.lock,
                Rgba {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 255
                },
                false
            ),
        ]
    );
    let other = menu_scene::thumbnails(2, 0)[1];
    let scene = thumbnail_scene(other, None, Some(sprite(2, true)), None);
    assert_eq!(scene.hit, other.hit);
    assert!(scene.commands.is_empty());
}

#[test]
fn unselected_thumbnail_has_no_marker_even_when_source_is_supplied() {
    let placement = menu_scene::thumbnails(2, 0)[1];
    let scene = thumbnail_scene(
        placement,
        None,
        Some(sprite(2, true)),
        Some(sprite(3, true)),
    );
    assert_eq!(
        scene.commands,
        vec![menu_scene::sprite(
            sprite(3, true).texture,
            sprite(3, true).source,
            placement.lock,
            Rgba {
                r: 255,
                g: 255,
                b: 255,
                a: 255
            },
            true
        )]
    );
    if let Command::Texture(TextureQuad { filter, tint, .. }) = scene.commands[0] {
        assert_eq!(filter, Filter::Nearest);
        assert_eq!(tint.a, 255);
    } else {
        panic!("expected sprite command");
    }
}

#[test]
fn missing_resources_preserve_hit_and_all_none_emits_no_commands() {
    let placement = menu_scene::thumbnails(1, 0)[0];
    let scene = thumbnail_scene(placement, None, None, None);
    assert_eq!(scene.hit, placement.hit);
    assert!(scene.commands.is_empty());
}

fn layout() -> TextLayout {
    TextLayout {
        glyphs: vec![PlacedGlyph {
            font: "s".into(),
            source: rect(1.0, 2.0, 3.0, 4.0),
            x: 0.0,
            y: 0.0,
            width: 3.0,
            height: 4.0,
        }],
        advance: 20.0,
        line: 10.0,
        left: 0.0,
        top: 0.0,
        right: 3.0,
        bottom: 4.0,
    }
}

#[test]
fn four_item_hud_has_fixed_actions_geometry_order_and_owned_draw_snapshots() {
    let prompts = std::array::from_fn(|i| HudPrompt {
        icon: Some(sprite(i + 1, i == 2)),
        icon_width: [36.0, 36.0, 96.0, 84.0][i],
        layout: layout(),
    });
    let atlases = BTreeMap::from([(
        "s".to_owned(),
        TextureId {
            generation: 7,
            slot: 9,
        },
    )]);
    let tint = Rgba {
        r: 10,
        g: 20,
        b: 30,
        a: 40,
    };
    for nes in [false, true] {
        let scene = hud_scene(nes, 8.0, 24.0, &prompts, &atlases, tint);
        assert_eq!(
            scene.items.each_ref().map(|item| item.action),
            [
                HudAction::Settings,
                HudAction::ResumePanel,
                HudAction::Sort,
                HudAction::Start
            ]
        );
        let placements = menu_scene::hud(
            nes,
            8.0,
            24.0,
            &[36.0, 36.0, 96.0, 84.0].map(|icon_width| menu_scene::HudMetrics {
                icon_width,
                label_advance: 20.0,
            }),
        );
        for (index, item) in scene.items.iter().enumerate() {
            assert_eq!(item.hit, placements[index].hit);
            assert_eq!(item.commands.len(), 2);
            assert_eq!(
                item.commands[0],
                menu_scene::sprite(
                    sprite(index + 1, index == 2).texture,
                    sprite(index + 1, index == 2).source,
                    placements[index].icon,
                    Rgba {
                        r: 255,
                        g: 255,
                        b: 255,
                        a: 255
                    },
                    index == 2
                )
            );
            assert_eq!(
                item.commands[1],
                menu_scene::text(
                    &layout(),
                    &atlases,
                    menu_scene::TextPosition {
                        x: placements[index].label_x,
                        y: placements[index].label_y,
                        max_width: 1200.0,
                        line_anchor: true,
                        tint
                    }
                )[0]
            );
        }
    }
}

#[test]
fn missing_hud_icon_or_font_skips_only_that_draw_and_retains_hit() {
    let prompts = std::array::from_fn(|i| HudPrompt {
        icon: (i != 0).then(|| sprite(i, false)),
        icon_width: 30.0,
        layout: layout(),
    });
    let scene = hud_scene(
        true,
        8.0,
        24.0,
        &prompts,
        &BTreeMap::new(),
        Rgba {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        },
    );
    assert_eq!(scene.items[0].action, HudAction::Settings);
    assert_eq!(scene.items[0].commands.len(), 0);
    assert_eq!(scene.items[1].commands.len(), 1);
    assert_eq!(
        scene.items[1].hit,
        menu_scene::hud(
            true,
            8.0,
            24.0,
            &[30.0; 4].map(|icon_width| menu_scene::HudMetrics {
                icon_width,
                label_advance: 20.0
            })
        )[1]
        .hit
    );
}

#[test]
fn menu_bars_preserve_order_geometry_resources_and_system_region_filter() {
    let upper = sprite(11, true);
    let lower = sprite(12, false);
    let overlay = sprite(13, true);
    let white = Rgba {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };
    let expected = vec![
        menu_scene::sprite(
            upper.texture,
            upper.source,
            rect(640.0, 36.0, 1296.0, 120.0),
            white,
            true,
        ),
        menu_scene::sprite(
            lower.texture,
            lower.source,
            rect(640.0, 690.0, 1296.0, 120.0),
            white,
            false,
        ),
    ];
    assert_eq!(
        menu_bars(false, false, Some(upper), Some(lower), Some(overlay)),
        expected
    );
    assert_eq!(
        menu_bars(true, true, Some(upper), Some(lower), Some(overlay)),
        expected
    );
    let mut with_overlay = expected.clone();
    with_overlay.push(menu_scene::sprite(
        overlay.texture,
        overlay.source,
        rect(640.0, 36.0, 480.0, 120.0),
        white,
        true,
    ));
    assert_eq!(
        menu_bars(true, false, Some(upper), Some(lower), Some(overlay)),
        with_overlay
    );
    assert_eq!(menu_bars(true, false, None, None, Some(overlay)).len(), 1);
    assert!(menu_bars(true, false, None, None, None).is_empty());
    assert!(menu_bars(false, false, None, None, Some(overlay)).is_empty());
    // Resource snapshots are carried through unchanged; stale IDs are not rebound.
    let stale = AtlasSprite {
        texture: TextureId {
            generation: 1,
            slot: 99,
        },
        ..upper
    };
    assert_eq!(
        menu_bars(false, false, Some(stale), None, None)[0],
        menu_scene::sprite(
            stale.texture,
            stale.source,
            rect(640.0, 36.0, 1296.0, 120.0),
            white,
            stale.flip_x
        )
    );
}
