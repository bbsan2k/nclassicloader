use clover_portable::frontend::menu_scene::{self, AtlasSprite, Command, Rect, TextureId};
use std::collections::BTreeMap;

fn snapshots(plan: &[menu_scene::SpritePlacement]) -> BTreeMap<String, AtlasSprite> {
    plan.iter()
        .enumerate()
        .map(|(slot, placement)| {
            (
                placement.name.clone(),
                AtlasSprite {
                    texture: TextureId {
                        generation: 17,
                        slot,
                    },
                    source: Rect {
                        x: slot as f32,
                        y: 5.0,
                        w: 32.0,
                        h: 32.0,
                    },
                    flip_x: slot == 3,
                },
            )
        })
        .collect()
}

#[test]
fn selection_scene_emits_eight_typed_commands_in_plan_order_for_both_systems() {
    for nes in [false, true] {
        let plan = menu_scene::selection(
            nes,
            Rect {
                x: 640.0,
                y: 360.0,
                w: 278.0,
                h: 302.0,
            },
            0.75,
        );
        let sources = snapshots(&plan);
        let commands = menu_scene::sprite_scene(&plan, &sources);
        assert_eq!(commands.len(), 8);
        for (placement, command) in plan.iter().zip(&commands) {
            let source = sources[&placement.name];
            assert_eq!(
                *command,
                menu_scene::sprite(
                    source.texture,
                    source.source,
                    placement.centered,
                    placement.tint,
                    source.flip_x,
                )
            );
        }
        assert!(commands_are_nearest(&commands));
    }
}

// Rebuild a small representative command list for shared builder filter/tint assertions.
fn commands_are_nearest(commands: &[Command]) -> bool {
    commands.iter().all(|command| match command {
        Command::Texture(quad) => quad.filter == menu_scene::Filter::Nearest && quad.tint.a == 255,
        Command::Fill { .. } => false,
    })
}

#[test]
fn missing_sources_skip_only_draws_and_stale_ids_are_kept_after_inputs_drop() {
    let commands = {
        let plan = menu_scene::selection(
            false,
            Rect {
                x: 20.0,
                y: 30.0,
                w: 100.0,
                h: 80.0,
            },
            0.0,
        );
        let mut sources = snapshots(&plan);
        let missing = plan[2].name.clone();
        sources.remove(&missing);
        if let Some(source) = sources.get_mut(&plan[0].name) {
            source.texture.generation = 2;
        }
        let output = menu_scene::sprite_scene(&plan, &sources);
        assert_eq!(output.len(), 7);
        output
    };
    assert_eq!(commands.len(), 7);
    let Command::Texture(first) = commands[0] else {
        panic!("texture command")
    };
    assert_eq!(first.texture.generation, 2);
    assert_eq!(first.filter, menu_scene::Filter::Nearest);
}

#[test]
fn empty_plan_returns_no_commands() {
    assert!(menu_scene::sprite_scene(&[], &BTreeMap::new()).is_empty());
}
