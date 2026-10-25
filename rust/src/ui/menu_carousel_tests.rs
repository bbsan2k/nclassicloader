use super::*;
use crate::frontend::{
    font::{Font, Glyph, Rect},
    menu_carousel::{self, CardInput, CardPart, ResumeState},
    menu_chrome::{self, HudPrompt},
    menu_frame::{BaseMenuInput, BaseMenuScene, GameKey, Step},
    menu_overlay,
    menu_scene::{self, Command, Filter, Rgba},
    menu_toolbar::{self, ToolbarInput, ToolbarItemResources, TooltipSources, TooltipStyle},
};
use sdl3::pixels::PixelFormat;
use std::{collections::BTreeMap, path::PathBuf};

pub(super) fn canvas(video: &sdl3::VideoSubsystem) -> Canvas<Window> {
    video
        .window("carousel retained scene test", 1280, 720)
        .hidden()
        .build()
        .unwrap()
        .into_canvas()
}

pub(super) fn add_texture(
    assets: &mut UiAssets,
    key: &str,
    width: u32,
    height: u32,
    color: [u8; 4],
) {
    let mut rgba = vec![0; (width * height * 4) as usize];
    for (index, pixel) in rgba.chunks_exact_mut(4).enumerate() {
        let x = index as u32 % width;
        let y = index as u32 / width;
        let shade = if (x / 8 + y / 8).is_multiple_of(2) {
            0
        } else {
            35
        };
        pixel.copy_from_slice(&[
            color[0].saturating_add(shade),
            color[1].saturating_add(shade),
            color[2].saturating_add(shade),
            color[3],
        ]);
    }
    let surface =
        Surface::from_data(&mut rgba, width, height, width * 4, PixelFormat::RGBA32).unwrap();
    let mut texture = assets.creator.create_texture_from_surface(surface).unwrap();
    texture.set_blend_mode(BlendMode::Blend);
    texture.set_scale_mode(ScaleMode::Nearest);
    assets.textures.insert(key.to_owned(), texture);
    assets.texture_bindings.bind(key);
}

pub(super) fn assets(canvas: &Canvas<Window>) -> UiAssets {
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
        title_color: Color::RGBA(230, 210, 170, 255),
        hud_color: Color::RGBA(255, 255, 255, 255),
        region: String::new(),
        locale: String::new(),
    };
    add_texture(&mut assets, "skin/packed.png", 256, 256, [80, 30, 170, 255]);
    add_texture(
        &mut assets,
        "games/demo/demo.png",
        32,
        24,
        [20, 180, 70, 255],
    );
    add_texture(
        &mut assets,
        "skin/font-title.png",
        32,
        16,
        [230, 180, 15, 255],
    );
    assets.fonts.insert(
        "title".into(),
        Font {
            size: 16.0,
            line: 18.0,
            baseline: 12.0,
            glyphs: BTreeMap::from([(
                'A',
                Glyph {
                    source: Rect {
                        x: 0.0,
                        y: 0.0,
                        w: 8.0,
                        h: 10.0,
                    },
                    x: 0.0,
                    y: 0.0,
                    advance: 9.0,
                },
            )]),
            kernings: BTreeMap::new(),
        },
    );
    for nes in [false, true] {
        for active in [false, true] {
            let plan = menu_carousel::card(CardInput {
                nes,
                x: 640.0,
                shift: 0.0,
                active,
                firmware_locked: true,
                players: 2,
                simultaneous: true,
                slots: [
                    ResumeState::Empty,
                    ResumeState::Saved,
                    ResumeState::Locked,
                    ResumeState::Empty,
                ],
            });
            for part in plan.parts {
                if let CardPart::Sprite(sprite) = part
                    && !assets.sprites.contains_key(&format!("{}.png", sprite.name))
                {
                    let slot = assets.sprites.len() % 20;
                    assets.sprites.insert(
                        format!("{}.png", sprite.name),
                        FRect::new((slot * 10) as f32, 0.0, 8.0, 8.0),
                    );
                }
            }
        }
        let selection = menu_scene::selection(
            nes,
            Rect {
                x: 640.0,
                y: 360.0,
                w: 278.0,
                h: 302.0,
            },
            0.75,
        );
        for sprite in selection {
            if !assets.sprites.contains_key(&format!("{}.png", sprite.name)) {
                let slot = assets.sprites.len() % 20;
                assets.sprites.insert(
                    format!("{}.png", sprite.name),
                    FRect::new((slot * 10) as f32, 10.0, 8.0, 8.0),
                );
            }
        }
    }
    for name in ["caption_title", "arrow_left", "arrow_right"] {
        let slot = assets.sprites.len() % 20;
        assets.sprites.insert(
            format!("{name}.png"),
            FRect::new((slot * 10) as f32, 20.0, 8.0, 8.0),
        );
    }
    assets
}

fn scene(
    assets: &UiAssets,
    nes: bool,
    title_shift: f32,
    card_shift: f32,
) -> (Vec<Command>, Vec<Rect>) {
    let title = assets.measure("A", "title");
    let commands = menu_carousel::title_scene(
        nes,
        title_shift,
        &title,
        &assets.font_atlases(),
        assets.sprite_source("caption_title"),
        Rgba {
            r: assets.title_color.r,
            g: assets.title_color.g,
            b: assets.title_color.b,
            a: assets.title_color.a,
        },
    );
    assert_eq!(
        &commands[1..],
        assets.text_commands(
            "A",
            640.0,
            176.0 + title_shift,
            "title",
            assets.title_color,
            990.0,
            true
        )
    );
    let mut all = commands;
    let plan = menu_carousel::card(CardInput {
        nes,
        x: 640.0,
        shift: card_shift,
        active: true,
        firmware_locked: true,
        players: 2,
        simultaneous: true,
        slots: [
            ResumeState::Empty,
            ResumeState::Saved,
            ResumeState::Locked,
            ResumeState::Empty,
        ],
    });
    let hit = plan.hit;
    let resources = assets.card_resources(&plan, "games/demo/demo.png");
    all.extend(menu_carousel::card_scene(&plan, &resources).commands);
    let selection = menu_scene::selection(
        nes,
        Rect {
            x: 640.0,
            y: 360.0,
            w: 278.0,
            h: 302.0,
        },
        0.75,
    );
    let mut sources = BTreeMap::new();
    for sprite in &selection {
        if let Some(source) = assets.sprite_source(&sprite.name) {
            sources.insert(sprite.name.clone(), source);
        }
    }
    all.extend(menu_scene::sprite_scene(&selection, &sources));
    all.extend(
        menu_carousel::arrows(
            card_shift,
            assets.sprite_source("arrow_left"),
            assets.sprite_source("arrow_right"),
        )
        .commands,
    );
    (all, vec![hit])
}

pub(super) fn capture(canvas: &Canvas<Window>) -> Vec<u8> {
    let surface = canvas
        .read_pixels(None)
        .unwrap()
        .convert_format(PixelFormat::RGBA32)
        .unwrap();
    assert_eq!((surface.width(), surface.height()), (1280, 720));
    let pitch = surface.pitch() as usize;
    let mut pixels = Vec::with_capacity(1280 * 720 * 4);
    surface.with_lock(|bytes| {
        for y in 0..720 {
            pixels.extend_from_slice(&bytes[y * pitch..y * pitch + 1280 * 4]);
        }
    });
    pixels
}

fn render(canvas: &mut Canvas<Window>, assets: &mut UiAssets, commands: &[Command]) {
    canvas.set_draw_color(Color::RGBA(0, 0, 0, 255));
    canvas.clear();
    for command in commands {
        assert!(assets.draw_command(canvas, command));
    }
}

fn changed(pixels: &[u8]) -> bool {
    pixels.chunks_exact(4).any(|p| p[..3] != [0, 0, 0])
}

fn full_frame(
    assets: &mut UiAssets,
    nes: bool,
    suspend: bool,
) -> (BaseMenuScene, menu_frame::BindingContext) {
    assets.nes = nes;
    let generation = assets.texture_bindings.generation();
    let shift = if suspend { -69.0 } else { 0.0 };
    let title_shift = if suspend { -52.0 } else { 0.0 };
    let background = Rgba {
        r: 8,
        g: 12,
        b: 28,
        a: 255,
    };
    let wallpaper = menu_scene::wallpaper(
        nes,
        background,
        assets.texture_bindings.id("skin/wall.png"),
        17.0,
    );
    let title = menu_carousel::title_scene(
        nes,
        title_shift,
        &assets.measure("A", "title"),
        &assets.font_atlases(),
        assets.sprite_source("caption_title"),
        Rgba {
            r: 230,
            g: 210,
            b: 170,
            a: 255,
        },
    );
    let card_plan = menu_carousel::card(CardInput {
        nes,
        x: 640.0,
        shift,
        active: true,
        firmware_locked: true,
        players: 2,
        simultaneous: true,
        slots: [
            ResumeState::Empty,
            ResumeState::Saved,
            ResumeState::Locked,
            ResumeState::Empty,
        ],
    });
    for part in &card_plan.parts {
        if let CardPart::Sprite(sprite) = part {
            let name = format!("{}.png", sprite.name);
            if !assets.sprites.contains_key(&name) {
                let slot = assets.sprites.len() % 20;
                assets
                    .sprites
                    .insert(name, FRect::new((slot * 10) as f32, 30.0, 8.0, 8.0));
            }
        }
    }
    let card_resources = assets.card_resources(&card_plan, "games/demo/demo.png");
    let card = menu_carousel::card_scene(&card_plan, &card_resources);
    let selection_plan = menu_scene::selection(
        nes,
        Rect {
            x: 640.0,
            y: 360.0,
            w: 278.0,
            h: 302.0,
        },
        0.75,
    );
    let mut selection_sources = BTreeMap::new();
    for placement in &selection_plan {
        if let Some(source) = assets.sprite_source(&placement.name) {
            selection_sources.insert(placement.name.clone(), source);
        }
    }
    let selection = menu_scene::sprite_scene(&selection_plan, &selection_sources);
    let arrows = menu_carousel::arrows(
        shift,
        assets.sprite_source("arrow_left"),
        assets.sprite_source("arrow_right"),
    );
    let thumbnail_placement = menu_scene::thumbnails(1, 0).remove(0);
    let thumbnail = menu_chrome::thumbnail_scene(
        thumbnail_placement,
        assets.image_source("games/demo/demo_small.png"),
        assets.sprite_source("curor_thumbnail_1"),
        assets.sprite_source("icon_lock"),
    );
    let prompts = std::array::from_fn(|i| {
        let icons = [
            "icon_direction_up_light",
            "icon_direction_down_light",
            "icon_select_light",
            "icon_start_light",
        ];
        HudPrompt {
            icon: assets.sprite_source(icons[i]),
            icon_width: [30.0, 30.0, 90.0, 78.0][i],
            layout: assets.measure("A", "title"),
        }
    });
    let hud = menu_chrome::hud_scene(
        nes,
        8.0,
        24.0,
        &prompts,
        &assets.font_atlases(),
        Rgba {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        },
    );
    let bars = menu_chrome::menu_bars(
        nes,
        false,
        assets.sprite_source("menubar_U"),
        assets.sprite_source("menubar_L"),
        assets.sprite_source("menubar_U_2"),
    );
    let frame = crate::frontend::menu_frame::compose(BaseMenuInput {
        generation,
        wallpaper,
        title,
        cards: vec![(GameKey("demo".into()), card)],
        selection,
        arrows,
        thumbnails: vec![(GameKey("demo".into()), thumbnail)],
        hud,
        bars,
    });
    let binding = menu_frame::BindingContext::new(generation, ["demo".to_owned()]);
    (frame, binding)
}

fn install_complete_frame_resources(assets: &mut UiAssets) {
    add_texture(assets, "skin/wall.png", 32, 16, [12, 30, 80, 255]);
    add_texture(
        assets,
        "games/demo/demo_small.png",
        32,
        24,
        [190, 70, 30, 255],
    );
    let names = [
        "caption_title",
        "arrow_left",
        "arrow_right",
        "curor_thumbnail_1",
        "icon_lock",
        "icon_direction_up_light",
        "icon_direction_down_light",
        "icon_select_light",
        "icon_start_light",
        "menubar_U",
        "menubar_L",
        "menubar_U_2",
    ];
    for (slot, name) in names.into_iter().enumerate() {
        assets.sprites.insert(
            format!("{name}.png"),
            FRect::new((slot * 10) as f32, 40.0, 8.0, 8.0),
        );
    }
}

fn install_toolbar_resources(assets: &mut UiAssets) {
    let names = [
        "btn_display",
        "btn_option",
        "btn_language",
        "btn_copyright",
        "manual",
        "btn_menubar_active",
        "caption",
        "caption_edge",
        "caption_arrow",
    ];
    for (index, name) in names.into_iter().enumerate() {
        assets.sprites.insert(
            format!("{name}.png"),
            FRect::new((index * 16) as f32, 80.0, 12.0, 12.0),
        );
    }
    assets.sprites.insert(
        "caption_edge_left.png".into(),
        FRect::new(128.0, 80.0, 12.0, 12.0),
    );
    add_texture(assets, "skin/font-s.png", 32, 16, [230, 180, 15, 255]);
    assets.fonts.insert(
        "s".into(),
        Font {
            size: 16.0,
            line: 18.0,
            baseline: 12.0,
            glyphs: BTreeMap::from([(
                'A',
                Glyph {
                    source: Rect {
                        x: 0.0,
                        y: 0.0,
                        w: 8.0,
                        h: 10.0,
                    },
                    x: 0.0,
                    y: 0.0,
                    advance: 9.0,
                },
            )]),
            kernings: BTreeMap::new(),
        },
    );
}

fn project_toolbar(
    assets: &UiAssets,
    hover: &str,
    panel: &str,
    focus: bool,
    index: usize,
) -> menu_toolbar::ToolbarScene {
    let plans = menu_toolbar::toolbar(&ToolbarInput {
        hover: hover.into(),
        panel: panel.into(),
        keyboard_focus: focus,
        keyboard_index: index,
    });
    let resources = std::array::from_fn(|i| {
        let sources = TooltipSources {
            body: assets.sprite_source("caption"),
            left: assets.sprite_source("caption_edge_left"),
            right: assets.sprite_source("caption_edge"),
            arrow: assets.sprite_source("caption_arrow"),
        };
        let mut item = ToolbarItemResources {
            active: assets.sprite_source("btn_menubar_active"),
            icon: assets.sprite_source(
                [
                    "btn_display",
                    "btn_option",
                    "btn_language",
                    "btn_copyright",
                    "manual",
                ][i],
            ),
            tooltip: Vec::new(),
        };
        if plans[i].tooltip {
            let color = assets.palette_color(
                "caption",
                if assets.nes {
                    Color::BLACK
                } else {
                    Color::WHITE
                },
            );
            item.tooltip = menu_toolbar::tooltip(
                plans[i].center_x,
                TooltipStyle {
                    body_y: 122.0,
                    height: 66.0,
                    edge_width: 12.0,
                    minimum_width: 144.0,
                    text_y: 128.0,
                },
                &assets.measure("A", "s"),
                &assets.font_atlases(),
                &sources,
                Rgba {
                    r: color.r,
                    g: color.g,
                    b: color.b,
                    a: color.a,
                },
            );
        }
        item
    });
    menu_toolbar::toolbar_scene(assets.texture_bindings.generation(), &plans, &resources)
}

fn combined_toolbar_pixels(
    canvas: &mut Canvas<Window>,
    assets: &mut UiAssets,
    frame: &BaseMenuScene,
    binding: &menu_frame::BindingContext,
    notice: &crate::frontend::menu_overlay::NoticeScene,
) -> (Vec<u8>, Vec<(String, Rect)>) {
    canvas.set_draw_color(Color::RGBA(0, 0, 0, 255));
    canvas.clear();
    let mut native_hits = Vec::new();
    assert!(assets.execute_base_menu(canvas, frame, binding, &mut native_hits));
    assert!(assets.execute_notice(canvas, notice));
    let hits = native_hits
        .into_iter()
        .map(|hit| {
            (
                hit.action,
                Rect {
                    x: hit.rect.x,
                    y: hit.rect.y,
                    w: hit.rect.w,
                    h: hit.rect.h,
                },
            )
        })
        .collect();
    (capture(canvas), hits)
}

fn frame_pixels(
    canvas: &mut Canvas<Window>,
    assets: &mut UiAssets,
    frame: &BaseMenuScene,
    binding: &menu_frame::BindingContext,
) -> (Vec<u8>, Vec<(String, Rect)>) {
    canvas.set_draw_color(Color::RGBA(0, 0, 0, 255));
    canvas.clear();
    let mut native_hits = Vec::new();
    assert!(assets.execute_base_menu(canvas, frame, binding, &mut native_hits));
    let hits = native_hits
        .into_iter()
        .map(|hit| {
            (
                hit.action,
                Rect {
                    x: hit.rect.x,
                    y: hit.rect.y,
                    w: hit.rect.w,
                    h: hit.rect.h,
                },
            )
        })
        .collect();
    (capture(canvas), hits)
}

#[test]
#[ignore = "requires caller-provided SDL_VIDEODRIVER=dummy and SDL_RENDER_DRIVER=software"]
fn retained_base_menu_frames_replay_with_fresh_assets_and_bindings() {
    assert_eq!(std::env::var("SDL_VIDEODRIVER").as_deref(), Ok("dummy"));
    assert_eq!(
        std::env::var("SDL_RENDER_DRIVER").as_deref(),
        Ok("software")
    );
    let context = sdl3::init().expect("caller-provided SDL drivers should initialize");
    let video = context.video().unwrap();
    let mut first_canvas = canvas(&video);
    let mut first_assets = assets(&first_canvas);
    install_complete_frame_resources(&mut first_assets);
    let cases = [(false, false), (false, true), (true, false)];
    let old_generation = first_assets.texture_bindings.generation();
    let mut retained = Vec::new();
    for (nes, suspend) in cases {
        let (frame, binding) = full_frame(&mut first_assets, nes, suspend);
        assert_eq!(frame.generation, old_generation);
        assert_eq!(binding.generation, old_generation);
        let (pixels, hits) = frame_pixels(&mut first_canvas, &mut first_assets, &frame, &binding);
        assert!(changed(&pixels));
        assert_eq!(
            hits.iter()
                .map(|(action, _)| action.as_str())
                .collect::<Vec<_>>(),
            [
                "game:demo",
                "previous",
                "next",
                "game:demo",
                "settings",
                "resume-panel",
                "sort",
                "start"
            ]
        );
        assert_eq!(hits[0].1.y, if suspend { 153.0 } else { 222.0 });
        assert_eq!(
            hits[1].1,
            Rect {
                x: 0.0,
                y: 240.0,
                w: 65.0,
                h: 255.0
            }
        );
        assert_eq!(hits[3].1.y, 526.0);
        let replay = frame_pixels(&mut first_canvas, &mut first_assets, &frame, &binding);
        assert_eq!(replay, (pixels.clone(), hits.clone()));
        retained.push((frame, binding, pixels, hits));
    }

    drop(first_assets);
    drop(first_canvas);
    let mut second_canvas = canvas(&video);
    let mut second_assets = assets(&second_canvas);
    install_complete_frame_resources(&mut second_assets);
    let new_generation = second_assets.texture_bindings.generation();
    assert_ne!(new_generation, old_generation);
    for (frame, binding, old_pixels, old_hits) in &retained {
        second_canvas.set_draw_color(Color::RGBA(0, 0, 0, 255));
        second_canvas.clear();
        let clear_pixels = capture(&second_canvas);
        let mut hits = vec![Hit {
            action: "sentinel".into(),
            rect: FRect::new(9.0, 8.0, 1.0, 1.0),
        }];
        assert!(!second_assets.execute_base_menu(&mut second_canvas, frame, binding, &mut hits));
        assert_eq!(capture(&second_canvas), clear_pixels);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].action, "sentinel");

        let (nes, suspend) = if old_hits[0].1.y == 153.0 {
            (false, true)
        } else if frame.steps.iter().any(|step| {
            matches!(
                step,
                crate::frontend::menu_frame::Step::Draw(Command::Fill { .. })
            )
        }) {
            (true, false)
        } else {
            (false, false)
        };
        let (fresh_frame, fresh_binding) = full_frame(&mut second_assets, nes, suspend);
        assert_eq!(fresh_frame.generation, new_generation);
        let replay = frame_pixels(
            &mut second_canvas,
            &mut second_assets,
            &fresh_frame,
            &fresh_binding,
        );
        assert_eq!(&replay.0, old_pixels);
        assert_eq!(&replay.1, old_hits);
    }
}

#[test]
#[ignore = "requires caller-provided SDL_VIDEODRIVER=dummy and SDL_RENDER_DRIVER=software"]
fn retained_toolbar_tooltips_and_notices_replay_with_fresh_assets() {
    assert_eq!(std::env::var("SDL_VIDEODRIVER").as_deref(), Ok("dummy"));
    assert_eq!(
        std::env::var("SDL_RENDER_DRIVER").as_deref(),
        Ok("software")
    );
    let context = sdl3::init().expect("caller-provided SDL drivers should initialize");
    let video = context.video().unwrap();
    let cases = [
        (false, "toolbar:0", "", false, 0),
        (false, "toolbar:0", "", true, 2),
        (true, "toolbar:1", "", false, 0),
        (false, "toolbar:0", "settings", true, 2),
    ];
    let mut first_canvas = canvas(&video);
    let mut first_assets = assets(&first_canvas);
    install_complete_frame_resources(&mut first_assets);
    install_toolbar_resources(&mut first_assets);
    let old_generation = first_assets.texture_bindings.generation();
    let mut retained = Vec::new();
    for (nes, hover, panel, keyboard_focus, keyboard_index) in cases {
        first_assets.nes = nes;
        let (base, binding) = full_frame(&mut first_assets, nes, false);
        let toolbar = project_toolbar(&first_assets, hover, panel, keyboard_focus, keyboard_index);
        let plans = menu_toolbar::toolbar(&ToolbarInput {
            hover: hover.into(),
            panel: panel.into(),
            keyboard_focus,
            keyboard_index,
        });
        let frame = crate::frontend::menu_frame::with_toolbar(base, toolbar).unwrap();
        assert_eq!(frame.generation, old_generation);
        let notice = menu_overlay::notice(
            old_generation,
            &first_assets.measure("A", "s"),
            &first_assets.font_atlases(),
        );
        assert!(!notice.commands.is_empty());
        let replay = combined_toolbar_pixels(
            &mut first_canvas,
            &mut first_assets,
            &frame,
            &binding,
            &notice,
        );
        assert!(changed(&replay.0));
        assert_eq!(replay.1.len(), 13);
        assert_eq!(
            replay.1[8..]
                .iter()
                .map(|(action, _)| action.as_str())
                .collect::<Vec<_>>(),
            [
                "toolbar:0",
                "toolbar:1",
                "toolbar:2",
                "toolbar:3",
                "toolbar:4"
            ]
        );
        for (offset, plan) in plans.iter().enumerate() {
            assert_eq!(replay.1[8 + offset].1, plan.hit);
        }
        let target = plans[2].center_x;
        let last =
            crate::frontend::menu_frame::last_hit(&replay.1, target, 40.0, |(_, rect)| *rect)
                .unwrap();
        assert_eq!(last.0, "toolbar:2");
        assert!(frame.steps.iter().any(
            |step| matches!(step, Step::Draw(Command::Texture(q)) if q.filter == Filter::Linear)
        ));
        assert!(frame.steps.iter().any(
            |step| matches!(step, Step::Draw(Command::Texture(q)) if q.filter == Filter::Nearest)
        ));
        if panel == "settings" {
            assert!(plans.iter().all(|plan| !plan.tooltip));
        }
        let warm = combined_toolbar_pixels(
            &mut first_canvas,
            &mut first_assets,
            &frame,
            &binding,
            &notice,
        );
        assert_eq!(warm, replay);
        retained.push((
            frame,
            binding,
            notice,
            replay,
            plans,
            (nes, hover, panel, keyboard_focus, keyboard_index),
        ));
    }
    for (case, expected) in retained.iter().zip([1, 2, 1, 0]) {
        assert_eq!(case.4.iter().filter(|plan| plan.tooltip).count(), expected);
    }
    assert_ne!(
        retained[0].3.0, retained[1].3.0,
        "keyboard focus adds a second tooltip"
    );
    assert_ne!(
        retained[1].3.0, retained[3].3.0,
        "eligible hover+keyboard tooltips render while an open panel blocks them"
    );

    drop(first_assets);
    drop(first_canvas);
    let mut second_canvas = canvas(&video);
    let mut second_assets = assets(&second_canvas);
    install_complete_frame_resources(&mut second_assets);
    install_toolbar_resources(&mut second_assets);
    let new_generation = second_assets.texture_bindings.generation();
    assert_ne!(new_generation, old_generation);
    for (old_frame, old_binding, old_notice, old_replay, _plans, case) in &retained {
        let retained_frame = old_frame.clone();
        let retained_notice = old_notice.clone();
        second_canvas.set_draw_color(Color::RGBA(0, 0, 0, 255));
        second_canvas.clear();
        let blank = capture(&second_canvas);
        let mut hits = vec![Hit {
            action: "sentinel".into(),
            rect: FRect::new(0.0, 0.0, 1.0, 1.0),
        }];
        assert!(!second_assets.execute_base_menu(
            &mut second_canvas,
            old_frame,
            old_binding,
            &mut hits
        ));
        assert_eq!(capture(&second_canvas), blank);
        assert_eq!(hits.len(), 1);
        assert!(!second_assets.execute_notice(&mut second_canvas, old_notice));
        assert_eq!(capture(&second_canvas), blank);
        assert_eq!(hits.len(), 1);

        let (nes, hover, panel, focus, index) = *case;
        second_assets.nes = nes;
        let (base, binding) = full_frame(&mut second_assets, nes, false);
        let toolbar = project_toolbar(&second_assets, hover, panel, focus, index);
        let fresh_frame = crate::frontend::menu_frame::with_toolbar(base, toolbar).unwrap();
        let fresh_notice = menu_overlay::notice(
            new_generation,
            &second_assets.measure("A", "s"),
            &second_assets.font_atlases(),
        );
        let replay = combined_toolbar_pixels(
            &mut second_canvas,
            &mut second_assets,
            &fresh_frame,
            &binding,
            &fresh_notice,
        );
        assert_eq!(&replay.0, &old_replay.0);
        assert_eq!(&replay.1, &old_replay.1);
        assert_eq!(old_frame, &retained_frame);
        assert_eq!(old_notice, &retained_notice);
    }
}

#[test]
#[ignore = "requires caller-provided SDL_VIDEODRIVER=dummy and SDL_RENDER_DRIVER=software"]
fn retained_carousel_title_scenes_require_current_assets_generation() {
    assert_eq!(std::env::var("SDL_VIDEODRIVER").as_deref(), Ok("dummy"));
    assert_eq!(
        std::env::var("SDL_RENDER_DRIVER").as_deref(),
        Ok("software")
    );
    let context = sdl3::init().expect("caller-provided SDL drivers should initialize");
    let video = context.video().unwrap();
    let mut first_canvas = canvas(&video);
    let mut first_assets = assets(&first_canvas);
    let (normal, normal_hits) = scene(&first_assets, false, 0.0, 0.0);
    let retained_normal = normal.clone();
    let (suspend, _) = scene(&first_assets, false, -52.0, -69.0);
    let (nes, nes_hits) = scene(&first_assets, true, 0.0, 0.0);
    assert_eq!(normal_hits[0].y, 222.0);
    assert_eq!(
        suspend[0],
        menu_carousel::title_scene(
            false,
            -52.0,
            &first_assets.measure("A", "title"),
            &first_assets.font_atlases(),
            first_assets.sprite_source("caption_title"),
            Rgba {
                r: 230,
                g: 210,
                b: 170,
                a: 255
            }
        )[0]
    );
    assert_eq!(nes_hits[0].y, 222.0);
    let packed = first_assets.texture_bindings.id("skin/packed.png").unwrap();
    let cover = first_assets
        .image_source("games/demo/demo.png")
        .unwrap()
        .texture;
    let cover_source = first_assets.image_source("games/demo/demo.png").unwrap();
    assert_eq!((cover_source.width, cover_source.height), (32, 24));
    let font = first_assets.font_atlases()["title"];
    assert_ne!(packed, cover);
    assert_ne!(packed, font);
    assert!(normal.iter().any(
        |c| matches!(c, Command::Texture(q) if q.texture == packed && q.filter == Filter::Nearest)
    ));
    assert!(normal.iter().any(
        |c| matches!(c, Command::Texture(q) if q.texture == cover && q.filter == Filter::Linear)
    ));
    assert!(normal.iter().any(
        |c| matches!(c, Command::Texture(q) if q.texture == font && q.filter == Filter::Linear)
    ));

    render(&mut first_canvas, &mut first_assets, &normal);
    let baseline = capture(&first_canvas);
    assert!(changed(&baseline));
    render(&mut first_canvas, &mut first_assets, &normal);
    assert_eq!(capture(&first_canvas), baseline);
    render(&mut first_canvas, &mut first_assets, &suspend);
    let suspend_pixels = capture(&first_canvas);
    assert!(changed(&suspend_pixels));
    assert_ne!(suspend_pixels, baseline);
    render(&mut first_canvas, &mut first_assets, &nes);
    let nes_pixels = capture(&first_canvas);
    assert!(changed(&nes_pixels));
    for (commands, expected) in [(&suspend, &suspend_pixels), (&nes, &nes_pixels)] {
        render(&mut first_canvas, &mut first_assets, commands);
        assert_eq!(&capture(&first_canvas), expected);
    }

    let old_generation = packed.generation;
    drop(first_assets);
    drop(first_canvas);
    let mut second_canvas = canvas(&video);
    let mut second_assets = assets(&second_canvas);
    let fresh_packed = second_assets
        .texture_bindings
        .id("skin/packed.png")
        .unwrap();
    assert_ne!(old_generation, fresh_packed.generation);
    second_canvas.set_draw_color(Color::RGBA(0, 0, 0, 255));
    second_canvas.clear();
    let cleared = capture(&second_canvas);
    for command in &normal {
        assert!(!second_assets.draw_command(&mut second_canvas, command));
    }
    assert_eq!(capture(&second_canvas), cleared);
    let (fresh, fresh_hits) = scene(&second_assets, false, 0.0, 0.0);
    assert_eq!(fresh_hits, normal_hits);
    render(&mut second_canvas, &mut second_assets, &fresh);
    assert_eq!(capture(&second_canvas), baseline);
    for (nes, title_shift, card_shift, expected) in [
        (false, -52.0, -69.0, &suspend_pixels),
        (true, 0.0, 0.0, &nes_pixels),
    ] {
        let (fresh, _) = scene(&second_assets, nes, title_shift, card_shift);
        render(&mut second_canvas, &mut second_assets, &fresh);
        assert_eq!(&capture(&second_canvas), expected);
    }
    assert_eq!(normal, retained_normal);
}
