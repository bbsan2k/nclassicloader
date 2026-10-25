use clover_portable::frontend::launcher_scene::{
    ControllerIllustration, DrawCommand, FontRole, Rgba, TextCommand, detail_header, detail_panel,
    detail_separator, unknown_controller_panel,
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
fn detail_header_sequence_and_styles_are_exact_for_all_controller_and_badge_states() {
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

    for controller in [
        None,
        Some(ControllerIllustration::Nes),
        Some(ControllerIllustration::Famicom),
        Some(ControllerIllustration::Snes),
    ] {
        for entry_exists in [false, true] {
            for ready in [false, true] {
                let mut expected = vec![DrawCommand::Geometry(detail_panel())];
                if let Some(kind) = controller {
                    expected.extend(
                        clover_portable::frontend::launcher_scene::controller_illustration(
                            kind, 1010.0, 272.0,
                        )
                        .into_iter()
                        .map(DrawCommand::Geometry),
                    );
                } else {
                    expected.push(DrawCommand::Geometry(unknown_controller_panel()));
                    expected.push(text("?", 993.0, 242.0, 44, FontRole::Semibold, MUTED, 50.0));
                }
                expected.push(text(
                    "Classic title",
                    866.0,
                    353.0,
                    25,
                    FontRole::Semibold,
                    INK,
                    290.0,
                ));
                expected.push(text(
                    "Version 1.2",
                    866.0,
                    390.0,
                    16,
                    FontRole::Regular,
                    MUTED,
                    290.0,
                ));
                expected.push(text(
                    if ready {
                        "VERIFIED CACHE"
                    } else if entry_exists {
                        "READY TO IMPORT"
                    } else {
                        "AWAITING GAME DATA"
                    },
                    866.0,
                    433.0,
                    12,
                    FontRole::Semibold,
                    ACCENT,
                    290.0,
                ));
                expected.push(DrawCommand::Geometry(detail_separator()));

                assert_eq!(
                    detail_header(
                        "Classic title",
                        "Version 1.2",
                        controller,
                        entry_exists,
                        ready,
                    ),
                    expected,
                    "controller={controller:?} entry_exists={entry_exists} ready={ready}"
                );
            }
        }
    }
}

#[test]
fn detail_header_owns_borrowed_title_and_version() {
    let commands = {
        let mut title = String::from("Borrowed title");
        let mut version = String::from("Borrowed version");
        let commands = detail_header(&title, &version, None, false, false);
        title.clear();
        version.clear();
        commands
    };

    assert!(commands.contains(&text(
        "Borrowed title",
        866.0,
        353.0,
        25,
        FontRole::Semibold,
        Rgba {
            r: 236,
            g: 242,
            b: 245,
            a: 255,
        },
        290.0,
    )));
    assert!(commands.contains(&text(
        "Borrowed version",
        866.0,
        390.0,
        16,
        FontRole::Regular,
        Rgba {
            r: 156,
            g: 171,
            b: 186,
            a: 255,
        },
        290.0,
    )));
}
