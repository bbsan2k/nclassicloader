use super::menu_carousel_tests::{add_texture, assets, canvas, capture};
use super::panel_scene::PanelBindings;
use super::*;
use crate::frontend::{
    menu_panel::{self, PanelInput, PanelKey, PanelScene},
    menu_scene::{Command, Filter, Rect},
};

#[derive(Copy, Clone)]
struct Case {
    message: bool,
    entries: usize,
    focus: usize,
    footer: Option<&'static str>,
}

fn panel_assets(canvas: &Canvas<Window>) -> UiAssets {
    let mut art = assets(canvas);
    for name in ["l", "m", "s"] {
        art.fonts.insert(name.into(), art.fonts["title"].clone());
        add_texture(
            &mut art,
            &format!("skin/font-{name}.png"),
            32,
            16,
            [230, 180, 15, 255],
        );
    }
    art.sprites
        .insert("btn_init_on.png".into(), FRect::new(0.0, 40.0, 8.0, 8.0));
    art.sprites
        .insert("icon_B_light.png".into(), FRect::new(20.0, 40.0, 8.0, 8.0));
    art
}

fn project(art: &UiAssets, case: Case) -> (PanelScene, PanelBindings) {
    let fonts = art.font_atlases();
    let heading = art.measure("A", "l");
    let body = vec![art.measure("A", if case.message { "s" } else { "m" }); 2];
    let text = menu_panel::text_scene(&heading, &body, case.message, &fonts);
    let viewport = menu_panel::row_viewport(case.entries, case.focus, text.rows_y);
    let entries: Vec<_> = (0..case.entries)
        .map(|i| (format!("profile:{i}"), "A".into()))
        .collect();
    let mut actions: Vec<_> = entries
        .iter()
        .map(|(key, _)| PanelKey(key.clone()))
        .collect();
    // This is the production capture helper, not a test-local row-position model.
    let rows = art.project_panel_rows(entries, viewport, text.rows_y, "profile:0");
    let mut expected_y = text.rows_y - 14.0;
    for (offset, (key, row)) in rows.iter().enumerate() {
        assert_eq!(key.0, format!("profile:{}", viewport.first_row + offset));
        assert!((row.hit.y - expected_y).abs() < 0.001);
        expected_y += viewport.row_height;
    }
    let footer_key = case.footer.map(|key| PanelKey(key.into()));
    if let Some(key) = &footer_key {
        actions.push(key.clone());
    }
    let generation = art.texture_bindings.generation();
    let binding = PanelBindings::new(generation, actions.clone());
    let scene = menu_panel::compose_panel(PanelInput {
        generation,
        text,
        rows,
        footer: menu_panel::footer(
            &art.measure("A", "s"),
            &fonts,
            art.sprite_source("icon_B_light"),
            footer_key.is_some(),
        ),
        footer_key,
        actions,
    });
    assert!(scene.steps.iter().any(|step| matches!(step,
        menu_panel::PanelStep::Draw(Command::Texture(q)) if q.filter == Filter::Linear)));
    assert!(scene.steps.iter().any(|step| matches!(step,
        menu_panel::PanelStep::Draw(Command::Texture(q)) if q.filter == Filter::Nearest)));
    (scene, binding)
}

type Rendered = (Vec<u8>, Vec<(String, Rect)>, Vec<String>);

fn render(
    canvas: &mut Canvas<Window>,
    art: &mut UiAssets,
    scene: &PanelScene,
    binding: &PanelBindings,
) -> Rendered {
    canvas.set_blend_mode(BlendMode::Blend);
    canvas.set_draw_color(Color::RGB(70, 90, 110));
    canvas.clear();
    let mut hits = vec![Hit {
        action: "replaced".into(),
        rect: FRect::new(0.0, 0.0, 1.0, 1.0),
    }];
    let mut actions = vec!["replaced".into()];
    assert!(art.execute_panel(canvas, scene, binding, &mut hits, &mut actions));
    let hits = hits
        .into_iter()
        .map(|hit| (hit.action, from_frect(hit.rect)))
        .collect();
    (capture(canvas), hits, actions)
}

#[test]
#[ignore = "requires caller-provided SDL_VIDEODRIVER=dummy and SDL_RENDER_DRIVER=software"]
fn retained_panels_replay_after_resources_and_canvas_are_recreated() {
    assert_eq!(std::env::var("SDL_VIDEODRIVER").as_deref(), Ok("dummy"));
    assert_eq!(
        std::env::var("SDL_RENDER_DRIVER").as_deref(),
        Ok("software")
    );
    let sdl = sdl3::init().unwrap();
    let video = sdl.video().unwrap();
    let mut first_canvas = canvas(&video);
    let mut first = panel_assets(&first_canvas);
    let cases = [
        Case {
            message: false,
            entries: 20,
            focus: 19,
            footer: Some("close"),
        },
        Case {
            message: true,
            entries: 0,
            focus: 99,
            footer: Some("close"),
        },
        Case {
            message: false,
            entries: 1,
            focus: 0,
            footer: Some("cancel-import"),
        },
        Case {
            message: false,
            entries: 1,
            focus: 1,
            footer: None,
        },
    ];
    let mut retained = Vec::new();
    for case in cases {
        let (scene, binding) = project(&first, case);
        let original = scene.clone();
        let rendered = render(&mut first_canvas, &mut first, &scene, &binding);
        assert_eq!(
            rendered.2.len(),
            case.entries + usize::from(case.footer.is_some())
        );
        assert!(!rendered.2.iter().any(|action| action == "replaced"));
        let expected_keys: Vec<_> = scene
            .steps
            .iter()
            .filter_map(|step| match step {
                menu_panel::PanelStep::Hit { key, rect } => Some((key.0.clone(), *rect)),
                _ => None,
            })
            .collect();
        assert_eq!(rendered.1, expected_keys);
        if case.entries == 20 {
            assert!(rendered.1.len() < rendered.2.len());
            assert!(rendered.1.windows(2).all(|hits| hits[0].1.y < hits[1].1.y));
            assert_eq!(rendered.2[0], "profile:0");
            assert_eq!(rendered.2[20], "close");
        }
        assert_eq!(
            render(&mut first_canvas, &mut first, &scene, &binding),
            rendered
        );
        assert_eq!(scene, original);
        retained.push((case, scene, binding, rendered));
    }
    drop(first);
    drop(first_canvas);
    let mut second_canvas = canvas(&video);
    let mut second = panel_assets(&second_canvas);
    for (case, old_scene, old_binding, old_rendered) in retained {
        let original = old_scene.clone();
        second_canvas.set_draw_color(Color::BLACK);
        second_canvas.clear();
        let blank = capture(&second_canvas);
        let mut hits = vec![Hit {
            action: "sentinel".into(),
            rect: FRect::new(1.0, 2.0, 3.0, 4.0),
        }];
        let mut actions = vec!["sentinel".into()];
        assert!(!second.execute_panel(
            &mut second_canvas,
            &old_scene,
            &old_binding,
            &mut hits,
            &mut actions
        ));
        assert_eq!(capture(&second_canvas), blank);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].action, "sentinel");
        assert_eq!(actions, ["sentinel"]);
        let (fresh, binding) = project(&second, case);
        assert_ne!(fresh.generation, old_scene.generation);
        assert_eq!(
            render(&mut second_canvas, &mut second, &fresh, &binding),
            old_rendered
        );
        assert_eq!(old_scene, original);
    }
}
