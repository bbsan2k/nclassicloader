//! Asset-independent startup library. Importing stays off the SDL thread.
use super::*;
mod controller;
mod view;

pub(super) struct Selection {
    pub assets: PathBuf,
    pub data: PathBuf,
}

enum Entry {
    Cached(crate::profile::Profile),
    Image(PathBuf),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum LibraryFilter {
    #[default]
    All,
    Nes,
    Snes,
}

impl LibraryFilter {
    fn matches(self, entry: &Entry) -> bool {
        match self {
            Self::All => true,
            Self::Nes => matches!(view::system_group(entry), 0),
            Self::Snes => matches!(view::system_group(entry), 1),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::All => "ALL",
            Self::Nes => "NES",
            Self::Snes => "SNES",
        }
    }

    fn next(self, direction: i32) -> Self {
        let index = match self {
            Self::All => 0,
            Self::Nes => 1,
            Self::Snes => 2,
        };
        [Self::All, Self::Nes, Self::Snes][(index + direction).rem_euclid(3) as usize]
    }
}

fn visible_entries(entries: &[Entry], filter: LibraryFilter) -> Vec<&Entry> {
    entries
        .iter()
        .filter(|entry| filter.matches(entry))
        .collect()
}

fn change_filter(
    entries: &[Entry],
    filter: &mut LibraryFilter,
    next: LibraryFilter,
    selected: &mut usize,
    focus: &mut Option<usize>,
) {
    let key = visible_entries(entries, *filter)
        .get(*selected)
        .map(|entry| entry_key(entry));
    *filter = next;
    *selected = visible_entries(entries, next)
        .iter()
        .position(|entry| key.as_ref().is_some_and(|key| entry_key(entry) == *key))
        .unwrap_or(0);
    *focus = None;
}

fn pad_action(
    action: controller::Action,
    entries: &[Entry],
    filter: &mut LibraryFilter,
    selected: &mut usize,
    focus: &mut Option<usize>,
) -> Option<i32> {
    match action {
        controller::Action::Cancel => Some(-1),
        controller::Action::FilterPrevious | controller::Action::FilterNext => {
            let next = filter.next(if action == controller::Action::FilterPrevious {
                -1
            } else {
                1
            });
            change_filter(entries, filter, next, selected, focus);
            None
        }
        _ => {
            action.navigate(selected, visible_entries(entries, *filter).len(), focus);
            action.activation(*focus)
        }
    }
}

fn entry_key(entry: &Entry) -> String {
    match entry {
        Entry::Cached(profile) => profile
            .directory
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        Entry::Image(path) => path.to_string_lossy().into_owned(),
    }
}

type ReleaseKey = (String, String, String);

fn append_unimported_images(entries: &mut Vec<Entry>, images: impl IntoIterator<Item = PathBuf>) {
    let mut ready_sources = HashSet::new();
    let mut ready_releases: HashSet<ReleaseKey> = HashSet::new();
    for entry in entries.iter() {
        if let Entry::Cached(profile) = entry {
            if let Some(source) = profile.manifest.get("source").and_then(Value::as_str)
                && let Ok(source) = fs::canonicalize(source)
            {
                ready_sources.insert(source);
            }
            if let Some(release) = view::release_key(entry) {
                ready_releases.insert(release);
            }
        }
    }

    for path in images {
        let same_source = fs::canonicalize(&path)
            .ok()
            .is_some_and(|path| ready_sources.contains(&path));
        let image = Entry::Image(path);
        let same_release =
            view::release_key(&image).is_some_and(|release| ready_releases.contains(&release));
        if !same_source && !same_release {
            entries.push(image);
        }
    }
}

enum Message {
    Image(PathBuf),
    Assets(PathBuf),
    Status(String),
    Imported(Result<crate::profile::Profile, String>),
    Error(String),
    PickerClosed,
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
            "Classic Menu - Game Data Library",
            view::WIDTH,
            view::HEIGHT,
        )
        .position_centered()
        .resizable()
        .high_pixel_density()
        .build()
        .map_err(|e| e.to_string())?;
    let mut canvas = window.into_canvas();
    let _ = icon::install(canvas.window_mut());
    #[cfg(target_os = "macos")]
    unsafe {
        clover_install_native_menu(1);
    }
    let mut pacer = timing::FramePacer::new(&canvas, preview.is_none());
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
        places.push(cwd.join("NES-Mini-Kernels"));
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        places.push(parent.to_owned());
        places.push(parent.join("../../.."));
        places.push(parent.join("../../../SNES-Mini-Kernels"));
        places.push(parent.join("../../../NES-Mini-Kernels"));
        // Repository app bundles live in dist/*.app/Contents/MacOS;
        // their image folders are one directory above dist, not inside it.
        places.push(parent.join("../../../../SNES-Mini-Kernels"));
        places.push(parent.join("../../../../NES-Mini-Kernels"));
    }
    append_unimported_images(&mut entries, crate::profile::discover_images(&places));
    // Keep each system family together while preserving the existing order
    // of verified caches and discovered versions within each group.
    entries.sort_by_key(view::system_group);
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
    let settings = read_json(&home.join("launcher.json")).ok();
    let mut filter = match settings
        .as_ref()
        .and_then(|value| value.get("systemFilter"))
        .and_then(Value::as_str)
    {
        Some("NES") => LibraryFilter::Nes,
        Some("SNES") => LibraryFilter::Snes,
        _ => LibraryFilter::All,
    };
    if filter != LibraryFilter::All {
        let next = filter;
        filter = LibraryFilter::All;
        change_filter(&entries, &mut filter, next, &mut selected, &mut None);
    }
    let mut saved_filter = filter;
    let (sender, receiver) = mpsc::channel();
    let mut events = context.event_pump().map_err(|e| e.to_string())?;
    let mut status =
        "Choose a version to get started. You can also drop an image anywhere in this window."
            .to_owned();
    let mut error = false;
    let mut hover = None;
    let mut focus = None;
    let started = Instant::now();
    let mut pad = context.gamepad().ok().map(controller::Controller::new);
    let mut focused = canvas.window().has_input_focus();
    let mut picker_pending = false;
    let mut busy = false;
    let mut pending: Option<std::thread::JoinHandle<()>> = None;
    let mut closing = false;
    let mut launcher_fullscreen = false;
    // Snapshot-only states never affect a normal launch or import.
    if preview.is_some() {
        if let Ok(value) = std::env::var("CLOVER_LAUNCHER_PREVIEW_FILTER") {
            filter = match value.as_str() {
                "NES" => LibraryFilter::Nes,
                "SNES" => LibraryFilter::Snes,
                _ => LibraryFilter::All,
            };
            selected = 0;
        }
        if let Ok(system) = std::env::var("CLOVER_LAUNCHER_PREVIEW_SYSTEM")
            && let Some(index) = visible_entries(&entries, filter)
                .iter()
                .position(|entry| view::preview_matches(entry, &system))
        {
            selected = index;
        }
        match std::env::var("CLOVER_LAUNCHER_PREVIEW_STATE").as_deref() {
            Ok("empty") => entries.clear(),
            Ok("busy") => {
                busy = true;
                status = "Decoding Clover artwork and fonts… Your source image remains untouched."
                    .into();
            }
            Ok("error") => {
                error = true;
                status = "This image could not be imported. Choose a supported NES/SNES Classic .hsqs or .tar.gz filesystem, or open an existing data folder.".into();
            }
            _ => {}
        }
    }
    loop {
        let mut displayed = visible_entries(&entries, filter);
        selected = selected.min(displayed.len().saturating_sub(1));
        let mut action = None;
        for event in events.poll_iter() {
            if let Some(pad) = pad.as_mut() {
                for input in pad.event(
                    &event,
                    started.elapsed(),
                    !busy && focused && !picker_pending && !closing,
                ) {
                    match pad_action(input, &entries, &mut filter, &mut selected, &mut focus) {
                        Some(-1) => closing = true,
                        Some(code) => {
                            action.get_or_insert(code);
                        }
                        _ => {}
                    }
                }
                displayed = visible_entries(&entries, filter);
            }
            match event {
                Event::Quit { .. } => closing = true,
                Event::KeyDown {
                    scancode: Some(key),
                    repeat,
                    ..
                } if !busy && !picker_pending => match key {
                    Scancode::Escape => closing = true,
                    Scancode::F11 if !repeat => {
                        launcher_fullscreen = !launcher_fullscreen;
                        canvas
                            .window_mut()
                            .set_fullscreen(launcher_fullscreen)
                            .map_err(|e| e.to_string())?;
                    }
                    Scancode::_1
                    | Scancode::Kp1
                    | Scancode::_2
                    | Scancode::Kp2
                    | Scancode::_3
                    | Scancode::Kp3
                        if !repeat =>
                    {
                        let next = match key {
                            Scancode::_2 | Scancode::Kp2 => LibraryFilter::Nes,
                            Scancode::_3 | Scancode::Kp3 => LibraryFilter::Snes,
                            _ => LibraryFilter::All,
                        };
                        change_filter(&entries, &mut filter, next, &mut selected, &mut focus);
                        displayed = visible_entries(&entries, filter);
                    }
                    Scancode::Up => {
                        selected = selected.saturating_sub(1);
                        focus = None;
                    }
                    Scancode::Down => {
                        selected = (selected + 1).min(displayed.len().saturating_sub(1));
                        focus = None;
                    }
                    Scancode::Return if !repeat => {
                        action.get_or_insert(focus.unwrap_or(0) as i32);
                    }
                    Scancode::Tab => {
                        focus = Some((focus.unwrap_or(2) + 1) % 3);
                    }
                    Scancode::I if !repeat => {
                        action.get_or_insert(1);
                    }
                    Scancode::D if !repeat => {
                        action.get_or_insert(2);
                    }
                    _ => {}
                },
                #[cfg(target_os = "macos")]
                Event::User { type_, code, .. }
                    if type_ == unsafe { clover_native_menu_event() } =>
                {
                    match code {
                        8 => {
                            launcher_fullscreen = !launcher_fullscreen;
                            canvas
                                .window_mut()
                                .set_fullscreen(launcher_fullscreen)
                                .map_err(|e| e.to_string())?;
                        }
                        10 => closing = true,
                        2 if !busy && !picker_pending => {
                            action.get_or_insert(1);
                        }
                        _ => {}
                    }
                }
                Event::Window {
                    win_event: sdl3::event::WindowEvent::FocusLost,
                    ..
                } => {
                    focused = false;
                    if let Some(pad) = pad.as_mut() {
                        pad.suspend();
                    }
                }
                Event::Window {
                    win_event: sdl3::event::WindowEvent::FocusGained,
                    ..
                } => {
                    focused = true;
                    if let Some(pad) = pad.as_mut() {
                        pad.suspend();
                    }
                }
                Event::DropFile { filename, .. } if !busy && !picker_pending => {
                    let _ = sender.send(Message::Image(PathBuf::from(filename)));
                }
                Event::MouseButtonDown {
                    mouse_btn: sdl3::mouse::MouseButton::Left,
                    x,
                    y,
                    clicks,
                    ..
                } if !busy && !picker_pending => {
                    let (width, height) = canvas.window().size();
                    let (x, y) = coordinates(width, height, x, y);
                    for row in 0..view::PAGE_SIZE {
                        let index = (selected / view::PAGE_SIZE) * view::PAGE_SIZE + row;
                        if index < displayed.len() && point_inside(x, y, view::row_rect(row)) {
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
                    for (index, rect) in FILTER_TABS.iter().enumerate() {
                        if point_inside(x, y, *rect) {
                            let next =
                                [LibraryFilter::All, LibraryFilter::Nes, LibraryFilter::Snes]
                                    [index];
                            change_filter(&entries, &mut filter, next, &mut selected, &mut focus);
                            displayed = visible_entries(&entries, filter);
                        }
                    }
                }
                Event::MouseMotion { x, y, .. } => {
                    let (width, height) = canvas.window().size();
                    hover = Some(coordinates(width, height, x, y));
                }
                Event::MouseWheel { y, .. } if !busy && !picker_pending => {
                    selected = if y > 0.0 {
                        selected.saturating_sub(1)
                    } else {
                        (selected + 1).min(displayed.len().saturating_sub(1))
                    };
                }
                _ => {}
            }
        }
        if let Some(pad) = pad.as_mut() {
            for input in pad.tick(
                started.elapsed(),
                !busy && focused && !picker_pending && !closing,
            ) {
                match pad_action(input, &entries, &mut filter, &mut selected, &mut focus) {
                    Some(-1) => closing = true,
                    Some(code) => {
                        action.get_or_insert(code);
                    }
                    _ => {}
                }
            }
            displayed = visible_entries(&entries, filter);
        }
        if preview.is_none() && saved_filter != filter {
            atomic_json(
                &home.join("launcher.json"),
                &json!({"systemFilter":filter.label()}),
            )?;
            saved_filter = filter;
        }
        if closing && !busy {
            return Ok(None);
        }
        if action == Some(0) && displayed.is_empty() {
            action = Some(1);
        }
        if let Some(action) = action {
            match action {
                0 => match displayed.get(selected).copied() {
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
                    picker_pending = true;
                    if let Some(pad) = pad.as_mut() {
                        pad.suspend();
                    }
                    let sender = sender.clone();
                    dialog::show_open_file_dialog(
                        &[DialogFileFilter {
                            name: "NES / SNES Classic filesystem",
                            pattern: "hsqs;gz;img",
                        }],
                        None::<&Path>,
                        false,
                        Some(canvas.window()),
                        Box::new(move |result, _| {
                            match result {
                                Ok(paths) => {
                                    if let Some(path) = paths.into_iter().next() {
                                        let _ = sender.send(Message::Image(path));
                                    }
                                }
                                Err(DialogError::Canceled) => {}
                                Err(error) => {
                                    let _ = sender.send(Message::Error(error.to_string()));
                                }
                            }
                            let _ = sender.send(Message::PickerClosed);
                        }),
                    )
                    .map_err(|e| e.to_string())?;
                }
                _ => {
                    picker_pending = true;
                    if let Some(pad) = pad.as_mut() {
                        pad.suspend();
                    }
                    let sender = sender.clone();
                    dialog::show_open_folder_dialog(
                        None::<&Path>,
                        false,
                        Some(canvas.window()),
                        Box::new(move |result, _| {
                            match result {
                                Ok(paths) => {
                                    if let Some(path) = paths.into_iter().next() {
                                        let _ = sender.send(Message::Assets(path));
                                    }
                                }
                                Err(DialogError::Canceled) => {}
                                Err(error) => {
                                    let _ = sender.send(Message::Error(error.to_string()));
                                }
                            }
                            let _ = sender.send(Message::PickerClosed);
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
                Message::PickerClosed => {
                    picker_pending = false;
                    if let Some(pad) = pad.as_mut() {
                        pad.suspend();
                    }
                }
                _ => {}
            }
        }
        view.draw(
            &mut canvas,
            &displayed,
            selected,
            busy,
            &status,
            error,
            hover,
            focus,
            filter.label(),
            pad.as_ref().is_some_and(controller::Controller::connected)
                || (preview.is_some()
                    && std::env::var("CLOVER_LAUNCHER_PREVIEW_CONTROLLER").as_deref() == Ok("1")),
            started.elapsed().as_secs_f32(),
        )?;
        if let Some(path) = preview {
            save_screenshot(&canvas, path)?;
            return Ok(None);
        }
        canvas.present();
        pacer.finish_frame();
    }
}

// Kept in the launcher so click handling matches the compact tabs drawn by the view.
const FILTER_TABS: [FRect; 3] = [
    FRect {
        x: 272.0,
        y: 146.0,
        w: 72.0,
        h: 30.0,
    },
    FRect {
        x: 350.0,
        y: 146.0,
        w: 72.0,
        h: 30.0,
    },
    FRect {
        x: 428.0,
        y: 146.0,
        w: 72.0,
        h: 30.0,
    },
];

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

    fn sample_entries() -> Vec<Entry> {
        vec![
            Entry::Image(PathBuf::from("dp-nes-release-v1.0.7.hsqs")),
            Entry::Image(PathBuf::from("dp-hvc-release-v1.0.5.hsqs")),
            Entry::Cached(crate::profile::Profile {
                directory: PathBuf::from("cached-snes"),
                manifest: json!({"system":"snes","ui":"shvc"}),
            }),
            Entry::Image(PathBuf::from("custom.hsqs")),
        ]
    }

    #[test]
    fn filters_cover_images_caches_and_both_regional_families() {
        let entries = sample_entries();
        assert_eq!(visible_entries(&entries, LibraryFilter::All).len(), 4);
        assert_eq!(visible_entries(&entries, LibraryFilter::Nes).len(), 2);
        assert_eq!(visible_entries(&entries, LibraryFilter::Snes).len(), 1);
        assert_eq!(LibraryFilter::All.next(-1), LibraryFilter::Snes);
    }

    #[test]
    fn filtering_retains_identity_and_never_launches_stale_unfiltered_indices() {
        let entries = sample_entries();
        let mut filter = LibraryFilter::All;
        let mut selected = 1;
        let mut focus = Some(2);
        change_filter(
            &entries,
            &mut filter,
            LibraryFilter::Nes,
            &mut selected,
            &mut focus,
        );
        assert_eq!(selected, 1);
        assert_eq!(focus, None);
        change_filter(
            &entries,
            &mut filter,
            LibraryFilter::Snes,
            &mut selected,
            &mut focus,
        );
        assert_eq!(selected, 0);
        assert!(matches!(
            visible_entries(&entries, filter)[selected],
            Entry::Cached(_)
        ));
        change_filter(
            &entries,
            &mut filter,
            LibraryFilter::All,
            &mut selected,
            &mut focus,
        );
        assert_eq!(selected, 2);
    }

    #[test]
    fn ready_profile_replaces_only_the_matching_version_image_row() {
        let ready = Entry::Cached(crate::profile::Profile {
            directory: PathBuf::from("ready-nes-1.0.2"),
            manifest: json!({
                "system":"nes", "ui":"nes", "region":"USA", "version":"1.0.2"
            }),
        });
        let same_release = Entry::Image(PathBuf::from("dp-nes-release-v1.0.2-0-g99e37e1.hsqs"));
        assert_eq!(view::release_key(&ready), view::release_key(&same_release));
        let mut entries = vec![ready];
        append_unimported_images(
            &mut entries,
            [
                PathBuf::from("dp-nes-release-v1.0.2-0-g99e37e1.hsqs"),
                PathBuf::from("dp-nes-release-v1.0.3-0-g99e37e1.hsqs"),
                PathBuf::from("dp-sneseur-release-v1.0.2-0-g99e37e1.hsqs"),
            ],
        );

        assert_eq!(entries.len(), 3);
        assert!(matches!(entries[0], Entry::Cached(_)));
        assert!(entries.iter().any(|entry| matches!(entry,
            Entry::Image(path) if path.to_string_lossy().contains("v1.0.3"))));
        assert!(entries.iter().any(|entry| matches!(entry,
            Entry::Image(path) if path.to_string_lossy().contains("sneseur"))));
        assert!(!entries.iter().any(|entry| matches!(entry,
            Entry::Image(path) if path.to_string_lossy().contains("dp-nes-release-v1.0.2"))));
    }

    #[test]
    fn controller_filter_and_empty_navigation_are_safe() {
        let entries = sample_entries();
        let mut filter = LibraryFilter::Nes;
        let mut selected = 1;
        let mut focus = None;
        pad_action(
            controller::Action::FilterNext,
            &entries,
            &mut filter,
            &mut selected,
            &mut focus,
        );
        assert_eq!(filter, LibraryFilter::Snes);
        assert_eq!(selected, 0);
        for _ in 0..12 {
            pad_action(
                controller::Action::LibraryNext,
                &[],
                &mut filter,
                &mut selected,
                &mut focus,
            );
        }
        assert_eq!(selected, 0);
        assert_eq!(
            pad_action(
                controller::Action::OpenFolder,
                &[],
                &mut filter,
                &mut selected,
                &mut focus
            ),
            Some(2)
        );
    }

    #[test]
    fn letterboxed_hit_testing_matches_layout() {
        let (x, y) = coordinates(1600, 760, 1100.0, 680.0);
        assert!(point_inside(x, y, view::BUTTONS[0]));
        let (x, y) = coordinates(960, 608, 720.0, 544.0);
        assert!(point_inside(x, y, view::BUTTONS[0]));
        assert!(!point_inside(250.0, 300.0, view::BUTTONS[1]));
    }
}
