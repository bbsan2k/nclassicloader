use clover_portable::frontend::{
    font::{PlacedGlyph, TextLayout},
    menu_scene::{self, Command, Filter, Rect, Rgba, TextPosition, TextureId, TextureQuad},
};
use std::collections::BTreeMap;

fn glyph(font: &str, x: f32, y: f32, width: f32, height: f32, source: Rect) -> PlacedGlyph {
    PlacedGlyph {
        font: font.into(),
        source,
        x,
        y,
        width,
        height,
    }
}

#[test]
fn commands_preserve_glyph_order_fallback_atlases_and_fit_placement() {
    let layout = TextLayout {
        glyphs: vec![
            glyph(
                "regular",
                -2.0,
                1.0,
                8.0,
                10.0,
                Rect {
                    x: 1.0,
                    y: 2.0,
                    w: 8.0,
                    h: 10.0,
                },
            ),
            glyph(
                "missing",
                7.0,
                2.0,
                3.0,
                4.0,
                Rect {
                    x: 9.0,
                    y: 8.0,
                    w: 3.0,
                    h: 4.0,
                },
            ),
            glyph(
                "fallback",
                8.0,
                3.0,
                6.0,
                7.0,
                Rect {
                    x: 4.0,
                    y: 5.0,
                    w: 6.0,
                    h: 7.0,
                },
            ),
        ],
        advance: 20.0,
        line: 12.0,
        left: -2.0,
        top: 1.0,
        right: 14.0,
        bottom: 10.0,
    };
    let atlases = BTreeMap::from([
        (
            "regular".into(),
            TextureId {
                generation: 3,
                slot: 0,
            },
        ),
        (
            "fallback".into(),
            TextureId {
                generation: 4,
                slot: 1,
            },
        ),
    ]);
    let tint = Rgba {
        r: 12,
        g: 34,
        b: 56,
        a: 78,
    };
    let commands = menu_scene::text(
        &layout,
        &atlases,
        TextPosition {
            x: 50.0,
            y: 30.0,
            max_width: 8.0,
            line_anchor: false,
            tint,
        },
    );
    assert_eq!(
        commands,
        vec![
            Command::Texture(TextureQuad {
                texture: atlases["regular"],
                destination: Rect {
                    x: 46.0,
                    y: 27.75,
                    w: 4.0,
                    h: 5.0
                },
                source: Some(layout.glyphs[0].source),
                tint,
                filter: Filter::Linear,
                flip_x: false,
            }),
            Command::Texture(TextureQuad {
                texture: atlases["fallback"],
                destination: Rect {
                    x: 51.0,
                    y: 28.75,
                    w: 3.0,
                    h: 3.5
                },
                source: Some(layout.glyphs[2].source),
                tint,
                filter: Filter::Linear,
                flip_x: false,
            }),
        ]
    );
}

#[test]
fn empty_and_missing_atlases_produce_owned_empty_commands() {
    let commands = {
        let layout = TextLayout {
            glyphs: vec![glyph(
                "absent",
                0.0,
                0.0,
                1.0,
                1.0,
                Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 1.0,
                    h: 1.0,
                },
            )],
            advance: 1.0,
            line: 1.0,
            left: 0.0,
            top: 0.0,
            right: 1.0,
            bottom: 1.0,
        };
        let atlases = BTreeMap::new();
        menu_scene::text(
            &layout,
            &atlases,
            TextPosition {
                x: 0.0,
                y: 0.0,
                max_width: 10.0,
                line_anchor: true,
                tint: Rgba {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 0,
                },
            },
        )
    };
    assert!(commands.is_empty());
    let empty = TextLayout {
        glyphs: Vec::new(),
        advance: 0.0,
        line: 0.0,
        left: 0.0,
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
    };
    assert!(
        menu_scene::text(
            &empty,
            &BTreeMap::new(),
            TextPosition {
                x: 0.0,
                y: 0.0,
                max_width: 0.0,
                line_anchor: false,
                tint: Rgba {
                    r: 1,
                    g: 2,
                    b: 3,
                    a: 4
                }
            }
        )
        .is_empty()
    );
}
