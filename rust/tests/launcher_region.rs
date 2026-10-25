use clover_portable::frontend::launcher_scene::{DrawCommand, Rect, Region, detail_flag, row_flag};

#[test]
fn region_markers_keep_exact_aliases_and_four_display_labels() {
    for (marker, expected, label) in [
        ("JPN", Region::Japan, "Japan"),
        ("Japan", Region::Japan, "Japan"),
        ("EUR", Region::Europe, "Europe"),
        ("Europe", Region::Europe, "Europe"),
        ("USA", Region::Usa, "United States"),
        ("IMG", Region::Unknown, "Unknown"),
        ("ALL", Region::Unknown, "Unknown"),
        ("unknown", Region::Unknown, "Unknown"),
        ("jpn", Region::Unknown, "Unknown"),
        ("jApAn", Region::Unknown, "Unknown"),
        ("europe", Region::Unknown, "Unknown"),
        ("Usa", Region::Unknown, "Unknown"),
        (" EUR", Region::Unknown, "Unknown"),
        ("Japan ", Region::Unknown, "Unknown"),
        (" 日本 ", Region::Unknown, "Unknown"),
        ("US", Region::Unknown, "Unknown"),
        ("", Region::Unknown, "Unknown"),
    ] {
        assert_eq!(Region::from_marker(marker), expected, "marker={marker:?}");
        assert_eq!(Region::from_marker(marker).label(), label);
    }
}

#[test]
fn flag_commands_are_fixed_semantic_regions_with_exact_row_and_detail_geometry() {
    for region in [Region::Japan, Region::Europe, Region::Usa, Region::Unknown] {
        for (row, expected) in [
            (
                0,
                Rect {
                    x: 286.0,
                    y: 214.0,
                    w: 32.0,
                    h: 24.0,
                },
            ),
            (
                5,
                Rect {
                    x: 286.0,
                    y: 574.0,
                    w: 32.0,
                    h: 24.0,
                },
            ),
        ] {
            let DrawCommand::Flag {
                region: actual_region,
                rect,
            } = row_flag(row, region)
            else {
                panic!("row flag must remain a flag command");
            };
            assert_eq!(actual_region, region);
            assert_eq!(rect, expected);
        }

        assert_eq!(
            detail_flag(region),
            DrawCommand::Flag {
                region,
                rect: Rect {
                    x: 866.0,
                    y: 505.0,
                    w: 24.0,
                    h: 18.0,
                },
            }
        );
    }
}
