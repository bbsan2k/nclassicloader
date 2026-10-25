use clover_portable::frontend::menu_scene::{HudMetrics, HudPlacement, Rect, hud};

fn metrics() -> [HudMetrics; 4] {
    [
        HudMetrics {
            icon_width: 30.0,
            label_advance: 20.0,
        },
        HudMetrics {
            icon_width: 36.0,
            label_advance: 35.0,
        },
        HudMetrics {
            icon_width: 90.0,
            label_advance: 40.0,
        },
        HudMetrics {
            icon_width: 78.0,
            label_advance: 50.0,
        },
    ]
}

#[test]
fn hud_produces_exact_snes_and_nes_icon_label_and_hit_layouts() {
    let input = metrics();
    let snes = hud(false, 8.0, 24.0, &input);
    let nes = hud(true, 8.0, 24.0, &input);
    let expected_x = [398.5, 480.5, 583.5, 745.5];
    let icon_widths = [30.0, 36.0, 90.0, 78.0];
    let label_advances = [20.0, 35.0, 40.0, 50.0];
    let whole = [58.0, 79.0, 138.0, 136.0];
    for index in 0..4 {
        let x = expected_x[index];
        let icon_height = if index == 1 { 36.0 } else { 30.0 };
        let expected = HudPlacement {
            icon: Rect {
                x: x + icon_widths[index] * 0.5,
                y: 609.0,
                w: icon_widths[index],
                h: icon_height,
            },
            label_x: x + icon_widths[index] + 8.0 + label_advances[index] * 0.5,
            label_y: 609.0,
            hit: Rect {
                x: x - 5.0,
                y: 585.0,
                w: whole[index] + 10.0,
                h: 48.0,
            },
        };
        assert_eq!(snes[index], expected);
        let mut nes_expected = expected;
        nes_expected.icon.y = 612.0;
        nes_expected.label_y = 612.0;
        nes_expected.icon.h = if index == 0 { 30.0 } else { 24.0 };
        assert_eq!(nes[index], nes_expected);
    }
    assert_eq!(input, metrics());
}

#[test]
fn hud_keeps_large_advance_and_fractional_or_negative_gap_arithmetic() {
    let prompts = [
        HudMetrics {
            icon_width: 10.0,
            label_advance: 1000.0,
        },
        HudMetrics {
            icon_width: 11.0,
            label_advance: 2000.0,
        },
        HudMetrics {
            icon_width: 12.0,
            label_advance: 3000.0,
        },
        HudMetrics {
            icon_width: 13.0,
            label_advance: 4000.0,
        },
    ];
    let placements = hud(false, -2.5, 3.25, &prompts);
    let total = 1007.5 + 2008.5 + 3009.5 + 4010.5 + 9.75;
    let start = 640.0 - total * 0.5;
    assert_eq!(placements[0].icon.x, start + 5.0);
    assert_eq!(placements[0].label_x, start + 10.0 - 2.5 + 500.0);
    assert_eq!(placements[0].hit.x, start - 5.0);
    assert_eq!(placements[0].hit.w, 1017.5);
    assert_eq!(placements[1].hit.x, start + 1007.5 + 3.25 - 5.0);
}
