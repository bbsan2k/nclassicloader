//! Shared cancellation control for one native import job.

use crate::frontend::import_lifecycle::{CancelResult, CheckError, Lifecycle, StartError, State};
use std::sync::{Arc, Mutex, MutexGuard};

/// Cloneable control handle for one import job.
#[derive(Clone, Debug, Default)]
pub struct ImportControl {
    lifecycle: Arc<Mutex<Lifecycle>>,
}

impl ImportControl {
    /// Creates a control handle for a new import job.
    pub fn new() -> Self {
        Self::default()
    }

    fn lifecycle(&self) -> MutexGuard<'_, Lifecycle> {
        // The mutex protects only pure lifecycle transitions. Recovering the
        // inner value keeps cancellation fail-closed without poisoning the API.
        self.lifecycle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Requests cancellation of this job.
    pub fn cancel(&self) -> CancelResult {
        self.lifecycle().request_cancel()
    }

    /// Returns the job's current state.
    pub fn state(&self) -> State {
        self.lifecycle().state()
    }

    pub(crate) fn start(&self) -> Result<(), StartError> {
        self.lifecycle().start()
    }

    pub(crate) fn checkpoint(&self) -> Result<(), CheckError> {
        self.lifecycle().checkpoint()
    }

    pub(crate) fn begin_commit(&self) -> Result<(), CheckError> {
        self.lifecycle().begin_commit()
    }

    pub(crate) fn finish_success(&self) -> bool {
        self.lifecycle().finish_success()
    }

    pub(crate) fn finish_failure(&self) -> bool {
        self.lifecycle().finish_failure()
    }
}

/// Error returned by a cancellable import.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ImportError {
    /// Cancellation was acknowledged at a lifecycle checkpoint.
    Cancelled,
    /// This control handle has already been used for another import attempt.
    AlreadyStarted,
    /// An ordinary import failure, preserving its original display text.
    Failed(String),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("Import cancelled"),
            Self::AlreadyStarted => formatter.write_str("Import already started"),
            Self::Failed(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for ImportError {}

impl From<String> for ImportError {
    fn from(message: String) -> Self {
        Self::Failed(message)
    }
}

impl From<&str> for ImportError {
    fn from(message: &str) -> Self {
        Self::Failed(message.to_owned())
    }
}

pub(crate) fn start_error(error: StartError) -> ImportError {
    match error {
        StartError::Cancelled => ImportError::Cancelled,
        StartError::AlreadyStarted => ImportError::AlreadyStarted,
    }
}

pub(crate) fn checkpoint_error(error: CheckError) -> ImportError {
    match error {
        CheckError::Cancelled => ImportError::Cancelled,
        CheckError::InvalidState => ImportError::Failed("Invalid import lifecycle state".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::{ImportControl, State};
    use crate::{
        frontend::import_lifecycle::{CancelResult, CheckError},
        import_job::ImportError,
        profile::import_image_cancellable,
    };
    use std::{
        fs,
        path::PathBuf,
        sync::{Arc, Barrier},
        thread,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temporary(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("clover-import-job-{name}-{nonce}"))
    }

    #[test]
    fn prestart_cancellation_does_not_touch_the_filesystem() {
        let root = temporary("cancel-before-start");
        let source = root.join("missing.img");
        let cache = root.join("cache");
        let control = ImportControl::new();
        assert_eq!(control.cancel(), CancelResult::Requested);
        assert!(matches!(
            import_image_cancellable(&source, &cache, None, &control),
            Err(ImportError::Cancelled)
        ));
        assert_eq!(control.state(), State::Cancelled);
        assert!(!root.exists());
    }

    #[test]
    fn ordinary_failure_is_failed_and_a_control_cannot_start_a_second_job() {
        let root = temporary("failed-job");
        let control = ImportControl::new();
        let error = import_image_cancellable(
            &root.join("missing.img"),
            &root.join("cache"),
            None,
            &control,
        )
        .unwrap_err();
        assert!(matches!(error, ImportError::Failed(_)));
        assert_eq!(control.state(), State::Failed);
        assert!(matches!(
            import_image_cancellable(&root.join("missing.img"), &root, None, &control),
            Err(ImportError::AlreadyStarted)
        ));
        assert_eq!(control.state(), State::Failed);
    }

    #[test]
    fn hashing_progress_cancellation_stops_before_cache_or_stage_creation() {
        let root = temporary("cancel-hashing");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("input.img");
        fs::write(&source, b"not an Android boot image").unwrap();
        let cache = root.join("cache");
        let control = ImportControl::new();
        let callback_control = control.clone();
        let result = import_image_cancellable(
            &source,
            &cache,
            Some(&mut move |message| {
                if message == "Hashing filesystem image..." {
                    assert_eq!(callback_control.cancel(), CancelResult::Requested);
                }
            }),
            &control,
        );
        assert!(matches!(result, Err(ImportError::Cancelled)));
        assert_eq!(control.state(), State::Cancelled);
        assert!(!cache.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn concurrent_reuse_is_rejected_without_acknowledging_the_active_executor() {
        let root = temporary("concurrent-reuse");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("input.img");
        fs::write(&source, b"not an Android boot image").unwrap();
        let cache = root.join("cache");
        let control = ImportControl::new();
        let worker_control = control.clone();
        let reached_hashing = Arc::new(Barrier::new(2));
        let resume_hashing = Arc::new(Barrier::new(2));
        let worker_reached = reached_hashing.clone();
        let worker_resume = resume_hashing.clone();
        let worker_source = source.clone();
        let worker_cache = cache.clone();

        let executor = thread::spawn(move || {
            let mut progress = |message: &str| {
                if message == "Hashing filesystem image..." {
                    worker_reached.wait();
                    worker_resume.wait();
                }
            };
            super::super::profile::import_image_cancellable(
                &worker_source,
                &worker_cache,
                Some(&mut progress),
                &worker_control,
            )
        });

        reached_hashing.wait();
        assert!(matches!(
            import_image_cancellable(&source, &cache, None, &control),
            Err(ImportError::AlreadyStarted)
        ));
        assert_eq!(control.state(), State::Running);
        assert_eq!(control.cancel(), CancelResult::Requested);
        resume_hashing.wait();
        assert!(matches!(
            executor.join().unwrap(),
            Err(ImportError::Cancelled)
        ));
        assert_eq!(control.state(), State::Cancelled);
        assert!(!cache.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn commit_and_cancel_race_has_one_serialized_winner() {
        for _ in 0..64 {
            let control = ImportControl::new();
            assert_eq!(control.start(), Ok(()));
            let gate = Arc::new(Barrier::new(3));

            let cancel_control = control.clone();
            let cancel_gate = gate.clone();
            let cancel = thread::spawn(move || {
                cancel_gate.wait();
                cancel_control.cancel()
            });

            let commit_control = control.clone();
            let commit_gate = gate.clone();
            let commit = thread::spawn(move || {
                commit_gate.wait();
                commit_control.begin_commit()
            });

            gate.wait();
            let cancel_result = cancel.join().unwrap();
            let commit_result = commit.join().unwrap();
            match (cancel_result, commit_result) {
                (CancelResult::Requested, Err(CheckError::Cancelled)) => {
                    assert_eq!(control.state(), State::Cancelled);
                }
                (CancelResult::TooLate, Ok(())) => {
                    assert_eq!(control.state(), State::Committing);
                }
                outcomes => panic!("unexpected serialized transition outcomes: {outcomes:?}"),
            }
        }
    }
}
