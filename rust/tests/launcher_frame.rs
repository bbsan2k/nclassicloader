use clover_portable::frontend::{
    import_lifecycle::{CancelResult, CheckError, Lifecycle},
    launcher_activity::{Activity, ImportOutcome},
    launcher_controller::Action as ControllerAction,
    launcher_frame::{
        self, Activation, COMPLETION_BUDGET, Capture, HostEffect, INPUT_BUDGET, ImportDecision,
        InputFrame, PickerDecision, PickerOutcome,
    },
    launcher_navigation::{Event as NavigationEvent, Item, Navigation},
    library::{LibraryFilter, SystemFamily},
    requests::{Capability, Outcome, RequestId, RequestTracker},
};
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PickerKind {
    Image,
    Folder,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Effect {
    OpenSelected,
    OpenImagePicker,
    OpenFolderPicker,
    StartImport,
    ResolveFolder,
    RequestCancel,
    InputReleased,
    OpenImported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WorkerResult {
    Succeeded,
    Cancelled,
    Failed,
}

enum RawInput {
    Logical(NavigationEvent, Capture),
    Release,
    Focus(bool),
    Close,
}

enum Notification {
    Progress(RequestId, String),
    Completed(RequestId, WorkerResult),
}

struct FakeHost {
    frame: InputFrame,
    navigation: Navigation,
    activity: Activity,
    focused: bool,
    items: Vec<Item>,
    saved_filter: LibraryFilter,
    persist_succeeds: bool,
    picker_requests: RequestTracker,
    picker_kind: Option<PickerKind>,
    worker_requests: RequestTracker,
    worker_id: Option<RequestId>,
    lifecycle: Lifecycle,
    effects: Vec<Effect>,
    opens: usize,
}

impl FakeHost {
    fn new() -> Self {
        Self {
            frame: InputFrame::default(),
            navigation: Navigation::default(),
            activity: Activity::new("idle"),
            focused: true,
            items: vec![
                Item {
                    key: "nes-one".into(),
                    family: SystemFamily::Nes,
                },
                Item {
                    key: "snes-one".into(),
                    family: SystemFamily::Snes,
                },
                Item {
                    key: "unknown".into(),
                    family: SystemFamily::Unknown,
                },
                Item {
                    key: "nes-two".into(),
                    family: SystemFamily::Nes,
                },
            ],
            saved_filter: LibraryFilter::All,
            persist_succeeds: true,
            picker_requests: RequestTracker::default(),
            picker_kind: None,
            worker_requests: RequestTracker::default(),
            worker_id: None,
            lifecycle: Lifecycle::default(),
            effects: Vec::new(),
            opens: 0,
        }
    }

    fn begin_tick(&mut self) {
        self.frame = InputFrame::default();
    }

    fn input(&mut self, event: NavigationEvent, capture: Capture) -> Option<HostEffect> {
        let effect = self.frame.reduce_navigation(
            &mut self.navigation,
            &mut self.activity,
            &self.items,
            self.focused,
            event,
            capture,
        );
        if effect == Some(HostEffect::CancelImport) {
            self.effects.push(Effect::RequestCancel);
            self.lifecycle.request_cancel();
            // The host request is not a completion acknowledgment.
            assert!(self.activity.busy);
        }
        effect
    }

    fn finish_tick(&mut self) -> launcher_frame::FramePlan {
        self.frame.finish_input(
            &self.navigation,
            &self.activity,
            &self.items,
            Some(self.saved_filter),
        )
    }

    fn persist_filter(&mut self, filter: LibraryFilter) {
        if self.persist_succeeds {
            self.saved_filter = filter;
        }
    }

    fn execute_activation(&mut self, activation: Activation) {
        match activation {
            Activation::Selected => self.effects.push(Effect::OpenSelected),
            Activation::ImagePicker => {
                self.effects.push(Effect::OpenImagePicker);
                self.begin_picker(PickerKind::Image);
            }
            Activation::FolderPicker => {
                self.effects.push(Effect::OpenFolderPicker);
                self.begin_picker(PickerKind::Folder);
            }
        }
    }

    fn begin_picker(&mut self, kind: PickerKind) -> RequestId {
        let id = self.picker_requests.begin(Capability::Available).unwrap();
        assert!(
            self.activity
                .reduce(clover_portable::frontend::launcher_activity::Event::PickerOpened)
        );
        self.picker_kind = Some(kind);
        id
    }

    /// The fake owner validates its request namespace and picker kind before
    /// allowing the production completion plan to see the semantic result.
    fn picker_completion(
        &mut self,
        id: RequestId,
        kind: PickerKind,
        outcome: Outcome<(), String>,
    ) -> PickerDecision {
        if self.picker_kind != Some(kind) {
            return PickerDecision::Rejected;
        }
        let Some(outcome) = self.picker_requests.complete(id, outcome) else {
            return PickerDecision::Rejected;
        };
        self.picker_kind = None;
        let semantic = match outcome {
            Outcome::Selected(()) => match kind {
                PickerKind::Image => PickerOutcome::SelectedImage,
                PickerKind::Folder => PickerOutcome::SelectedFolder,
            },
            Outcome::Cancelled => PickerOutcome::Cancelled,
            Outcome::Denied(error) | Outcome::Failed(error) => PickerOutcome::Failed(error),
        };
        let decision = launcher_frame::complete_picker(&mut self.activity, &semantic);
        match decision {
            PickerDecision::ImportImage => self.start_import(),
            PickerDecision::ResolveFolder => self.effects.push(Effect::ResolveFolder),
            PickerDecision::Rejected | PickerDecision::Done => {}
        }
        decision
    }

    fn start_import(&mut self) {
        assert!(launcher_frame::source_allowed(
            self.activity.busy,
            self.activity.closing,
            self.activity.picker_pending
        ));
        assert!(
            self.activity
                .reduce(clover_portable::frontend::launcher_activity::Event::ImportStarted)
        );
        assert_eq!(self.lifecycle.start(), Ok(()));
        self.worker_id = Some(self.worker_requests.begin(Capability::Available).unwrap());
        self.effects.push(Effect::StartImport);
    }

    fn progress(&mut self, id: RequestId, message: String) {
        // Request ownership is checked here, before shared Activity is touched.
        if self.worker_id != Some(id) || self.activity.closing {
            return;
        }
        self.activity
            .reduce(clover_portable::frontend::launcher_activity::Event::Progress(message));
    }

    fn worker_completion(&mut self, id: RequestId, result: WorkerResult) -> Option<ImportDecision> {
        // The host owns job identity; the shared completion plan never receives it.
        if self.worker_id != Some(id) {
            return None;
        }
        let tracker_outcome = match result {
            WorkerResult::Succeeded => Outcome::Selected(()),
            WorkerResult::Cancelled => Outcome::Cancelled,
            WorkerResult::Failed => Outcome::Failed("worker failed".to_owned()),
        };
        self.worker_requests.complete(id, tracker_outcome)?;
        self.worker_id = None;
        match result {
            WorkerResult::Succeeded => {
                if self.lifecycle.state()
                    == clover_portable::frontend::import_lifecycle::State::Running
                {
                    assert_eq!(self.lifecycle.begin_commit(), Ok(()));
                }
                assert!(self.lifecycle.finish_success());
            }
            WorkerResult::Cancelled => {
                if self.lifecycle.state()
                    == clover_portable::frontend::import_lifecycle::State::CancelRequested
                {
                    assert_eq!(self.lifecycle.checkpoint(), Err(CheckError::Cancelled));
                }
            }
            WorkerResult::Failed => assert!(self.lifecycle.finish_failure()),
        }
        let outcome = match result {
            WorkerResult::Succeeded => ImportOutcome::Succeeded,
            WorkerResult::Cancelled => ImportOutcome::Cancelled,
            WorkerResult::Failed => ImportOutcome::Failed("worker failed".into()),
        };
        let decision = launcher_frame::complete_import(&mut self.activity, outcome)?;
        if decision == ImportDecision::OpenImported {
            self.opens += 1;
            self.effects.push(Effect::OpenImported);
        }
        Some(decision)
    }

    fn drain_notifications(&mut self, queue: &mut VecDeque<Notification>) -> usize {
        let mut drained = 0;
        while drained < COMPLETION_BUDGET {
            let Some(notification) = queue.pop_front() else {
                break;
            };
            drained += 1;
            match notification {
                Notification::Progress(id, message) => self.progress(id, message),
                Notification::Completed(id, result) => {
                    self.worker_completion(id, result);
                }
            }
        }
        drained
    }
}

#[test]
fn production_frame_supports_external_host_navigation_and_activation_plans() {
    let mut host = FakeHost::new();
    host.navigation.selected = 3;
    host.begin_tick();
    host.input(
        NavigationEvent::Filter {
            filter: LibraryFilter::Nes,
            repeat: false,
        },
        Capture::First,
    );
    assert_eq!(
        host.navigation.selected, 1,
        "opaque nes-two identity is retained"
    );
    host.input(
        NavigationEvent::Filter {
            filter: LibraryFilter::All,
            repeat: false,
        },
        Capture::First,
    );
    assert_eq!(host.navigation.selected, 3);
    assert_eq!(host.finish_tick().persist_filter, None);

    host.begin_tick();
    host.input(NavigationEvent::Confirm { repeat: false }, Capture::First);
    host.input(
        NavigationEvent::Controller(ControllerAction::OpenFolder),
        Capture::First,
    );
    assert_eq!(host.finish_tick().activation, Some(Activation::Selected));

    host.begin_tick();
    host.input(
        NavigationEvent::BrowseImage { repeat: false },
        Capture::First,
    );
    host.input(NavigationEvent::ActivateButton(2), Capture::Replace);
    assert_eq!(
        host.finish_tick().activation,
        Some(Activation::FolderPicker)
    );

    host.navigation = Navigation::default();
    host.begin_tick();
    host.input(NavigationEvent::Confirm { repeat: false }, Capture::First);
    host.items.clear();
    let fallback = host.finish_tick().activation.unwrap();
    assert_eq!(fallback, Activation::ImagePicker);
    host.execute_activation(fallback);
    assert!(host.activity.picker_pending);

    host.picker_requests.invalidate();
    host.picker_kind = None;
    host.activity = Activity::new("idle");
    host.items = vec![Item {
        key: "opaque".into(),
        family: SystemFamily::Unknown,
    }];
    host.begin_tick();
    host.input(NavigationEvent::Confirm { repeat: false }, Capture::First);
    host.input(
        NavigationEvent::BrowseImage { repeat: false },
        Capture::Replace,
    );
    host.input(NavigationEvent::Escape { repeat: false }, Capture::First);
    let plan = host.finish_tick();
    assert!(plan.shutdown_services && plan.exit);
    assert_eq!(
        plan.activation, None,
        "close suppresses queued picker effects"
    );

    host.activity = Activity::new("idle");
    let picker_id = host.begin_picker(PickerKind::Image);
    host.begin_tick();
    host.input(
        NavigationEvent::BrowseFolder { repeat: false },
        Capture::Replace,
    );
    assert_eq!(host.finish_tick().activation, None);
    assert!(!launcher_frame::source_allowed(
        host.activity.busy,
        host.activity.closing,
        host.activity.picker_pending
    ));
    assert_eq!(
        host.picker_completion(picker_id, PickerKind::Image, Outcome::Selected(())),
        PickerDecision::ImportImage
    );
    assert!(host.activity.busy && !host.activity.picker_pending);
    assert_eq!(host.effects.last(), Some(&Effect::StartImport));
}

#[test]
fn fake_owner_filters_stale_notifications_and_lifecycle_results_without_shared_ids() {
    let mut host = FakeHost::new();
    host.start_import();
    host.navigation.selected = 2;
    let first_id = host.worker_id.unwrap();
    assert_eq!(
        host.input(NavigationEvent::Escape { repeat: false }, Capture::First),
        Some(HostEffect::CancelImport)
    );
    assert!(host.activity.busy, "request is not acknowledgment");
    assert_eq!(
        host.lifecycle.state(),
        clover_portable::frontend::import_lifecycle::State::CancelRequested
    );
    assert_eq!(
        host.worker_completion(first_id, WorkerResult::Cancelled),
        Some(ImportDecision::Stay)
    );
    assert!(!host.activity.busy && !host.activity.error);
    assert_eq!(host.navigation.selected, 2, "selection is preserved");

    host.lifecycle = Lifecycle::default();
    host.start_import();
    let old_id = host.worker_id.unwrap();
    assert_eq!(
        host.worker_completion(old_id, WorkerResult::Failed),
        Some(ImportDecision::Stay)
    );
    assert!(host.activity.error);
    assert_eq!(host.activity.status, "worker failed");
    assert_eq!(host.navigation.selected, 2);

    host.activity.error = false;
    host.activity.status = "new job".into();
    host.lifecycle = Lifecycle::default();
    host.start_import();
    let current_id = host.worker_id.unwrap();
    assert_ne!(old_id, current_id);
    host.progress(old_id, "stale progress".into());
    assert_eq!(
        host.worker_completion(old_id, WorkerResult::Succeeded),
        None
    );
    assert!(host.activity.busy);
    assert_eq!(host.activity.status, "Starting read-only import...");
    assert_eq!(host.opens, 0);

    assert_eq!(
        host.worker_completion(current_id, WorkerResult::Succeeded),
        Some(ImportDecision::OpenImported)
    );
    assert_eq!(host.opens, 1);
    assert_eq!(
        host.worker_completion(current_id, WorkerResult::Succeeded),
        None
    );
    assert_eq!(host.opens, 1, "duplicate result cannot open twice");

    host.lifecycle = Lifecycle::default();
    host.start_import();
    let too_late_id = host.worker_id.unwrap();
    assert_eq!(host.lifecycle.begin_commit(), Ok(()));
    assert_eq!(host.lifecycle.request_cancel(), CancelResult::TooLate);
    assert_eq!(
        host.worker_completion(too_late_id, WorkerResult::Succeeded),
        Some(ImportDecision::OpenImported)
    );
    assert_eq!(host.opens, 2);

    host.lifecycle = Lifecycle::default();
    host.start_import();
    let closing_id = host.worker_id.unwrap();
    host.activity
        .reduce(clover_portable::frontend::launcher_activity::Event::CloseRequested);
    assert_eq!(
        host.worker_completion(closing_id, WorkerResult::Succeeded),
        Some(ImportDecision::Exit)
    );
    assert_eq!(host.opens, 2, "closing success is never opened");
}

#[test]
fn bounded_input_and_notification_queues_retain_fifo_safety_transitions() {
    let mut host = FakeHost::new();
    host.items = vec![Item {
        key: "one".into(),
        family: SystemFamily::Nes,
    }];
    let mut raw: VecDeque<_> = (0..INPUT_BUDGET)
        .map(|_| RawInput::Logical(NavigationEvent::Up, Capture::First))
        .chain([RawInput::Release, RawInput::Focus(false), RawInput::Close])
        .collect();
    host.begin_tick();
    let mut consumed = 0;
    while consumed < INPUT_BUDGET {
        let Some(input) = raw.pop_front() else { break };
        consumed += 1;
        match input {
            RawInput::Logical(event, capture) => {
                host.input(event, capture);
            }
            RawInput::Release => host.effects.push(Effect::InputReleased),
            RawInput::Focus(focused) => host.focused = focused,
            RawInput::Close => host.frame.request_close(&mut host.activity),
        }
    }
    assert_eq!(consumed, INPUT_BUDGET);
    assert_eq!(raw.len(), 3, "bounded take leaves the FIFO tail untouched");
    host.finish_tick();

    host.begin_tick();
    while let Some(input) = raw.pop_front() {
        match input {
            RawInput::Logical(event, capture) => {
                host.input(event, capture);
            }
            RawInput::Release => host.effects.push(Effect::InputReleased),
            RawInput::Focus(focused) => host.focused = focused,
            RawInput::Close => host.frame.request_close(&mut host.activity),
        }
    }
    assert!(!host.focused);
    assert!(host.finish_tick().exit, "deferred close remains eligible");

    let mut notifications = VecDeque::new();
    host.activity = Activity::new("import");
    host.lifecycle = Lifecycle::default();
    host.start_import();
    let job = host.worker_id.unwrap();
    for index in 0..COMPLETION_BUDGET {
        notifications.push_back(Notification::Progress(job, format!("progress-{index}")));
    }
    notifications.push_back(Notification::Completed(job, WorkerResult::Succeeded));

    assert_eq!(
        host.drain_notifications(&mut notifications),
        COMPLETION_BUDGET
    );
    assert_eq!(notifications.len(), 1);
    assert!(host.activity.busy, "65th completion has not been drained");
    host.begin_tick();
    host.input(NavigationEvent::Confirm { repeat: false }, Capture::First);
    assert_eq!(host.finish_tick().activation, None);
    assert_eq!(host.opens, 0);

    assert_eq!(host.drain_notifications(&mut notifications), 1);
    assert!(!host.activity.busy);
    assert_eq!(host.opens, 1);
    assert!(notifications.is_empty());
}

#[test]
fn failed_filter_persistence_retries_until_host_acknowledges_success() {
    let mut host = FakeHost::new();
    host.begin_tick();
    host.input(
        NavigationEvent::Filter {
            filter: LibraryFilter::Snes,
            repeat: false,
        },
        Capture::First,
    );
    let first_plan = host.finish_tick();
    assert_eq!(first_plan.persist_filter, Some(LibraryFilter::Snes));
    host.persist_succeeds = false;
    host.persist_filter(first_plan.persist_filter.unwrap());
    assert_eq!(host.saved_filter, LibraryFilter::All);

    host.begin_tick();
    let retry = host.finish_tick();
    assert_eq!(retry.persist_filter, Some(LibraryFilter::Snes));
    host.persist_succeeds = true;
    host.persist_filter(retry.persist_filter.unwrap());
    assert_eq!(host.saved_filter, LibraryFilter::Snes);

    host.begin_tick();
    assert_eq!(host.finish_tick().persist_filter, None);
}
