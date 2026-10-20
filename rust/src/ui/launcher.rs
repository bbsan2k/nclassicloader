//! Asset-independent startup library. Importing stays off the SDL thread.
use super::*;
mod view;

pub(super) struct Selection {
    pub assets: PathBuf,
    pub data: PathBuf,
}

enum Entry {
    Cached(crate::profile::Profile),
    Image(PathBuf),
}

enum Message {
    Image(PathBuf),
    Assets(PathBuf),
    Status(String),
    Imported(Result<crate::profile::Profile, String>),
    Error(String),
}

fn finish(home: &Path, profile: crate::profile::Profile) -> Result<Selection, String> {
    let name = profile
        .directory
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Invalid profile path")?;
    atomic_json(&home.join("last-profile.json"), &json!({"directory": name}))?;
    Ok(Selection {
        assets: profile.assets(),
        data: profile.data(),
    })
}

pub(super) fn select(home: &Path, preview: Option<&Path>) -> Result<Option<Selection>, String> {
    let context = sdl3::init().map_err(|e| e.to_string())?;
    let video = context.video().map_err(|e| e.to_string())?;
    let window = video
        .window(
            "Classic Menu Rust - Game Data Library",
            view::WIDTH,
            view::HEIGHT,
        )
        .position_centered()
        .resizable()
        .high_pixel_density()
        .build()
        .map_err(|e| e.to_string())?;
    let mut canvas = window.into_canvas();
    canvas
        .set_logical_size(
            view::WIDTH,
            view::HEIGHT,
            sdl3::sys::render::SDL_LOGICAL_PRESENTATION_LETTERBOX,
        )
        .map_err(|e| e.to_string())?;
    canvas
        .window_mut()
        .set_minimum_size(960, 608)
        .map_err(|e| e.to_string())?;
    let mut view = view::View::new(&canvas)?;
    let cache = home.join("profiles");
    let mut entries: Vec<_> = crate::profile::list_profiles(&cache)
        .into_iter()
        .map(Entry::Cached)
        .collect();
    let mut places = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        places.push(cwd.clone());
        places.push(cwd.join("SNES-Mini-Kernels"));
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        places.push(parent.to_owned());
        places.push(parent.join("../../.."));
        places.push(parent.join("../../../SNES-Mini-Kernels"));
    }
    entries.extend(
        crate::profile::discover_images(&places)
            .into_iter()
            .map(Entry::Image),
    );
    let last = read_json(&home.join("last-profile.json")).ok();
    let mut selected = entries
        .iter()
        .position(|entry| match entry {
            Entry::Cached(profile) => {
                profile.directory.file_name().and_then(|n| n.to_str())
                    == last
                        .as_ref()
                        .and_then(|v| v.get("directory"))
                        .and_then(Value::as_str)
            }
            _ => false,
        })
        .unwrap_or(0);
    let (sender, receiver) = mpsc::channel();
    let mut events = context.event_pump().map_err(|e| e.to_string())?;
    let mut status =
        "Choose a version to get started. You can also drop an image anywhere in this window."
            .to_owned();
    let mut error = false;
    let mut hover = None;
    let mut focus = None;
    let started = Instant::now();
    let mut busy = false;
    let mut pending: Option<std::thread::JoinHandle<()>> = None;
    let mut closing = false;
    // Snapshot-only states never affect a normal launch or import.
    if preview.is_some() {
        match std::env::var("CLOVER_LAUNCHER_PREVIEW_STATE").as_deref() {
            Ok("empty") => entries.clear(),
            Ok("busy") => {
                busy = true;
                status = "Decoding Clover artwork and fonts… Your source image remains untouched."
                    .into();
            }
            Ok("error") => {
                error = true;
                status = "This image could not be imported. Choose a supported SNES Classic .hsqs or .tar.gz filesystem, or open an existing data folder.".into();
            }
            _ => {}
        }
    }
    loop {
        let page = selected / view::PAGE_SIZE;
        let mut action = None;
        for event in events.poll_iter() {
            match event {
                Event::Quit { .. } => closing = true,
                Event::KeyDown {
                    scancode: Some(key),
                    ..
                } if !busy => match key {
                    Scancode::Escape => closing = true,
                    Scancode::Up => {
                        selected = selected.saturating_sub(1);
                        focus = None;
                    }
                    Scancode::Down => {
                        selected = (selected + 1).min(entries.len().saturating_sub(1));
                        focus = None;
                    }
                    Scancode::Return => action = Some(focus.unwrap_or(0) as i32),
                    Scancode::Tab => {
                        focus = Some((focus.unwrap_or(2) + 1) % 3);
                    }
                    Scancode::I => action = Some(1),
                    Scancode::D => action = Some(2),
                    _ => {}
                },
                Event::DropFile { filename, .. } if !busy => {
                    let _ = sender.send(Message::Image(PathBuf::from(filename)));
                }
                Event::MouseButtonDown {
                    mouse_btn: sdl3::mouse::MouseButton::Left,
                    x,
                    y,
                    clicks,
                    ..
                } if !busy => {
                    let (width, height) = canvas.window().size();
                    let (x, y) = coordinates(width, height, x, y);
                    for row in 0..view::PAGE_SIZE {
                        let index = page * view::PAGE_SIZE + row;
                        if index < entries.len() && point_inside(x, y, view::row_rect(row)) {
                            selected = index;
                            focus = None;
                            if clicks >= 2 {
                                action = Some(0);
                            }
                        }
                    }
                    for (index, rect) in view::BUTTONS.iter().enumerate() {
                        if point_inside(x, y, *rect) {
                            action = Some(index as i32);
                        }
                    }
                }
                Event::MouseMotion { x, y, .. } => {
                    let (width, height) = canvas.window().size();
                    hover = Some(coordinates(width, height, x, y));
                }
                Event::MouseWheel { y, .. } if !busy => {
                    selected = if y > 0.0 {
                        selected.saturating_sub(1)
                    } else {
                        (selected + 1).min(entries.len().saturating_sub(1))
                    };
                }
                _ => {}
            }
        }
        if closing && !busy {
            return Ok(None);
        }
        if action == Some(0) && entries.is_empty() {
            action = Some(1);
        }
        if let Some(action) = action {
            match action {
                0 => match entries.get(selected) {
                    Some(Entry::Cached(profile)) => {
                        let old = profile
                            .manifest
                            .get("importerVersion")
                            .and_then(Value::as_u64)
                            .unwrap_or(0)
                            < u64::from(crate::profile::IMPORTER_VERSION);
                        let source = profile
                            .manifest
                            .get("source")
                            .and_then(Value::as_str)
                            .map(PathBuf::from);
                        if old && let Some(source) = source.filter(|path| path.is_file()) {
                            let _ = sender.send(Message::Image(source));
                        } else {
                            return finish(home, profile.clone()).map(Some);
                        }
                    }
                    Some(Entry::Image(path)) => {
                        let _ = sender.send(Message::Image(path.clone()));
                    }
                    None => {
                        status =
                            "Browse for a supplied .hsqs, .tar.gz or prepared assets folder.".into()
                    }
                },
                1 => {
                    let sender = sender.clone();
                    dialog::show_open_file_dialog(
                        &[DialogFileFilter {
                            name: "SNES Classic filesystem",
                            pattern: "hsqs;gz;img",
                        }],
                        None::<&Path>,
                        false,
                        Some(canvas.window()),
                        Box::new(move |result, _| match result {
                            Ok(paths) => {
                                if let Some(path) = paths.into_iter().next() {
                                    let _ = sender.send(Message::Image(path));
                                }
                            }
                            Err(DialogError::Canceled) => {}
                            Err(error) => {
                                let _ = sender.send(Message::Error(error.to_string()));
                            }
                        }),
                    )
                    .map_err(|e| e.to_string())?;
                }
                _ => {
                    let sender = sender.clone();
                    dialog::show_open_folder_dialog(
                        None::<&Path>,
                        false,
                        Some(canvas.window()),
                        Box::new(move |result, _| match result {
                            Ok(paths) => {
                                if let Some(path) = paths.into_iter().next() {
                                    let _ = sender.send(Message::Assets(path));
                                }
                            }
                            Err(DialogError::Canceled) => {}
                            Err(error) => {
                                let _ = sender.send(Message::Error(error.to_string()));
                            }
                        }),
                    );
                }
            }
        }
        while let Ok(message) = receiver.try_recv() {
            match message {
                Message::Image(path) if !busy => {
                    busy = true;
                    error = false;
                    status = "Starting read-only import...".into();
                    let sender = sender.clone();
                    let cache = cache.clone();
                    pending = Some(std::thread::spawn(move || {
                        let result = crate::profile::import_image(
                            &path,
                            &cache,
                            Some(&mut |progress| {
                                let _ = sender.send(Message::Status(progress.to_owned()));
                            }),
                        );
                        let _ = sender.send(Message::Imported(result));
                    }));
                }
                Message::Assets(mut path) if !busy => {
                    if path.join("assets/catalog.json").is_file() {
                        path = path.join("assets");
                    }
                    match read_games(&path) {
                        Ok(_) => {
                            use sha2::{Digest, Sha256};
                            let absolute = fs::canonicalize(&path).map_err(|e| e.to_string())?;
                            let key = format!(
                                "{:x}",
                                Sha256::digest(absolute.to_string_lossy().as_bytes())
                            );
                            return Ok(Some(Selection {
                                assets: absolute,
                                data: home.join("external-data").join(key),
                            }));
                        }
                        Err(message) => {
                            error = true;
                            status = format!(
                                "Choose a folder containing catalog.json and skin/: {message}"
                            )
                        }
                    }
                }
                Message::Status(value) => status = value,
                Message::Imported(result) => {
                    busy = false;
                    if let Some(worker) = pending.take() {
                        let _ = worker.join();
                    }
                    if closing {
                        return Ok(None);
                    }
                    match result {
                        Ok(profile) => return finish(home, profile).map(Some),
                        Err(message) => {
                            status = message;
                            error = true;
                        }
                    }
                }
                Message::Error(message) => {
                    status = message;
                    error = true;
                }
                _ => {}
            }
        }
        view.draw(
            &mut canvas,
            &entries,
            selected,
            busy,
            &status,
            error,
            hover,
            focus,
            started.elapsed().as_secs_f32(),
        )?;
        if let Some(path) = preview {
            save_screenshot(&canvas, path)?;
            return Ok(None);
        }
        canvas.present();
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
}

fn coordinates(width: u32, height: u32, x: f32, y: f32) -> (f32, f32) {
    let scale = (width as f32 / view::WIDTH as f32)
        .min(height as f32 / view::HEIGHT as f32)
        .max(0.001);
    (
        (x - (width as f32 - view::WIDTH as f32 * scale) * 0.5) / scale,
        (y - (height as f32 - view::HEIGHT as f32 * scale) * 0.5) / scale,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letterboxed_hit_testing_matches_layout() {
        let (x, y) = coordinates(1600, 760, 1100.0, 680.0);
        assert!(point_inside(x, y, view::BUTTONS[0]));
        let (x, y) = coordinates(960, 608, 720.0, 544.0);
        assert!(point_inside(x, y, view::BUTTONS[0]));
        assert!(!point_inside(250.0, 300.0, view::BUTTONS[1]));
    }
}
