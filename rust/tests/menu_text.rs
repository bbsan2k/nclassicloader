use clover_portable::frontend::font::{PlacedGlyph, Rect, TextLayout, TextPlacement, placement};

#[test]
fn empty_layout_and_right_overhang_preserve_extent_floor_and_legacy_widths() {
    let mut layout = TextLayout {
        glyphs: Vec::new(),
        advance: 0.0,
        line: 0.0,
        left: 0.0,
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
    };
    for anchor in [false, true] {
        assert_eq!(placement(&layout, 10.0, 20.0, 0.5, anchor).scale, 0.5);
        assert_eq!(placement(&layout, 10.0, 20.0, -2.0, anchor).scale, -2.0);
        assert_eq!(
            placement(&layout, 10.0, 20.0, f32::INFINITY, anchor).scale,
            1.0
        );
    }
    layout.advance = 8.0;
    layout.right = 16.0;
    assert_eq!(
        placement(&layout, 10.0, 20.0, 8.0, true),
        TextPlacement {
            left: 8.0,
            top: 20.0,
            scale: 0.5,
        }
    );
}

#[test]
fn host_neutral_text_placement_centers_ink_and_line_and_fits_only_downward() {
    let layout = TextLayout {
        glyphs: Vec::new(),
        advance: 40.0,
        line: 18.0,
        left: -6.0,
        top: -2.0,
        right: 26.0,
        bottom: 14.0,
    };
    assert_eq!(
        placement(&layout, 100.0, 50.0, 8.0, false),
        TextPlacement {
            left: 97.5,
            top: 48.5,
            scale: 0.25,
        }
    );
    assert_eq!(
        placement(&layout, 100.0, 50.0, 100.0, true),
        TextPlacement {
            left: 80.0,
            top: 41.0,
            scale: 1.0,
        }
    );
}

#[test]
fn glyph_placement_keeps_atlas_source_and_scales_destination_without_rounding() {
    let layout = TextLayout {
        glyphs: Vec::new(),
        advance: 12.0,
        line: 10.0,
        left: -2.0,
        top: -1.0,
        right: 6.0,
        bottom: 7.0,
    };
    let glyph = PlacedGlyph {
        font: "regular".into(),
        source: Rect {
            x: 3.0,
            y: 4.0,
            w: 5.0,
            h: 6.0,
        },
        x: -1.5,
        y: 2.25,
        width: 5.5,
        height: 7.25,
    };
    let destination = placement(&layout, 20.0, 30.0, 4.0, false).glyph_rect(&glyph);
    assert_eq!(
        destination,
        Rect {
            x: 18.25,
            y: 29.625,
            w: 2.75,
            h: 3.625,
        }
    );
    assert_eq!(
        glyph.source,
        Rect {
            x: 3.0,
            y: 4.0,
            w: 5.0,
            h: 6.0
        }
    );
}
