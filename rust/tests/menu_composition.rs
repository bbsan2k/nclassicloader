use clover_portable::frontend::{
    font::{PlacedGlyph, TextLayout},
    menu_carousel::{self, CardInput, CardPart, CardResources, ResumeState},
    menu_scene::{
        self, AtlasSprite, Command, Filter, ImageSource, Rect, Rgba, TextPosition, TextureId,
    },
};
use std::collections::{BTreeMap, HashSet};

fn layout() -> TextLayout {
    TextLayout {
        glyphs: vec![PlacedGlyph {
            font: "title".into(),
            source: Rect {
                x: 1.0,
                y: 2.0,
                w: 6.0,
                h: 8.0,
            },
            x: 0.0,
            y: 0.0,
            width: 6.0,
            height: 8.0,
        }],
        advance: 8.0,
        line: 10.0,
        left: 0.0,
        top: 0.0,
        right: 6.0,
        bottom: 8.0,
    }
}

fn id(generation: usize, slot: usize) -> TextureId {
    TextureId { generation, slot }
}
fn sprite(generation: usize, slot: usize) -> AtlasSprite {
    AtlasSprite {
        texture: id(generation, slot),
        source: Rect {
            x: slot as f32,
            y: 1.0,
            w: 8.0,
            h: 9.0,
        },
        flip_x: slot.is_multiple_of(2),
    }
}
fn card_input(x: f32, active: bool, shift: f32) -> CardInput {
    CardInput {
        nes: false,
        x,
        shift,
        active,
        firmware_locked: true,
        players: 2,
        simultaneous: false,
        slots: [
            ResumeState::Empty,
            ResumeState::Saved,
            ResumeState::Locked,
            ResumeState::Empty,
        ],
    }
}
fn resources(plan: &menu_carousel::CardPlan, generation: usize, start: usize) -> CardResources {
    let sprites = plan
        .parts
        .iter()
        .enumerate()
        .filter_map(|(n, part)| match part {
            CardPart::Sprite(p) => Some((p.name.clone(), sprite(generation, start + n))),
            CardPart::Cover { .. } => None,
        })
        .collect();
    CardResources {
        sprites,
        cover: Some(ImageSource {
            texture: id(generation, start + 20),
            width: 80,
            height: 40,
        }),
    }
}

// This deliberately test-local assembly exercises public subview builders; it is not a
// production full-frame composer and its recorded hit rectangles grant no action authority.
fn compose(
    generation: usize,
    suspend: bool,
) -> (Vec<Command>, Vec<Rect>, [menu_carousel::ArrowHit; 2]) {
    let shift = if suspend { -52.0 } else { 0.0 };
    let layout = layout();
    let font_ids = BTreeMap::from([("title".into(), id(generation, 70))]);
    let caption = Some(sprite(generation, 71));
    let mut commands = menu_carousel::title_scene(
        false,
        shift,
        &layout,
        &font_ids,
        caption,
        Rgba {
            r: 10,
            g: 20,
            b: 30,
            a: 40,
        },
    );
    let first = menu_carousel::card(card_input(400.0, false, if suspend { -69.0 } else { 0.0 }));
    let first_scene = menu_carousel::card_scene(&first, &resources(&first, generation, 0));
    let second = menu_carousel::card(card_input(700.0, true, if suspend { -69.0 } else { 0.0 }));
    let second_scene = menu_carousel::card_scene(&second, &resources(&second, generation, 30));
    commands.extend(first_scene.commands);
    commands.extend(second_scene.commands);
    let selection = menu_scene::selection(
        false,
        Rect {
            x: 640.0,
            y: 360.0,
            w: 278.0,
            h: 302.0,
        },
        0.5,
    );
    let selection_sources: BTreeMap<_, _> = selection
        .iter()
        .enumerate()
        .map(|(n, p)| (p.name.clone(), sprite(generation, 50 + n)))
        .collect();
    commands.extend(menu_scene::sprite_scene(&selection, &selection_sources));
    let arrow_scene = menu_carousel::arrows(
        if suspend { -69.0 } else { 0.0 },
        Some(sprite(generation, 80)),
        Some(sprite(generation, 81)),
    );
    commands.extend(arrow_scene.commands);
    (
        commands,
        vec![first_scene.hit, second_scene.hit],
        arrow_scene.hits,
    )
}

#[test]
fn fake_host_composes_ordered_subviews_and_preserves_hit_data_across_suspend_and_missing_assets() {
    for suspend in [false, true] {
        let (commands, card_hits, arrow_hits) = compose(5, suspend);
        assert_eq!(commands.len(), 2 + 8 + 8 + 8 + 2);
        assert_eq!(
            card_hits,
            [
                Rect {
                    x: 274.0,
                    y: if suspend { 153.0 } else { 222.0 },
                    w: 252.0,
                    h: 276.0
                },
                Rect {
                    x: 574.0,
                    y: if suspend { 153.0 } else { 222.0 },
                    w: 252.0,
                    h: 276.0
                },
            ]
        );
        assert_eq!(
            commands[0],
            menu_scene::sprite(
                id(5, 71),
                sprite(5, 71).source,
                Rect {
                    x: 640.0,
                    y: 178.0 + if suspend { -52.0 } else { 0.0 },
                    w: 1044.0,
                    h: 66.0
                },
                Rgba {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 255
                },
                sprite(5, 71).flip_x
            )
        );
        let Command::Texture(glyph) = commands[1] else {
            panic!("title glyph follows caption")
        };
        assert_eq!(glyph.filter, Filter::Linear);
        assert_eq!(glyph.texture, id(5, 70));
        assert_eq!(
            commands[2],
            Command::Texture(TextureQuadExpected::card_backdrop(
                5,
                0,
                if suspend { -69.0 } else { 0.0 },
            ))
        );
        assert_eq!(arrow_hits[0].action, menu_carousel::ArrowAction::Previous);
        assert_eq!(
            arrow_hits[0].rect,
            Rect {
                x: 0.0,
                y: 240.0,
                w: 65.0,
                h: 255.0
            }
        );
        assert_eq!(arrow_hits[1].action, menu_carousel::ArrowAction::Next);
        assert_eq!(
            arrow_hits[1].rect,
            Rect {
                x: 1215.0,
                y: 240.0,
                w: 65.0,
                h: 255.0
            }
        );
    }
    let missing = menu_carousel::card(card_input(100.0, false, 0.0));
    let scene = menu_carousel::card_scene(&missing, &CardResources::default());
    assert!(scene.commands.is_empty());
    assert_eq!(
        scene.hit,
        Rect {
            x: -26.0,
            y: 222.0,
            w: 252.0,
            h: 276.0
        }
    );
    let arrows = menu_carousel::arrows(-69.0, None, None);
    assert!(arrows.commands.is_empty());
    assert_eq!(arrows.hits[0].rect.y, 240.0);
}

struct TextureQuadExpected;
impl TextureQuadExpected {
    fn card_backdrop(generation: usize, slot: usize, shift: f32) -> menu_scene::TextureQuad {
        let placement = menu_carousel::card(card_input(400.0, false, shift));
        let CardPart::Sprite(p) = &placement.parts[0] else {
            unreachable!()
        };
        let source = sprite(generation, slot);
        let Command::Texture(quad) = menu_scene::sprite(
            source.texture,
            source.source,
            p.centered,
            p.tint,
            source.flip_x,
        ) else {
            unreachable!()
        };
        quad
    }
}

#[test]
fn retained_commands_are_owned_and_read_only_replay_is_stable() {
    let first = {
        let (mut commands, _, _) = compose(6, false);
        let mut layout = layout();
        let mut fonts = BTreeMap::from([("title".to_owned(), id(6, 90))]);
        commands.extend(menu_scene::text(
            &layout,
            &fonts,
            TextPosition {
                x: 0.0,
                y: 0.0,
                max_width: 100.0,
                line_anchor: false,
                tint: Rgba {
                    r: 10,
                    g: 20,
                    b: 30,
                    a: 40,
                },
            },
        ));
        layout.glyphs.clear();
        fonts.clear();
        commands
    };
    assert_eq!(
        first.last(),
        Some(&Command::Texture(menu_scene::TextureQuad {
            texture: id(6, 90),
            destination: Rect {
                x: -3.0,
                y: -4.0,
                w: 6.0,
                h: 8.0
            },
            source: Some(Rect {
                x: 1.0,
                y: 2.0,
                w: 6.0,
                h: 8.0
            }),
            tint: Rgba {
                r: 10,
                g: 20,
                b: 30,
                a: 40
            },
            filter: Filter::Linear,
            flip_x: false,
        }))
    );
    let replay = first.clone();
    assert_eq!(replay, first);
    assert!(
        replay
            .iter()
            .all(|command| matches!(command, Command::Texture(_)))
    );
}

fn fake_accept(commands: &[Command], generation: usize, slots: &HashSet<usize>) -> Vec<Command> {
    commands
        .iter()
        .copied()
        .filter(|command| match command {
            Command::Fill { .. } => true,
            Command::Texture(quad) => {
                quad.texture.generation == generation && slots.contains(&quad.texture.slot)
            }
        })
        .collect()
}

#[test]
fn fake_host_rejects_foreign_and_unknown_ids_without_rebinding() {
    let retained = compose(1, false).0;
    let original = retained.clone();
    let slots: HashSet<_> = retained
        .iter()
        .filter_map(|command| match command {
            Command::Texture(quad) => Some(quad.texture.slot),
            Command::Fill { .. } => None,
        })
        .collect();
    assert_eq!(fake_accept(&retained, 1, &slots), retained);
    assert!(fake_accept(&retained, 2, &slots).is_empty());
    let Command::Texture(mut invalid) = retained[0] else {
        unreachable!()
    };
    invalid.texture.slot = usize::MAX;
    assert!(fake_accept(&[Command::Texture(invalid)], 1, &slots).is_empty());
    let mut foreign = invalid;
    foreign.texture = id(2, 71);
    assert_eq!(
        fake_accept(
            &[
                retained[0],
                Command::Texture(invalid),
                Command::Texture(foreign)
            ],
            1,
            &slots
        ),
        vec![retained[0]]
    );
    let rebuilt = compose(2, false).0;
    assert_eq!(fake_accept(&rebuilt, 2, &slots), rebuilt);
    for (old, new) in retained.iter().zip(&rebuilt) {
        let (Command::Texture(mut old), Command::Texture(new)) = (*old, *new) else {
            unreachable!()
        };
        assert_eq!(old.texture.generation, 1);
        assert_eq!(new.texture.generation, 2);
        old.texture.generation = 2;
        assert_eq!(old, new);
    }
    assert_eq!(retained, original);
}
