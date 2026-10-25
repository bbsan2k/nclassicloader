use clover_portable::frontend::menu_scene::{
    self, Command, Filter, Rect, Rgba, TextureId, TextureQuad,
};

#[test]
fn sprite_keys_preserve_legacy_alias_and_case_sensitive_suffix() {
    assert_eq!(
        menu_scene::sprite_key("caption_edge_left"),
        ("caption_edge.png".into(), true)
    );
    assert_eq!(
        menu_scene::sprite_key("caption_edge"),
        ("caption_edge.png".into(), false)
    );
    assert_eq!(
        menu_scene::sprite_key("caption_edge_left.png"),
        ("caption_edge_left.png".into(), false)
    );
    assert_eq!(
        menu_scene::sprite_key("thing.png"),
        ("thing.png".into(), false)
    );
    assert_eq!(
        menu_scene::sprite_key("thing.PNG"),
        ("thing.PNG.png".into(), false)
    );
}

#[test]
fn sprite_command_keeps_source_tint_filter_flip_and_rounds_centered_destination() {
    let texture = TextureId {
        generation: 8,
        slot: 3,
    };
    let source = Rect {
        x: 1.25,
        y: -2.5,
        w: 13.0,
        h: 7.0,
    };
    let centered = Rect {
        x: 10.25,
        y: -4.75,
        w: 5.0,
        h: 9.0,
    };
    let tint = Rgba {
        r: 11,
        g: 22,
        b: 33,
        a: 44,
    };
    let command = menu_scene::sprite(texture, source, centered, tint, true);
    assert_eq!(
        command,
        Command::Texture(TextureQuad {
            texture,
            destination: Rect {
                x: 8.0,
                y: -9.0,
                w: 5.0,
                h: 9.0
            },
            source: Some(source),
            tint,
            filter: Filter::Nearest,
            flip_x: true,
        })
    );
}

#[test]
fn source_snapshots_feed_existing_sprite_and_image_builders() {
    let sprite = menu_scene::AtlasSprite {
        texture: TextureId {
            generation: 12,
            slot: 2,
        },
        source: Rect {
            x: 3.0,
            y: 4.0,
            w: 15.0,
            h: 16.0,
        },
        flip_x: true,
    };
    assert_eq!(
        menu_scene::sprite(
            sprite.texture,
            sprite.source,
            Rect {
                x: 20.0,
                y: 30.0,
                w: 10.0,
                h: 12.0
            },
            Rgba {
                r: 1,
                g: 2,
                b: 3,
                a: 4
            },
            sprite.flip_x,
        ),
        Command::Texture(TextureQuad {
            texture: sprite.texture,
            destination: Rect {
                x: 15.0,
                y: 24.0,
                w: 10.0,
                h: 12.0
            },
            source: Some(sprite.source),
            tint: Rgba {
                r: 1,
                g: 2,
                b: 3,
                a: 4
            },
            filter: Filter::Nearest,
            flip_x: true,
        })
    );

    let image = menu_scene::ImageSource {
        texture: TextureId {
            generation: 13,
            slot: 0,
        },
        width: 80,
        height: 40,
    };
    assert_eq!(
        menu_scene::fit_image(
            image.texture,
            image.width,
            image.height,
            Rect {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 100.0
            },
            77,
        ),
        Command::Texture(TextureQuad {
            texture: image.texture,
            destination: Rect {
                x: 0.0,
                y: 25.0,
                w: 100.0,
                h: 50.0
            },
            source: None,
            tint: Rgba {
                r: 255,
                g: 255,
                b: 255,
                a: 77
            },
            filter: Filter::Linear,
            flip_x: false,
        })
    );
}

#[test]
fn sprite_commands_own_all_plain_presentation_values() {
    let mut input = String::from("avatar");
    let (key, flip) = menu_scene::sprite_key(&input);
    let command = menu_scene::sprite(
        TextureId {
            generation: 2,
            slot: 1,
        },
        Rect {
            x: 0.0,
            y: 1.0,
            w: 2.0,
            h: 3.0,
        },
        Rect {
            x: 4.0,
            y: 5.0,
            w: 6.0,
            h: 7.0,
        },
        Rgba {
            r: 1,
            g: 2,
            b: 3,
            a: 4,
        },
        flip,
    );
    input.clear();
    assert_eq!(key, "avatar.png");
    assert!(!flip);
    assert!(matches!(command, Command::Texture(_)));
}

#[test]
fn fit_image_preserves_portrait_landscape_square_and_fractional_geometry() {
    let texture = TextureId {
        generation: 17,
        slot: 4,
    };
    let fixtures = [
        (
            100,
            200,
            Rect {
                x: 0.0,
                y: 0.0,
                w: 300.0,
                h: 300.0,
            },
            Rect {
                x: 75.0,
                y: 0.0,
                w: 150.0,
                h: 300.0,
            },
        ),
        (
            200,
            100,
            Rect {
                x: 0.0,
                y: 0.0,
                w: 300.0,
                h: 300.0,
            },
            Rect {
                x: 0.0,
                y: 75.0,
                w: 300.0,
                h: 150.0,
            },
        ),
        (
            3,
            3,
            Rect {
                x: 0.0,
                y: 0.0,
                w: 10.0,
                h: 8.0,
            },
            Rect {
                x: 1.0,
                y: 0.0,
                w: 8.0,
                h: 8.0,
            },
        ),
        (
            4,
            3,
            Rect {
                x: 0.2,
                y: -1.1,
                w: 7.5,
                h: 6.5,
            },
            Rect {
                x: 0.2,
                y: -0.6625,
                w: 7.5,
                h: 5.625,
            },
        ),
    ];
    for (width, height, bounds, destination) in fixtures {
        assert_eq!(
            menu_scene::fit_image(texture, width, height, bounds, 93),
            Command::Texture(TextureQuad {
                texture,
                destination,
                source: None,
                tint: Rgba {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 93,
                },
                filter: Filter::Linear,
                flip_x: false,
            })
        );
    }
}

#[test]
fn fit_image_keeps_legacy_zero_and_negative_input_arithmetic() {
    let texture = TextureId {
        generation: 3,
        slot: 0,
    };
    let zero = menu_scene::fit_image(
        texture,
        0,
        0,
        Rect {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        },
        255,
    );
    let Command::Texture(quad) = zero else {
        panic!("fit_image returns a texture command")
    };
    assert!(quad.destination.w.is_nan());
    assert!(quad.destination.h.is_nan());

    let negative = menu_scene::fit_image(
        texture,
        2,
        1,
        Rect {
            x: 5.0,
            y: 7.0,
            w: -4.0,
            h: 10.0,
        },
        7,
    );
    let Command::Texture(quad) = negative else {
        panic!("fit_image returns a texture command")
    };
    assert_eq!(
        quad.destination,
        Rect {
            x: 5.0,
            y: 13.0,
            w: -4.0,
            h: -2.0,
        }
    );
}

#[test]
fn selection_plans_eight_names_and_exact_row_major_centered_rectangles() {
    let centered = Rect {
        x: 50.0,
        y: 40.0,
        w: 100.0,
        h: 80.0,
    };
    let placements = menu_scene::selection(false, centered, 0.0);
    let names = [
        "cursor_1",
        "cursor_game_2",
        "cursor_3",
        "cursor_game_4",
        "cursor_game_6",
        "cursor_7",
        "cursor_game_8",
        "cursor_9",
    ];
    let positions = [
        (16.0, 16.0, 32.0, 32.0),
        (50.0, 16.0, 36.0, 32.0),
        (84.0, 16.0, 32.0, 32.0),
        (16.0, 40.0, 32.0, 16.0),
        (84.0, 40.0, 32.0, 16.0),
        (16.0, 64.0, 32.0, 32.0),
        (50.0, 64.0, 36.0, 32.0),
        (84.0, 64.0, 32.0, 32.0),
    ];
    assert_eq!(placements.len(), 8);
    for ((placement, name), (x, y, w, h)) in placements.iter().zip(names).zip(positions) {
        assert_eq!(placement.name, name);
        assert_eq!(placement.centered, Rect { x, y, w, h });
        assert_eq!(
            placement.tint,
            Rgba {
                r: 191,
                g: 191,
                b: 255,
                a: 255,
            }
        );
    }
    assert!(
        menu_scene::selection(true, centered, 0.0)
            .iter()
            .all(|placement| placement.name.starts_with("cursor_game_"))
    );
}

#[test]
fn selection_preserves_pulse_boundaries_negative_wrap_and_small_dimensions() {
    let small = Rect {
        x: 3.0,
        y: 4.0,
        w: 20.0,
        h: 10.0,
    };
    let expected_red = [
        (0.0, 191),
        (0.5, 255),
        (1.25, 255),
        (2.0, 191),
        (-0.25, 191),
    ];
    for (time, red) in expected_red {
        let plan = menu_scene::selection(false, small, time);
        assert_eq!(plan.len(), 8);
        assert!(plan.iter().all(|sprite| sprite.tint.r == red));
    }
    let plan = menu_scene::selection(false, small, 1.625);
    assert!(plan.iter().all(|sprite| sprite.tint.r == 223));
    assert_eq!(plan[1].centered.w, -44.0);
    assert_eq!(plan[3].centered.h, -54.0);
}

#[test]
fn nes_wallpaper_is_one_filled_rect_with_the_supplied_rgba() {
    let background = Rgba {
        r: 9,
        g: 18,
        b: 27,
        a: 36,
    };
    assert_eq!(
        menu_scene::wallpaper(true, background, None, 123.5),
        vec![Command::Fill {
            rect: Rect {
                x: 0.0,
                y: 87.0,
                w: 1280.0,
                h: 552.0,
            },
            color: background,
        }]
    );
}

#[test]
fn snes_wallpaper_tiles_in_order_with_expected_filter_tint_and_offset() {
    let texture = TextureId {
        generation: 4,
        slot: 2,
    };
    for (offset, xs) in [
        (0.0, [-1664.0, -128.0, 1408.0]),
        (0.25, [-1664.25, -128.25, 1407.75]),
    ] {
        let commands = menu_scene::wallpaper(
            false,
            Rgba {
                r: 1,
                g: 2,
                b: 3,
                a: 4,
            },
            Some(texture),
            offset,
        );
        assert_eq!(commands.len(), 3);
        for (command, x) in commands.into_iter().zip(xs) {
            assert_eq!(
                command,
                Command::Texture(TextureQuad {
                    texture,
                    destination: Rect {
                        x,
                        y: 87.0,
                        w: 1536.0,
                        h: 552.0
                    },
                    source: None,
                    tint: Rgba {
                        r: 255,
                        g: 255,
                        b: 255,
                        a: 255
                    },
                    filter: Filter::Linear,
                    flip_x: false,
                })
            );
        }
    }
    assert!(
        menu_scene::wallpaper(
            false,
            Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 0
            },
            None,
            0.0
        )
        .is_empty()
    );
}
