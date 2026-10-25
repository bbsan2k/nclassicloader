use clover_portable::frontend::{
    font::{PlacedGlyph, TextLayout},
    menu_carousel,
    menu_scene::{
        self, AtlasSprite, Command, Filter, Rect, Rgba, TextPosition, TextureId, TextureQuad,
    },
};
use std::collections::BTreeMap;

fn layout() -> TextLayout {
    TextLayout {
        glyphs: vec![
            PlacedGlyph {
                font: "title".into(),
                source: Rect {
                    x: 2.0,
                    y: 3.0,
                    w: 8.0,
                    h: 10.0,
                },
                x: -4.0,
                y: 1.0,
                width: 8.0,
                height: 10.0,
            },
            PlacedGlyph {
                font: "fallback".into(),
                source: Rect {
                    x: 12.0,
                    y: 13.0,
                    w: 6.0,
                    h: 7.0,
                },
                x: 5.0,
                y: 2.0,
                width: 6.0,
                height: 7.0,
            },
        ],
        advance: 20.0,
        line: 12.0,
        left: -4.0,
        top: 1.0,
        right: 11.0,
        bottom: 11.0,
    }
}

#[test]
fn title_scene_orders_caption_then_glyphs_with_native_geometry_and_tints() {
    let title_layout = layout();
    let atlases = BTreeMap::from([
        (
            "title".into(),
            TextureId {
                generation: 3,
                slot: 1,
            },
        ),
        (
            "fallback".into(),
            TextureId {
                generation: 3,
                slot: 2,
            },
        ),
    ]);
    let caption = AtlasSprite {
        texture: TextureId {
            generation: 3,
            slot: 0,
        },
        source: Rect {
            x: 7.0,
            y: 9.0,
            w: 30.0,
            h: 12.0,
        },
        flip_x: true,
    };
    let tint = Rgba {
        r: 21,
        g: 43,
        b: 65,
        a: 87,
    };
    for (nes, shift, width) in [(true, 0.0, 1026.0), (false, -52.0, 1044.0)] {
        let commands =
            menu_carousel::title_scene(nes, shift, &title_layout, &atlases, Some(caption), tint);
        assert_eq!(commands.len(), 3);
        assert_eq!(
            commands[0],
            menu_scene::sprite(
                caption.texture,
                caption.source,
                Rect {
                    x: 640.0,
                    y: 178.0 + shift,
                    w: width,
                    h: 66.0
                },
                Rgba {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 255
                },
                true
            )
        );
        let text = menu_scene::text(
            &title_layout,
            &atlases,
            TextPosition {
                x: 640.0,
                y: 176.0 + shift,
                max_width: 990.0,
                line_anchor: true,
                tint,
            },
        );
        assert_eq!(&commands[1..], text.as_slice());
        assert_eq!(
            commands[1],
            Command::Texture(TextureQuad {
                texture: atlases["title"],
                destination: Rect {
                    x: 626.0,
                    y: 171.0 + shift,
                    w: 8.0,
                    h: 10.0
                },
                source: Some(title_layout.glyphs[0].source),
                tint,
                filter: Filter::Linear,
                flip_x: false,
            })
        );
        assert_eq!(
            commands[2],
            Command::Texture(TextureQuad {
                texture: atlases["fallback"],
                destination: Rect {
                    x: 635.0,
                    y: 172.0 + shift,
                    w: 6.0,
                    h: 7.0
                },
                source: Some(title_layout.glyphs[1].source),
                tint,
                filter: Filter::Linear,
                flip_x: false,
            })
        );
    }
}

#[test]
fn missing_caption_or_glyph_atlas_skips_only_that_draw_without_fallback() {
    let layout = layout();
    let atlases = BTreeMap::from([(
        "fallback".into(),
        TextureId {
            generation: 4,
            slot: 1,
        },
    )]);
    let commands = menu_carousel::title_scene(
        false,
        0.0,
        &layout,
        &atlases,
        None,
        Rgba {
            r: 1,
            g: 2,
            b: 3,
            a: 4,
        },
    );
    assert_eq!(commands.len(), 1);
    let Command::Texture(glyph) = commands[0] else {
        panic!("glyph retained")
    };
    assert_eq!(glyph.texture, atlases["fallback"]);
    assert_eq!(glyph.source, Some(layout.glyphs[1].source));
}

#[test]
fn title_commands_remain_owned_after_layout_and_atlases_drop() {
    let commands = {
        let layout = layout();
        let atlases = BTreeMap::from([(
            "title".into(),
            TextureId {
                generation: 8,
                slot: 1,
            },
        )]);
        menu_carousel::title_scene(
            false,
            -52.0,
            &layout,
            &atlases,
            None,
            Rgba {
                r: 5,
                g: 6,
                b: 7,
                a: 8,
            },
        )
    };
    assert_eq!(commands.len(), 1);
    assert!(matches!(commands[0], Command::Texture(_)));
}
