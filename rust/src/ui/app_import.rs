use crate::{
    frontend::import_lifecycle::CancelResult,
    import_job::{ImportControl, ImportError},
    profile::Profile,
    ui::launcher::import_worker::{Action, CancellationView, JobId, Worker},
};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
};

pub(super) const IMPORT_NOTICE: &str = "Importing image... You may continue browsing.";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Target {
    pub profile: PathBuf,
    pub game: String,
}

#[derive(Debug)]
pub(super) enum Poll {
    Switch(Profile),
    CachedOnly,
    Cancelled,
    Error(String),
    Silent,
    RetiredSilent,
}

#[derive(Debug, Default)]
pub(super) struct Update {
    pub progress: Option<String>,
    pub completion: Option<Poll>,
}

#[derive(Default)]
pub(super) struct AppImport {
    worker: Worker,
    target: Option<Target>,
    job_id: Option<JobId>,
    progress_sender: Option<Sender<(JobId, String)>>,
    progress_receiver: Option<Receiver<(JobId, String)>>,
    notice_text: String,
    suppress_progress: bool,
}

impl AppImport {
    pub(super) fn is_busy(&self) -> bool {
        self.worker.is_busy()
    }

    pub(super) fn start(
        &mut self,
        target: Target,
        image: PathBuf,
        cache: PathBuf,
    ) -> Result<JobId, String> {
        self.start_with(target, move |id, control, sender| {
            let mut progress = |message: &str| {
                let _ = sender.send((id, message.to_owned()));
            };
            crate::profile::import_image_cancellable(&image, &cache, Some(&mut progress), &control)
        })
    }

    fn start_with(
        &mut self,
        target: Target,
        run: impl FnOnce(JobId, ImportControl, Sender<(JobId, String)>) -> Result<Profile, ImportError>
        + Send
        + 'static,
    ) -> Result<JobId, String> {
        if self.is_busy() {
            return Err("An image import is already running".into());
        }
        let (sender, receiver) = mpsc::channel();
        let worker_sender = sender.clone();
        let worker = self
            .worker
            .start(move |id, control| run(id, control, worker_sender))?;
        self.target = Some(target);
        self.job_id = Some(worker);
        self.progress_sender = Some(sender);
        self.progress_receiver = Some(receiver);
        self.notice_text = IMPORT_NOTICE.into();
        self.suppress_progress = false;
        Ok(worker)
    }

    /// Invalidates activation without closing the worker's reusable request pool.
    pub(super) fn invalidate(&mut self) -> Option<CancelResult> {
        self.target = None;
        self.progress_sender = None;
        self.worker.cancel()
    }

    pub(super) fn release_notice(&mut self) {
        self.target = None;
        self.progress_sender = None;
    }

    pub(super) fn cancel_notice(
        &mut self,
        current: &Target,
        notice_owned: bool,
        playing: bool,
    ) -> Option<CancelResult> {
        if !notice_owned || playing || self.target.as_ref() != Some(current) {
            return None;
        }
        let result = self.worker.cancel()?;
        match result {
            CancelResult::Requested => {
                self.notice_text =
                    "Cancellation requested; waiting for a safe stopping point...".into();
                self.suppress_progress = true;
            }
            CancelResult::TooLate => {
                self.notice_text = "Finishing import; cancellation is too late.".into();
                self.suppress_progress = true;
            }
            CancelResult::AlreadyRequested | CancelResult::Finished => {}
        }
        Some(result)
    }

    pub(super) fn cancellation_view(
        &self,
        current: &Target,
        notice_owned: bool,
        playing: bool,
    ) -> CancellationView {
        if !notice_owned || playing || self.target.as_ref() != Some(current) {
            CancellationView::Unavailable
        } else {
            self.worker.cancellation_view()
        }
    }

    pub(super) fn shutdown(&mut self) {
        self.target = None;
        self.worker.shutdown();
    }

    pub(super) fn poll(
        &mut self,
        current: &Target,
        notice_owned: bool,
        playing: bool,
        closing: bool,
    ) -> Option<Poll> {
        let action = self.worker.poll(closing)?;
        let eligible = self.target.as_ref().is_some_and(|target| target == current)
            && notice_owned
            && !playing
            && !closing;
        self.target = None;
        self.job_id = None;
        self.progress_sender = None;
        self.progress_receiver = None;
        Some(match action {
            Action::Open(profile) if eligible => Poll::Switch(profile),
            Action::Open(_) => Poll::CachedOnly,
            Action::Cancelled if eligible => Poll::Cancelled,
            Action::Error(error) if eligible => Poll::Error(error),
            Action::Retired => Poll::RetiredSilent,
            Action::Cancelled | Action::Error(_) => Poll::Silent,
        })
    }

    pub(super) fn notice_text(&self) -> &str {
        &self.notice_text
    }

    pub(super) fn update(
        &mut self,
        current: &Target,
        notice_owned: bool,
        playing: bool,
        closing: bool,
    ) -> Update {
        let mut update = Update::default();
        let eligible =
            self.target.as_ref() == Some(current) && notice_owned && !playing && !closing;
        if !notice_owned || self.target.as_ref() != Some(current) || playing {
            self.release_notice();
        }
        if let (Some(id), Some(receiver)) = (self.job_id, self.progress_receiver.as_ref()) {
            for _ in 0..64 {
                match receiver.try_recv() {
                    Ok((message_id, message))
                        if eligible
                            && message_id == id
                            && !self.suppress_progress
                            && self.worker.eligible_status(message_id, closing) =>
                    {
                        self.notice_text = message.clone();
                        update.progress = Some(message);
                    }
                    Ok(_) => {}
                    Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
                }
            }
        }
        update.completion = self.poll(current, notice_owned, playing, closing);
        update
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{Arc, Barrier},
        time::{Duration, Instant},
    };

    fn target(profile: &str, game: &str) -> Target {
        Target {
            profile: profile.into(),
            game: game.into(),
        }
    }

    #[test]
    #[ignore = "requires CLOVER_IMPORT_FAULT_IMAGE local image"]
    fn app_import_real_image_notice_lifecycle() {
        let source = PathBuf::from(
            std::env::var_os("CLOVER_IMPORT_FAULT_IMAGE")
                .expect("CLOVER_IMPORT_FAULT_IMAGE must name a local image"),
        );
        assert!(source.is_file());
        for scenario in ["switch", "browse", "cancel"] {
            run_real_image_notice_case(&source, scenario);
        }
    }

    fn run_real_image_notice_case(source: &std::path::Path, scenario: &str) {
        let root = std::env::temp_dir().join(format!(
            "clover-app-import-{scenario}-{}-{}",
            std::process::id(),
            crate::ui::chrono_now()
        ));
        let cache = root.join("import-cache");
        let current_data = root.join("current-profile-data");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::create_dir_all(&current_data).unwrap();
        let marker = current_data.join("keep.bin");
        std::fs::write(&marker, b"current profile data must remain untouched").unwrap();
        let source_before = crate::hash::file_digest(source).unwrap();
        let marker_before = crate::hash::file_digest(&marker).unwrap();
        let current = Target {
            profile: current_data.clone(),
            game: "game".into(),
        };
        let (preparing_tx, preparing_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let mut controller = AppImport::default();
        let image = source.to_path_buf();
        let worker_cache = cache.clone();
        controller
            .start_with(current.clone(), move |id, control, sender| {
                let mut held = false;
                let mut report = |text: &str| {
                    let _ = sender.send((id, text.to_owned()));
                    if !held && text.starts_with("Preparing ") {
                        held = true;
                        let _ = preparing_tx.send(text.to_owned());
                        let _ = release_rx.recv_timeout(Duration::from_secs(300));
                    }
                };
                crate::profile::import_image_cancellable(
                    &image,
                    &worker_cache,
                    Some(&mut report),
                    &control,
                )
            })
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(300);
        let phase = preparing_rx.recv_timeout(Duration::from_secs(300)).ok();
        let progress_update = phase
            .as_ref()
            .map(|_| controller.update(&current, true, false, false));
        let cancel = if scenario == "cancel" {
            controller.cancel_notice(&current, true, false)
        } else {
            None
        };
        let feedback = controller.notice_text().to_owned();
        if scenario == "browse" {
            controller.release_notice();
        }
        let _ = release_tx.send(());

        let completion = if phase.is_some() {
            loop {
                let update = controller.update(&current, scenario != "browse", false, false);
                if let Some(done) = update.completion {
                    break Some(done);
                }
                if Instant::now() >= deadline {
                    break None;
                }
                std::thread::yield_now();
            }
        } else {
            None
        };
        let source_after = crate::hash::file_digest(source).unwrap();
        let marker_after = crate::hash::file_digest(&marker).unwrap();
        let published = crate::profile::list_profiles(&cache);
        let switch_result = matches!(completion, Some(Poll::Switch(_)));
        let cached_result = matches!(completion, Some(Poll::CachedOnly));
        let cancel_result = matches!(completion, Some(Poll::Cancelled));
        let progress_was_owned = progress_update.is_some_and(|update| {
            update
                .progress
                .as_deref()
                .is_some_and(|text| text.starts_with("Preparing "))
        });
        let fresh_profile_valid = published.iter().any(|profile| profile.directory.is_dir());
        let expected_feedback =
            feedback == "Cancellation requested; waiting for a safe stopping point...";
        let _ = std::fs::remove_dir_all(&root);

        assert!(
            phase.is_some(),
            "real importer never reached a Preparing boundary"
        );
        assert!(
            progress_was_owned,
            "Preparing progress was not delivered to the owned notice"
        );
        assert_eq!(
            source_after, source_before,
            "source image changed during import"
        );
        assert_eq!(
            marker_after, marker_before,
            "current profile data changed during import"
        );
        match scenario {
            "switch" => {
                assert!(
                    switch_result,
                    "current owned import did not switch to the imported profile"
                );
                assert!(
                    fresh_profile_valid,
                    "successful import did not publish a valid profile"
                );
                assert!(
                    controller.poll(&current, true, false, false).is_none(),
                    "completion switched more than once"
                );
            }
            "browse" => {
                assert!(
                    cached_result,
                    "dismissed notice did not complete as cached-only"
                );
                assert!(
                    fresh_profile_valid,
                    "browse-only import did not leave the valid profile cache"
                );
            }
            "cancel" => {
                assert_eq!(cancel, Some(CancelResult::Requested));
                assert!(expected_feedback, "cancel feedback did not own the notice");
                assert!(
                    cancel_result,
                    "requested import cancellation was not acknowledged"
                );
                assert!(published.is_empty(), "cancelled import published a profile");
            }
            _ => unreachable!(),
        }
    }

    fn profile() -> Profile {
        Profile {
            directory: PathBuf::from("synthetic"),
            manifest: serde_json::json!({"synthetic": true}),
        }
    }

    fn wait(controller: &mut AppImport, current: &Target, notice: bool, playing: bool) -> Poll {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(result) = controller.poll(current, notice, playing, false) {
                return result;
            }
            assert!(Instant::now() < deadline, "import worker did not return");
            std::thread::yield_now();
        }
    }

    fn start_barrier_job(
        controller: &mut AppImport,
        ticket: Target,
        entered: Arc<Barrier>,
        release: Arc<Barrier>,
        result: Result<Profile, ImportError>,
    ) {
        let worker_entered = entered.clone();
        let worker_release = release.clone();
        controller
            .start_with(ticket, move |_, _, _| {
                worker_entered.wait();
                worker_release.wait();
                result
            })
            .unwrap();
    }

    #[test]
    fn unowned_notice_cannot_reacquire_the_activation_ticket() {
        let mut controller = AppImport::default();
        let current = target("profile", "game");
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        start_barrier_job(
            &mut controller,
            current.clone(),
            entered.clone(),
            release.clone(),
            Ok(profile()),
        );
        entered.wait();
        let update = controller.update(&current, false, false, false);
        release.wait();
        assert!(update.completion.is_none());
        assert!(controller.target.is_none());
        assert!(matches!(
            wait(&mut controller, &current, true, false),
            Poll::CachedOnly
        ));
    }

    #[test]
    fn progress_is_correlated_owned_and_drained_at_most_64_per_update() {
        let mut controller = AppImport::default();
        let current = target("profile", "game");
        let (entered, release) = (Arc::new(Barrier::new(2)), Arc::new(Barrier::new(2)));
        let worker_entered = entered.clone();
        let worker_release = release.clone();
        controller
            .start_with(current.clone(), move |id, _, sender| {
                for n in 0..100 {
                    let _ = sender.send((id, format!("step {n}")));
                }
                worker_entered.wait();
                worker_release.wait();
                Ok(profile())
            })
            .unwrap();
        entered.wait();
        let first = controller.update(&current, true, false, false);
        assert_eq!(first.progress.as_deref(), Some("step 63"));
        assert!(first.completion.is_none());
        let second = controller.update(&current, true, false, false);
        assert_eq!(second.progress.as_deref(), Some("step 99"));
        release.wait();
        assert!(matches!(
            wait(&mut controller, &current, true, false),
            Poll::Switch(_)
        ));
        assert_eq!(controller.job_id, None);
    }

    #[test]
    fn progress_requires_current_target_and_worker_eligibility() {
        let mut controller = AppImport::default();
        let current = target("profile", "game");
        let (entered, release) = (Arc::new(Barrier::new(2)), Arc::new(Barrier::new(2)));
        let worker_entered = entered.clone();
        let worker_release = release.clone();
        controller
            .start_with(current.clone(), move |id, _, sender| {
                let _ = sender.send((id, "hidden".into()));
                worker_entered.wait();
                worker_release.wait();
                Ok(profile())
            })
            .unwrap();
        entered.wait();
        assert!(
            controller
                .update(&target("other-profile", "game"), true, false, false)
                .progress
                .is_none()
        );
        controller.invalidate();
        assert!(
            controller
                .update(&current, true, false, false)
                .progress
                .is_none()
        );
        release.wait();
        assert!(matches!(
            wait(&mut controller, &current, true, false),
            Poll::CachedOnly
        ));
    }

    #[test]
    fn progress_and_completion_in_one_update_preserves_owned_switch() {
        let mut controller = AppImport::default();
        let current = target("profile", "game");
        let (entered, release) = (Arc::new(Barrier::new(2)), Arc::new(Barrier::new(2)));
        let worker_entered = entered.clone();
        let worker_release = release.clone();
        controller
            .start_with(current.clone(), move |id, _, sender| {
                let _ = sender.send((id, "final phase".into()));
                worker_entered.wait();
                worker_release.wait();
                Ok(profile())
            })
            .unwrap();
        entered.wait();
        release.wait();
        std::thread::sleep(Duration::from_millis(1));
        let update = controller.update(&current, true, false, false);
        assert_eq!(update.progress.as_deref(), Some("final phase"));
        assert!(matches!(update.completion, Some(Poll::Switch(_))));
    }

    #[test]
    fn playing_suppresses_progress_and_profile_switch() {
        let mut controller = AppImport::default();
        let current = target("profile", "game");
        controller
            .start_with(current.clone(), move |id, _, sender| {
                let _ = sender.send((id, "must stay hidden".into()));
                Ok(profile())
            })
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let update = controller.update(&current, true, true, false);
            assert!(update.progress.is_none());
            if let Some(completion) = update.completion {
                assert!(matches!(completion, Poll::CachedOnly));
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
    }

    #[test]
    fn cancellation_feedback_keeps_job_and_too_late_success_eligible() {
        let current = target("profile", "game");
        let mut controller = AppImport::default();
        let (entered, release) = (Arc::new(Barrier::new(2)), Arc::new(Barrier::new(2)));
        let worker_entered = entered.clone();
        let worker_release = release.clone();
        controller
            .start_with(current.clone(), move |id, control, sender| {
                control.start().unwrap();
                worker_entered.wait();
                let _ = sender.send((id, "late progress".into()));
                worker_release.wait();
                control
                    .checkpoint()
                    .map_err(crate::import_job::checkpoint_error)?;
                Ok(profile())
            })
            .unwrap();
        entered.wait();
        assert_eq!(
            controller.cancel_notice(&current, true, false),
            Some(CancelResult::Requested)
        );
        assert_eq!(
            controller.notice_text(),
            "Cancellation requested; waiting for a safe stopping point..."
        );
        assert_eq!(
            controller.cancel_notice(&current, true, false),
            Some(CancelResult::AlreadyRequested)
        );
        assert!(controller.is_busy());
        release.wait();
        assert!(matches!(
            wait(&mut controller, &current, true, false),
            Poll::Cancelled
        ));

        let mut controller = AppImport::default();
        let (entered, release) = (Arc::new(Barrier::new(2)), Arc::new(Barrier::new(2)));
        let worker_entered = entered.clone();
        let worker_release = release.clone();
        controller
            .start_with(current.clone(), move |_, control, _| {
                control.start().unwrap();
                control.begin_commit().unwrap();
                // Readiness must mean the commit boundary has been crossed;
                // otherwise the caller can still legitimately cancel first.
                worker_entered.wait();
                worker_release.wait();
                control.finish_success();
                Ok(profile())
            })
            .unwrap();
        entered.wait();
        assert_eq!(
            controller.cancel_notice(&current, true, false),
            Some(CancelResult::TooLate)
        );
        assert_eq!(
            controller.notice_text(),
            "Finishing import; cancellation is too late."
        );
        release.wait();
        assert!(matches!(
            wait(&mut controller, &current, true, false),
            Poll::Switch(_)
        ));
    }

    #[test]
    fn dismissing_notice_releases_activation_but_import_completes_cached_only() {
        let mut controller = AppImport::default();
        let current = target("profile", "game");
        let (entered, release) = (Arc::new(Barrier::new(2)), Arc::new(Barrier::new(2)));
        start_barrier_job(
            &mut controller,
            current.clone(),
            entered.clone(),
            release.clone(),
            Ok(profile()),
        );
        entered.wait();
        controller.release_notice();
        assert_eq!(controller.notice_text(), IMPORT_NOTICE);
        assert_eq!(controller.cancel_notice(&current, true, false), None);
        release.wait();
        assert!(matches!(
            wait(&mut controller, &current, true, false),
            Poll::CachedOnly
        ));
    }

    #[test]
    fn successful_current_notice_switches_once() {
        let mut controller = AppImport::default();
        let current = target("profile", "game");
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        start_barrier_job(
            &mut controller,
            current.clone(),
            entered.clone(),
            release.clone(),
            Ok(profile()),
        );
        entered.wait();
        assert!(controller.poll(&current, true, false, false).is_none());
        release.wait();
        assert!(matches!(
            wait(&mut controller, &current, true, false),
            Poll::Switch(_)
        ));
        assert!(controller.poll(&current, true, false, false).is_none());
    }

    #[test]
    fn stale_or_unowned_success_is_cached_only() {
        for (current, notice, playing, invalidate) in [
            (target("other-profile", "game"), true, false, false),
            (target("profile", "other-game"), true, false, false),
            (target("profile", "game"), true, true, false),
            (target("profile", "game"), false, false, false),
            (target("profile", "game"), true, false, true),
        ] {
            let mut controller = AppImport::default();
            let ticket = target("profile", "game");
            let entered = Arc::new(Barrier::new(2));
            let release = Arc::new(Barrier::new(2));
            start_barrier_job(
                &mut controller,
                ticket,
                entered.clone(),
                release.clone(),
                Ok(profile()),
            );
            entered.wait();
            if invalidate {
                assert_eq!(controller.invalidate(), Some(CancelResult::Requested));
            }
            release.wait();
            assert!(matches!(
                wait(&mut controller, &current, notice, playing),
                Poll::CachedOnly
            ));
        }
    }

    #[test]
    fn invalidation_requests_cancellation_and_keeps_worker_busy_until_retired() {
        let mut controller = AppImport::default();
        let current = target("profile", "game");
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let worker_entered = entered.clone();
        let worker_release = release.clone();
        controller
            .start_with(current.clone(), move |_, control, _| {
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
        assert_eq!(controller.invalidate(), Some(CancelResult::Requested));
        assert!(controller.is_busy());
        assert_eq!(
            controller.start(current.clone(), PathBuf::new(), PathBuf::new()),
            Err("An image import is already running".into())
        );
        release.wait();
        assert!(matches!(
            wait(&mut controller, &current, true, false),
            Poll::Silent
        ));
        assert!(!controller.is_busy());
    }

    #[test]
    fn cancellation_and_error_are_notice_scoped_and_panic_is_an_error() {
        let current = target("profile", "game");
        for (result, notice, expected) in [
            (Err(ImportError::Cancelled), true, "cancelled"),
            (Err(ImportError::Cancelled), false, "silent"),
            (
                Err(ImportError::Failed("worker failed".into())),
                false,
                "silent",
            ),
            (
                Err(ImportError::Failed("worker failed".into())),
                true,
                "error",
            ),
        ] {
            let mut controller = AppImport::default();
            let entered = Arc::new(Barrier::new(2));
            let release = Arc::new(Barrier::new(2));
            start_barrier_job(
                &mut controller,
                current.clone(),
                entered.clone(),
                release.clone(),
                result,
            );
            entered.wait();
            release.wait();
            let poll = wait(&mut controller, &current, notice, false);
            match expected {
                "cancelled" => assert!(matches!(poll, Poll::Cancelled)),
                "error" => {
                    assert!(matches!(poll, Poll::Error(message) if message == "worker failed"))
                }
                _ => assert!(matches!(poll, Poll::Silent)),
            }
        }

        let mut controller = AppImport::default();
        controller
            .start_with(current.clone(), |_, _, _| panic!("controller test panic"))
            .unwrap();
        assert!(matches!(
            wait(&mut controller, &current, true, false),
            Poll::Error(message) if message == "Import worker panicked"
        ));

        let mut controller = AppImport::default();
        controller
            .start_with(current.clone(), |_, _, _| panic!("stale controller panic"))
            .unwrap();
        controller.invalidate();
        assert!(matches!(
            wait(&mut controller, &current, true, false),
            Poll::Silent
        ));
    }

    #[test]
    fn closing_retires_silently_and_old_ticket_cannot_clear_a_new_job() {
        let mut controller = AppImport::default();
        let current = target("profile", "game");
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        start_barrier_job(
            &mut controller,
            current.clone(),
            entered.clone(),
            release.clone(),
            Ok(profile()),
        );
        entered.wait();
        controller.shutdown();
        release.wait();
        let deadline = Instant::now() + Duration::from_secs(2);
        let result = loop {
            if let Some(result) = controller.poll(&current, true, false, true) {
                break result;
            }
            assert!(Instant::now() < deadline, "closing worker did not retire");
            std::thread::yield_now();
        };
        assert!(matches!(result, Poll::RetiredSilent));
        assert!(!controller.is_busy());
    }

    #[test]
    fn completed_invalidated_job_retires_before_a_new_ticket_is_installed() {
        let mut controller = AppImport::default();
        let current = target("profile", "game");
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        start_barrier_job(
            &mut controller,
            current.clone(),
            entered.clone(),
            release.clone(),
            Ok(profile()),
        );
        entered.wait();
        controller.invalidate();
        release.wait();
        assert!(matches!(
            wait(&mut controller, &current, true, false),
            Poll::CachedOnly
        ));
        assert!(!controller.is_busy());

        let next = target("profile", "next-game");
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        start_barrier_job(
            &mut controller,
            next.clone(),
            entered.clone(),
            release.clone(),
            Ok(profile()),
        );
        entered.wait();
        assert!(controller.is_busy());
        assert!(controller.poll(&next, true, false, false).is_none());
        release.wait();
        assert!(matches!(
            wait(&mut controller, &next, true, false),
            Poll::Switch(_)
        ));
    }

    #[test]
    fn control_state_is_terminal_while_ticket_remains_until_result_poll() {
        let mut controller = AppImport::default();
        let current = target("profile", "game");
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let worker_entered = entered.clone();
        let worker_release = release.clone();
        controller
            .start_with(current.clone(), move |_, control, _| {
                control.start().unwrap();
                worker_entered.wait();
                control.begin_commit().unwrap();
                assert!(control.finish_success());
                worker_release.wait();
                Ok(profile())
            })
            .unwrap();
        entered.wait();
        while controller.worker.cancellation_view()
            != crate::ui::launcher::import_worker::CancellationView::Finishing
        {
            std::thread::yield_now();
        }
        assert!(controller.is_busy());
        release.wait();
        assert!(matches!(
            wait(&mut controller, &current, true, false),
            Poll::Switch(_)
        ));
        assert!(!controller.is_busy());
    }
}
