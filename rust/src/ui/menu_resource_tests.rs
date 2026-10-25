use super::*;
use crate::frontend::{
    menu_carousel::ArrowAction,
    menu_chrome::HudAction,
    menu_frame::{BaseMenuScene, GameKey, Hit as SceneHit, HitAction, Step},
    menu_overlay,
    menu_scene::{AtlasSprite, Command, Filter, Rect, Rgba, TextureId, TextureQuad},
    menu_toolbar::{self, ToolbarInput, ToolbarItemResources},
};
use sdl3::pixels::PixelFormat;
use std::{collections::BTreeMap, path::PathBuf};

fn make_canvas(video: &sdl3::VideoSubsystem) -> Canvas<Window> {
    video
        .window("typed texture command test", 8, 8)
        .hidden()
        .build()
        .unwrap()
        .into_canvas()
}

fn assets(canvas: &Canvas<Window>) -> (UiAssets, TextureId) {
    let mut assets = UiAssets {
        nes: false,
        palette: json!({}),
        root: PathBuf::new(),
        creator: canvas.texture_creator(),
        textures: HashMap::new(),
        texture_bindings: texture_resources::TextureBindings::new().unwrap(),
        sprites: HashMap::new(),
        fonts: BTreeMap::new(),
        layout: json!({}),
        strings: json!({}),
        default_strings: json!({}),
        languages: BTreeMap::new(),
        title_color: Color::RGBA(255, 255, 255, 255),
        hud_color: Color::RGBA(255, 255, 255, 255),
        region: String::new(),
        locale: String::new(),
    };
    let mut rgba = [255, 0, 0, 255, 0, 255, 0, 255];
    let surface = Surface::from_data(&mut rgba, 2, 1, 8, PixelFormat::RGBA32).unwrap();
    let mut texture = assets.creator.create_texture_from_surface(surface).unwrap();
    texture.set_blend_mode(BlendMode::Blend);
    texture.set_scale_mode(ScaleMode::Nearest);
    assets.textures.insert("synthetic.png".into(), texture);
    let id = assets.texture_bindings.bind("synthetic.png");
    (assets, id)
}

fn command(texture: TextureId) -> Command {
    Command::Texture(TextureQuad {
        texture,
        destination: Rect {
            x: 0.0,
            y: 0.0,
            w: 8.0,
            h: 8.0,
        },
        source: None,
        tint: Rgba {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        },
        filter: Filter::Nearest,
        flip_x: false,
    })
}

fn texture_quad(command: Command) -> TextureQuad {
    match command {
        Command::Texture(quad) => quad,
        Command::Fill { .. } => panic!("expected texture command"),
    }
}

fn capture(canvas: &Canvas<Window>) -> Vec<u8> {
    let surface = canvas
        .read_pixels(None)
        .unwrap()
        .convert_format(PixelFormat::RGBA32)
        .unwrap();
    assert_eq!((surface.width(), surface.height()), (8, 8));
    let pitch = surface.pitch() as usize;
    let mut result = Vec::with_capacity(8 * 8 * 4);
    surface.with_lock(|source| {
        for y in 0..8 {
            result.extend_from_slice(&source[y * pitch..y * pitch + 8 * 4]);
        }
    });
    result
}

fn pixel(image: &[u8], x: usize, y: usize) -> [u8; 4] {
    image[(y * 8 + x) * 4..(y * 8 + x + 1) * 4]
        .try_into()
        .unwrap()
}

fn clear(canvas: &mut Canvas<Window>) -> Vec<u8> {
    canvas.set_draw_color(Color::RGBA(0, 0, 0, 255));
    canvas.clear();
    capture(canvas)
}

fn test_fill(color: Rgba) -> Command {
    Command::Fill {
        rect: Rect {
            x: 0.0,
            y: 0.0,
            w: 8.0,
            h: 8.0,
        },
        color,
    }
}

fn scene_hit(action: HitAction, x: f32) -> SceneHit {
    SceneHit {
        action,
        rect: Rect {
            x,
            y: 1.0,
            w: 2.0,
            h: 3.0,
        },
    }
}

fn test_frame(generation: usize, texture: TextureId) -> BaseMenuScene {
    BaseMenuScene {
        generation,
        steps: vec![
            Step::Draw(test_fill(Rgba {
                r: 10,
                g: 20,
                b: 30,
                a: 255,
            })),
            Step::Draw(command(texture)),
            Step::Hit(scene_hit(HitAction::Game(GameKey("stable".into())), 1.0)),
        ],
    }
}

#[test]
#[ignore = "requires SDL_VIDEODRIVER=dummy and SDL_RENDER_DRIVER=software"]
fn base_menu_executor_rejects_wrong_context_before_any_mutation() {
    assert_eq!(std::env::var("SDL_VIDEODRIVER").as_deref(), Ok("dummy"));
    assert_eq!(
        std::env::var("SDL_RENDER_DRIVER").as_deref(),
        Ok("software")
    );
    let context = sdl3::init().expect("caller-provided SDL drivers should initialize");
    let video = context.video().expect("SDL video should initialize");
    let mut canvas = make_canvas(&video);
    let (mut assets, valid_id) = assets(&canvas);
    let generation = valid_id.generation;
    let wrong_scene = test_frame(generation.wrapping_add(1), valid_id);
    let matching_scene = test_frame(generation, valid_id);
    let foreign_generation = generation.wrapping_add(1);
    let foreign_scene = test_frame(foreign_generation, valid_id);
    let current = super::menu_frame::BindingContext::new(
        generation,
        ["stable".to_owned(), "another-current-key".to_owned()],
    );
    let wrong_context =
        super::menu_frame::BindingContext::new(foreign_generation, ["stable".into()]);
    let foreign_context =
        super::menu_frame::BindingContext::new(foreign_generation, ["stable".into()]);
    let mut hits = vec![super::Hit {
        action: "sentinel".into(),
        rect: FRect::new(6.0, 5.0, 1.0, 1.0),
    }];

    let baseline = clear(&mut canvas);
    assert!(!assets.execute_base_menu(&mut canvas, &wrong_scene, &current, &mut hits));
    assert_eq!(capture(&canvas), baseline);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].action, "sentinel");

    assert!(!assets.execute_base_menu(&mut canvas, &matching_scene, &wrong_context, &mut hits));
    assert_eq!(capture(&canvas), baseline);
    assert_eq!(hits.len(), 1);

    assert!(!assets.execute_base_menu(&mut canvas, &foreign_scene, &foreign_context, &mut hits));
    assert_eq!(capture(&canvas), baseline);
    assert_eq!(hits.len(), 1);

    let mut valid_scene = BaseMenuScene {
        generation,
        steps: vec![
            Step::Draw(test_fill(Rgba {
                r: 0,
                g: 0,
                b: 200,
                a: 255,
            })),
            Step::Draw(command(valid_id)),
            Step::Draw(command(TextureId {
                generation,
                slot: usize::MAX,
            })),
            Step::Draw(command(TextureId {
                generation: foreign_generation,
                slot: valid_id.slot,
            })),
            Step::Hit(scene_hit(HitAction::Game(GameKey("stable".into())), 10.0)),
            Step::Hit(scene_hit(HitAction::Game(GameKey("removed".into())), 11.0)),
            Step::Hit(scene_hit(HitAction::Arrow(ArrowAction::Previous), 12.0)),
            Step::Hit(scene_hit(HitAction::Arrow(ArrowAction::Next), 13.0)),
            Step::Hit(scene_hit(HitAction::Hud(HudAction::Settings), 14.0)),
            Step::Hit(scene_hit(HitAction::Hud(HudAction::ResumePanel), 15.0)),
            Step::Hit(scene_hit(HitAction::Hud(HudAction::Sort), 16.0)),
            Step::Hit(scene_hit(HitAction::Hud(HudAction::Start), 17.0)),
            Step::Draw(Command::Fill {
                rect: Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 1.0,
                    h: 1.0,
                },
                color: Rgba {
                    r: 12,
                    g: 34,
                    b: 56,
                    a: 255,
                },
            }),
        ],
    };
    let before = hits.len();
    assert!(assets.execute_base_menu(&mut canvas, &valid_scene, &current, &mut hits));
    let image = capture(&canvas);
    assert_eq!(pixel(&image, 1, 4), [255, 0, 0, 255]);
    assert_eq!(pixel(&image, 6, 4), [0, 255, 0, 255]);
    assert_eq!(pixel(&image, 0, 0), [12, 34, 56, 255]);
    assert_eq!(hits.len(), before + 7);
    assert_eq!(
        hits[before..]
            .iter()
            .map(|hit| hit.action.as_str())
            .collect::<Vec<_>>(),
        [
            "game:stable",
            "previous",
            "next",
            "settings",
            "resume-panel",
            "sort",
            "start",
        ]
    );
    assert_eq!(hits[before].rect.x, 10.0);
    assert_eq!(hits[before + 6].rect.x, 17.0);
    valid_scene.steps.clear();
}

#[test]
#[ignore = "requires SDL_VIDEODRIVER=dummy and SDL_RENDER_DRIVER=software"]
fn typed_texture_commands_reject_stale_ids_and_replay_with_fresh_assets() {
    assert_eq!(std::env::var("SDL_VIDEODRIVER").as_deref(), Ok("dummy"));
    assert_eq!(
        std::env::var("SDL_RENDER_DRIVER").as_deref(),
        Ok("software")
    );
    let context = sdl3::init().expect("caller-provided SDL drivers should initialize");
    let video = context.video().expect("SDL video should initialize");
    let mut first_canvas = make_canvas(&video);
    let (mut first_assets, old_id) = assets(&first_canvas);
    let retained = command(old_id);

    clear(&mut first_canvas);
    assert!(first_assets.draw_command(&mut first_canvas, &retained));
    let baseline = capture(&first_canvas);
    assert_eq!(pixel(&baseline, 1, 4), [255, 0, 0, 255]);
    assert_eq!(pixel(&baseline, 6, 4), [0, 255, 0, 255]);

    for invalid in [
        TextureId {
            generation: 0,
            slot: old_id.slot,
        },
        TextureId {
            generation: old_id.generation.wrapping_add(1),
            slot: old_id.slot,
        },
        TextureId {
            generation: old_id.generation,
            slot: usize::MAX,
        },
    ] {
        let mut stale = texture_quad(retained);
        stale.texture = invalid;
        assert!(!first_assets.draw_command(&mut first_canvas, &Command::Texture(stale)));
        assert_eq!(capture(&first_canvas), baseline);
    }

    let mut flipped = texture_quad(retained);
    flipped.flip_x = true;
    assert!(first_assets.draw_command(&mut first_canvas, &Command::Texture(flipped)));
    let flipped_pixels = capture(&first_canvas);
    assert_eq!(pixel(&flipped_pixels, 1, 4), [0, 255, 0, 255]);
    assert_eq!(pixel(&flipped_pixels, 6, 4), [255, 0, 0, 255]);

    let mut modified = texture_quad(retained);
    modified.source = Some(Rect {
        x: 1.0,
        y: 0.0,
        w: 1.0,
        h: 1.0,
    });
    modified.tint = Rgba {
        r: 255,
        g: 128,
        b: 255,
        a: 128,
    };
    modified.filter = Filter::Linear;
    assert!(first_assets.draw_command(&mut first_canvas, &Command::Texture(modified)));
    let tinted = capture(&first_canvas);
    assert!((56..=72).contains(&pixel(&tinted, 4, 4)[1]));
    assert_ne!(tinted, baseline);
    assert!(first_assets.draw_command(&mut first_canvas, &retained));
    assert_eq!(capture(&first_canvas), baseline);

    assert!(first_assets.draw_command(
        &mut first_canvas,
        &Command::Fill {
            rect: Rect {
                x: 0.0,
                y: 0.0,
                w: 8.0,
                h: 8.0
            },
            color: Rgba {
                r: 12,
                g: 34,
                b: 56,
                a: 255
            },
        }
    ));
    assert_eq!(pixel(&capture(&first_canvas), 4, 4), [12, 34, 56, 255]);

    drop(first_assets);
    drop(first_canvas);
    let mut second_canvas = make_canvas(&video);
    let (mut second_assets, new_id) = assets(&second_canvas);
    assert_ne!(old_id, new_id);
    let cleared = clear(&mut second_canvas);
    assert!(!second_assets.draw_command(&mut second_canvas, &retained));
    assert_eq!(capture(&second_canvas), cleared);
    let fresh = command(new_id);
    assert!(second_assets.draw_command(&mut second_canvas, &fresh));
    assert_eq!(capture(&second_canvas), baseline);
}

#[test]
#[ignore = "requires caller-provided SDL_VIDEODRIVER=dummy and SDL_RENDER_DRIVER=software"]
fn combined_toolbar_and_notice_reject_stale_owners() {
    assert_eq!(std::env::var("SDL_VIDEODRIVER").as_deref(), Ok("dummy"));
    assert_eq!(
        std::env::var("SDL_RENDER_DRIVER").as_deref(),
        Ok("software")
    );
    let context = sdl3::init().expect("caller-provided SDL drivers should initialize");
    let video = context.video().expect("SDL video should initialize");
    let mut canvas = make_canvas(&video);
    let (mut art, texture) = assets(&canvas);
    let generation = texture.generation;
    let source = AtlasSprite {
        texture,
        source: Rect {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        },
        flip_x: false,
    };
    let mut plans = menu_toolbar::toolbar(&ToolbarInput {
        hover: "toolbar:0".into(),
        panel: String::new(),
        keyboard_focus: true,
        keyboard_index: 2,
    });
    for (index, plan) in plans.iter_mut().enumerate() {
        plan.center_x = 2.0 + index as f32;
        plan.hit = Rect {
            x: index as f32,
            y: 1.0,
            w: 1.0,
            h: 2.0,
        };
        plan.active = index == 0;
        plan.tooltip = index == 0;
    }
    let resources: [ToolbarItemResources; 5] = std::array::from_fn(|index| ToolbarItemResources {
        active: (index == 0).then_some(source),
        icon: Some(source),
        tooltip: if index == 0 {
            vec![Command::Fill {
                rect: Rect {
                    x: 6.0,
                    y: 6.0,
                    w: 1.0,
                    h: 1.0,
                },
                color: Rgba {
                    r: 0,
                    g: 0,
                    b: 240,
                    a: 255,
                },
            }]
        } else {
            vec![]
        },
    });
    let toolbar = menu_toolbar::toolbar_scene(generation, &plans, &resources);
    let base = BaseMenuScene {
        generation,
        steps: vec![
            Step::Draw(test_fill(Rgba {
                r: 12,
                g: 34,
                b: 56,
                a: 255,
            })),
            Step::Hit(scene_hit(HitAction::Hud(HudAction::Settings), 0.0)),
        ],
    };
    let combined = crate::frontend::menu_frame::with_toolbar(base, toolbar).unwrap();
    let current = super::menu_frame::BindingContext::new(generation, std::iter::empty());
    let mut hits = vec![super::Hit {
        action: "sentinel".into(),
        rect: FRect::new(7.0, 7.0, 1.0, 1.0),
    }];
    let baseline = clear(&mut canvas);
    assert!(art.execute_base_menu(&mut canvas, &combined, &current, &mut hits));
    let rendered = capture(&canvas);
    assert_ne!(rendered, baseline);
    assert_eq!(
        hits.iter()
            .map(|hit| hit.action.as_str())
            .collect::<Vec<_>>(),
        [
            "sentinel",
            "settings",
            "toolbar:0",
            "toolbar:1",
            "toolbar:2",
            "toolbar:3",
            "toolbar:4"
        ]
    );
    let toolbar_hits: Vec<_> = hits.iter().skip(2).collect();
    assert_eq!(toolbar_hits.len(), 5);

    let wrong_scene = BaseMenuScene {
        generation: generation.wrapping_add(1),
        steps: combined.steps.clone(),
    };
    let before_hits = hits.clone();
    clear(&mut canvas);
    let blank = capture(&canvas);
    assert!(!art.execute_base_menu(&mut canvas, &wrong_scene, &current, &mut hits));
    assert_eq!(capture(&canvas), blank);
    assert_eq!(hits.len(), before_hits.len());
    let foreign_context =
        super::menu_frame::BindingContext::new(generation.wrapping_add(1), std::iter::empty());
    let foreign_scene = BaseMenuScene {
        generation: generation.wrapping_add(1),
        steps: combined.steps.clone(),
    };
    assert!(!art.execute_base_menu(&mut canvas, &foreign_scene, &foreign_context, &mut hits));
    assert_eq!(capture(&canvas), blank);
    assert_eq!(hits.len(), before_hits.len());
    let wrong_context =
        super::menu_frame::BindingContext::new(generation.wrapping_add(1), std::iter::empty());
    assert!(!art.execute_base_menu(&mut canvas, &combined, &wrong_context, &mut hits));
    assert_eq!(capture(&canvas), blank);
    assert_eq!(hits.len(), before_hits.len());

    let mixed = BaseMenuScene {
        generation,
        steps: vec![
            Step::Draw(test_fill(Rgba {
                r: 8,
                g: 9,
                b: 10,
                a: 255,
            })),
            Step::Draw(command(TextureId {
                generation: generation.wrapping_add(1),
                slot: texture.slot,
            })),
            Step::Draw(command(TextureId {
                generation,
                slot: usize::MAX,
            })),
            Step::Hit(scene_hit(
                HitAction::Toolbar(menu_toolbar::ToolbarAction::Manual),
                4.0,
            )),
        ],
    };
    clear(&mut canvas);
    assert!(art.execute_base_menu(&mut canvas, &mixed, &current, &mut hits));
    let mixed_pixels = capture(&canvas);
    assert_eq!(pixel(&mixed_pixels, 4, 4), [8, 9, 10, 255]);
    assert_eq!(hits.last().unwrap().action, "toolbar:4");

    let missing_resources = std::array::from_fn(|_| ToolbarItemResources::default());
    let missing_scene = menu_toolbar::toolbar_scene(generation, &plans, &missing_resources);
    assert_eq!(
        missing_scene
            .steps
            .iter()
            .filter(|step| matches!(step, Step::Hit(_)))
            .count(),
        5
    );
    let mut missing_hits = Vec::new();
    let missing_frame = BaseMenuScene {
        generation,
        steps: missing_scene.steps,
    };
    let before_missing = capture(&canvas);
    assert!(art.execute_base_menu(&mut canvas, &missing_frame, &current, &mut missing_hits));
    assert_eq!(capture(&canvas), before_missing);
    assert_eq!(
        missing_hits
            .iter()
            .map(|hit| hit.action.as_str())
            .collect::<Vec<_>>(),
        [
            "toolbar:0",
            "toolbar:1",
            "toolbar:2",
            "toolbar:3",
            "toolbar:4"
        ]
    );

    let mut notice = menu_overlay::notice(
        generation,
        &crate::frontend::font::TextLayout {
            glyphs: vec![],
            advance: 0.0,
            line: 0.0,
            left: 0.0,
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
        },
        &BTreeMap::new(),
    );
    notice.dialog = Rect {
        x: 0.0,
        y: 0.0,
        w: 8.0,
        h: 8.0,
    };
    let mut notice_texture = texture_quad(command(texture));
    notice_texture.destination = Rect {
        x: 3.0,
        y: 3.0,
        w: 1.0,
        h: 1.0,
    };
    notice.commands.push(Command::Texture(notice_texture));
    let mut stale_glyph = texture_quad(command(texture));
    stale_glyph.texture.generation = generation.wrapping_add(1);
    stale_glyph.destination = Rect {
        x: 4.0,
        y: 4.0,
        w: 1.0,
        h: 1.0,
    };
    notice.commands.push(Command::Texture(stale_glyph));
    let mut stale_notice = notice.clone();
    stale_notice.generation = generation.wrapping_add(1);
    clear(&mut canvas);
    let notice_blank = capture(&canvas);
    assert!(!art.execute_notice(&mut canvas, &stale_notice));
    assert_eq!(capture(&canvas), notice_blank);
    assert!(art.execute_notice(&mut canvas, &notice));
    let notice_pixels = capture(&canvas);
    assert_eq!(pixel(&notice_pixels, 0, 0), [255, 255, 255, 255]);
    assert_eq!(pixel(&notice_pixels, 3, 3), [0, 255, 0, 255]);
    assert_eq!(pixel(&notice_pixels, 4, 4), [0, 0, 0, 255]);
    assert_ne!(notice_pixels, notice_blank);
    assert_eq!(hits.last().unwrap().action, "toolbar:4");
}

#[test]
#[ignore = "requires caller-provided SDL_VIDEODRIVER=dummy and SDL_RENDER_DRIVER=software"]
fn panel_executor_checks_owners_before_replacing_hits_actions_or_pixels() {
    use super::panel_scene::PanelBindings;
    use crate::frontend::menu_panel::{PanelKey, PanelScene, PanelStep};
    assert_eq!(std::env::var("SDL_VIDEODRIVER").as_deref(), Ok("dummy"));
    assert_eq!(
        std::env::var("SDL_RENDER_DRIVER").as_deref(),
        Ok("software")
    );
    let sdl = sdl3::init().unwrap();
    let video = sdl.video().unwrap();
    let mut canvas = make_canvas(&video);
    let (mut art, texture) = assets(&canvas);
    let generation = texture.generation;
    let key = |value: &str| PanelKey(value.into());
    let context = PanelBindings::new(generation, [key("hidden"), key("allowed"), key("close")]);
    let foreign = PanelBindings::new(generation + 1, [key("allowed")]);
    let mut valid = texture_quad(command(texture));
    valid.source = Some(Rect {
        x: 0.0,
        y: 0.0,
        w: 1.0,
        h: 1.0,
    });
    valid.destination = Rect {
        x: 3.0,
        y: 3.0,
        w: 1.0,
        h: 1.0,
    };
    let mut stale = valid;
    stale.texture.generation += 1;
    stale.destination.x = 4.0;
    let mut unknown = stale;
    unknown.texture = TextureId {
        generation,
        slot: usize::MAX,
    };
    unknown.destination.x = 5.0;
    let scene = PanelScene {
        generation,
        steps: vec![
            PanelStep::Draw(test_fill(Rgba {
                r: 20,
                g: 30,
                b: 40,
                a: 255,
            })),
            PanelStep::Dialog(Rect {
                x: 0.0,
                y: 0.0,
                w: 8.0,
                h: 8.0,
            }),
            PanelStep::Draw(Command::Texture(valid)),
            PanelStep::Draw(Command::Texture(stale)),
            PanelStep::Draw(Command::Texture(unknown)),
            PanelStep::Hit {
                key: key("allowed"),
                rect: valid.destination,
            },
            PanelStep::Hit {
                key: key("removed"),
                rect: stale.destination,
            },
            PanelStep::Hit {
                key: key("cancel-import"),
                rect: stale.destination,
            },
            PanelStep::Hit {
                key: key("close"),
                rect: unknown.destination,
            },
        ],
        actions: ["hidden", "allowed", "removed", "cancel-import", "close"]
            .map(key)
            .into(),
    };
    let retained = scene.clone();
    let mut wrong = scene.clone();
    wrong.generation += 1;
    let mut hits = vec![Hit {
        action: "sentinel".into(),
        rect: FRect::new(1.0, 2.0, 3.0, 4.0),
    }];
    let mut actions = vec!["sentinel-action".into()];
    let blank = clear(&mut canvas);
    for (frame, binding) in [(&wrong, &context), (&scene, &foreign), (&wrong, &foreign)] {
        assert!(!art.execute_panel(&mut canvas, frame, binding, &mut hits, &mut actions));
        assert_eq!(capture(&canvas), blank);
        assert_eq!(actions, ["sentinel-action"]);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].action, "sentinel");
        assert_eq!(
            from_frect(hits[0].rect),
            Rect {
                x: 1.0,
                y: 2.0,
                w: 3.0,
                h: 4.0
            }
        );
    }
    assert!(art.execute_panel(&mut canvas, &scene, &context, &mut hits, &mut actions));
    assert_eq!(
        actions,
        ["hidden", "allowed", "close"],
        "offscreen keyboard actions remain ordered"
    );
    assert_eq!(
        hits.iter()
            .map(|hit| hit.action.as_str())
            .collect::<Vec<_>>(),
        ["allowed", "close"]
    );
    let rendered = capture(&canvas);
    assert_eq!(pixel(&rendered, 0, 0), [255, 255, 255, 255]);
    assert_eq!(pixel(&rendered, 3, 3), [255, 0, 0, 255]);
    assert_eq!(pixel(&rendered, 4, 3), [0, 0, 0, 255]);
    assert_eq!(pixel(&rendered, 5, 3), [255, 255, 255, 255]);
    assert_eq!(scene, retained);
    let empty = PanelScene {
        generation,
        steps: vec![],
        actions: vec![],
    };
    assert!(art.execute_panel(&mut canvas, &empty, &context, &mut hits, &mut actions));
    assert!(
        hits.is_empty() && actions.is_empty(),
        "successful execution replaces, not appends"
    );
    assert_eq!(capture(&canvas), rendered);
}
