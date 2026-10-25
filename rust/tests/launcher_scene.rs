use clover_portable::frontend::launcher_scene::{
    BUTTONS, ControllerIllustration, FILTER_TABS, GeometryCommand, HEIGHT, PAGE_SIZE, Rect, Rgba,
    WIDTH, background, brand_accent, controller_illustration, coordinates, detail_panel,
    detail_separator, empty_library_panel, filter_tab_background, import_progress, library_heading,
    library_row_background, library_row_flag_background, primary_button_background, row_rect,
    sidebar_button_background, unknown_controller_panel,
};

#[test]
fn extracted_launcher_chrome_geometry_is_exact_and_ordered() {
    let fill = |rect, color| GeometryCommand::FillRect { rect, color };
    let rounded = |rect, color, radius| GeometryCommand::RoundedRect {
        rect,
        color,
        radius,
    };
    let accent = Rgba {
        r: 94,
        g: 234,
        b: 210,
        a: 255,
    };
    assert_eq!(
        brand_accent(),
        fill(
            Rect {
                x: 28.0,
                y: 122.0,
                w: 36.0,
                h: 3.0
            },
            accent
        )
    );
    assert_eq!(
        library_heading(),
        [
            rounded(
                Rect {
                    x: 16.0,
                    y: 215.0,
                    w: 208.0,
                    h: 48.0
                },
                Rgba {
                    r: 28,
                    g: 46,
                    b: 55,
                    a: 255
                },
                8.0
            ),
            fill(
                Rect {
                    x: 16.0,
                    y: 230.0,
                    w: 3.0,
                    h: 18.0
                },
                accent
            ),
        ]
    );

    for index in (0..=3).chain([usize::MAX]) {
        for active in [false, true] {
            for busy in [false, true] {
                assert_eq!(
                    sidebar_button_background(index, active, busy),
                    if (1..=2).contains(&index) && active && !busy {
                        Some(rounded(
                            BUTTONS[index],
                            Rgba {
                                r: 35,
                                g: 49,
                                b: 64,
                                a: 255,
                            },
                            8.0,
                        ))
                    } else {
                        None
                    },
                );
            }
        }
    }

    for index in (0..=FILTER_TABS.len()).chain([usize::MAX]) {
        for active in [false, true] {
            for hot in [false, true] {
                for busy in [false, true] {
                    let expected = FILTER_TABS.get(index).map(|rect| {
                        rounded(
                            *rect,
                            if active {
                                accent
                            } else if hot && !busy {
                                Rgba {
                                    r: 45,
                                    g: 66,
                                    b: 80,
                                    a: 255,
                                }
                            } else {
                                Rgba {
                                    r: 29,
                                    g: 44,
                                    b: 58,
                                    a: 255,
                                }
                            },
                            6.0,
                        )
                    });
                    assert_eq!(filter_tab_background(index, active, hot, busy), expected);
                }
            }
        }
    }
}

#[test]
fn library_row_and_empty_panel_commands_match_native_geometry() {
    let accent = Rgba {
        r: 94,
        g: 234,
        b: 210,
        a: 255,
    };
    let rounded = |rect, color, radius| GeometryCommand::RoundedRect {
        rect,
        color,
        radius,
    };
    for (row, y) in [(0, 194.0), (5, 554.0)] {
        let rect = Rect {
            x: 264.0,
            y,
            w: 556.0,
            h: 62.0,
        };
        for chosen in [false, true] {
            for hot in [false, true] {
                for busy in [false, true] {
                    let expected = if chosen {
                        vec![
                            rounded(rect, accent, 9.0),
                            rounded(
                                Rect {
                                    x: 265.0,
                                    y: y + 1.0,
                                    w: 554.0,
                                    h: 60.0,
                                },
                                Rgba {
                                    r: 28,
                                    g: 51,
                                    b: 60,
                                    a: 255,
                                },
                                8.0,
                            ),
                        ]
                    } else {
                        let color = if hot && !busy {
                            Rgba {
                                r: 36,
                                g: 49,
                                b: 65,
                                a: 255,
                            }
                        } else {
                            Rgba {
                                r: 25,
                                g: 36,
                                b: 49,
                                a: 255,
                            }
                        };
                        vec![rounded(rect, color, 8.0)]
                    };
                    assert_eq!(library_row_background(row, chosen, hot, busy), expected);
                }
            }
            assert_eq!(
                library_row_flag_background(row, chosen),
                rounded(
                    Rect {
                        x: 278.0,
                        y: y + 12.0,
                        w: 48.0,
                        h: 38.0
                    },
                    if chosen {
                        Rgba {
                            r: 54,
                            g: 83,
                            b: 88,
                            a: 255,
                        }
                    } else {
                        Rgba {
                            r: 41,
                            g: 54,
                            b: 69,
                            a: 255,
                        }
                    },
                    6.0,
                ),
            );
        }
    }
    assert_eq!(
        empty_library_panel(),
        rounded(
            Rect {
                x: 264.0,
                y: 194.0,
                w: 556.0,
                h: 422.0
            },
            Rgba {
                r: 25,
                g: 36,
                b: 49,
                a: 255
            },
            10.0,
        ),
    );
}

#[test]
fn detail_and_progress_geometry_are_exact() {
    let rounded = |rect, color, radius| GeometryCommand::RoundedRect {
        rect,
        color,
        radius,
    };
    let fill = |rect, color| GeometryCommand::FillRect { rect, color };
    assert_eq!(
        detail_panel(),
        rounded(
            Rect {
                x: 844.0,
                y: 194.0,
                w: 332.0,
                h: 422.0
            },
            Rgba {
                r: 23,
                g: 34,
                b: 47,
                a: 255
            },
            12.0,
        )
    );
    assert_eq!(
        unknown_controller_panel(),
        rounded(
            Rect {
                x: 966.0,
                y: 234.0,
                w: 88.0,
                h: 76.0
            },
            Rgba {
                r: 43,
                g: 59,
                b: 77,
                a: 255
            },
            12.0,
        )
    );
    assert_eq!(
        detail_separator(),
        fill(
            Rect {
                x: 866.0,
                y: 464.0,
                w: 286.0,
                h: 1.0
            },
            Rgba {
                r: 48,
                g: 62,
                b: 76,
                a: 255
            },
        )
    );
    for (enabled, hot, color) in [
        (
            false,
            false,
            Rgba {
                r: 43,
                g: 57,
                b: 68,
                a: 255,
            },
        ),
        (
            false,
            true,
            Rgba {
                r: 43,
                g: 57,
                b: 68,
                a: 255,
            },
        ),
        (
            true,
            false,
            Rgba {
                r: 94,
                g: 234,
                b: 210,
                a: 255,
            },
        ),
        (
            true,
            true,
            Rgba {
                r: 143,
                g: 249,
                b: 226,
                a: 255,
            },
        ),
    ] {
        assert_eq!(
            primary_button_background(enabled, hot),
            rounded(BUTTONS[0], color, 9.0)
        );
    }

    assert!(import_progress(false, 7.0).is_empty());
    for (elapsed, thumb_x) in [(0.0, 272.0), (1.0, 442.0), (-1.0, 522.0), (42.0, 272.0)] {
        assert_eq!(
            import_progress(true, elapsed),
            vec![
                rounded(
                    Rect {
                        x: 272.0,
                        y: 640.0,
                        w: 548.0,
                        h: 3.0
                    },
                    Rgba {
                        r: 43,
                        g: 61,
                        b: 74,
                        a: 255
                    },
                    1.0,
                ),
                fill(
                    Rect {
                        x: thumb_x,
                        y: 640.0,
                        w: 128.0,
                        h: 3.0
                    },
                    Rgba {
                        r: 94,
                        g: 234,
                        b: 210,
                        a: 255
                    },
                ),
            ],
        );
    }
}

#[test]
fn shared_launcher_layout_and_letterbox_coordinates_are_stable() {
    assert_eq!((WIDTH, HEIGHT, PAGE_SIZE), (1200, 760, 6));
    assert_eq!(
        BUTTONS,
        [
            Rect {
                x: 866.0,
                y: 650.0,
                w: 294.0,
                h: 58.0
            },
            Rect {
                x: 24.0,
                y: 285.0,
                w: 192.0,
                h: 48.0
            },
            Rect {
                x: 24.0,
                y: 342.0,
                w: 192.0,
                h: 48.0
            },
        ]
    );
    assert_eq!(
        FILTER_TABS,
        [
            Rect {
                x: 272.0,
                y: 146.0,
                w: 72.0,
                h: 30.0
            },
            Rect {
                x: 350.0,
                y: 146.0,
                w: 72.0,
                h: 30.0
            },
            Rect {
                x: 428.0,
                y: 146.0,
                w: 72.0,
                h: 30.0
            },
        ]
    );
    assert_eq!(
        (0..PAGE_SIZE).map(row_rect).collect::<Vec<_>>(),
        [194.0, 266.0, 338.0, 410.0, 482.0, 554.0]
            .into_iter()
            .map(|y| Rect {
                x: 264.0,
                y,
                w: 556.0,
                h: 62.0,
            })
            .collect::<Vec<_>>()
    );

    assert_eq!(coordinates(WIDTH, HEIGHT, 0.0, 0.0), (0.0, 0.0));
    assert_eq!(coordinates(1600, 760, 200.0, 0.0), (0.0, 0.0));
    assert_eq!(coordinates(1600, 760, 0.0, 0.0), (-200.0, 0.0));
    assert_eq!(coordinates(2400, 1520, 1732.0, 1300.0), (866.0, 650.0));
    assert_eq!(coordinates(1200, 1200, 0.0, 0.0), (0.0, -220.0));
    assert_eq!(
        coordinates(1600, 760, 200.0 + 1200.0, 760.0),
        (1200.0, 760.0)
    );

    let (portrait_x, portrait_y) =
        coordinates(760, 1200, 0.0, (1200.0 - 760.0 * 760.0 / 1200.0) / 2.0);
    assert!((portrait_x - 0.0).abs() < 0.001);
    assert!((portrait_y - 0.0).abs() < 0.001);
    let (zero_x, zero_y) = coordinates(0, 0, 0.0, 0.0);
    assert_eq!((zero_x, zero_y), (600.0, 380.0));
    let (tiny_x, tiny_y) = coordinates(1, 1, 0.0, 0.0);
    assert!((tiny_x - 100.0).abs() < 0.001);
    assert!((tiny_y + 120.0).abs() < 0.001);

    // Rectangle edges map without hidden hit-test padding or rounding.
    for rect in BUTTONS
        .into_iter()
        .chain(FILTER_TABS)
        .chain((0..6).map(row_rect))
    {
        assert_eq!(coordinates(WIDTH, HEIGHT, rect.x, rect.y), (rect.x, rect.y));
        assert_eq!(
            coordinates(WIDTH, HEIGHT, rect.x + rect.w, rect.y + rect.h),
            (rect.x + rect.w, rect.y + rect.h)
        );
    }
}

type Panel = (f32, f32, f32, f32, u8, u8, u8, u8, f32);
type RgbPanel = (f32, f32, f32, f32, u8, u8, u8, f32);

fn panels(commands: &[GeometryCommand]) -> Vec<Panel> {
    commands
        .iter()
        .map(|command| match command {
            GeometryCommand::FillRect { .. } => panic!("illustration unexpectedly contains a fill"),
            GeometryCommand::RoundedRect {
                rect,
                color,
                radius,
            } => (
                rect.x, rect.y, rect.w, rect.h, color.r, color.g, color.b, color.a, *radius,
            ),
        })
        .collect()
}

#[test]
fn background_has_exact_scanlines_then_sidebar_and_separator() {
    let commands = background();
    assert_eq!(commands.len(), 762);
    for (y, command) in commands[..760].iter().enumerate() {
        let GeometryCommand::FillRect { rect, color } = command else {
            panic!("background scanline is not a fill");
        };
        assert_eq!(
            (rect.x, rect.y, rect.w, rect.h),
            (0.0, y as f32, 1200.0, 1.0)
        );
        assert_eq!(color.a, 255);
    }
    let fill = |rect, color| GeometryCommand::FillRect { rect, color };
    assert_eq!(
        commands[0],
        fill(
            Rect {
                x: 0.0,
                y: 0.0,
                w: 1200.0,
                h: 1.0
            },
            Rgba {
                r: 20,
                g: 30,
                b: 43,
                a: 255
            },
        )
    );
    assert_eq!(
        commands[380],
        fill(
            Rect {
                x: 0.0,
                y: 380.0,
                w: 1200.0,
                h: 1.0
            },
            Rgba {
                r: 16,
                g: 25,
                b: 36,
                a: 255
            },
        )
    );
    assert_eq!(
        commands[759],
        fill(
            Rect {
                x: 0.0,
                y: 759.0,
                w: 1200.0,
                h: 1.0
            },
            Rgba {
                r: 13,
                g: 20,
                b: 30,
                a: 255
            },
        )
    );
    assert_eq!(
        commands[760],
        fill(
            Rect {
                x: 0.0,
                y: 0.0,
                w: 240.0,
                h: 760.0
            },
            Rgba {
                r: 10,
                g: 16,
                b: 24,
                a: 225
            },
        )
    );
    assert_eq!(
        commands[761],
        fill(
            Rect {
                x: 239.0,
                y: 0.0,
                w: 1.0,
                h: 760.0
            },
            Rgba {
                r: 39,
                g: 52,
                b: 65,
                a: 255
            },
        )
    );
}

fn expected(rgb: &[RgbPanel]) -> Vec<Panel> {
    rgb.iter()
        .map(|&(x, y, w, h, r, g, b, radius)| (x, y, w, h, r, g, b, 255, radius))
        .collect()
}

#[test]
fn exact_ordered_illustration_geometry_at_origin() {
    let nes = expected(&[
        (-120., -43., 240., 94., 13, 24, 35, 8.),
        (-118., -49., 236., 94., 190, 198, 204, 6.),
        (-108., -39., 216., 74., 31, 39, 46, 3.),
        (-24., -25., 65., 5., 85, 97, 107, 2.),
        (-24., -15., 65., 5., 85, 97, 107, 2.),
        (-24., -5., 65., 5., 85, 97, 107, 2.),
        (-24., 26., 65., 5., 85, 97, 107, 2.),
        (-91., -6., 47., 15., 17, 25, 32, 2.),
        (-75., -22., 15., 47., 17, 25, 32, 2.),
        (-18., 10., 22., 8., 109, 122, 132, 4.),
        (13., 10., 22., 8., 109, 122, 132, 4.),
        (47., -1., 26., 26., 215, 50, 61, 13.),
        (76., -1., 26., 26., 215, 50, 61, 13.),
    ]);
    let famicom = expected(&[
        (-120., -43., 240., 94., 13, 24, 35, 8.),
        (-118., -49., 236., 94., 150, 40, 49, 6.),
        (-108., -39., 216., 74., 205, 179, 111, 3.),
        (-24., -25., 65., 5., 150, 40, 49, 2.),
        (-24., -15., 65., 5., 150, 40, 49, 2.),
        (-24., -5., 65., 5., 150, 40, 49, 2.),
        (-24., 26., 65., 5., 150, 40, 49, 2.),
        (-91., -6., 47., 15., 17, 25, 32, 2.),
        (-75., -22., 15., 47., 17, 25, 32, 2.),
        (-18., 10., 22., 8., 109, 122, 132, 4.),
        (13., 10., 22., 8., 109, 122, 132, 4.),
        (47., -1., 26., 26., 150, 40, 49, 13.),
        (76., -1., 26., 26., 150, 40, 49, 13.),
    ]);
    let snes = expected(&[
        (-118., -42., 236., 94., 13, 24, 35, 42.),
        (-114., -48., 228., 94., 59, 76, 94, 42.),
        (-106., -40., 212., 78., 43, 59, 77, 34.),
        (-84., -8., 48., 16., 18, 29, 41, 3.),
        (-68., -24., 16., 48., 18, 29, 41, 3.),
        (50., -28., 20., 20., 146, 142, 221, 10.),
        (71., -9., 20., 20., 94, 234, 210, 10.),
        (29., -9., 20., 20., 91, 170, 194, 10.),
        (50., 10., 20., 20., 214, 147, 179, 10.),
        (-14., 4., 17., 6., 131, 151, 168, 3.),
        (9., 4., 17., 6., 131, 151, 168, 3.),
    ]);
    for (kind, wanted) in [
        (ControllerIllustration::Nes, nes),
        (ControllerIllustration::Famicom, famicom),
        (ControllerIllustration::Snes, snes),
    ] {
        assert_eq!(panels(&controller_illustration(kind, 0., 0.)), wanted);
    }
}

#[test]
fn translation_preserves_all_non_position_geometry_and_returns_owned_vectors() {
    for kind in [
        ControllerIllustration::Nes,
        ControllerIllustration::Famicom,
        ControllerIllustration::Snes,
    ] {
        let origin = controller_illustration(kind, 0., 0.);
        let shifted = controller_illustration(kind, 37.5, -12.25);
        assert_eq!(origin.len(), shifted.len());
        for (a, b) in origin.iter().zip(&shifted) {
            let (
                GeometryCommand::RoundedRect {
                    rect: ar,
                    color: ac,
                    radius: aa,
                },
                GeometryCommand::RoundedRect {
                    rect: br,
                    color: bc,
                    radius: ba,
                },
            ) = (a, b)
            else {
                panic!("illustration unexpectedly contains a fill");
            };
            assert_eq!(
                (br.x, br.y, br.w, br.h, bc, ba),
                (ar.x + 37.5, ar.y - 12.25, ar.w, ar.h, ac, aa)
            );
        }
        let before = origin.clone();
        let mut other = controller_illustration(kind, 0., 0.);
        other.clear();
        assert!(other.is_empty());
        assert!(!origin.is_empty());
        assert_eq!(origin, before);
    }
}
