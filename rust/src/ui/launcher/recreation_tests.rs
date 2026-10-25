use super::*;
use crate::frontend::launcher_presentation::{Presentation, compose};
use crate::frontend::launcher_scene::Region;
use sdl3::{VideoSubsystem, pixels::PixelFormat};

fn canvas(video: &VideoSubsystem) -> Canvas<Window> {
    let window = video
        .window("launcher scene recreation", WIDTH, HEIGHT)
        .hidden()
        .build()
        .expect("dummy SDL window should build");
    let mut canvas = window.into_canvas();
    canvas
        .set_logical_size(
            WIDTH,
            HEIGHT,
            sdl3::sys::render::SDL_LOGICAL_PRESENTATION_LETTERBOX,
        )
        .expect("launcher stage should fit the dummy canvas");
    canvas
}

fn pixels(canvas: &Canvas<Window>) -> Vec<u8> {
    let surface = canvas
        .read_pixels(None)
        .expect("software renderer should expose its current pixels");
    let rgba = surface
        .convert_format(PixelFormat::RGBA32)
        .expect("readback should convert to RGBA32");
    let (width, height, pitch) = (
        rgba.width() as usize,
        rgba.height() as usize,
        rgba.pitch() as usize,
    );
    assert_eq!((width, height), (WIDTH as usize, HEIGHT as usize));
    let mut result = Vec::with_capacity(width * height * 4);
    rgba.with_lock(|source| {
        for row in 0..height {
            let start = row * pitch;
            result.extend_from_slice(&source[start..start + width * 4]);
        }
    });
    result
}

fn render_scenes(
    view: &mut View,
    canvas: &mut Canvas<Window>,
    scenes: &[crate::frontend::launcher_presentation::Scene],
) -> Vec<Vec<u8>> {
    scenes
        .iter()
        .map(|scene| {
            view.draw_scene(canvas, scene)
                .expect("retained launcher scene should draw");
            let image = pixels(canvas);
            canvas.present();
            image
        })
        .collect()
}

fn projection(
    entries: &[&Entry],
    selected: usize,
    busy: bool,
    status: &str,
    error: bool,
    elapsed: f32,
) -> Presentation {
    super::projection::project(
        entries,
        selected,
        busy,
        CancellationView::Requested,
        status,
        error,
        None,
        None,
        "ALL",
        false,
        elapsed,
    )
}

#[test]
#[ignore = "requires SDL_VIDEODRIVER=dummy and SDL_RENDER_DRIVER=software"]
fn retained_scenes_replay_after_view_and_canvas_recreation() {
    assert_eq!(std::env::var("SDL_VIDEODRIVER").as_deref(), Ok("dummy"));
    assert_eq!(
        std::env::var("SDL_RENDER_DRIVER").as_deref(),
        Ok("software")
    );

    let mut scenes = Vec::new();
    let entries = [
        Entry::Image(PathBuf::from("dp-snesusa-release-v1.hsqs")),
        Entry::Image(PathBuf::from("dp-sneseur-release-v1.hsqs")),
        Entry::Image(PathBuf::from("dp-shvc-release-v1.hsqs")),
        Entry::Image(PathBuf::from("custom-image.hsqs")),
    ];
    let visible: Vec<_> = entries.iter().collect();
    let normal = projection(&visible, 0, false, "Library ready", false, 0.0);
    assert_eq!(
        normal.rows.iter().map(|row| row.region).collect::<Vec<_>>(),
        [Region::Usa, Region::Europe, Region::Japan, Region::Unknown]
    );
    scenes.push(compose(&normal));
    drop(normal);
    drop(visible);
    drop(entries);

    let empty = projection(&[], 0, false, "No game data selected", false, 0.0);
    scenes.push(compose(&empty));
    drop(empty);

    let busy_entries = [Entry::Image(PathBuf::from("dp-nes-release-v2.hsqs"))];
    let busy_visible: Vec<_> = busy_entries.iter().collect();
    let busy = projection(
        &busy_visible,
        0,
        true,
        "Importing the selected image while validating its contents and preparing a safe local profile for the launcher",
        true,
        2.75,
    );
    scenes.push(compose(&busy));
    drop(busy);
    drop(busy_visible);
    drop(busy_entries);

    let context = sdl3::init().expect("caller-provided dummy SDL drivers should initialize");
    let video = context
        .video()
        .expect("dummy SDL video subsystem should initialize");

    let mut first_canvas = canvas(&video);
    let mut first_view =
        View::new(&first_canvas).expect("first launcher view resources should load");
    assert!(first_view.glyphs.is_empty());
    let baseline = render_scenes(&mut first_view, &mut first_canvas, &scenes);
    assert!(first_view.glyphs.keys().any(|(_, _, semibold)| !semibold));
    assert!(first_view.glyphs.keys().any(|(_, _, semibold)| *semibold));
    let warm_replay = render_scenes(&mut first_view, &mut first_canvas, &scenes);
    assert_eq!(warm_replay, baseline);
    drop(first_view);
    drop(first_canvas);

    let mut second_canvas = canvas(&video);
    let mut second_view =
        View::new(&second_canvas).expect("recreated launcher view resources should load");
    assert!(second_view.glyphs.is_empty());
    let recreated = render_scenes(&mut second_view, &mut second_canvas, &scenes);
    assert_eq!(recreated, baseline);
}
