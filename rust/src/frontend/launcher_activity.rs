//! Host-neutral launcher service activity policy, not a complete frame/update model.
use crate::frontend::import_lifecycle::CancelResult;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ImportOutcome {
    Succeeded,
    Cancelled,
    Failed(String),
    Retired,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Event {
    PickerOpened,
    PickerFinished { error: Option<String> },
    ImportStarted,
    Progress(String),
    CancelFeedback(CancelResult),
    ImportFinished(ImportOutcome),
    CloseRequested,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Activity {
    pub busy: bool,
    pub picker_pending: bool,
    pub closing: bool,
    pub status: String,
    pub error: bool,
    cancel_feedback: bool,
}

impl Activity {
    pub fn new(status: impl Into<String>) -> Self {
        Self {
            busy: false,
            picker_pending: false,
            closing: false,
            status: status.into(),
            error: false,
            cancel_feedback: false,
        }
    }

    /// Reduces one already-owner-validated service event. `false` means its phase guard rejected it.
    pub fn reduce(&mut self, event: Event) -> bool {
        match event {
            Event::PickerOpened if !self.busy && !self.closing && !self.picker_pending => {
                self.picker_pending = true;
            }
            Event::PickerFinished { error } if self.picker_pending => {
                self.picker_pending = false;
                if !self.closing
                    && let Some(message) = error
                {
                    self.status = message;
                    self.error = true;
                }
            }
            Event::ImportStarted if !self.busy && !self.picker_pending && !self.closing => {
                self.busy = true;
                self.error = false;
                self.status = "Starting read-only import...".into();
                self.cancel_feedback = false;
            }
            Event::Progress(value) if self.busy && !self.closing && !self.cancel_feedback => {
                self.status = value;
            }
            Event::CancelFeedback(result) if self.busy && !self.closing => match result {
                CancelResult::Requested => {
                    self.status =
                        "Cancellation requested; waiting for a safe stopping point...".into();
                    self.error = false;
                    self.cancel_feedback = true;
                }
                CancelResult::TooLate => {
                    self.status = "Finishing import; cancellation is too late.".into();
                    self.error = false;
                    self.cancel_feedback = true;
                }
                CancelResult::AlreadyRequested | CancelResult::Finished => {}
            },
            Event::ImportFinished(outcome) if self.busy => {
                self.busy = false;
                self.cancel_feedback = false;
                if !self.closing {
                    match outcome {
                        ImportOutcome::Succeeded | ImportOutcome::Retired => {}
                        ImportOutcome::Cancelled => {
                            self.status = "Import cancelled.".into();
                            self.error = false;
                        }
                        ImportOutcome::Failed(message) => {
                            self.status = message;
                            self.error = true;
                        }
                    }
                }
            }
            Event::CloseRequested => self.closing = true,
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::import_lifecycle::CancelResult;

    fn activity() -> Activity {
        Activity::new("initial")
    }

    #[test]
    fn picker_pending_blocks_start_and_picker_finish_does_not_clear_busy() {
        let mut state = activity();
        assert!(state.reduce(Event::PickerOpened));
        assert!(!state.reduce(Event::ImportStarted));
        assert!(!state.reduce(Event::ImportStarted));
        assert!(state.reduce(Event::PickerFinished { error: None }));
        assert!(state.reduce(Event::ImportStarted));
        assert!(!state.reduce(Event::PickerOpened));
        assert!(state.busy);
    }

    #[test]
    fn picker_finish_while_import_is_busy_clears_only_picker_phase() {
        let mut state = activity();
        assert!(state.reduce(Event::ImportStarted));
        state.picker_pending = true;
        assert!(state.reduce(Event::PickerFinished { error: None }));
        assert!(state.busy);
        assert!(!state.picker_pending);
    }

    #[test]
    fn cancellation_feedback_keeps_busy_and_suppresses_progress_until_result() {
        let mut state = activity();
        assert!(state.reduce(Event::ImportStarted));
        assert!(state.reduce(Event::CancelFeedback(CancelResult::Requested)));
        let feedback = state.status.clone();
        assert!(!state.reduce(Event::Progress("late progress".into())));
        assert_eq!(state.status, feedback);
        assert!(state.busy);
        assert!(state.reduce(Event::ImportFinished(ImportOutcome::Cancelled)));
        assert!(!state.busy);
        assert_eq!(state.status, "Import cancelled.");
    }

    #[test]
    fn too_late_feedback_is_not_overwritten_by_cancelled_notice_without_result() {
        let mut state = activity();
        state.reduce(Event::ImportStarted);
        state.reduce(Event::CancelFeedback(CancelResult::TooLate));
        let finishing = state.status.clone();
        assert!(!state.reduce(Event::Progress("late progress".into())));
        assert_eq!(state.status, finishing);
        assert!(state.busy);
        state.reduce(Event::ImportFinished(ImportOutcome::Cancelled));
        assert_eq!(state.status, "Import cancelled.");
    }

    #[test]
    fn closing_retirement_preserves_notice_and_stale_events_are_ignored() {
        let mut state = activity();
        state.reduce(Event::ImportStarted);
        state.status = "existing notice".into();
        state.error = true;
        state.reduce(Event::CloseRequested);
        assert!(state.reduce(Event::ImportFinished(ImportOutcome::Retired)));
        assert_eq!(state.status, "existing notice");
        assert!(state.error);
        assert!(!state.busy);
        assert!(!state.reduce(Event::Progress("stale".into())));
        assert!(!state.reduce(Event::ImportFinished(ImportOutcome::Cancelled)));
    }

    #[test]
    fn picker_error_and_import_error_or_cancel_are_distinct() {
        let mut state = activity();
        state.reduce(Event::PickerOpened);
        assert!(state.reduce(Event::PickerFinished {
            error: Some("picker failed".into()),
        }));
        assert_eq!(state.status, "picker failed");
        assert!(state.error);
        state.reduce(Event::ImportStarted);
        assert!(!state.error);
        state.reduce(Event::ImportFinished(ImportOutcome::Failed(
            "import failed".into(),
        )));
        assert_eq!(state.status, "import failed");
        assert!(state.error);
        state.reduce(Event::ImportStarted);
        state.reduce(Event::ImportFinished(ImportOutcome::Cancelled));
        assert_eq!(state.status, "Import cancelled.");
        assert!(!state.error);
    }

    #[test]
    fn duplicate_finish_and_idle_progress_are_rejected() {
        let mut state = activity();
        assert!(!state.reduce(Event::Progress("stale".into())));
        assert!(!state.reduce(Event::ImportFinished(ImportOutcome::Cancelled)));
        assert!(state.reduce(Event::ImportStarted));
        assert!(state.reduce(Event::ImportFinished(ImportOutcome::Succeeded)));
        assert!(!state.reduce(Event::ImportFinished(ImportOutcome::Succeeded)));
    }
}
