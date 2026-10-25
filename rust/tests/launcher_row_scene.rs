use clover_portable::frontend::launcher_scene::{
    DrawCommand, FontRole, GeometryCommand, Rect, Region, Rgba, RowDisplay, TextCommand,
    library_row, library_row_background, library_row_flag_background, row_rect,
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
fn complete_row_command_order_and_text_styles_are_exact() {
    const ACCENT: Rgba = Rgba {
        r: 94,
        g: 234,
        b: 210,
        a: 255,
    };
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

    for row in [0, 5] {
        let rect = row_rect(row);
        for chosen in [false, true] {
            for hot in [false, true] {
                for busy in [false, true] {
                    for cached in [false, true] {
                        for region in [Region::Japan, Region::Europe, Region::Usa, Region::Unknown]
                        {
                            let mut expected: Vec<_> =
                                library_row_background(row, chosen, hot, busy)
                                    .into_iter()
                                    .map(DrawCommand::Geometry)
                                    .collect();
                            expected.push(text(
                                "SNES",
                                rect.x + 438.0,
                                rect.y + 14.0,
                                12,
                                FontRole::Semibold,
                                if chosen { ACCENT } else { MUTED },
                                70.0,
                            ));
                            expected.push(DrawCommand::Geometry(library_row_flag_background(
                                row, chosen,
                            )));
                            expected.push(DrawCommand::Flag {
                                region,
                                rect: Rect {
                                    x: rect.x + 22.0,
                                    y: rect.y + 20.0,
                                    w: 32.0,
                                    h: 24.0,
                                },
                            });
                            expected.push(text(
                                "A sample Classic",
                                rect.x + 78.0,
                                rect.y + 8.0,
                                20,
                                FontRole::Semibold,
                                INK,
                                340.0,
                            ));
                            expected.push(text(
                                if cached {
                                    "Version 2.0  ·  Ready to play"
                                } else {
                                    "Version 2.0  ·  Filesystem image"
                                },
                                rect.x + 78.0,
                                rect.y + 35.0,
                                14,
                                FontRole::Regular,
                                MUTED,
                                445.0,
                            ));
                            if chosen {
                                expected.push(text(
                                    "›",
                                    rect.x + 523.0,
                                    rect.y + 13.0,
                                    27,
                                    FontRole::Semibold,
                                    ACCENT,
                                    22.0,
                                ));
                            }

                            assert_eq!(
                                library_row(
                                    row,
                                    chosen,
                                    hot,
                                    busy,
                                    RowDisplay {
                                        title: "A sample Classic",
                                        system_label: "SNES",
                                        version: "Version 2.0",
                                        cached,
                                        region,
                                    },
                                ),
                                expected,
                                "row={row} chosen={chosen} hot={hot} busy={busy} cached={cached} region={region:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn returned_text_is_owned_after_input_mutation_and_drop() {
    let commands = {
        let mut title = String::from("Borrowed title");
        let mut version = String::from("Version 9");
        let mut system_label = String::from("FAMICOM");
        let commands = library_row(
            0,
            false,
            false,
            false,
            RowDisplay {
                title: &title,
                system_label: &system_label,
                version: &version,
                cached: true,
                region: Region::Japan,
            },
        );
        title.clear();
        version.clear();
        system_label.clear();
        commands
    };

    assert!(commands.contains(&text(
        "FAMICOM",
        702.0,
        208.0,
        12,
        FontRole::Semibold,
        Rgba {
            r: 156,
            g: 171,
            b: 186,
            a: 255,
        },
        70.0,
    )));
    assert!(commands.contains(&text(
        "Borrowed title",
        342.0,
        202.0,
        20,
        FontRole::Semibold,
        Rgba {
            r: 236,
            g: 242,
            b: 245,
            a: 255,
        },
        340.0,
    )));
    assert!(commands.contains(&text(
        "Version 9  ·  Ready to play",
        342.0,
        229.0,
        14,
        FontRole::Regular,
        Rgba {
            r: 156,
            g: 171,
            b: 186,
            a: 255,
        },
        445.0,
    )));
    assert!(!commands.iter().any(|command| matches!(
        command,
        DrawCommand::Geometry(GeometryCommand::FillRect { .. })
    )));
}
