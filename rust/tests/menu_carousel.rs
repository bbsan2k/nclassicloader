use clover_portable::frontend::menu_carousel::{self, CardInput, CardPart, ResumeState};
use clover_portable::frontend::menu_scene::{
    self, AtlasSprite, Command, Filter, ImageSource, TextureId, TextureQuad,
};
use clover_portable::frontend::menu_scene::{Rect, Rgba, SpritePlacement};
use std::collections::BTreeMap;

fn input(nes: bool, active: bool, firmware_locked: bool, shift: f32) -> CardInput {
    CardInput {
        nes,
        x: 640.0,
        shift,
        active,
        firmware_locked,
        players: 2,
        simultaneous: true,
        slots: [
            ResumeState::Empty,
            ResumeState::Saved,
            ResumeState::Locked,
            ResumeState::Empty,
        ],
    }
}

#[test]
fn snes_card_recipe_keeps_part_order_styles_slots_and_hit_geometry() {
    let plan = menu_carousel::card(input(false, true, true, -69.0));
    let white = Rgba {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };
    assert_eq!(plan.parts.len(), 8);
    assert_eq!(
        plan.parts[0],
        CardPart::Sprite(SpritePlacement {
            name: "game_card_active".into(),
            centered: Rect {
                x: 640.0,
                y: 291.0,
                w: 252.0,
                h: 276.0
            },
            tint: white,
        })
    );
    assert_eq!(
        plan.parts[1],
        CardPart::Cover {
            bounds: Rect {
                x: 526.0,
                y: 165.0,
                w: 228.0,
                h: 204.0
            },
            alpha: 165
        }
    );
    let names = [
        "icon_lock",
        "icon_2P_simultaneous",
        "icon_Resume_off_active",
        "icon_Resume_on_active",
        "icon_Resume_lock_active",
        "icon_Resume_off_active",
    ];
    let rects = [
        Rect {
            x: 738.0,
            y: 170.0,
            w: 28.0,
            h: 36.0,
        },
        Rect {
            x: 721.0,
            y: 393.0,
            w: 66.0,
            h: 42.0,
        },
        Rect {
            x: 544.0,
            y: 399.0,
            w: 18.0,
            h: 18.0,
        },
        Rect {
            x: 568.0,
            y: 399.0,
            w: 18.0,
            h: 18.0,
        },
        Rect {
            x: 592.0,
            y: 399.0,
            w: 18.0,
            h: 18.0,
        },
        Rect {
            x: 616.0,
            y: 399.0,
            w: 18.0,
            h: 18.0,
        },
    ];
    for (offset, (name, rect)) in names.into_iter().zip(rects).enumerate() {
        let CardPart::Sprite(actual) = &plan.parts[offset + 2] else {
            panic!("sprite expected")
        };
        assert_eq!(actual.name, name);
        assert_eq!(actual.centered, rect);
        assert_eq!(actual.tint, white);
    }
    assert_eq!(
        plan.hit,
        Rect {
            x: 514.0,
            y: 153.0,
            w: 252.0,
            h: 276.0
        }
    );
}

#[test]
fn nes_inactive_card_uses_nes_offsets_and_unsuffixed_player_and_slots() {
    let plan = menu_carousel::card(input(true, false, false, 0.0));
    assert_eq!(plan.parts.len(), 7);
    assert_eq!(
        plan.parts[0],
        CardPart::Sprite(SpritePlacement {
            name: "game_card".into(),
            centered: Rect {
                x: 643.0,
                y: 363.0,
                w: 246.0,
                h: 270.0
            },
            tint: Rgba {
                r: 255,
                g: 255,
                b: 255,
                a: 255
            },
        })
    );
    assert_eq!(
        plan.parts[1],
        CardPart::Cover {
            bounds: Rect {
                x: 526.0,
                y: 234.0,
                w: 228.0,
                h: 204.0
            },
            alpha: 255
        }
    );
    let CardPart::Sprite(player) = &plan.parts[2] else {
        panic!("player sprite expected")
    };
    assert_eq!(player.name, "icon_2P_simultaneous");
    assert_eq!(
        player.centered,
        Rect {
            x: 716.5,
            y: 463.5,
            w: 57.0,
            h: 33.0
        }
    );
    for (index, state) in [
        "icon_Resume_off",
        "icon_Resume_on",
        "icon_Resume_lock",
        "icon_Resume_off",
    ]
    .into_iter()
    .enumerate()
    {
        let CardPart::Sprite(slot) = &plan.parts[index + 3] else {
            panic!("slot expected")
        };
        assert_eq!(slot.name, state);
        assert_eq!(
            slot.centered,
            Rect {
                x: 552.0 + index as f32 * 24.0,
                y: 468.0,
                w: 18.0,
                h: 18.0
            }
        );
    }
    assert_eq!(
        plan.hit,
        Rect {
            x: 514.0,
            y: 222.0,
            w: 252.0,
            h: 276.0
        }
    );
}

#[test]
fn player_count_policy_and_nes_active_suffix_are_preserved() {
    let mut value = input(true, true, false, -69.0);
    value.players = 1;
    value.simultaneous = false;
    let plan = menu_carousel::card(value);
    let CardPart::Sprite(player) = &plan.parts[2] else {
        panic!("player sprite expected")
    };
    assert_eq!(player.name, "icon_1P_active");
    assert_eq!(player.centered.y, 394.5);
    assert!(
        plan.parts[3..].iter().all(
            |part| matches!(part, CardPart::Sprite(sprite) if sprite.name.ends_with("_active"))
        )
    );
}

#[test]
fn typed_card_scene_preserves_order_geometry_and_current_descriptors() {
    let plan = menu_carousel::card(input(true, true, true, -69.0));
    let mut sprites = BTreeMap::new();
    for (slot, part) in plan.parts.iter().enumerate() {
        if let CardPart::Sprite(placement) = part {
            sprites.insert(
                placement.name.clone(),
                AtlasSprite {
                    texture: TextureId {
                        generation: 9,
                        slot,
                    },
                    source: Rect {
                        x: slot as f32,
                        y: 2.0,
                        w: 10.0,
                        h: 11.0,
                    },
                    flip_x: slot == 0,
                },
            );
        }
    }
    let cover = ImageSource {
        texture: TextureId {
            generation: 9,
            slot: 30,
        },
        width: 100,
        height: 50,
    };
    let resources = menu_carousel::CardResources {
        sprites: sprites.clone(),
        cover: Some(cover),
    };
    let scene = menu_carousel::card_scene(&plan, &resources);
    assert_eq!(scene.hit, plan.hit);
    assert_eq!(scene.commands.len(), plan.parts.len());
    for (index, (part, command)) in plan.parts.iter().zip(&scene.commands).enumerate() {
        match (part, command) {
            (CardPart::Sprite(placement), Command::Texture(quad)) => {
                let source = sprites[&placement.name];
                let expected = menu_scene::sprite(
                    source.texture,
                    source.source,
                    placement.centered,
                    placement.tint,
                    source.flip_x,
                );
                assert_eq!(*command, expected);
                assert_eq!(quad.filter, Filter::Nearest);
            }
            (CardPart::Cover { bounds, alpha }, Command::Texture(quad)) => {
                assert_eq!(
                    *command,
                    menu_scene::fit_image(
                        cover.texture,
                        cover.width,
                        cover.height,
                        *bounds,
                        *alpha
                    )
                );
                assert_eq!(quad.filter, Filter::Linear);
            }
            _ => panic!("typed scene retains plan part order at {index}"),
        }
    }
    let Command::Texture(first) = scene.commands[0] else {
        panic!("sprite command")
    };
    assert_eq!(first.texture.generation, 9);
    assert!(first.flip_x);
    assert_eq!(
        first.source,
        Some(Rect {
            x: 0.0,
            y: 2.0,
            w: 10.0,
            h: 11.0
        })
    );
}

#[test]
fn missing_resources_skip_draws_without_dropping_hit_and_stale_ids_stay_stale() {
    let plan = menu_carousel::card(input(false, false, false, 0.0));
    let stale = AtlasSprite {
        texture: TextureId {
            generation: 1,
            slot: 5,
        },
        source: Rect {
            x: 1.0,
            y: 2.0,
            w: 3.0,
            h: 4.0,
        },
        flip_x: false,
    };
    let mut sprites = BTreeMap::new();
    let sprite_name = match &plan.parts[0] {
        CardPart::Sprite(sprite) => sprite.name.clone(),
        _ => unreachable!(),
    };
    sprites.insert(sprite_name.clone(), stale);
    let scene = menu_carousel::card_scene(
        &plan,
        &menu_carousel::CardResources {
            sprites,
            cover: None,
        },
    );
    assert_eq!(scene.hit, plan.hit);
    assert_eq!(scene.commands.len(), 1);
    assert_eq!(
        scene.commands[0],
        Command::Texture(TextureQuad {
            texture: stale.texture,
            destination: match &plan.parts[0] {
                CardPart::Sprite(sprite) => Rect {
                    x: (sprite.centered.x - sprite.centered.w * 0.5).round(),
                    y: (sprite.centered.y - sprite.centered.h * 0.5).round(),
                    w: sprite.centered.w,
                    h: sprite.centered.h
                },
                _ => unreachable!(),
            },
            source: Some(stale.source),
            tint: Rgba {
                r: 255,
                g: 255,
                b: 255,
                a: 255
            },
            filter: Filter::Nearest,
            flip_x: false,
        })
    );
}

#[test]
fn scene_commands_are_owned_after_plan_and_resource_maps_drop() {
    let commands = {
        let plan = menu_carousel::card(input(false, false, false, 0.0));
        let mut sprites = BTreeMap::new();
        for (slot, part) in plan.parts.iter().enumerate() {
            if let CardPart::Sprite(sprite) = part {
                sprites.insert(
                    sprite.name.clone(),
                    AtlasSprite {
                        texture: TextureId {
                            generation: 7,
                            slot,
                        },
                        source: Rect {
                            x: 0.0,
                            y: 0.0,
                            w: 2.0,
                            h: 2.0,
                        },
                        flip_x: false,
                    },
                );
            }
        }
        menu_carousel::card_scene(
            &plan,
            &menu_carousel::CardResources {
                sprites,
                cover: Some(ImageSource {
                    texture: TextureId {
                        generation: 7,
                        slot: 99,
                    },
                    width: 2,
                    height: 1,
                }),
            },
        )
        .commands
    };
    assert_eq!(commands.len(), 7);
    assert!(
        commands
            .iter()
            .all(|command| matches!(command, Command::Texture(_)))
    );
}
