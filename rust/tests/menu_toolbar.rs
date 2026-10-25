use clover_portable::frontend::{
    font::{PlacedGlyph, TextLayout},
    menu_scene::{AtlasSprite, Command, Filter, Rgba, TextureId},
    menu_toolbar::{
        ALL, ToolbarAction, ToolbarInput, ToolbarItemResources, TooltipSources, TooltipStyle,
        toolbar, toolbar_scene, tooltip,
    },
};
use std::collections::BTreeMap;

fn input(hover: &str, panel: &str, keyboard_focus: bool, keyboard_index: usize) -> ToolbarInput {
    ToolbarInput {
        hover: hover.into(),
        panel: panel.into(),
        keyboard_focus,
        keyboard_index,
    }
}

#[test]
fn fixed_actions_positions_and_hits() {
    assert_eq!(
        ALL,
        [
            ToolbarAction::Display,
            ToolbarAction::Settings,
            ToolbarAction::Language,
            ToolbarAction::Copyright,
            ToolbarAction::Manual
        ]
    );
    let plans = toolbar(&input("", "", false, 0));
    for (i, plan) in plans.iter().enumerate() {
        let x = 640.0 + (i as f32 - 2.0) * 96.0;
        assert_eq!(plan.center_x, x);
        assert_eq!(
            plan.hit,
            clover_portable::frontend::menu_scene::Rect {
                x: x - 46.0,
                y: 27.0,
                w: 92.0,
                h: 70.0
            }
        );
        assert!(!plan.active && !plan.tooltip);
    }
}

#[test]
fn hover_panel_and_keyboard_policy() {
    let hovered = toolbar(&input("toolbar:2", "", false, 0));
    assert!(hovered[2].active && hovered[2].tooltip);
    let open = toolbar(&input("toolbar:2", "toolbar:2", true, 2));
    assert!(open[2].active && !open[2].tooltip);
    let blocked = toolbar(&input("", "settings", true, 2));
    assert!(!blocked[2].active && !blocked[2].tooltip);
    let out_of_range = toolbar(&input("", "", true, 5));
    assert!(
        out_of_range
            .iter()
            .all(|item| !item.active && !item.tooltip)
    );
    let exact = toolbar(&input("toolbar:02", "", false, 2));
    assert!(!exact[2].active && !exact[2].tooltip);
}

#[test]
fn tooltip_orders_owned_sprite_and_glyph_commands() {
    let make = |slot| AtlasSprite {
        texture: TextureId {
            generation: 7,
            slot,
        },
        source: clover_portable::frontend::font::Rect {
            x: slot as f32,
            y: 2.0,
            w: 10.0,
            h: 8.0,
        },
        flip_x: slot == 2,
    };
    let layout = TextLayout {
        glyphs: vec![],
        advance: 180.0,
        line: 1.0,
        left: 0.0,
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
    };
    let cmds = tooltip(
        300.0,
        TooltipStyle {
            body_y: 100.0,
            height: 60.0,
            edge_width: 10.0,
            minimum_width: 144.0,
            text_y: 128.0,
        },
        &layout,
        &BTreeMap::new(),
        &TooltipSources {
            body: Some(make(1)),
            left: Some(make(2)),
            right: Some(make(3)),
            arrow: Some(make(4)),
        },
        Rgba {
            r: 1,
            g: 2,
            b: 3,
            a: 4,
        },
    );
    assert_eq!(cmds.len(), 4);
    for (cmd, slot, x, flip) in [
        (&cmds[0], 1, 210.0, false),
        (&cmds[1], 2, 200.0, true),
        (&cmds[2], 3, 390.0, false),
        (&cmds[3], 4, 288.0, false),
    ] {
        let Command::Texture(q) = cmd else {
            panic!("expected texture")
        };
        assert_eq!(q.texture.slot, slot);
        assert_eq!(
            q.tint,
            Rgba {
                r: 255,
                g: 255,
                b: 255,
                a: 255
            }
        );
        assert_eq!(q.filter, Filter::Nearest);
        assert_eq!(q.flip_x, flip);
        assert_eq!(q.destination.x, x);
    }
}

#[test]
fn tooltip_minimum_width_and_missing_sources_preserve_glyphs() {
    let mut layout = TextLayout {
        glyphs: vec![],
        advance: 20.0,
        line: 1.0,
        left: 0.0,
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
    };
    layout.glyphs.push(PlacedGlyph {
        font: "s".into(),
        source: clover_portable::frontend::font::Rect {
            x: 0.0,
            y: 0.0,
            w: 5.0,
            h: 5.0,
        },
        x: 0.0,
        y: 0.0,
        width: 5.0,
        height: 5.0,
    });
    let atlas = BTreeMap::from([(
        "s".into(),
        TextureId {
            generation: 2,
            slot: 8,
        },
    )]);
    let style = TooltipStyle {
        body_y: 100.0,
        height: 60.0,
        edge_width: 10.0,
        minimum_width: 144.0,
        text_y: 128.0,
    };
    let commands = tooltip(
        300.0,
        style,
        &layout,
        &atlas,
        &TooltipSources {
            body: None,
            left: None,
            right: None,
            arrow: None,
        },
        Rgba {
            r: 8,
            g: 7,
            b: 6,
            a: 5,
        },
    );
    assert_eq!(commands.len(), 1);
    let Command::Texture(glyph) = commands[0] else {
        panic!("expected glyph")
    };
    assert_eq!(glyph.texture.slot, 8);
    assert_eq!(
        glyph.tint,
        Rgba {
            r: 8,
            g: 7,
            b: 6,
            a: 5
        }
    );
    drop(layout);
    assert_eq!(commands.len(), 1);
}

#[test]
fn toolbar_scene_preserves_each_items_draw_hit_tooltip_order() {
    let plans = toolbar(&input("toolbar:2", "", false, 0));
    let sprite = |slot| AtlasSprite {
        texture: TextureId {
            generation: 9,
            slot,
        },
        source: clover_portable::frontend::font::Rect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        },
        flip_x: false,
    };
    let resources = std::array::from_fn(|i| ToolbarItemResources {
        active: Some(sprite(20 + i)),
        icon: if i == 1 { None } else { Some(sprite(10 + i)) },
        tooltip: vec![Command::Fill {
            rect: clover_portable::frontend::font::Rect {
                x: i as f32,
                y: 0.0,
                w: 1.0,
                h: 1.0,
            },
            color: Rgba {
                r: 1,
                g: 1,
                b: 1,
                a: 1,
            },
        }],
    });
    let scene = toolbar_scene(9, &plans, &resources);
    assert_eq!(scene.generation, 9);
    let mut cursor = 0;
    for (i, action) in ALL.iter().enumerate() {
        if i == 2 {
            assert!(matches!(
                scene.steps[cursor],
                clover_portable::frontend::menu_frame::Step::Draw(_)
            ));
            cursor += 1;
        }
        if i != 1 {
            assert!(matches!(
                scene.steps[cursor],
                clover_portable::frontend::menu_frame::Step::Draw(_)
            ));
            cursor += 1;
        }
        assert!(
            matches!(&scene.steps[cursor], clover_portable::frontend::menu_frame::Step::Hit(hit) if hit.action == clover_portable::frontend::menu_frame::HitAction::Toolbar(*action))
        );
        cursor += 1;
        if i == 2 {
            assert!(matches!(
                scene.steps[cursor],
                clover_portable::frontend::menu_frame::Step::Draw(Command::Fill { .. })
            ));
            cursor += 1;
        }
    }
    assert_eq!(cursor, scene.steps.len());
}
