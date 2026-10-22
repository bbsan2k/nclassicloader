use crate::{
    frontend::import_lifecycle::{CancelResult, State},
    frontend::requests::{BeginError, Capability, Outcome, RequestId, RequestTracker},
    import_job::{ImportControl, ImportError},
    profile::Profile,
};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::mpsc::{self, Receiver, TryRecvError},
    thread::{self, JoinHandle},
};

/// Import-worker identity is deliberately distinct from picker request IDs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ui) struct JobId(RequestId);

#[derive(Debug)]
pub(in crate::ui) enum Action {
    Open(Profile),
    Cancelled,
    Error(String),
    Retired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ui) enum CancellationView {
    Unavailable,
    Available,
    Requested,
    Finishing,
}

#[derive(Default)]
pub(super) struct Requests {
    tracker: RequestTracker,
    active: Option<(JobId, ImportControl)>,
    closed: bool,
}

/// Owns the one import thread. An unfinished handle is detached on drop after
/// cancellation is requested; its worker-local stage guard still owns cleanup.
/// Early launcher exits therefore do not guarantee the thread is joined.
#[derive(Default)]
pub(in crate::ui) struct Worker {
    requests: Requests,
    handle: Option<Running>,
}

struct Running {
    id: JobId,
    handle: JoinHandle<()>,
    result: Receiver<Result<Profile, ImportError>>,
}

impl Worker {
    pub(in crate::ui) fn start(
        &mut self,
        run: impl FnOnce(JobId, ImportControl) -> Result<Profile, ImportError> + Send + 'static,
    ) -> Result<JobId, String> {
        self.start_with_tail(run, || {})
    }

    fn start_with_tail(
        &mut self,
        run: impl FnOnce(JobId, ImportControl) -> Result<Profile, ImportError> + Send + 'static,
        after_send: impl FnOnce() + Send + 'static,
    ) -> Result<JobId, String> {
        let (id, control) = self
            .requests
            .begin()
            .map_err(|error| format!("Cannot start import worker: {error:?}"))?;
        let (sender, result) = mpsc::channel();
        let handle = match thread::Builder::new()
            .name("clover-import".into())
            .spawn(move || {
                let result = catch_unwind(AssertUnwindSafe(|| run(id, control)))
                    .unwrap_or_else(|_| Err(ImportError::Failed("Import worker panicked".into())));
                let _ = sender.send(result);
                after_send();
            }) {
            Ok(handle) => handle,
            Err(error) => {
                let _ = self.requests.finish(
                    id,
                    Err(ImportError::Failed(format!(
                        "Cannot start import worker: {error}"
                    ))),
                    false,
                );
                return Err(format!("Cannot start import worker: {error}"));
            }
        };
        self.handle = Some(Running { id, handle, result });
        Ok(id)
    }

    pub(in crate::ui) fn eligible_status(&self, id: JobId, closing: bool) -> bool {
        self.requests.eligible_status(id, closing)
    }

    pub(in crate::ui) fn cancel(&self) -> Option<CancelResult> {
        self.requests.cancel()
    }

    pub(in crate::ui) fn cancellation_view(&self) -> CancellationView {
        self.requests.cancellation_view()
    }

    pub(in crate::ui) fn shutdown(&mut self) {
        self.requests.shutdown();
    }

    /// Polls a per-job result channel; never joins the worker thread.
    pub(in crate::ui) fn poll(&mut self, closing: bool) -> Option<Action> {
        let running = self.handle.as_ref()?;
        let result = match running.result.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => Err(ImportError::Failed(
                "Import worker result channel disconnected".into(),
            )),
        };
        let running = self.handle.take()?;
        drop(running.handle);
        self.requests.finish(running.id, result, closing)
    }

    /// The worker remains busy until its result is consumed and retired.
    pub(in crate::ui) fn is_busy(&self) -> bool {
        self.handle.is_some()
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.requests.shutdown();
        // Dropping a JoinHandle detaches it. Never join, including after a
        // result arrives: thread tail/TLS teardown may still be in progress.
    }
}

impl Requests {
    pub(super) fn begin(&mut self) -> Result<(JobId, ImportControl), BeginError> {
        if self.closed {
            return Err(BeginError::Closed);
        }
        let id = JobId(self.tracker.begin(Capability::Available)?);
        let control = ImportControl::new();
        self.active = Some((id, control.clone()));
        Ok((id, control))
    }

    pub(super) fn eligible_status(&self, id: JobId, closing: bool) -> bool {
        !self.closed
            && !closing
            && self.active.as_ref().is_some_and(|(active, control)| {
                *active == id
                    && !matches!(control.state(), State::CancelRequested | State::Cancelled)
            })
    }

    pub(super) fn cancel(&self) -> Option<CancelResult> {
        if self.closed {
            return None;
        }
        self.active.as_ref().map(|(_, control)| control.cancel())
    }

    pub(super) fn cancellation_view(&self) -> CancellationView {
        let Some((_, control)) = &self.active else {
            return CancellationView::Unavailable;
        };
        match control.state() {
            State::Created | State::Running => CancellationView::Available,
            State::CancelRequested | State::Cancelled => CancellationView::Requested,
            State::Committing | State::Completed | State::Failed => CancellationView::Finishing,
        }
    }

    pub(super) fn finish(
        &mut self,
        id: JobId,
        result: Result<Profile, ImportError>,
        closing: bool,
    ) -> Option<Action> {
        let outcome = result
            .map(Outcome::Selected)
            .unwrap_or_else(|error| match error {
                ImportError::Cancelled => Outcome::Cancelled,
                ImportError::AlreadyStarted => Outcome::Failed(
                    "Import worker contract violation: control already started".into(),
                ),
                ImportError::Failed(message) => Outcome::Failed(message),
            });
        let outcome = self.tracker.complete(id.0, outcome)?;
        let (active, _) = self.active.take()?;
        if active != id {
            return None;
        }
        if closing || self.closed {
            return Some(Action::Retired);
        }
        Some(match outcome {
            Outcome::Selected(profile) => Action::Open(profile),
            Outcome::Cancelled => Action::Cancelled,
            Outcome::Denied(message) | Outcome::Failed(message) => Action::Error(message),
        })
    }

    pub(super) fn shutdown(&mut self) {
        self.closed = true;
        if let Some((_, control)) = &self.active {
            let _ = control.cancel();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        sync::{Arc, Barrier, mpsc, mpsc::Receiver},
        time::{Duration, Instant},
    };

    #[derive(Default)]
    struct Host {
        requests: Requests,
        busy: bool,
        status: String,
        error: bool,
        opens: usize,
        reseeds: usize,
    }

    impl Host {
        fn begin(&mut self) -> JobId {
            let (id, _) = self.requests.begin().unwrap();
            self.busy = true;
            id
        }

        fn progress(&mut self, id: JobId, value: &str, closing: bool) {
            if self.requests.eligible_status(id, closing) {
                self.status = value.into();
            }
        }

        fn complete(&mut self, id: JobId, result: Result<Profile, ImportError>, closing: bool) {
            let Some(action) = self.requests.finish(id, result, closing) else {
                return;
            };
            self.busy = false;
            match action {
                Action::Open(_) => self.opens += 1,
                Action::Cancelled => {
                    self.status = "Import cancelled.".into();
                    self.error = false;
                }
                Action::Error(message) => {
                    self.status = message;
                    self.error = true;
                }
                Action::Retired => {}
            }
        }
    }

    fn profile() -> Profile {
        Profile {
            directory: PathBuf::from("synthetic"),
            manifest: serde_json::json!({"synthetic": true}),
        }
    }

    #[derive(Default)]
    struct WorkerHost {
        worker: Worker,
        busy: bool,
        picker_pending: bool,
        closing: bool,
        status: String,
        error: bool,
        opens: usize,
    }

    impl WorkerHost {
        fn start(
            &mut self,
            run: impl FnOnce(JobId, ImportControl) -> Result<Profile, ImportError> + Send + 'static,
        ) -> Result<JobId, String> {
            if !crate::ui::launcher::image_picker::accept_queued_source(
                self.busy,
                self.closing,
                self.picker_pending,
            ) {
                return Err("launcher action is ineligible".into());
            }
            let id = self.worker.start(run)?;
            self.busy = true;
            Ok(id)
        }

        fn progress(&mut self, id: JobId, message: &str) {
            if self.worker.eligible_status(id, self.closing) {
                self.status = message.into();
            }
        }

        fn poll(&mut self) -> Option<Action> {
            let action = self.worker.poll(self.closing)?;
            self.busy = false;
            match &action {
                Action::Open(_) => self.opens += 1,
                Action::Cancelled => {
                    self.status = "Import cancelled.".into();
                    self.error = false;
                }
                Action::Error(message) => {
                    self.status = message.clone();
                    self.error = true;
                }
                Action::Retired => {}
            }
            Some(action)
        }
    }

    fn wait_result(worker: &mut Worker, closing: bool) -> Action {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(action) = worker.poll(closing) {
                return action;
            }
            assert!(Instant::now() < deadline, "worker did not finish");
            thread::yield_now();
        }
    }

    fn wait_host(host: &mut WorkerHost) -> Action {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(action) = host.poll() {
                return action;
            }
            assert!(Instant::now() < deadline, "worker result was not delivered");
            thread::yield_now();
        }
    }

    #[test]
    #[ignore = "requires CLOVER_IMPORT_FAULT_IMAGE local image"]
    fn native_worker_real_image_cancellation() {
        let source = PathBuf::from(
            std::env::var_os("CLOVER_IMPORT_FAULT_IMAGE")
                .expect("CLOVER_IMPORT_FAULT_IMAGE must name a local image"),
        );
        assert!(source.is_file());
        let source_hash = crate::hash::file_digest(&source).unwrap();
        let filesystem = crate::profile::resolve_filesystem(&source).unwrap();
        let filesystem_hash = crate::hash::file_digest(&filesystem).unwrap();
        let root = temporary_root("real-cancel");
        let cache = root.join("cancel-cache");
        fs::create_dir_all(&cache).unwrap();
        let previous = fixture_previous_profile(&cache, &filesystem_hash);
        let previous_digest = tree_digest(&previous.directory);
        let destination = cache.join(format!(
            "{filesystem_hash}-v{}",
            crate::profile::IMPORTER_VERSION
        ));

        let (progress_tx, progress_rx) = mpsc::channel();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let mut host = WorkerHost::default();
        let id = start_real_import(
            &mut host,
            source.clone(),
            cache.clone(),
            progress_tx,
            Some((entered_tx, release_rx)),
        );
        let preparing = wait_until_preparing(
            &mut host,
            id,
            &progress_rx,
            &entered_rx,
            Duration::from_secs(300),
        );
        assert!(host.busy);
        assert_eq!(host.status, preparing);
        assert!(!host.error);
        assert_eq!(host.opens, 0);

        let mut activity = crate::frontend::launcher_activity::Activity::new(host.status.clone());
        activity.busy = host.busy;
        super::super::request_cancel(&host.worker, &mut activity);
        host.status = activity.status.clone();
        host.error = activity.error;
        assert_eq!(
            host.status,
            "Cancellation requested; waiting for a safe stopping point..."
        );
        assert!(!host.error);
        assert!(host.busy);
        assert_eq!(host.worker.cancellation_view(), CancellationView::Requested);
        assert!(!host.worker.eligible_status(id, false));
        activity.reduce(crate::frontend::launcher_activity::Event::Progress(
            "stale progress after cancellation request".into(),
        ));
        assert_eq!(
            activity.status,
            "Cancellation requested; waiting for a safe stopping point..."
        );
        assert!(host.worker.poll(false).is_none());
        assert!(host.busy);
        for _queued_source in [
            crate::ui::launcher::image_picker::Kind::Image,
            crate::ui::launcher::image_picker::Kind::PreparedFolder,
        ] {
            assert!(!crate::ui::launcher::image_picker::accept_queued_source(
                host.busy,
                host.closing,
                host.picker_pending,
            ));
            assert!(host.start(|_, _| Ok(profile())).is_err());
        }

        release_tx.send(()).unwrap();
        assert!(matches!(
            wait_host_timeout(&mut host, Duration::from_secs(300)),
            Action::Cancelled
        ));
        assert!(!host.busy);
        assert!(!host.error);
        assert_eq!(host.status, "Import cancelled.");
        assert_eq!(host.opens, 0);
        assert!(host.poll().is_none(), "cancelled result delivered twice");
        assert!(!destination.exists());
        assert_no_owned_stages(&cache);
        assert_previous_data(&previous, &previous_digest);
        assert_eq!(crate::hash::file_digest(&source).unwrap(), source_hash);

        // The same worker/request owner can start a new real import after the
        // cancelled job is acknowledged. This checks the successful path too.
        let (success_progress_tx, _success_progress_rx) = mpsc::channel();
        start_real_import(
            &mut host,
            source.clone(),
            cache.clone(),
            success_progress_tx,
            None,
        );
        let opened = wait_host_timeout(&mut host, Duration::from_secs(300));
        let imported = match opened {
            Action::Open(profile) => profile,
            other => panic!("expected one successful real import, got {other:?}"),
        };
        assert_eq!(host.opens, 1);
        assert!(!host.busy);
        assert!(!host.error);
        assert!(crate::profile::valid_profile(&imported));
        assert_eq!(imported.directory, destination);
        assert_eq!(
            fs::read(imported.directory.join("data/saves/worker-sentinel")).unwrap(),
            b"worker save sentinel"
        );
        let migrated_preferences: serde_json::Value = serde_json::from_slice(
            &fs::read(imported.directory.join("data/preferences.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(migrated_preferences["theme"], "classic");
        assert_eq!(migrated_preferences["fixture"], "worker");
        assert_eq!(crate::hash::file_digest(&source).unwrap(), source_hash);
        assert!(host.poll().is_none(), "successful result delivered twice");
        fs::remove_dir_all(&root).unwrap();

        let shutdown_root = temporary_root("real-shutdown");
        let shutdown_cache = shutdown_root.join("shutdown-cache");
        fs::create_dir_all(&shutdown_cache).unwrap();
        let shutdown_previous = fixture_previous_profile(&shutdown_cache, &filesystem_hash);
        let shutdown_previous_digest = tree_digest(&shutdown_previous.directory);
        let shutdown_destination = shutdown_cache.join(format!(
            "{filesystem_hash}-v{}",
            crate::profile::IMPORTER_VERSION
        ));
        let (shutdown_progress_tx, shutdown_progress_rx) = mpsc::channel();
        let (shutdown_entered_tx, shutdown_entered_rx) = mpsc::channel();
        let (shutdown_release_tx, shutdown_release_rx) = mpsc::channel();
        let mut shutdown_host = WorkerHost::default();
        let shutdown_id = start_real_import(
            &mut shutdown_host,
            source.clone(),
            shutdown_cache.clone(),
            shutdown_progress_tx,
            Some((shutdown_entered_tx, shutdown_release_rx)),
        );
        let shutdown_progress = wait_until_preparing(
            &mut shutdown_host,
            shutdown_id,
            &shutdown_progress_rx,
            &shutdown_entered_rx,
            Duration::from_secs(300),
        );
        assert_eq!(shutdown_host.status, shutdown_progress);
        assert!(shutdown_host.busy);
        shutdown_host.closing = true;
        shutdown_host.worker.shutdown();
        assert!(shutdown_host.worker.start(|_, _| Ok(profile())).is_err());
        assert!(!shutdown_host.worker.eligible_status(shutdown_id, false));
        assert!(shutdown_host.worker.poll(true).is_none());
        shutdown_release_tx.send(()).unwrap();
        assert!(matches!(
            wait_host_timeout(&mut shutdown_host, Duration::from_secs(300)),
            Action::Retired
        ));
        assert!(!shutdown_host.busy);
        assert!(!shutdown_host.error);
        assert_eq!(shutdown_host.opens, 0);
        assert!(
            shutdown_host.poll().is_none(),
            "retired result delivered twice"
        );
        assert!(!shutdown_destination.exists());
        assert_no_owned_stages(&shutdown_cache);
        assert_previous_data(&shutdown_previous, &shutdown_previous_digest);
        assert_eq!(crate::hash::file_digest(&source).unwrap(), source_hash);
        fs::remove_dir_all(&shutdown_root).unwrap();
    }

    fn wait_host_timeout(host: &mut WorkerHost, timeout: Duration) -> Action {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(action) = host.poll() {
                return action;
            }
            assert!(Instant::now() < deadline, "worker result was not delivered");
            thread::yield_now();
        }
    }

    fn temporary_root(label: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "clover-worker-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    fn fixture_previous_profile(cache: &std::path::Path, source_sha: &str) -> Profile {
        use serde_json::json;

        let directory = cache.join(format!("{source_sha}-v6"));
        fs::create_dir_all(directory.join("assets/skin")).unwrap();
        fs::create_dir_all(directory.join("data/saves")).unwrap();
        let catalog = directory.join("assets/catalog.json");
        let layout = directory.join("assets/skin/layout.json");
        let preferences = directory.join("data/preferences.json");
        let save = directory.join("data/saves/worker-sentinel");
        fs::write(&catalog, b"synthetic catalog fixture").unwrap();
        fs::write(&layout, b"synthetic skin fixture").unwrap();
        fs::write(&preferences, br#"{"theme":"classic","fixture":"worker"}"#).unwrap();
        fs::write(&save, b"worker save sentinel").unwrap();
        let profile = Profile {
            directory: directory.clone(),
            manifest: json!({
                "importerVersion": 6,
                "sourceSHA256": source_sha,
                "files": {
                    "assets/catalog.json": crate::hash::file_digest(&catalog).unwrap(),
                    "assets/skin/layout.json": crate::hash::file_digest(&layout).unwrap(),
                },
            }),
        };
        let mut manifest = serde_json::to_vec_pretty(&profile.manifest).unwrap();
        manifest.push(b'\n');
        fs::write(directory.join("profile.json"), manifest).unwrap();
        assert!(crate::profile::valid_profile(&profile));
        profile
    }

    fn tree_digest(directory: &std::path::Path) -> String {
        fn visit(
            root: &std::path::Path,
            current: &std::path::Path,
            entries: &mut Vec<(String, String)>,
        ) {
            for entry in fs::read_dir(current).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                if entry.file_type().unwrap().is_dir() {
                    visit(root, &path, entries);
                } else {
                    entries.push((
                        path.strip_prefix(root)
                            .unwrap()
                            .to_string_lossy()
                            .into_owned(),
                        crate::hash::file_digest(&path).unwrap(),
                    ));
                }
            }
        }
        let mut entries = Vec::new();
        visit(directory, directory, &mut entries);
        entries.sort();
        crate::hash::digest(&serde_json::to_vec(&entries).unwrap())
    }

    fn assert_no_owned_stages(cache: &std::path::Path) {
        assert!(fs::read_dir(cache).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".import-")
        }));
    }

    fn start_real_import(
        host: &mut WorkerHost,
        source: PathBuf,
        cache: PathBuf,
        progress: mpsc::Sender<(JobId, String)>,
        pause: Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>,
    ) -> JobId {
        host.start(move |id, control| {
            let mut callback = |message: &str| {
                let _ = progress.send((id, message.to_owned()));
                if message.starts_with("Preparing ")
                    && let Some((entered, release)) = &pause
                {
                    let _ = entered.send(());
                    release
                        .recv_timeout(Duration::from_secs(300))
                        .expect("test did not release paused import progress callback");
                }
            };
            crate::profile::import_image_cancellable(&source, &cache, Some(&mut callback), &control)
        })
        .unwrap()
    }

    fn wait_until_preparing(
        host: &mut WorkerHost,
        id: JobId,
        progress: &Receiver<(JobId, String)>,
        entered: &Receiver<()>,
        timeout: Duration,
    ) -> String {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let (message_id, message) = progress
                .recv_timeout(remaining)
                .expect("real import did not reach a preparing callback");
            host.progress(message_id, &message);
            if message.starts_with("Preparing ") {
                assert_eq!(message_id, id);
                entered
                    .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    .expect("worker did not pause in preparing callback");
                return message;
            }
        }
    }

    fn assert_previous_data(profile: &Profile, digest: &str) {
        assert!(crate::profile::valid_profile(profile));
        assert_eq!(tree_digest(&profile.directory), digest);
        assert_eq!(
            fs::read(profile.directory.join("data/preferences.json")).unwrap(),
            br#"{"theme":"classic","fixture":"worker"}"#
        );
        assert_eq!(
            fs::read(profile.directory.join("data/saves/worker-sentinel")).unwrap(),
            b"worker save sentinel"
        );
    }

    #[test]
    fn polls_result_once_and_retires_request() {
        let mut worker = Worker::default();
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let worker_entered = entered.clone();
        let worker_release = release.clone();
        let id = worker
            .start(move |_, _| {
                worker_entered.wait();
                worker_release.wait();
                Ok(profile())
            })
            .unwrap();
        assert!(worker.is_busy());
        entered.wait();
        assert!(worker.poll(false).is_none());
        assert!(worker.requests.eligible_status(id, false));
        release.wait();
        assert!(worker.is_busy());
        assert!(matches!(wait_result(&mut worker, false), Action::Open(_)));
        assert!(!worker.is_busy());
        assert!(worker.poll(false).is_none());
        assert!(worker.start(|_, _| Ok(profile())).is_ok());
        assert!(matches!(wait_result(&mut worker, false), Action::Open(_)));
    }

    #[test]
    fn result_can_be_polled_while_thread_tail_is_still_running() {
        let mut worker = Worker::default();
        let tail_entered = Arc::new(Barrier::new(2));
        let tail_release = Arc::new(Barrier::new(2));
        let worker_entered = tail_entered.clone();
        let worker_release = tail_release.clone();
        let (tail_done_tx, tail_done_rx) = mpsc::channel();
        worker
            .start_with_tail(
                |_, _| Ok(profile()),
                move || {
                    worker_entered.wait();
                    worker_release.wait();
                    let _ = tail_done_tx.send(());
                },
            )
            .unwrap();
        tail_entered.wait();
        assert!(matches!(worker.poll(false), Some(Action::Open(_))));
        assert!(worker.poll(false).is_none());
        tail_release.wait();
        tail_done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    }

    #[test]
    fn panic_becomes_scoped_error_and_worker_can_restart() {
        let mut worker = Worker::default();
        worker.start(|_, _| panic!("worker test panic")).unwrap();
        let action = wait_result(&mut worker, false);
        assert!(matches!(action, Action::Error(message) if message == "Import worker panicked"));
        assert!(worker.start(|_, _| Ok(profile())).is_ok());
        assert!(matches!(wait_result(&mut worker, false), Action::Open(_)));
    }

    #[test]
    fn shutdown_cancels_and_retires_after_worker_returns() {
        let mut worker = Worker::default();
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let worker_entered = entered.clone();
        let worker_release = release.clone();
        let id = worker
            .start(move |_, _| {
                worker_entered.wait();
                worker_release.wait();
                Ok(profile())
            })
            .unwrap();
        entered.wait();
        worker.shutdown();
        assert!(worker.is_busy());
        assert!(matches!(
            worker
                .requests
                .active
                .as_ref()
                .map(|(_, control)| control.state()),
            Some(crate::frontend::import_lifecycle::State::CancelRequested)
        ));
        assert!(worker.start(|_, _| Ok(profile())).is_err());
        assert!(!worker.eligible_status(id, false));
        assert!(worker.poll(true).is_none());
        release.wait();
        assert!(matches!(wait_result(&mut worker, true), Action::Retired));
        assert!(!worker.is_busy());
        assert!(worker.poll(true).is_none());
    }

    #[test]
    fn remains_busy_after_control_is_cancelled_or_completed_until_result_is_polled() {
        let mut worker = Worker::default();
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let worker_entered = entered.clone();
        let worker_release = release.clone();
        worker
            .start(move |_, control| {
                control.start().unwrap();
                worker_entered.wait();
                control.begin_commit().unwrap();
                assert!(control.finish_success());
                worker_release.wait();
                Ok(profile())
            })
            .unwrap();
        entered.wait();
        while worker.cancellation_view() != CancellationView::Finishing {
            std::thread::yield_now();
        }
        assert!(worker.is_busy());
        assert!(worker.poll(false).is_none());
        release.wait();
        assert!(matches!(wait_result(&mut worker, false), Action::Open(_)));
        assert!(!worker.is_busy());

        let mut worker = Worker::default();
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let worker_entered = entered.clone();
        let worker_release = release.clone();
        worker
            .start(move |_, control| {
                control.start().unwrap();
                worker_entered.wait();
                worker_release.wait();
                control
                    .checkpoint()
                    .map_err(crate::import_job::checkpoint_error)?;
                Ok(profile())
            })
            .unwrap();
        entered.wait();
        assert_eq!(worker.cancel(), Some(CancelResult::Requested));
        assert!(worker.is_busy());
        assert!(worker.poll(false).is_none());
        release.wait();
        assert!(matches!(wait_result(&mut worker, false), Action::Cancelled));
        assert!(!worker.is_busy());
    }

    #[test]
    fn user_cancel_keeps_job_active_and_suppresses_late_progress_until_acknowledged() {
        let mut host = WorkerHost::default();
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let worker_entered = entered.clone();
        let worker_release = release.clone();
        let id = host
            .start(move |_, control| {
                control.start().unwrap();
                worker_entered.wait();
                worker_release.wait();
                control
                    .checkpoint()
                    .map_err(crate::import_job::checkpoint_error)?;
                Ok(profile())
            })
            .unwrap();
        entered.wait();
        assert_eq!(host.worker.cancellation_view(), CancellationView::Available);
        assert_eq!(host.worker.cancel(), Some(CancelResult::Requested));
        assert_eq!(host.worker.cancel(), Some(CancelResult::AlreadyRequested));
        assert_eq!(host.worker.cancellation_view(), CancellationView::Requested);
        assert!(host.busy);
        assert!(!crate::ui::launcher::image_picker::accept_queued_source(
            host.busy,
            host.closing,
            host.picker_pending
        ));
        assert!(!host.error);
        assert!(
            host.worker
                .requests
                .active
                .as_ref()
                .is_some_and(|(active, _)| *active == id)
        );
        host.status = "Cancellation requested; waiting for a safe stopping point...".into();
        host.progress(id, "late progress");
        assert_eq!(
            host.status,
            "Cancellation requested; waiting for a safe stopping point..."
        );
        assert!(host.busy);
        assert!(host.worker.poll(false).is_none());
        assert!(host.busy);
        release.wait();
        assert!(matches!(wait_host(&mut host), Action::Cancelled));
        assert!(!host.busy);
        assert_eq!(host.opens, 0);
        assert!(!host.error);
        assert_eq!(host.status, "Import cancelled.");
        assert!(host.worker.start(|_, _| Ok(profile())).is_ok());
        assert!(matches!(wait_host(&mut host), Action::Open(_)));
    }

    #[test]
    fn cancellation_after_commit_is_too_late_and_success_opens_once() {
        let mut host = WorkerHost::default();
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let worker_entered = entered.clone();
        let worker_release = release.clone();
        host.start(move |_, control| {
            control.start().unwrap();
            control.begin_commit().unwrap();
            worker_entered.wait();
            worker_release.wait();
            assert!(control.finish_success());
            Ok(profile())
        })
        .unwrap();
        entered.wait();
        assert_eq!(host.worker.cancellation_view(), CancellationView::Finishing);
        assert_eq!(host.worker.cancel(), Some(CancelResult::TooLate));
        assert!(host.busy);
        assert_eq!(host.opens, 0);
        assert!(!host.error);
        release.wait();
        assert!(matches!(wait_host(&mut host), Action::Open(_)));
        assert_eq!(host.opens, 1);
        assert!(host.poll().is_none());
        assert_eq!(host.opens, 1);
    }

    #[test]
    fn inactive_cancel_is_unavailable_and_shutdown_retires() {
        let mut worker = Worker::default();
        assert_eq!(worker.cancel(), None);
        assert_eq!(worker.cancellation_view(), CancellationView::Unavailable);
        let id = worker.start(|_, _| Ok(profile())).unwrap();
        worker.shutdown();
        assert_eq!(worker.cancel(), None);
        assert!(matches!(wait_result(&mut worker, true), Action::Retired));
        assert!(!worker.requests.eligible_status(id, false));
    }

    #[test]
    fn duplicate_start_preserves_the_existing_worker() {
        let mut worker = Worker::default();
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let worker_entered = entered.clone();
        let worker_release = release.clone();
        let current = worker
            .start(move |_, _| {
                worker_entered.wait();
                worker_release.wait();
                Ok(profile())
            })
            .unwrap();
        entered.wait();
        assert!(worker.start(|_, _| Ok(profile())).is_err());
        assert!(worker.requests.eligible_status(current, false));
        release.wait();
        assert!(matches!(wait_result(&mut worker, false), Action::Open(_)));
    }

    #[test]
    fn stale_progress_and_completions_cannot_mutate_the_current_job() {
        let mut host = Host::default();
        let old = host.begin();
        assert!(
            host.requests
                .finish(old, Err(ImportError::Cancelled), false)
                .is_some()
        );
        host.busy = true;
        let current = host.begin();
        assert_ne!(old, current);
        let before = (
            host.status.clone(),
            host.error,
            host.busy,
            host.opens,
            host.reseeds,
        );
        host.progress(old, "stale progress", false);
        host.complete(old, Err(ImportError::Failed("stale".into())), false);
        host.complete(old, Ok(profile()), false);
        assert_eq!(
            (
                host.status.clone(),
                host.error,
                host.busy,
                host.opens,
                host.reseeds
            ),
            before
        );
        host.complete(current, Ok(profile()), false);
        assert_eq!(host.opens, 1);
        assert!(!host.busy);
        host.complete(current, Ok(profile()), false);
        assert_eq!(host.opens, 1);
    }

    #[test]
    fn cancellation_and_shutdown_retire_without_error_or_open() {
        let mut host = Host::default();
        let (id, _) = host.requests.begin().unwrap();
        host.busy = true;
        host.complete(id, Err(ImportError::Cancelled), false);
        assert_eq!(host.status, "Import cancelled.");
        assert!(!host.error);
        assert!(!host.busy);
        let (id, control) = host.requests.begin().unwrap();
        host.busy = true;
        host.requests.shutdown();
        assert!(host.requests.begin().is_err());
        assert!(!host.requests.eligible_status(id, false));
        assert!(matches!(
            control.state(),
            crate::frontend::import_lifecycle::State::CancelRequested
        ));
        host.complete(id, Ok(profile()), true);
        assert!(!host.busy);
        assert_eq!(host.opens, 0);
        assert_eq!(host.status, "Import cancelled.");
    }

    #[test]
    fn worker_and_picker_gates_reject_queued_images_and_assets() {
        let mut host = WorkerHost::default();
        for kind in [
            crate::ui::launcher::image_picker::Kind::Image,
            crate::ui::launcher::image_picker::Kind::PreparedFolder,
        ] {
            let mut picker = crate::ui::launcher::image_picker::Requests::default();
            picker.begin(kind, Capability::Available).unwrap();
            host.picker_pending = true;
            assert!(host.start(|_, _| Ok(profile())).is_err());
            host.picker_pending = false;
        }

        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let worker_entered = entered.clone();
        let worker_release = release.clone();
        host.start(move |_, _| {
            worker_entered.wait();
            worker_release.wait();
            Ok(profile())
        })
        .unwrap();
        entered.wait();
        let id = host
            .worker
            .requests
            .active
            .as_ref()
            .expect("active worker")
            .0;
        host.progress(id, "initial progress");
        let previous_focus = Some(1usize);
        let focus: Option<usize> = None;
        let controller_connected = true;
        host.progress(
            id,
            if previous_focus.is_some() && focus.is_none() && controller_connected {
                "after input reseed"
            } else {
                "unexpected"
            },
        );
        assert_eq!(host.status, "after input reseed");
        assert!(host.worker.eligible_status(id, false));
        for is_image in [true, false] {
            let accepted = crate::ui::launcher::image_picker::accept_queued_source(
                host.busy,
                host.closing,
                host.picker_pending,
            );
            assert!(!accepted, "queued source accepted: image={is_image}");
            assert!(host.start(|_, _| Ok(profile())).is_err());
        }
        release.wait();
        assert!(matches!(wait_host(&mut host), Action::Open(_)));

        host.picker_pending = true;
        assert!(host.start(|_, _| Ok(profile())).is_err());
        host.picker_pending = false;
        host.start(|_, _| Ok(profile())).unwrap();
        assert!(matches!(wait_host(&mut host), Action::Open(_)));
    }

    #[test]
    fn delayed_old_progress_cannot_replace_new_job_status() {
        let mut host = WorkerHost::default();
        let old = host
            .start(|_, _| Err(ImportError::Failed("old failure".into())))
            .unwrap();
        assert!(matches!(wait_host(&mut host), Action::Error(_)));
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let worker_entered = entered.clone();
        let worker_release = release.clone();
        host.start(move |_, _| {
            worker_entered.wait();
            worker_release.wait();
            Ok(profile())
        })
        .unwrap();
        entered.wait();
        host.status = "current progress".into();
        host.error = false;
        let before = (host.status.clone(), host.error, host.busy, host.opens);
        host.progress(old, "delayed stale progress");
        assert_eq!(
            (host.status.clone(), host.error, host.busy, host.opens),
            before
        );
        release.wait();
        assert!(matches!(wait_host(&mut host), Action::Open(_)));
    }

    #[test]
    fn shutdown_suppresses_same_frame_sources_cached_launch_and_picker_result() {
        let mut host = WorkerHost::default();
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let worker_entered = entered.clone();
        let worker_release = release.clone();
        host.start(move |_, _| {
            worker_entered.wait();
            worker_release.wait();
            Ok(profile())
        })
        .unwrap();
        entered.wait();
        host.closing = true;
        host.worker.shutdown();
        for picker_pending in [false, true] {
            host.picker_pending = picker_pending;
            assert!(!crate::ui::launcher::image_picker::accept_queued_source(
                host.busy,
                host.closing,
                host.picker_pending,
            ));
            for action in [Some(0), Some(1), Some(2)] {
                assert_eq!(
                    super::super::eligible_action(action, host.busy, host.closing, picker_pending),
                    None
                );
            }
        }
        assert!(host.start(|_, _| Ok(profile())).is_err());

        let mut picker = crate::ui::launcher::image_picker::Requests::default();
        let picker_id = picker
            .begin(
                crate::ui::launcher::image_picker::Kind::Image,
                Capability::Available,
            )
            .unwrap();
        assert!(matches!(
            crate::ui::launcher::image_picker::finish(
                &mut picker,
                picker_id,
                crate::ui::launcher::image_picker::Kind::Image,
                Outcome::Selected("late.hsqs".into()),
                host.busy,
                host.closing,
            ),
            Some(crate::ui::launcher::image_picker::Action::Done)
        ));
        let opens_before = host.opens;
        release.wait();
        assert!(matches!(wait_host(&mut host), Action::Retired));
        assert!(!host.busy);
        assert_eq!(host.opens, opens_before);
        assert!(host.poll().is_none());
        assert_eq!(host.opens, opens_before);
    }

    #[test]
    fn dropping_live_worker_cancels_and_detaches_without_waiting() {
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let worker_entered = entered.clone();
        let worker_release = release.clone();
        let (sender, receiver) = mpsc::channel();
        let mut worker = Worker::default();
        worker
            .start(move |_, control| {
                worker_entered.wait();
                worker_release.wait();
                let state = control.state();
                let _ = sender.send(state);
                Ok(profile())
            })
            .unwrap();
        entered.wait();
        let control = worker
            .requests
            .active
            .as_ref()
            .expect("active import control")
            .1
            .clone();
        drop(worker);
        assert_eq!(
            control.state(),
            crate::frontend::import_lifecycle::State::CancelRequested
        );
        release.wait();
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(2)).unwrap(),
            crate::frontend::import_lifecycle::State::CancelRequested
        );
    }
}
