use clover_portable::frontend::launcher_scene::{
    DrawCommand, FontRole, Rect, Region, Rgba, TextCommand, detail_metadata,
};

fn text(
    value: &str,
    x: f32,
    y: f32,
    size: u16,
    font: FontRole,
    color: Rgba,
    max_width: f32,
) -> DrawCommand {
    DrawCommand::Text(TextCommand {
        value: value.to_owned(),
        x,
        y,
        size,
        font,
        color,
        max_width,
    })
}

#[test]
fn detail_metadata_order_and_styles_cover_every_region_and_status() {
    const INK: Rgba = Rgba {
        r: 236,
        g: 242,
        b: 245,
        a: 255,
    };
    const MUTED: Rgba = Rgba {
        r: 156,
        g: 171,
        b: 186,
        a: 255,
    };

    for region in [Region::Japan, Region::Europe, Region::Usa, Region::Unknown] {
        for entry_exists in [false, true] {
            for ready in [false, true] {
                let expected = vec![
                    text("REGION", 866.0, 482.0, 11, FontRole::Semibold, MUTED, 120.0),
                    DrawCommand::Flag {
                        region,
                        rect: Rect {
                            x: 866.0,
                            y: 505.0,
                            w: 24.0,
                            h: 18.0,
                        },
                    },
                    text(
                        region.label(),
                        902.0,
                        503.0,
                        16,
                        FontRole::Semibold,
                        INK,
                        102.0,
                    ),
                    text(
                        "SOURCE",
                        1014.0,
                        482.0,
                        11,
                        FontRole::Semibold,
                        MUTED,
                        130.0,
                    ),
                    text(
                        if ready {
                            "Local cache"
                        } else if entry_exists {
                            "Local image"
                        } else {
                            "Not selected"
                        },
                        1014.0,
                        503.0,
                        18,
                        FontRole::Semibold,
                        INK,
                        140.0,
                    ),
                    text(
                        "/display/data/source.hsqs",
                        866.0,
                        550.0,
                        13,
                        FontRole::Regular,
                        MUTED,
                        286.0,
                    ),
                    text(
                        if ready {
                            "No original image needed."
                        } else {
                            "Imported locally. Never executed."
                        },
                        866.0,
                        576.0,
                        14,
                        FontRole::Regular,
                        MUTED,
                        286.0,
                    ),
                ];
                assert_eq!(
                    detail_metadata(region, entry_exists, ready, "/display/data/source.hsqs"),
                    expected,
                    "region={region:?} entry_exists={entry_exists} ready={ready}"
                );
            }
        }
    }
}

#[test]
fn detail_metadata_owns_source_display_text_after_input_mutation_and_drop() {
    let commands = {
        let mut source = String::from("data path shown to user");
        let commands = detail_metadata(Region::Europe, true, false, &source);
        source.clear();
        commands
    };

    assert!(commands.contains(&text(
        "data path shown to user",
        866.0,
        550.0,
        13,
        FontRole::Regular,
        Rgba {
            r: 156,
            g: 171,
            b: 186,
            a: 255,
        },
        286.0,
    )));
}
