use clover_portable::frontend::{
    font::{PlacedGlyph, TextLayout},
    menu_overlay::{OverlayRoute, notice, route},
    menu_scene::{Command, Filter, Rgba, TextureId},
};
use std::collections::BTreeMap;

#[test]
fn routes_all_existing_overlay_strings_exactly() {
    for (panel, expected) in [
        ("resume", OverlayRoute::Resume),
        ("confirm-overwrite", OverlayRoute::ResumeConfirmation),
        ("confirm-delete", OverlayRoute::ResumeConfirmation),
        ("settings", OverlayRoute::Toolbar),
        ("toolbar:", OverlayRoute::Toolbar),
        ("toolbar:any-suffix", OverlayRoute::Toolbar),
        ("emulator-settings", OverlayRoute::Panel),
        ("help", OverlayRoute::Panel),
        ("message", OverlayRoute::Panel),
        ("profiles", OverlayRoute::Panel),
        ("notice", OverlayRoute::Notice),
        ("", OverlayRoute::None),
        ("unknown", OverlayRoute::None),
        ("Resume", OverlayRoute::None),
        (" resume", OverlayRoute::None),
        ("settings ", OverlayRoute::None),
        ("Toolbar:1", OverlayRoute::None),
        ("toolbar", OverlayRoute::None),
    ] {
        assert_eq!(route(panel), expected, "{panel:?}");
    }
}

#[test]
fn notice_scene_owns_white_glyph_commands_and_fixed_dialog_geometry() {
    let layout = TextLayout {
        glyphs: vec![PlacedGlyph {
            font: "l".into(),
            source: clover_portable::frontend::font::Rect {
                x: 2.0,
                y: 3.0,
                w: 4.0,
                h: 5.0,
            },
            x: 0.0,
            y: 0.0,
            width: 4.0,
            height: 5.0,
        }],
        advance: 10.0,
        line: 12.0,
        left: 0.0,
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
    };
    let id = TextureId {
        generation: 17,
        slot: 3,
    };
    let atlases = BTreeMap::from([("l".into(), id)]);
    let scene = notice(17, &layout, &atlases);
    assert_eq!(scene.generation, 17);
    assert_eq!(
        scene.dialog,
        clover_portable::frontend::menu_scene::Rect {
            x: 304.0,
            y: 312.0,
            w: 672.0,
            h: 96.0
        }
    );
    assert_eq!(scene.commands.len(), 1);
    let Command::Texture(glyph) = scene.commands[0] else {
        panic!("expected glyph command")
    };
    assert_eq!(glyph.texture, id);
    assert_eq!(glyph.source, Some(layout.glyphs[0].source));
    assert_eq!(
        glyph.tint,
        Rgba {
            r: 255,
            g: 255,
            b: 255,
            a: 255
        }
    );
    assert_eq!(glyph.filter, Filter::Linear);
    assert_eq!(glyph.destination.x, 635.0);
    assert_eq!(glyph.destination.y, 350.0);
    let missing = notice(18, &layout, &BTreeMap::new());
    assert!(missing.commands.is_empty());
    assert_eq!(scene.commands.len(), 1);
}
