use clover_portable::frontend::{
    menu_carousel::{self, ArrowAction, ArrowHit},
    menu_scene::{self, AtlasSprite, Command, Rect, Rgba, TextureId},
};

fn sprite(generation: usize, slot: usize, flip_x: bool) -> AtlasSprite {
    AtlasSprite {
        texture: TextureId { generation, slot },
        source: Rect {
            x: slot as f32,
            y: 2.0,
            w: 8.0,
            h: 9.0,
        },
        flip_x,
    }
}

#[test]
fn arrows_preserve_command_and_hit_order_geometry_and_snapshots() {
    let left = sprite(11, 1, true);
    let right = sprite(12, 2, false);
    let white = Rgba {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };
    for (shift, y) in [(0.0, 360.0), (-69.0, 291.0)] {
        let scene = menu_carousel::arrows(shift, Some(left), Some(right));
        assert_eq!(
            scene.commands,
            vec![
                menu_scene::sprite(
                    left.texture,
                    left.source,
                    Rect {
                        x: 38.0,
                        y,
                        w: 32.0,
                        h: 44.0
                    },
                    white,
                    true
                ),
                menu_scene::sprite(
                    right.texture,
                    right.source,
                    Rect {
                        x: 1242.0,
                        y,
                        w: 32.0,
                        h: 44.0
                    },
                    white,
                    false
                ),
            ]
        );
        assert_eq!(
            scene.hits,
            [
                ArrowHit {
                    action: ArrowAction::Previous,
                    rect: Rect {
                        x: 0.0,
                        y: 240.0,
                        w: 65.0,
                        h: 255.0
                    }
                },
                ArrowHit {
                    action: ArrowAction::Next,
                    rect: Rect {
                        x: 1215.0,
                        y: 240.0,
                        w: 65.0,
                        h: 255.0
                    }
                },
            ]
        );
    }
}

#[test]
fn every_missing_source_combination_skips_only_its_draw_and_keeps_hits() {
    let left = sprite(3, 0, false);
    let right = sprite(3, 1, true);
    for (left_source, right_source, count) in [
        (None, None, 0),
        (Some(left), None, 1),
        (None, Some(right), 1),
        (Some(left), Some(right), 2),
    ] {
        let scene = menu_carousel::arrows(-52.0, left_source, right_source);
        assert_eq!(scene.commands.len(), count);
        assert_eq!(scene.hits[0].action, ArrowAction::Previous);
        assert_eq!(scene.hits[1].action, ArrowAction::Next);
        assert_eq!(
            scene.hits[0].rect,
            Rect {
                x: 0.0,
                y: 240.0,
                w: 65.0,
                h: 255.0
            }
        );
        assert_eq!(
            scene.hits[1].rect,
            Rect {
                x: 1215.0,
                y: 240.0,
                w: 65.0,
                h: 255.0
            }
        );
    }
}

#[test]
fn stale_texture_generation_is_retained_without_rebinding() {
    let old = sprite(1, 4, true);
    let scene = menu_carousel::arrows(0.0, Some(old), None);
    let Command::Texture(command) = scene.commands[0] else {
        panic!("arrow command")
    };
    assert_eq!(command.texture, old.texture);
    assert_eq!(command.texture.generation, 1);
    assert!(command.flip_x);
}
