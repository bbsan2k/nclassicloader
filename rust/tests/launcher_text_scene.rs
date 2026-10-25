use clover_portable::frontend::launcher_activity::CancellationView;
use clover_portable::frontend::launcher_scene::{
    DrawCommand, FILTER_TABS, FontRole, Rgba, TextCommand, brand_accent, branding, empty_library,
    empty_library_panel, filter_tab_background, footer, library_header, library_heading,
    page_indicator, primary_button, primary_button_background, sidebar, sidebar_button_background,
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
fn branding_is_exactly_the_ordered_owned_command_sequence() {
    assert_eq!(
        branding(),
        vec![
            DrawCommand::Text(TextCommand {
                value: "CLOVER".to_owned(),
                x: 28.0,
                y: 35.0,
                size: 32,
                font: FontRole::Semibold,
                color: Rgba {
                    r: 236,
                    g: 242,
                    b: 245,
                    a: 255
                },
                max_width: 180.0,
            }),
            DrawCommand::Text(TextCommand {
                value: "CLASSIC MENU / RUST".to_owned(),
                x: 29.0,
                y: 77.0,
                size: 12,
                font: FontRole::Semibold,
                color: Rgba {
                    r: 94,
                    g: 234,
                    b: 210,
                    a: 255
                },
                max_width: 190.0,
            }),
            DrawCommand::Geometry(brand_accent()),
            DrawCommand::Text(TextCommand {
                value: "A classic, reimagined.".to_owned(),
                x: 28.0,
                y: 146.0,
                size: 17,
                font: FontRole::Regular,
                color: Rgba {
                    r: 156,
                    g: 171,
                    b: 186,
                    a: 255
                },
                max_width: 190.0,
            }),
        ]
    );
}

#[test]
fn branding_strings_are_independently_owned() {
    let mut commands = branding();
    let DrawCommand::Text(first) = &mut commands[0] else {
        panic!("first branding command must be text");
    };
    first.value.push_str(" changed");

    let DrawCommand::Text(second) = &commands[1] else {
        panic!("second branding command must be text");
    };
    assert_eq!(second.value, "CLASSIC MENU / RUST");
    assert_eq!(
        branding()[0],
        DrawCommand::Text(TextCommand {
            value: "CLOVER".to_owned(),
            x: 28.0,
            y: 35.0,
            size: 32,
            font: FontRole::Semibold,
            color: Rgba {
                r: 236,
                g: 242,
                b: 245,
                a: 255
            },
            max_width: 180.0,
        })
    );
}

#[test]
fn empty_library_requests_preserve_geometry_text_order_and_filter_titles() {
    let ink = Rgba {
        r: 236,
        g: 242,
        b: 245,
        a: 255,
    };
    let muted = Rgba {
        r: 156,
        g: 171,
        b: 186,
        a: 255,
    };
    for (label, title) in [
        ("ALL", "Your library starts here.".to_owned()),
        ("NES", "No NES versions found.".to_owned()),
        ("SNES", "No SNES versions found.".to_owned()),
        ("unknown", "No unknown versions found.".to_owned()),
        ("Famicom 🎮", "No Famicom 🎮 versions found.".to_owned()),
    ] {
        assert_eq!(
            empty_library(label),
            vec![
                DrawCommand::Geometry(empty_library_panel()),
                text(&title, 294.0, 265.0, 28, FontRole::Semibold, ink, 490.0),
                text(
                    "Import a supplied .hsqs or .tar.gz image to begin.",
                    294.0,
                    317.0,
                    18,
                    FontRole::Regular,
                    muted,
                    490.0
                ),
                text(
                    "Already have game data? Open its assets folder.",
                    294.0,
                    347.0,
                    18,
                    FontRole::Regular,
                    muted,
                    490.0
                ),
                text(
                    "No game files are included or downloaded.",
                    294.0,
                    530.0,
                    16,
                    FontRole::Regular,
                    muted,
                    490.0
                ),
            ]
        );
    }
}

#[test]
fn empty_library_text_owns_filter_label_after_source_changes_and_drops() {
    let commands = {
        let mut label = String::from("NES");
        let commands = empty_library(&label);
        label.replace_range(.., "SNES");
        commands
    };
    let DrawCommand::Text(title) = &commands[1] else {
        panic!("second empty-library command must be text");
    };
    assert_eq!(title.value, "No NES versions found.");
}

#[test]
fn primary_button_commands_preserve_geometry_first_and_exact_text_for_all_states() {
    let dark = Rgba {
        r: 12,
        g: 35,
        b: 38,
        a: 255,
    };
    let muted = Rgba {
        r: 156,
        g: 171,
        b: 186,
        a: 255,
    };
    for (label, enabled) in [
        ("Cancel import", true),
        ("Cancelling…", false),
        ("Finishing import…", false),
        ("Preparing your Classic…", false),
    ] {
        for hot in [false, true] {
            assert_eq!(
                primary_button(label, enabled, hot),
                vec![
                    DrawCommand::Geometry(primary_button_background(enabled, hot)),
                    text(
                        label,
                        890.0,
                        666.0,
                        21,
                        FontRole::Semibold,
                        if enabled { dark } else { muted },
                        245.0,
                    ),
                ]
            );
        }
    }
}

#[test]
fn primary_button_command_owns_custom_unicode_label() {
    let commands = {
        let mut label = String::from("Custom 🎮");
        let commands = primary_button(&label, false, false);
        label.replace_range(.., "Changed");
        commands
    };
    let DrawCommand::Text(text) = &commands[1] else {
        panic!("second primary-button command must be text");
    };
    assert_eq!(text.value, "Custom 🎮");
}

#[test]
fn footer_commands_match_all_busy_cancellation_and_controller_combinations() {
    let controller_hint = "D-pad / Stick  Navigate    South / Start  Confirm    East  Back    Shoulders  System    F11  Full screen";
    let keyboard_hint = "Up / Down  Select    Enter  Launch    1 / 2 / 3  System    I  Import    D  Folder    F11  Full screen";
    let requested =
        "Cancellation requested    Waiting for a safe stopping point    F11  Full screen";
    for cancel in [
        CancellationView::Unavailable,
        CancellationView::Available,
        CancellationView::Requested,
        CancellationView::Finishing,
    ] {
        for controller_connected in [false, true] {
            for busy in [false, true] {
                let expected_value = match (busy, cancel, controller_connected) {
                    (false, _, true) | (true, CancellationView::Unavailable, true) => {
                        controller_hint
                    }
                    (false, _, false) | (true, CancellationView::Unavailable, false) => {
                        keyboard_hint
                    }
                    (true, CancellationView::Available, true) => {
                        "East  Cancel import    Waiting for a safe stopping point    F11  Full screen"
                    }
                    (true, CancellationView::Available, false) => {
                        "Escape  Cancel import    Waiting for a safe stopping point    F11  Full screen"
                    }
                    (true, CancellationView::Requested, _) => requested,
                    (true, CancellationView::Finishing, _) => {
                        "Finishing import    Cancellation is too late    F11  Full screen"
                    }
                };
                assert_eq!(
                    footer(busy, cancel, controller_connected),
                    text(
                        expected_value,
                        272.0,
                        728.0,
                        13,
                        FontRole::Regular,
                        Rgba {
                            r: 156,
                            g: 171,
                            b: 186,
                            a: 255,
                        },
                        880.0,
                    ),
                    "busy={busy}, cancel={cancel:?}, controller_connected={controller_connected}"
                );
            }
        }
    }
}

#[test]
fn page_indicator_preserves_legacy_arithmetic_and_exact_text_style() {
    for (selected, count, displayed) in [
        (0, 0, "0 / 0".to_owned()),
        (1, 0, "0 / 0".to_owned()),
        (usize::MAX, 0, "0 / 0".to_owned()),
        (0, 1, "1 / 1".to_owned()),
        (5, 6, "1 / 1".to_owned()),
        (6, 7, "2 / 2".to_owned()),
        (11, 12, "2 / 2".to_owned()),
        (12, 13, "3 / 3".to_owned()),
        (12, 1, "3 / 1".to_owned()),
        (
            usize::MAX,
            usize::MAX,
            format!(
                "{} / {}",
                usize::MAX / clover_portable::frontend::launcher_scene::PAGE_SIZE + 1,
                usize::MAX.div_ceil(clover_portable::frontend::launcher_scene::PAGE_SIZE)
            ),
        ),
    ] {
        assert_eq!(
            page_indicator(selected, count),
            text(
                &displayed,
                272.0,
                626.0,
                13,
                FontRole::Regular,
                Rgba {
                    r: 156,
                    g: 171,
                    b: 186,
                    a: 255,
                },
                100.0,
            ),
            "selected={selected}, count={count}"
        );
    }
}

#[test]
fn sidebar_commands_preserve_text_style_and_interleaving_for_all_states() {
    let accent = Rgba {
        r: 94,
        g: 234,
        b: 210,
        a: 255,
    };
    let ink = Rgba {
        r: 236,
        g: 242,
        b: 245,
        a: 255,
    };
    let muted = Rgba {
        r: 156,
        g: 171,
        b: 186,
        a: 255,
    };
    for busy in [false, true] {
        for first_active in [false, true] {
            for second_active in [false, true] {
                let active = [first_active, second_active];
                let mut expected: Vec<_> = library_heading()
                    .into_iter()
                    .map(DrawCommand::Geometry)
                    .collect();
                expected.push(text(
                    "Your library",
                    38.0,
                    226.0,
                    19,
                    FontRole::Semibold,
                    accent,
                    164.0,
                ));
                for (slot, index, label, y) in [
                    (0, 1, "+  Import image", 297.0),
                    (1, 2, "Open data folder", 354.0),
                ] {
                    if let Some(background) = sidebar_button_background(index, active[slot], busy) {
                        expected.push(DrawCommand::Geometry(background));
                    }
                    expected.push(text(
                        label,
                        38.0,
                        y,
                        18,
                        FontRole::Regular,
                        if busy { muted } else { ink },
                        172.0,
                    ));
                }
                expected.extend([
                    text(
                        "LOCAL. PRIVATE. YOURS.",
                        28.0,
                        590.0,
                        12,
                        FontRole::Semibold,
                        accent,
                        190.0,
                    ),
                    text(
                        "Your images stay read-only.",
                        28.0,
                        618.0,
                        15,
                        FontRole::Regular,
                        muted,
                        192.0,
                    ),
                    text(
                        "Saves live separately.",
                        28.0,
                        640.0,
                        15,
                        FontRole::Regular,
                        muted,
                        192.0,
                    ),
                    text(
                        "SDL3 + ares",
                        28.0,
                        714.0,
                        14,
                        FontRole::Regular,
                        muted,
                        192.0,
                    ),
                ]);
                assert_eq!(
                    sidebar(busy, active),
                    expected,
                    "busy={busy}, active={active:?}"
                );
            }
        }
    }
}

#[test]
fn library_header_commands_preserve_layout_and_all_filter_states() {
    let accent = Rgba {
        r: 94,
        g: 234,
        b: 210,
        a: 255,
    };
    let ink = Rgba {
        r: 236,
        g: 242,
        b: 245,
        a: 255,
    };
    let muted = Rgba {
        r: 156,
        g: 171,
        b: 186,
        a: 255,
    };
    let active_ink = Rgba {
        r: 12,
        g: 35,
        b: 38,
        a: 255,
    };
    for count in [0, 12, usize::MAX] {
        for selected in ["ALL", "NES", "SNES", "UNKNOWN"] {
            for busy in [false, true] {
                for hot_bits in 0..8 {
                    let hot = [hot_bits & 1 != 0, hot_bits & 2 != 0, hot_bits & 4 != 0];
                    let mut expected = vec![
                        text(
                            "MAKE YOURSELF AT HOME",
                            272.0,
                            35.0,
                            13,
                            FontRole::Semibold,
                            accent,
                            700.0,
                        ),
                        text(
                            "Choose your Classic.",
                            270.0,
                            59.0,
                            42,
                            FontRole::Semibold,
                            ink,
                            850.0,
                        ),
                        text(
                            "Pick a region and version, or bring your own filesystem image.",
                            272.0,
                            118.0,
                            18,
                            FontRole::Regular,
                            muted,
                            870.0,
                        ),
                        text(
                            &format!("YOUR LIBRARY  /  {count} SOURCES"),
                            530.0,
                            154.0,
                            12,
                            FontRole::Semibold,
                            muted,
                            285.0,
                        ),
                    ];
                    for (index, label) in ["ALL", "NES", "SNES"].into_iter().enumerate() {
                        let active = label == selected;
                        if let Some(background) =
                            filter_tab_background(index, active, hot[index], busy)
                        {
                            expected.push(DrawCommand::Geometry(background));
                        }
                        let tab = FILTER_TABS[index];
                        expected.push(text(
                            label,
                            tab.x + 14.0,
                            tab.y + 6.0,
                            13,
                            FontRole::Semibold,
                            if active { active_ink } else { ink },
                            52.0,
                        ));
                    }
                    assert_eq!(
                        library_header(count, selected, hot, busy),
                        expected,
                        "count={count}, selected={selected}, busy={busy}, hot={hot:?}"
                    );
                }
            }
        }
    }
}
