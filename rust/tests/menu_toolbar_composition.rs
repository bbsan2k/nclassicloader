use clover_portable::frontend::{
    font::{PlacedGlyph, Rect, TextLayout},
    menu_frame::{self, BaseMenuScene, HitAction, Step},
    menu_overlay::{self, OverlayRoute},
    menu_scene::{AtlasSprite, Command, Filter, Rgba, TextureId},
    menu_toolbar::{
        self, ToolbarAction, ToolbarInput, ToolbarItemResources, TooltipSources, TooltipStyle,
    },
};
use std::collections::BTreeMap;

fn id(slot: usize) -> TextureId {
    TextureId {
        generation: 44,
        slot,
    }
}
fn sprite(slot: usize, flip_x: bool) -> AtlasSprite {
    AtlasSprite {
        texture: id(slot),
        source: Rect {
            x: slot as f32,
            y: 2.0,
            w: 8.0,
            h: 9.0,
        },
        flip_x,
    }
}
fn fill(x: f32) -> Command {
    Command::Fill {
        rect: Rect {
            x,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        },
        color: Rgba {
            r: 1,
            g: 2,
            b: 3,
            a: 4,
        },
    }
}
fn layout() -> TextLayout {
    TextLayout {
        glyphs: vec![PlacedGlyph {
            font: "tiny".into(),
            source: Rect {
                x: 1.0,
                y: 2.0,
                w: 3.0,
                h: 4.0,
            },
            x: 0.0,
            y: 0.0,
            width: 3.0,
            height: 4.0,
        }],
        advance: 180.0,
        line: 10.0,
        left: 0.0,
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
    }
}

#[test]
fn combined_scene_orders_base_then_toolbar_items_and_reverse_hit_priority() {
    let input = ToolbarInput {
        hover: "toolbar:1".into(),
        panel: String::new(),
        keyboard_focus: true,
        keyboard_index: 3,
    };
    let plans = menu_toolbar::toolbar(&input);
    let resources: [ToolbarItemResources; 5] = std::array::from_fn(|i| ToolbarItemResources {
        active: Some(sprite(10 + i, false)),
        icon: Some(sprite(20 + i, false)),
        tooltip: if i == 1 || i == 3 {
            vec![fill(100.0 + i as f32)]
        } else {
            vec![]
        },
    });
    let scene = menu_toolbar::toolbar_scene(44, &plans, &resources);
    let base = BaseMenuScene {
        generation: 44,
        steps: vec![Step::Draw(fill(-1.0))],
    };
    let combined = menu_frame::with_toolbar(base, scene).unwrap();
    assert!(matches!(combined.steps[0], Step::Draw(Command::Fill { rect, .. }) if rect.x == -1.0));
    let toolbar_steps = &combined.steps[1..];
    let mut cursor = 0;
    for (i, plan) in plans.iter().enumerate() {
        if plan.active {
            let Step::Draw(Command::Texture(q)) = toolbar_steps[cursor] else {
                panic!("active draw")
            };
            assert_eq!(q.texture, id(10 + i));
            assert_eq!(
                q.destination,
                Rect {
                    x: plan.center_x - 48.0,
                    y: 27.0,
                    w: 96.0,
                    h: 70.0
                }
            );
            cursor += 1;
        }
        let Step::Draw(Command::Texture(icon)) = toolbar_steps[cursor] else {
            panic!("icon")
        };
        assert_eq!(icon.texture, id(20 + i));
        assert_eq!(
            icon.destination,
            Rect {
                x: plan.center_x - 36.0,
                y: 26.0,
                w: 72.0,
                h: 72.0
            }
        );
        cursor += 1;
        assert!(
            matches!(&toolbar_steps[cursor], Step::Hit(hit) if hit.action == HitAction::Toolbar(menu_toolbar::ALL[i]) && hit.rect == plan.hit)
        );
        cursor += 1;
        if plan.tooltip {
            assert!(
                matches!(toolbar_steps[cursor], Step::Draw(Command::Fill { rect, .. }) if rect.x == 100.0 + i as f32)
            );
            cursor += 1;
        }
    }
    assert_eq!(cursor, toolbar_steps.len());
    let hits: Vec<_> = combined
        .steps
        .iter()
        .filter_map(|step| match step {
            Step::Hit(hit) => Some(hit),
            _ => None,
        })
        .collect();
    assert_eq!(hits.len(), 5);
    assert_eq!(
        menu_frame::last_hit(&hits, plans[1].center_x, 40.0, |hit| hit.rect)
            .unwrap()
            .action,
        HitAction::Toolbar(ToolbarAction::Settings)
    );
    assert_eq!(
        menu_frame::last_hit(&hits, plans[1].center_x, 40.0, |hit| hit.rect)
            .unwrap()
            .action,
        HitAction::Toolbar(menu_toolbar::ALL[1])
    );
}

#[test]
fn missing_resources_keep_hits_and_open_panel_blocks_tooltip_draws() {
    let plans = menu_toolbar::toolbar(&ToolbarInput {
        hover: "toolbar:0".into(),
        panel: "open".into(),
        keyboard_focus: true,
        keyboard_index: 1,
    });
    let resources = std::array::from_fn(|_| ToolbarItemResources {
        active: None,
        icon: None,
        tooltip: vec![fill(1.0)],
    });
    let scene = menu_toolbar::toolbar_scene(44, &plans, &resources);
    assert_eq!(scene.steps.len(), 5);
    for (i, step) in scene.steps.iter().enumerate() {
        assert!(
            matches!(step, Step::Hit(hit) if hit.action == HitAction::Toolbar(menu_toolbar::ALL[i]))
        );
    }
    let inactive = menu_toolbar::toolbar(&ToolbarInput {
        hover: String::new(),
        panel: String::new(),
        keyboard_focus: false,
        keyboard_index: 0,
    });
    let resources = std::array::from_fn(|_| ToolbarItemResources::default());
    assert_eq!(
        menu_toolbar::toolbar_scene(44, &inactive, &resources)
            .steps
            .len(),
        5
    );
}

#[test]
fn scenes_own_captured_commands_and_preserve_mixed_ids_without_rebinding() {
    let input = ToolbarInput {
        hover: "toolbar:0".into(),
        panel: String::new(),
        keyboard_focus: false,
        keyboard_index: 0,
    };
    let plans = menu_toolbar::toolbar(&input);
    let mut atlases = BTreeMap::from([("tiny".to_owned(), id(90))]);
    let text_layout = layout();
    let tip = menu_toolbar::tooltip(
        640.0,
        TooltipStyle {
            body_y: 122.0,
            height: 66.0,
            edge_width: 12.0,
            minimum_width: 144.0,
            text_y: 128.0,
        },
        &text_layout,
        &atlases,
        &TooltipSources {
            body: Some(sprite(1, false)),
            left: None,
            right: None,
            arrow: Some(sprite(2, true)),
        },
        Rgba {
            r: 9,
            g: 8,
            b: 7,
            a: 6,
        },
    );
    let mut resources: [ToolbarItemResources; 5] =
        std::array::from_fn(|_| ToolbarItemResources::default());
    let mut stale_icon = sprite(7, false);
    stale_icon.texture.generation = 43;
    resources[0].icon = Some(stale_icon);
    resources[0].tooltip = tip;
    let scene = menu_toolbar::toolbar_scene(44, &plans, &resources);
    let toolbar = menu_toolbar::ToolbarScene {
        generation: scene.generation,
        steps: scene.steps.clone(),
    };
    let combined = menu_frame::with_toolbar(
        BaseMenuScene {
            generation: 44,
            steps: vec![Step::Draw(fill(3.0))],
        },
        toolbar,
    )
    .unwrap();
    atlases.insert("tiny".into(), id(99));
    resources[0].tooltip.clear();
    drop((input, plans, resources, atlases, text_layout));
    assert!(combined.steps.iter().any(
        |step| matches!(step, Step::Draw(Command::Texture(q)) if q.texture == TextureId { generation: 43, slot: 7 })
    ));
    assert!(combined.steps.iter().any(
        |step| matches!(step, Step::Draw(Command::Texture(q)) if q.texture == id(1) && !q.flip_x)
    ));
    assert!(combined.steps.iter().any(
        |step| matches!(step, Step::Draw(Command::Texture(q)) if q.texture == id(2) && q.flip_x)
    ));
    assert!(combined.steps.iter().any(|step| matches!(step, Step::Draw(Command::Texture(q)) if q.texture == id(90) && q.filter == Filter::Linear && q.tint == Rgba { r: 9, g: 8, b: 7, a: 6 })));
    assert!(
        menu_frame::with_toolbar(
            BaseMenuScene {
                generation: 43,
                steps: vec![Step::Draw(fill(0.0))]
            },
            scene
        )
        .is_none()
    );
}

#[test]
fn notices_are_owned_separate_and_routes_remain_closed() {
    let text_layout = layout();
    let atlases = BTreeMap::from([("tiny".to_owned(), id(5))]);
    let notice = menu_overlay::notice(44, &text_layout, &atlases);
    assert_eq!(notice.generation, 44);
    assert_eq!(
        notice.dialog,
        Rect {
            x: 304.0,
            y: 312.0,
            w: 672.0,
            h: 96.0
        }
    );
    assert_eq!(notice.commands.len(), 1);
    assert_eq!(menu_overlay::route("toolbar:bad"), OverlayRoute::Toolbar);
    assert_eq!(menu_overlay::route("toolbar"), OverlayRoute::None);
    assert_eq!(
        menu_overlay::route("confirm-delete"),
        OverlayRoute::ResumeConfirmation
    );
    let base = BaseMenuScene {
        generation: 44,
        steps: vec![],
    };
    let combined = menu_frame::with_toolbar(
        base,
        menu_toolbar::ToolbarScene {
            generation: 44,
            steps: vec![],
        },
    )
    .unwrap();
    assert!(combined.steps.is_empty());
    assert_eq!(notice.commands.len(), 1);
}
