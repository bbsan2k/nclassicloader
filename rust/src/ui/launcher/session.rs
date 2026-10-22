//! Native launcher session data. SDL resources and service owners stay outside.
use super::*;
use crate::frontend::launcher_navigation::{Item, Navigation};
use std::sync::mpsc::{self, Receiver, Sender};

const INITIAL_STATUS: &str =
    "Choose a version to get started. You can also drop an image anywhere in this window.";
const PREVIEW_BUSY_STATUS: &str =
    "Decoding Clover artwork and fonts… Your source image remains untouched.";
const PREVIEW_ERROR_STATUS: &str = "This image could not be imported. Choose a supported NES/SNES Classic .hsqs or .tar.gz filesystem, or open an existing data folder.";

pub(super) struct SessionState {
    pub entries: Vec<Entry>,
    pub items: Vec<Item>,
    pub navigation: Navigation,
    pub saved_filter: LibraryFilter,
    pub activity: crate::frontend::launcher_activity::Activity,
    pub hover: Option<(f32, f32)>,
    pub focused: bool,
    pub fullscreen: bool,
    pub preview_cancel: Option<import_worker::CancellationView>,
}

/// Owns launcher picker/import protocol state and the loop's private message channel.
/// Dropping an active worker requests cancellation and detaches it; it never joins.
pub(super) struct NativeServices {
    pub(super) picker: image_picker::Requests,
    pub(super) worker: import_worker::Worker,
    pub(super) sender: Sender<Message>,
    pub(super) receiver: Receiver<Message>,
}

impl Default for NativeServices {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            picker: image_picker::Requests::default(),
            worker: import_worker::Worker::default(),
            sender,
            receiver,
        }
    }
}

impl NativeServices {
    /// Closes new picker/worker requests and asks an active import to cancel.
    /// The worker remains active until its ordinary result is polled.
    pub fn shutdown(&mut self) {
        self.picker.close();
        self.worker.shutdown();
    }
}

impl Drop for NativeServices {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum PreviewState {
    Empty,
    Busy,
    Error,
    #[default]
    Normal,
}

pub(super) struct PreviewOptions {
    filter: Option<String>,
    system: Option<String>,
    state: PreviewState,
    cancel: Option<import_worker::CancellationView>,
}

impl PreviewOptions {
    pub fn from_env() -> Self {
        let state = match std::env::var("CLOVER_LAUNCHER_PREVIEW_STATE").as_deref() {
            Ok("empty") => PreviewState::Empty,
            Ok("busy") => PreviewState::Busy,
            Ok("error") => PreviewState::Error,
            _ => PreviewState::Normal,
        };
        let cancel = match std::env::var("CLOVER_LAUNCHER_PREVIEW_CANCEL").as_deref() {
            Ok("available") => Some(import_worker::CancellationView::Available),
            Ok("requested") => Some(import_worker::CancellationView::Requested),
            Ok("finishing") => Some(import_worker::CancellationView::Finishing),
            _ => None,
        };
        Self {
            filter: std::env::var("CLOVER_LAUNCHER_PREVIEW_FILTER").ok(),
            system: std::env::var("CLOVER_LAUNCHER_PREVIEW_SYSTEM").ok(),
            state,
            cancel,
        }
    }
}

impl SessionState {
    pub fn new(startup: startup::Startup, focused: bool) -> Self {
        let filter = startup.filter;
        let items = startup
            .entries
            .iter()
            .map(|entry| Item {
                key: entry_key(entry),
                family: family(entry),
            })
            .collect();
        Self {
            entries: startup.entries,
            items,
            navigation: Navigation {
                selected: startup.selected,
                filter,
                focus: None,
            },
            saved_filter: filter,
            activity: crate::frontend::launcher_activity::Activity::new(INITIAL_STATUS),
            hover: None,
            focused,
            fullscreen: false,
            preview_cancel: None,
        }
    }

    pub fn apply_preview(&mut self, options: PreviewOptions) {
        if let Some(filter) = options.filter.as_deref() {
            self.navigation.filter = LibraryFilter::from_label(Some(filter));
            self.navigation.selected = 0;
        }
        if let Some(system) = options.system.as_deref()
            && let Some(index) = visible_entries(&self.entries, self.navigation.filter)
                .iter()
                .position(|entry| view::preview_matches(entry, system))
        {
            self.navigation.selected = index;
        }
        match options.state {
            PreviewState::Empty => {
                self.entries.clear();
                self.items.clear();
            }
            PreviewState::Busy => {
                self.activity.busy = true;
                self.activity.status = PREVIEW_BUSY_STATUS.to_owned();
                self.preview_cancel = options.cancel;
            }
            PreviewState::Error => {
                self.activity.error = true;
                self.activity.status = PREVIEW_ERROR_STATUS.to_owned();
            }
            PreviewState::Normal => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{Arc, Barrier, mpsc},
        thread,
        time::Duration,
    };

    fn synthetic_startup() -> startup::Startup {
        startup::Startup {
            entries: vec![
                Entry::Image(PathBuf::from("dp-nes-release-v1.0.7.hsqs")),
                Entry::Image(PathBuf::from("dp-hvc-release-v1.0.5.hsqs")),
                Entry::Cached(crate::profile::Profile {
                    directory: PathBuf::from("cached-snes"),
                    manifest: json!({"system":"snes","ui":"shvc"}),
                }),
            ],
            selected: 1,
            filter: LibraryFilter::Nes,
        }
    }

    fn options(
        filter: Option<&str>,
        system: Option<&str>,
        state: PreviewState,
        cancel: Option<import_worker::CancellationView>,
    ) -> PreviewOptions {
        PreviewOptions {
            filter: filter.map(str::to_owned),
            system: system.map(str::to_owned),
            state,
            cancel,
        }
    }

    fn profile() -> crate::profile::Profile {
        crate::profile::Profile {
            directory: PathBuf::from("synthetic"),
            manifest: json!({"synthetic": true}),
        }
    }

    #[test]
    fn initializes_native_state_from_startup_and_host_focus() {
        let state = SessionState::new(synthetic_startup(), true);
        assert_eq!(state.navigation.selected, 1);
        assert_eq!(state.navigation.filter, LibraryFilter::Nes);
        assert_eq!(state.saved_filter, LibraryFilter::Nes);
        assert!(state.focused);
        assert!(!state.activity.busy && !state.activity.picker_pending && !state.activity.closing);
        assert_eq!(state.activity.status, INITIAL_STATUS);
    }

    #[test]
    fn preview_filter_resets_selection_but_system_match_selects_visible_row() {
        let mut state = SessionState::new(synthetic_startup(), false);
        state.apply_preview(options(
            Some("invalid"),
            Some("snes"),
            PreviewState::Normal,
            None,
        ));
        assert_eq!(state.navigation.filter, LibraryFilter::All);
        assert_eq!(state.navigation.selected, 2);
        assert_eq!(state.saved_filter, LibraryFilter::Nes);

        state.apply_preview(options(
            Some("NES"),
            Some("missing"),
            PreviewState::Normal,
            None,
        ));
        assert_eq!(state.navigation.filter, LibraryFilter::Nes);
        assert_eq!(state.navigation.selected, 0);
    }

    #[test]
    fn preview_empty_busy_and_error_are_data_only_state_changes() {
        let mut empty = SessionState::new(synthetic_startup(), false);
        empty.apply_preview(options(None, None, PreviewState::Empty, None));
        assert!(empty.entries.is_empty());

        for (cancel, expected) in [
            (None, None),
            (
                Some(import_worker::CancellationView::Available),
                Some(import_worker::CancellationView::Available),
            ),
            (
                Some(import_worker::CancellationView::Requested),
                Some(import_worker::CancellationView::Requested),
            ),
            (
                Some(import_worker::CancellationView::Finishing),
                Some(import_worker::CancellationView::Finishing),
            ),
        ] {
            let mut busy = SessionState::new(synthetic_startup(), false);
            busy.apply_preview(options(None, None, PreviewState::Busy, cancel));
            assert!(busy.activity.busy);
            assert_eq!(busy.preview_cancel, expected);
            assert_eq!(busy.activity.status, PREVIEW_BUSY_STATUS);
            assert!(!busy.activity.error);
        }

        let mut error = SessionState::new(synthetic_startup(), false);
        error.apply_preview(options(None, None, PreviewState::Error, None));
        assert!(error.activity.error);
        assert!(!error.activity.busy);
        assert_eq!(error.activity.status, PREVIEW_ERROR_STATUS);
    }

    #[test]
    fn preview_busy_without_job_keeps_services_idle_and_closeable() {
        let mut state = SessionState::new(synthetic_startup(), false);
        state.apply_preview(options(None, None, PreviewState::Busy, None));
        let mut services = NativeServices::default();
        assert!(state.activity.busy);
        assert_eq!(state.activity.status, PREVIEW_BUSY_STATUS);
        assert!(!services.worker.is_busy());
        assert_eq!(
            services.worker.cancellation_view(),
            import_worker::CancellationView::Unavailable
        );
        state.activity.closing = true;
        services.shutdown();
        assert!(
            state.activity.busy,
            "closing does not clear synthetic preview busy"
        );
        assert!(!services.worker.is_busy());
    }

    #[test]
    fn services_shutdown_closes_both_picker_kinds_and_ignores_pending_results() {
        let mut services = NativeServices::default();
        let image = services
            .picker
            .begin(image_picker::Kind::Image, Capability::Available)
            .unwrap();
        services.shutdown();
        services.shutdown();
        for kind in [
            image_picker::Kind::Image,
            image_picker::Kind::PreparedFolder,
        ] {
            assert!(services.picker.begin(kind, Capability::Available).is_err());
        }
        assert!(
            image_picker::finish(
                &mut services.picker,
                image,
                image_picker::Kind::Image,
                Outcome::Selected(PathBuf::from("late.hsqs")),
                false,
                false,
            )
            .is_none()
        );
        assert!(!services.worker.is_busy());
        assert!(services.worker.start(|_, _| Ok(profile())).is_err());
    }

    #[test]
    fn input_reseeding_does_not_invalidate_service_picker_requests() {
        let mut services = NativeServices::default();
        let request = services
            .picker
            .begin(image_picker::Kind::PreparedFolder, Capability::Available)
            .unwrap();
        let mut state = SessionState::new(synthetic_startup(), true);
        state.focused = false;
        state.focused = true;
        state.navigation.focus = None;
        assert!(matches!(
            image_picker::finish(
                &mut services.picker,
                request,
                image_picker::Kind::PreparedFolder,
                Outcome::Selected(PathBuf::from("prepared")),
                false,
                false,
            ),
            Some(image_picker::Action::Assets(path))
                if path.as_path() == std::path::Path::new("prepared")
        ));
        assert!(state.focused);
        assert!(!state.activity.picker_pending);
    }

    #[test]
    fn shutdown_cancels_active_job_until_closing_poll_retires_without_ui_mutation() {
        use crate::frontend::import_lifecycle::State as ImportState;

        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let worker_entered = entered.clone();
        let worker_release = release.clone();
        let (control_tx, control_rx) = mpsc::channel();
        let mut services = NativeServices::default();
        let mut state = SessionState::new(synthetic_startup(), false);
        state.activity.busy = true;
        state.activity.status = "existing status".into();
        state.activity.error = true;
        let id = services
            .worker
            .start(move |_, control| {
                control.start().unwrap();
                control_tx.send(control.clone()).unwrap();
                worker_entered.wait();
                worker_release.wait();
                control
                    .checkpoint()
                    .map_err(crate::import_job::checkpoint_error)?;
                Ok(profile())
            })
            .unwrap();
        let control = control_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        entered.wait();

        state.activity.closing = true;
        services.shutdown();
        assert_eq!(control.state(), ImportState::CancelRequested);
        assert!(services.worker.is_busy());
        assert!(
            state.activity.busy,
            "shutdown must not clear session busy early"
        );
        assert!(!services.worker.eligible_status(id, false));
        assert!(services.worker.poll(true).is_none());
        assert_eq!(state.activity.status, "existing status");
        assert!(state.activity.error);

        release.wait();
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(action) = services.worker.poll(true) {
                state.activity.busy = false;
                assert!(matches!(action, import_worker::Action::Retired));
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "worker did not retire"
            );
            thread::yield_now();
        }
        assert!(!state.activity.busy);
        assert!(!services.worker.is_busy());
        assert_eq!(state.activity.status, "existing status");
        assert!(state.activity.error);
    }

    #[test]
    fn dropping_services_detaches_held_worker_and_disconnects_late_messages() {
        use crate::frontend::import_lifecycle::State as ImportState;

        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let worker_entered = entered.clone();
        let worker_release = release.clone();
        let (control_tx, control_rx) = mpsc::channel();
        let (worker_done_tx, worker_done_rx) = mpsc::channel();
        let mut services = NativeServices::default();
        services
            .worker
            .start(move |_, control| {
                control_tx.send(control.clone()).unwrap();
                worker_entered.wait();
                worker_release.wait();
                worker_done_tx.send(control.state()).unwrap();
                Ok(profile())
            })
            .unwrap();
        let control = control_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        entered.wait();
        let late_sender = services.sender.clone();
        let (drop_tx, drop_rx) = mpsc::channel();
        let dropper = thread::spawn(move || {
            drop(services);
            drop_tx.send(()).unwrap();
        });
        let returned_before_release = drop_rx.recv_timeout(Duration::from_secs(2)).is_ok();
        assert_eq!(control.state(), ImportState::CancelRequested);
        assert!(
            late_sender
                .send(Message::Image(PathBuf::from("late.hsqs")))
                .is_err()
        );
        release.wait();
        dropper.join().unwrap();
        assert!(
            returned_before_release,
            "service Drop waited for the worker"
        );
        assert_eq!(
            worker_done_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            ImportState::CancelRequested
        );
    }
}
