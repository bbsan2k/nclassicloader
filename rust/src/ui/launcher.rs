//! Asset-independent startup library. Importing stays off the SDL thread.
use super::*;
use crate::frontend::launcher_frame::{
    self as launcher_frame, Activation as FrameActivation, COMPLETION_BUDGET,
    Capture as ActivationCapture, HostEffect as FrameHostEffect, INPUT_BUDGET, ImportDecision,
    InputFrame,
};
use crate::frontend::launcher_navigation::{
    Event as NavigationEvent, Item as NavigationItem, Navigation,
};
use crate::frontend::library::LibraryFilter;
use crate::frontend::library::{self, SystemFamily};
use crate::frontend::requests::{Capability, Outcome, RequestId};
mod controller;
mod effects;
mod image_picker;
pub(super) mod import_worker;
mod presentation;
mod session;
mod startup;
mod view;
#[cfg(test)]
use startup::append_unimported_images;

// Kept for queued-source service tests; per-frame activation eligibility is
// coordinated by frontend::launcher_frame.
#[cfg(test)]
fn eligible_action(
    action: Option<i32>,
    busy: bool,
    closing: bool,
    picker_pending: bool,
) -> Option<i32> {
    action.filter(|_| image_picker::accept_queued_source(busy, closing, picker_pending))
}

fn budgeted<I: Iterator>(iter: I, limit: usize) -> std::iter::Take<I> {
    iter.take(limit)
}

pub(super) struct Selection {
    pub assets: PathBuf,
    pub data: PathBuf,
}

pub(super) enum Entry {
    Cached(crate::profile::Profile),
    Image(PathBuf),
}

fn visible_entries(entries: &[Entry], filter: LibraryFilter) -> Vec<&Entry> {
    library::visible_entries(entries, family, filter)
}

fn change_filter(
    entries: &[Entry],
    filter: &mut LibraryFilter,
    next: LibraryFilter,
    selected: &mut usize,
    focus: &mut Option<usize>,
) {
    library::change_filter(entries, family, entry_key, filter, next, selected, focus)
}

fn family(entry: &Entry) -> SystemFamily {
    match view::system_group(entry) {
        0 => SystemFamily::Nes,
        1 => SystemFamily::Snes,
        _ => SystemFamily::Unknown,
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

enum Message {
    Image(PathBuf),
    Status(import_worker::JobId, String),
    Picked(RequestId, image_picker::Kind, Outcome<PathBuf, String>),
}

fn logical_key(key: Scancode, repeat: bool) -> Option<NavigationEvent> {
    Some(match key {
        Scancode::Up => NavigationEvent::Up,
        Scancode::Down => NavigationEvent::Down,
        Scancode::Tab => NavigationEvent::NextFocus,
        Scancode::Return => NavigationEvent::Confirm { repeat },
        Scancode::I => NavigationEvent::BrowseImage { repeat },
        Scancode::D => NavigationEvent::BrowseFolder { repeat },
        Scancode::Escape => NavigationEvent::Escape { repeat },
        Scancode::_1 | Scancode::Kp1 => NavigationEvent::Filter {
            filter: LibraryFilter::All,
            repeat,
        },
        Scancode::_2 | Scancode::Kp2 => NavigationEvent::Filter {
            filter: LibraryFilter::Nes,
            repeat,
        },
        Scancode::_3 | Scancode::Kp3 => NavigationEvent::Filter {
            filter: LibraryFilter::Snes,
            repeat,
        },
        _ => return None,
    })
}

#[allow(clippy::too_many_arguments)]
fn reduce_and_apply(
    frame: &mut InputFrame,
    navigation: &mut Navigation,
    items: &[NavigationItem],
    activity: &mut crate::frontend::launcher_activity::Activity,
    focused: bool,
    event: NavigationEvent,
    capture: ActivationCapture,
    worker: &import_worker::Worker,
) {
    if frame.reduce_navigation(navigation, activity, items, focused, event, capture)
        == Some(FrameHostEffect::CancelImport)
    {
        request_cancel(worker, activity);
    }
}

fn mouse_cancel_hit(
    busy: bool,
    focused: bool,
    picker_pending: bool,
    closing: bool,
    cancel_view: import_worker::CancellationView,
    point: (f32, f32),
) -> bool {
    busy && focused
        && !picker_pending
        && !closing
        && cancel_view == import_worker::CancellationView::Available
        && point_inside(point.0, point.1, view::BUTTONS[0])
}

fn request_cancel(
    worker: &import_worker::Worker,
    activity: &mut crate::frontend::launcher_activity::Activity,
) {
    if let Some(result) = worker.cancel()
        && (result != crate::frontend::import_lifecycle::CancelResult::TooLate
            || worker.cancellation_view() == import_worker::CancellationView::Finishing)
    {
        activity.reduce(crate::frontend::launcher_activity::Event::CancelFeedback(
            result,
        ));
    }
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
    let startup = startup::load(home);
    let mut state = session::SessionState::new(startup, canvas.window().has_input_focus());
    let mut services = session::NativeServices::default();
    let mut events = context.event_pump().map_err(|e| e.to_string())?;
    let started = Instant::now();
    let mut pad = context.gamepad().ok().map(controller::Controller::new);
    // Snapshot-only states never affect a normal launch or import.
    if preview.is_some() {
        state.apply_preview(session::PreviewOptions::from_env());
    }
    loop {
        let mut displayed = visible_entries(&state.entries, state.navigation.filter);
        state.navigation.selected = state
            .navigation
            .selected
            .min(displayed.len().saturating_sub(1));
        let mut frame = InputFrame::default();
        for event in budgeted(events.poll_iter(), INPUT_BUDGET) {
            if let Some(pad) = pad.as_mut() {
                for input in pad.event(
                    &event,
                    started.elapsed(),
                    state.focused && !state.activity.picker_pending && !state.activity.closing,
                ) {
                    reduce_and_apply(
                        &mut frame,
                        &mut state.navigation,
                        &state.items,
                        &mut state.activity,
                        state.focused,
                        NavigationEvent::Controller(input),
                        ActivationCapture::First,
                        &services.worker,
                    );
                }
                displayed = visible_entries(&state.entries, state.navigation.filter);
            }
            match event {
                Event::Quit { .. } => {
                    frame.request_close(&mut state.activity);
                }
                Event::KeyDown {
                    scancode: Some(key),
                    repeat,
                    ..
                } => {
                    if key == Scancode::F11
                        && !repeat
                        && !state.activity.busy
                        && !state.activity.picker_pending
                    {
                        state.fullscreen = !state.fullscreen;
                        canvas
                            .window_mut()
                            .set_fullscreen(state.fullscreen)
                            .map_err(|e| e.to_string())?;
                    } else if let Some(logical) = logical_key(key, repeat) {
                        reduce_and_apply(
                            &mut frame,
                            &mut state.navigation,
                            &state.items,
                            &mut state.activity,
                            state.focused,
                            logical,
                            ActivationCapture::First,
                            &services.worker,
                        );
                        if matches!(logical, NavigationEvent::Filter { .. }) {
                            displayed = visible_entries(&state.entries, state.navigation.filter);
                        }
                    }
                }
                #[cfg(target_os = "macos")]
                Event::User { type_, code, .. }
                    if type_ == unsafe { clover_native_menu_event() } =>
                {
                    match code {
                        8 => {
                            state.fullscreen = !state.fullscreen;
                            canvas
                                .window_mut()
                                .set_fullscreen(state.fullscreen)
                                .map_err(|e| e.to_string())?;
                        }
                        10 => {
                            frame.request_close(&mut state.activity);
                        }
                        2 if !state.activity.busy && !state.activity.picker_pending => {
                            frame.capture_activation(1, ActivationCapture::First);
                        }
                        _ => {}
                    }
                }
                Event::Window {
                    win_event: sdl3::event::WindowEvent::FocusLost,
                    ..
                } => {
                    state.focused = false;
                    if let Some(pad) = pad.as_mut() {
                        pad.suspend();
                    }
                }
                Event::Window {
                    win_event: sdl3::event::WindowEvent::FocusGained,
                    ..
                } => {
                    state.focused = true;
                    if let Some(pad) = pad.as_mut() {
                        pad.suspend();
                    }
                }
                Event::DropFile { filename, .. }
                    if !state.activity.busy && !state.activity.picker_pending =>
                {
                    let _ = services
                        .sender
                        .send(Message::Image(PathBuf::from(filename)));
                }
                Event::MouseButtonDown {
                    mouse_btn: sdl3::mouse::MouseButton::Left,
                    x,
                    y,
                    ..
                } if state.activity.busy
                    && state.focused
                    && !state.activity.picker_pending
                    && !state.activity.closing =>
                {
                    let cancel_view = state
                        .preview_cancel
                        .unwrap_or_else(|| services.worker.cancellation_view());
                    let (width, height) = canvas.window().size();
                    let (x, y) = coordinates(width, height, x, y);
                    if mouse_cancel_hit(
                        state.activity.busy,
                        state.focused,
                        state.activity.picker_pending,
                        state.activity.closing,
                        cancel_view,
                        (x, y),
                    ) {
                        request_cancel(&services.worker, &mut state.activity);
                    }
                }
                Event::MouseButtonDown {
                    mouse_btn: sdl3::mouse::MouseButton::Left,
                    x,
                    y,
                    clicks,
                    ..
                } if !state.activity.busy && !state.activity.picker_pending => {
                    let (width, height) = canvas.window().size();
                    let (x, y) = coordinates(width, height, x, y);
                    for row in 0..view::PAGE_SIZE {
                        let index =
                            (state.navigation.selected / view::PAGE_SIZE) * view::PAGE_SIZE + row;
                        if index < displayed.len() && point_inside(x, y, view::row_rect(row)) {
                            reduce_and_apply(
                                &mut frame,
                                &mut state.navigation,
                                &state.items,
                                &mut state.activity,
                                state.focused,
                                NavigationEvent::SelectVisible {
                                    index,
                                    double_click: clicks >= 2,
                                },
                                ActivationCapture::Replace,
                                &services.worker,
                            );
                        }
                    }
                    for (index, rect) in view::BUTTONS.iter().enumerate() {
                        if point_inside(x, y, *rect) {
                            reduce_and_apply(
                                &mut frame,
                                &mut state.navigation,
                                &state.items,
                                &mut state.activity,
                                state.focused,
                                NavigationEvent::ActivateButton(index as u8),
                                ActivationCapture::Replace,
                                &services.worker,
                            );
                        }
                    }
                    for (index, rect) in FILTER_TABS.iter().enumerate() {
                        if point_inside(x, y, *rect) {
                            let next =
                                [LibraryFilter::All, LibraryFilter::Nes, LibraryFilter::Snes]
                                    [index];
                            reduce_and_apply(
                                &mut frame,
                                &mut state.navigation,
                                &state.items,
                                &mut state.activity,
                                state.focused,
                                NavigationEvent::Filter {
                                    filter: next,
                                    repeat: false,
                                },
                                ActivationCapture::First,
                                &services.worker,
                            );
                            displayed = visible_entries(&state.entries, state.navigation.filter);
                        }
                    }
                }
                Event::MouseMotion { x, y, .. } => {
                    let (width, height) = canvas.window().size();
                    state.hover = Some(coordinates(width, height, x, y));
                }
                Event::MouseWheel { y, .. }
                    if !state.activity.busy && !state.activity.picker_pending =>
                {
                    reduce_and_apply(
                        &mut frame,
                        &mut state.navigation,
                        &state.items,
                        &mut state.activity,
                        state.focused,
                        NavigationEvent::Wheel { up: y > 0.0 },
                        ActivationCapture::Replace,
                        &services.worker,
                    );
                }
                _ => {}
            }
        }
        if let Some(pad) = pad.as_mut() {
            for input in pad.tick(
                started.elapsed(),
                state.focused && !state.activity.picker_pending && !state.activity.closing,
            ) {
                reduce_and_apply(
                    &mut frame,
                    &mut state.navigation,
                    &state.items,
                    &mut state.activity,
                    state.focused,
                    NavigationEvent::Controller(input),
                    ActivationCapture::First,
                    &services.worker,
                );
            }
            displayed = visible_entries(&state.entries, state.navigation.filter);
        }
        let plan = frame.finish_input(
            &state.navigation,
            &state.activity,
            &state.items,
            preview.is_none().then_some(state.saved_filter),
        );
        if let Some(filter) = plan.persist_filter {
            drop(displayed);
            debug_assert_eq!(filter, state.navigation.filter);
            effects::persist_filter(home, &mut state)?;
            displayed = visible_entries(&state.entries, state.navigation.filter);
        }
        if plan.shutdown_services {
            services.shutdown();
        }
        if plan.exit {
            return Ok(None);
        }
        if let Some(action) = plan.activation {
            match action {
                FrameActivation::Selected => {
                    match displayed.get(state.navigation.selected).copied() {
                        Some(entry) => match effects::activate_entry(home, entry)? {
                            effects::Activation::Select(selection) => return Ok(Some(selection)),
                            effects::Activation::Import(path) => {
                                effects::start_image_worker(
                                    path,
                                    &cache,
                                    pad.as_mut(),
                                    &mut state.activity,
                                    &mut services,
                                );
                            }
                        },
                        None => {
                            state.activity.status =
                                "Browse for a supplied .hsqs, .tar.gz or prepared assets folder."
                                    .into()
                        }
                    }
                }
                FrameActivation::ImagePicker => {
                    drop(displayed);
                    effects::open_image_picker(
                        canvas.window(),
                        pad.as_mut(),
                        &mut state,
                        &mut services,
                    )?;
                    displayed = visible_entries(&state.entries, state.navigation.filter);
                }
                FrameActivation::FolderPicker => {
                    drop(displayed);
                    effects::open_folder_picker(
                        canvas.window(),
                        pad.as_mut(),
                        &mut state,
                        &mut services,
                    )?;
                    displayed = visible_entries(&state.entries, state.navigation.filter);
                }
            }
        }
        let messages: Vec<_> = budgeted(services.receiver.try_iter(), COMPLETION_BUDGET).collect();
        drop(displayed);
        for message in messages {
            match message {
                Message::Image(path)
                    if image_picker::accept_queued_source(
                        state.activity.busy,
                        state.activity.closing,
                        state.activity.picker_pending,
                    ) =>
                {
                    services.picker.invalidate();
                    effects::start_image_worker(
                        path,
                        &cache,
                        pad.as_mut(),
                        &mut state.activity,
                        &mut services,
                    );
                }
                Message::Status(id, value)
                    if services.worker.eligible_status(id, state.activity.closing) =>
                {
                    state
                        .activity
                        .reduce(crate::frontend::launcher_activity::Event::Progress(value));
                }
                Message::Picked(id, kind, outcome) => {
                    let Some(action) = image_picker::finish(
                        &mut services.picker,
                        id,
                        kind,
                        outcome,
                        state.activity.busy,
                        state.activity.closing,
                    ) else {
                        continue;
                    };
                    let completion = effects::continue_picker_action(
                        home,
                        &cache,
                        action,
                        pad.as_mut(),
                        &mut state.activity,
                        &mut services,
                    )?;
                    if !completion.accepted {
                        continue;
                    }
                    if let Some(pad) = pad.as_mut() {
                        pad.suspend();
                    }
                    if let Some(selection) = completion.selection {
                        return Ok(Some(selection));
                    }
                }
                _ => {}
            }
        }
        if let Some(action) = services.worker.poll(state.activity.closing) {
            let outcome = match &action {
                import_worker::Action::Open(_) => {
                    crate::frontend::launcher_activity::ImportOutcome::Succeeded
                }
                import_worker::Action::Cancelled => {
                    crate::frontend::launcher_activity::ImportOutcome::Cancelled
                }
                import_worker::Action::Error(message) => {
                    crate::frontend::launcher_activity::ImportOutcome::Failed(message.clone())
                }
                import_worker::Action::Retired => {
                    crate::frontend::launcher_activity::ImportOutcome::Retired
                }
            };
            let mut imported_profile = match action {
                import_worker::Action::Open(profile) => Some(profile),
                import_worker::Action::Cancelled
                | import_worker::Action::Error(_)
                | import_worker::Action::Retired => None,
            };
            let decision = launcher_frame::complete_import(&mut state.activity, outcome);
            let Some(decision) = decision else {
                continue;
            };
            if let Some(pad) = pad.as_mut() {
                pad.suspend();
            }
            if decision == ImportDecision::Exit {
                return Ok(None);
            }
            if decision == ImportDecision::OpenImported
                && let Some(profile) = imported_profile.take()
            {
                return effects::finish(home, profile).map(Some);
            }
        }
        let snapshot = presentation::PresentationSnapshot::from_native(
            &state,
            &services.worker,
            preview.is_some(),
            pad.as_ref().is_some_and(controller::Controller::connected),
            started.elapsed().as_secs_f32(),
        );
        match presentation::draw(&mut view, &mut canvas, &snapshot, preview) {
            Ok(presentation::DrawResult::PreviewSaved) => return Ok(None),
            Ok(presentation::DrawResult::Drawn) => {}
            Err(error) => return Err(error),
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
    use crate::frontend::launcher_controller::Action as ControllerAction;
    use crate::frontend::launcher_navigation::{
        Effect as NavigationEffect, Gate as NavigationGate,
    };

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

    fn navigation_items(entries: &[Entry]) -> Vec<NavigationItem> {
        entries
            .iter()
            .map(|entry| NavigationItem {
                key: entry_key(entry),
                family: family(entry),
            })
            .collect()
    }

    #[test]
    fn budgeted_input_and_completion_iters_leave_fifo_tail_unconsumed() {
        use std::collections::VecDeque;

        let mut empty_source = VecDeque::<u8>::new();
        let mut empty_polls = 0;
        let empty_iter = std::iter::from_fn(|| {
            empty_polls += 1;
            empty_source.pop_front()
        });
        assert_eq!(budgeted(empty_iter, 0).next(), None);
        assert_eq!(empty_polls, 0);

        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum Input {
            Ordinary(u16),
            Quit,
            Release,
            Focus,
        }
        let mut inputs: VecDeque<_> = (0..INPUT_BUDGET as u16)
            .map(Input::Ordinary)
            .chain([Input::Quit, Input::Release, Input::Focus])
            .collect();
        let mut input_polls = 0;
        let first_batch = budgeted(
            std::iter::from_fn(|| {
                input_polls += 1;
                inputs.pop_front()
            }),
            INPUT_BUDGET,
        )
        .collect::<Vec<_>>();
        assert_eq!(first_batch.len(), INPUT_BUDGET);
        assert_eq!(input_polls, INPUT_BUDGET);
        assert_eq!(inputs.len(), 3);
        let safety_transitions =
            budgeted(std::iter::from_fn(|| inputs.pop_front()), INPUT_BUDGET).collect::<Vec<_>>();
        assert_eq!(
            safety_transitions,
            [Input::Quit, Input::Release, Input::Focus]
        );

        let mut completions: VecDeque<_> = (0..COMPLETION_BUDGET)
            .map(|index| format!("stale-{index}"))
            .chain(std::iter::once("eligible-current-completion".to_owned()))
            .collect();
        let mut completion_polls = 0;
        let first_completions = budgeted(
            std::iter::from_fn(|| {
                completion_polls += 1;
                completions.pop_front()
            }),
            COMPLETION_BUDGET,
        )
        .collect::<Vec<_>>();
        assert_eq!(first_completions.len(), COMPLETION_BUDGET);
        assert_eq!(completion_polls, COMPLETION_BUDGET);
        assert_eq!(completions.len(), 1);
        assert_eq!(
            budgeted(
                std::iter::from_fn(|| completions.pop_front()),
                COMPLETION_BUDGET
            )
            .next()
            .as_deref(),
            Some("eligible-current-completion")
        );
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
        let items = navigation_items(&entries);
        let mut nav = Navigation {
            selected: 1,
            focus: Some(2),
            ..Navigation::default()
        };
        nav.reduce(
            &items,
            NavigationGate::default(),
            NavigationEvent::Filter {
                filter: LibraryFilter::Nes,
                repeat: false,
            },
        );
        assert_eq!(nav.selected, 1);
        assert_eq!(nav.focus, None);
        nav.reduce(
            &items,
            NavigationGate::default(),
            NavigationEvent::Filter {
                filter: LibraryFilter::Snes,
                repeat: false,
            },
        );
        assert_eq!(nav.selected, 0);
        assert!(matches!(
            visible_entries(&entries, nav.filter)[nav.selected],
            Entry::Cached(_)
        ));
        nav.reduce(
            &items,
            NavigationGate::default(),
            NavigationEvent::Filter {
                filter: LibraryFilter::All,
                repeat: false,
            },
        );
        assert_eq!(nav.selected, 2);
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
        let items = navigation_items(&entries);
        let gate = NavigationGate {
            focused: true,
            ..NavigationGate::default()
        };
        let mut nav = Navigation {
            filter: LibraryFilter::Nes,
            selected: 1,
            ..Navigation::default()
        };
        nav.reduce(
            &items,
            gate,
            NavigationEvent::Controller(ControllerAction::FilterNext),
        );
        assert_eq!(nav.filter, LibraryFilter::Snes);
        assert_eq!(nav.selected, 0);
        let empty = [];
        for _ in 0..12 {
            nav.reduce(
                &empty,
                gate,
                NavigationEvent::Controller(ControllerAction::LibraryNext),
            );
        }
        assert_eq!(nav.selected, 0);
        assert_eq!(
            nav.reduce(
                &empty,
                gate,
                NavigationEvent::Controller(ControllerAction::OpenFolder)
            ),
            Some(NavigationEffect::Activate(2))
        );
    }

    #[test]
    fn logical_key_maps_native_scancodes_and_preserves_repeat() {
        for (key, logical) in [
            (Scancode::Up, NavigationEvent::Up),
            (Scancode::Down, NavigationEvent::Down),
            (Scancode::Tab, NavigationEvent::NextFocus),
            (Scancode::Return, NavigationEvent::Confirm { repeat: false }),
            (Scancode::I, NavigationEvent::BrowseImage { repeat: false }),
            (Scancode::D, NavigationEvent::BrowseFolder { repeat: false }),
            (Scancode::Escape, NavigationEvent::Escape { repeat: false }),
        ] {
            assert_eq!(logical_key(key, false), Some(logical));
        }
        for (key, filter) in [
            (Scancode::_1, LibraryFilter::All),
            (Scancode::Kp1, LibraryFilter::All),
            (Scancode::_2, LibraryFilter::Nes),
            (Scancode::Kp2, LibraryFilter::Nes),
            (Scancode::_3, LibraryFilter::Snes),
            (Scancode::Kp3, LibraryFilter::Snes),
        ] {
            for repeat in [false, true] {
                assert_eq!(
                    logical_key(key, repeat),
                    Some(NavigationEvent::Filter { filter, repeat })
                );
            }
        }
        assert_eq!(
            logical_key(Scancode::Return, true),
            Some(NavigationEvent::Confirm { repeat: true })
        );
        assert_eq!(
            logical_key(Scancode::I, true),
            Some(NavigationEvent::BrowseImage { repeat: true })
        );
        assert_eq!(
            logical_key(Scancode::D, true),
            Some(NavigationEvent::BrowseFolder { repeat: true })
        );
        assert_eq!(
            logical_key(Scancode::Escape, true),
            Some(NavigationEvent::Escape { repeat: true })
        );
        assert_eq!(logical_key(Scancode::F11, false), None);
        assert_eq!(logical_key(Scancode::A, false), None);
    }

    #[test]
    fn shared_controller_dispatch_honors_focus_busy_and_entry_counts() {
        let entries = sample_entries();
        let items = navigation_items(&entries);
        for count in 0..=items.len() {
            let subset = &items[..count];
            for focused in [false, true] {
                let gate = NavigationGate {
                    focused,
                    ..NavigationGate::default()
                };
                let mut nav = Navigation {
                    selected: usize::MAX,
                    ..Navigation::default()
                };
                let before = nav;
                nav.reduce(
                    subset,
                    gate,
                    NavigationEvent::Controller(ControllerAction::LibraryNext),
                );
                if focused {
                    assert_eq!(nav.selected, count.saturating_sub(1));
                } else {
                    assert_eq!(nav, before);
                }
            }
        }

        let mut nav = Navigation {
            filter: LibraryFilter::All,
            selected: 1,
            focus: Some(2),
        };
        let gate = NavigationGate {
            focused: true,
            ..NavigationGate::default()
        };
        assert_eq!(
            nav.reduce(
                &items,
                gate,
                NavigationEvent::Controller(ControllerAction::OpenFolder)
            ),
            Some(NavigationEffect::Activate(2))
        );
        assert_eq!(
            nav.reduce(
                &items,
                gate,
                NavigationEvent::Controller(ControllerAction::Cancel)
            ),
            Some(NavigationEffect::Close)
        );
        for expected in [LibraryFilter::Snes, LibraryFilter::Nes, LibraryFilter::All] {
            nav.reduce(
                &items,
                gate,
                NavigationEvent::Controller(ControllerAction::FilterPrevious),
            );
            assert_eq!(nav.filter, expected);
        }
        for input in [
            ControllerAction::LibraryPrevious,
            ControllerAction::LibraryNext,
            ControllerAction::FocusPrevious,
            ControllerAction::FocusNext,
            ControllerAction::FilterPrevious,
            ControllerAction::FilterNext,
            ControllerAction::Confirm,
            ControllerAction::Import,
            ControllerAction::OpenFolder,
            ControllerAction::Cancel,
        ] {
            let mut busy = nav;
            assert_eq!(
                busy.reduce(
                    &items,
                    NavigationGate {
                        busy: true,
                        focused: true,
                        ..NavigationGate::default()
                    },
                    NavigationEvent::Controller(input)
                ),
                (input == ControllerAction::Cancel).then_some(NavigationEffect::CancelImport)
            );
            let mut unfocused_busy = nav;
            assert_eq!(
                unfocused_busy.reduce(
                    &items,
                    NavigationGate {
                        busy: true,
                        ..NavigationGate::default()
                    },
                    NavigationEvent::Controller(input)
                ),
                None
            );
        }
    }

    #[test]
    fn dispatch_keeps_first_activation_and_close_suppresses_it() {
        let items = navigation_items(&sample_entries());
        let worker = import_worker::Worker::default();
        let mut nav = Navigation::default();
        let mut frame = InputFrame::default();
        let mut activity = crate::frontend::launcher_activity::Activity::new("");
        for event in [
            NavigationEvent::BrowseImage { repeat: false },
            NavigationEvent::BrowseFolder { repeat: false },
        ] {
            reduce_and_apply(
                &mut frame,
                &mut nav,
                &items,
                &mut activity,
                true,
                event,
                ActivationCapture::First,
                &worker,
            );
        }
        assert_eq!(
            frame.finish_input(&nav, &activity, &items, None).activation,
            Some(FrameActivation::ImagePicker)
        );
        frame.capture_activation(1, ActivationCapture::First);
        reduce_and_apply(
            &mut frame,
            &mut nav,
            &items,
            &mut activity,
            true,
            NavigationEvent::Controller(ControllerAction::OpenFolder),
            ActivationCapture::First,
            &worker,
        );
        reduce_and_apply(
            &mut frame,
            &mut nav,
            &items,
            &mut activity,
            true,
            NavigationEvent::ActivateButton(2),
            ActivationCapture::Replace,
            &worker,
        );
        assert_eq!(
            frame.finish_input(&nav, &activity, &items, None).activation,
            Some(FrameActivation::FolderPicker)
        );
        frame.capture_activation(2, ActivationCapture::First);
        reduce_and_apply(
            &mut frame,
            &mut nav,
            &items,
            &mut activity,
            true,
            NavigationEvent::Escape { repeat: true },
            ActivationCapture::First,
            &worker,
        );
        assert!(activity.closing);
        assert_eq!(
            frame.finish_input(&nav, &activity, &items, None).activation,
            None
        );
    }

    #[test]
    fn mouse_cancel_and_queued_action_guards_remain_native() {
        let point = (view::BUTTONS[0].x + 2.0, view::BUTTONS[0].y + 2.0);
        assert!(mouse_cancel_hit(
            true,
            true,
            false,
            false,
            import_worker::CancellationView::Available,
            point
        ));
        for state in [
            import_worker::CancellationView::Requested,
            import_worker::CancellationView::Finishing,
            import_worker::CancellationView::Unavailable,
        ] {
            assert!(!mouse_cancel_hit(true, true, false, false, state, point));
        }
        assert!(!mouse_cancel_hit(
            true,
            false,
            false,
            false,
            import_worker::CancellationView::Available,
            point
        ));
        assert!(!mouse_cancel_hit(
            true,
            true,
            true,
            false,
            import_worker::CancellationView::Available,
            point
        ));
        assert!(!mouse_cancel_hit(
            true,
            true,
            false,
            true,
            import_worker::CancellationView::Available,
            point
        ));
        assert!(!mouse_cancel_hit(
            false,
            true,
            false,
            false,
            import_worker::CancellationView::Available,
            point
        ));
        assert!(!mouse_cancel_hit(
            true,
            true,
            false,
            false,
            import_worker::CancellationView::Available,
            (point.0 + view::BUTTONS[0].w, point.1)
        ));

        // Frame-plan gating and close priority are covered by launcher_frame tests.
    }

    #[test]
    fn held_controller_cancel_and_confirm_are_reseeded_before_idle_routing() {
        use crate::frontend::launcher_controller::{Action, Button, Input, Snapshot};
        use std::time::Duration;

        for (button, busy_action) in [
            (Button::East, Action::Cancel),
            (Button::South, Action::Confirm),
        ] {
            let mut held = Snapshot::default();
            held.buttons[button as usize] = true;
            let mut input = Input::default();
            assert_eq!(input.advance(held, Duration::ZERO, true), vec![busy_action]);
            // The actual Controller::suspend path seeds the sampled held state.
            input.seed(held);
            assert!(
                input
                    .advance(held, Duration::from_millis(1), true)
                    .is_empty()
            );
            assert!(
                input
                    .advance(Snapshot::default(), Duration::from_millis(2), true)
                    .is_empty()
            );
            assert_eq!(
                input.advance(held, Duration::from_millis(3), true),
                vec![busy_action]
            );
        }
        // Idle B remains the pre-existing close action, not a busy cancel.
        let mut nav = Navigation::default();
        assert_eq!(
            nav.reduce(
                &[],
                NavigationGate {
                    focused: true,
                    ..NavigationGate::default()
                },
                NavigationEvent::Controller(Action::Cancel)
            ),
            Some(NavigationEffect::Close)
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
